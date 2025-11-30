//! Agentic tool loop for LLM execution.
//!
//! This module contains the main agentic loop that handles:
//! - Tool execution with HITL approval
//! - Loop detection and prevention
//! - Context window management
//! - Message history management

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use rig::completion::{AssistantContent, CompletionModel as RigCompletionModel, Message};
use rig::message::{Text, ToolCall, ToolResult, ToolResultContent, UserContent};
use rig::one_or_many::OneOrMany;
use serde_json::json;
use tokio::sync::{mpsc, oneshot, RwLock};
use vtcode_core::tools::ToolRegistry;

use super::context_manager::ContextManager;
use super::events::AiEvent;
use super::hitl::{ApprovalDecision, ApprovalRecorder, RiskLevel};
use super::loop_detection::{LoopDetectionResult, LoopDetector};
use super::sub_agent::{SubAgentContext, SubAgentRegistry, MAX_AGENT_DEPTH};
use super::sub_agent_executor::{execute_sub_agent, SubAgentExecutorContext};
use super::token_budget::TokenAlertLevel;
use super::tool_definitions::{
    get_all_tool_definitions, get_sub_agent_tool_definitions, get_tavily_tool_definitions,
};
use super::tool_executors::{
    execute_in_terminal, execute_indexer_tool, execute_tavily_tool, normalize_run_pty_cmd_args,
};
use super::tool_policy::{PolicyConstraintResult, ToolPolicy, ToolPolicyManager};
use crate::indexer::IndexerState;
use crate::pty::PtyManager;
use crate::tavily::TavilyState;

/// Maximum number of tool call iterations before stopping
pub const MAX_TOOL_ITERATIONS: usize = 100;

/// Context for the agentic loop execution.
pub struct AgenticLoopContext<'a> {
    pub event_tx: &'a mpsc::UnboundedSender<AiEvent>,
    pub tool_registry: &'a Arc<RwLock<ToolRegistry>>,
    pub sub_agent_registry: &'a Arc<RwLock<SubAgentRegistry>>,
    pub pty_manager: Option<&'a Arc<PtyManager>>,
    pub current_session_id: &'a Arc<RwLock<Option<String>>>,
    pub indexer_state: Option<&'a Arc<IndexerState>>,
    pub tavily_state: Option<&'a Arc<TavilyState>>,
    pub approval_recorder: &'a Arc<ApprovalRecorder>,
    pub pending_approvals: &'a Arc<RwLock<HashMap<String, oneshot::Sender<ApprovalDecision>>>>,
    pub tool_policy_manager: &'a Arc<ToolPolicyManager>,
    pub context_manager: &'a Arc<ContextManager>,
    pub loop_detector: &'a Arc<RwLock<LoopDetector>>,
}

/// Result of a single tool execution.
pub struct ToolExecutionResult {
    pub value: serde_json::Value,
    pub success: bool,
}

/// Execute a tool with HITL approval check.
pub async fn execute_with_hitl(
    tool_name: &str,
    tool_args: &serde_json::Value,
    tool_id: &str,
    context: &SubAgentContext,
    model: &rig_anthropic_vertex::CompletionModel,
    ctx: &AgenticLoopContext<'_>,
) -> Result<ToolExecutionResult> {
    // Step 1: Check if tool is denied by policy
    if ctx.tool_policy_manager.is_denied(tool_name).await {
        let _ = ctx.event_tx.send(AiEvent::ToolDenied {
            request_id: tool_id.to_string(),
            tool_name: tool_name.to_string(),
            args: tool_args.clone(),
            reason: "Tool is denied by policy".to_string(),
        });
        return Ok(ToolExecutionResult {
            value: json!({
                "error": format!("Tool '{}' is denied by policy", tool_name),
                "denied_by_policy": true
            }),
            success: false,
        });
    }

    // Step 2: Apply constraints and check for violations
    let (effective_args, constraint_note) = match ctx
        .tool_policy_manager
        .apply_constraints(tool_name, tool_args)
        .await
    {
        PolicyConstraintResult::Allowed => (tool_args.clone(), None),
        PolicyConstraintResult::Violated(reason) => {
            let _ = ctx.event_tx.send(AiEvent::ToolDenied {
                request_id: tool_id.to_string(),
                tool_name: tool_name.to_string(),
                args: tool_args.clone(),
                reason: reason.clone(),
            });
            return Ok(ToolExecutionResult {
                value: json!({
                    "error": format!("Tool constraint violated: {}", reason),
                    "constraint_violated": true
                }),
                success: false,
            });
        }
        PolicyConstraintResult::Modified(modified_args, note) => {
            tracing::info!("Tool '{}' args modified by constraint: {}", tool_name, note);
            (modified_args, Some(note))
        }
    };

    // Step 3: Check if tool is allowed by policy (bypasses HITL)
    let policy = ctx.tool_policy_manager.get_policy(tool_name).await;
    if policy == ToolPolicy::Allow {
        let reason = if let Some(note) = constraint_note {
            format!("Allowed by policy ({})", note)
        } else {
            "Allowed by tool policy".to_string()
        };
        let _ = ctx.event_tx.send(AiEvent::ToolAutoApproved {
            request_id: tool_id.to_string(),
            tool_name: tool_name.to_string(),
            args: effective_args.clone(),
            reason,
        });

        return execute_tool_direct(tool_name, &effective_args, context, model, ctx).await;
    }

    // Step 4: Check if tool should be auto-approved based on learned patterns
    if ctx.approval_recorder.should_auto_approve(tool_name).await {
        let _ = ctx.event_tx.send(AiEvent::ToolAutoApproved {
            request_id: tool_id.to_string(),
            tool_name: tool_name.to_string(),
            args: effective_args.clone(),
            reason: "Auto-approved based on learned patterns or always-allow list".to_string(),
        });

        return execute_tool_direct(tool_name, &effective_args, context, model, ctx).await;
    }

    // Step 5: Need approval - create request with stats
    let stats = ctx.approval_recorder.get_pattern(tool_name).await;
    let risk_level = RiskLevel::for_tool(tool_name);
    let config = ctx.approval_recorder.get_config().await;
    let can_learn = !config
        .always_require_approval
        .contains(&tool_name.to_string());
    let suggestion = ctx.approval_recorder.get_suggestion(tool_name).await;

    // Create oneshot channel for response
    let (tx, rx) = oneshot::channel::<ApprovalDecision>();

    // Store the sender
    {
        let mut pending = ctx.pending_approvals.write().await;
        pending.insert(tool_id.to_string(), tx);
    }

    // Emit approval request event with HITL metadata
    let _ = ctx.event_tx.send(AiEvent::ToolApprovalRequest {
        request_id: tool_id.to_string(),
        tool_name: tool_name.to_string(),
        args: effective_args.clone(),
        stats,
        risk_level,
        can_learn,
        suggestion,
    });

    // Wait for approval response (with timeout of 5 minutes)
    match tokio::time::timeout(std::time::Duration::from_secs(300), rx).await {
        Ok(Ok(decision)) => {
            if decision.approved {
                let _ = ctx
                    .approval_recorder
                    .record_approval(tool_name, true, decision.reason, decision.always_allow)
                    .await;

                execute_tool_direct(tool_name, &effective_args, context, model, ctx).await
            } else {
                let _ = ctx
                    .approval_recorder
                    .record_approval(tool_name, false, decision.reason, false)
                    .await;

                Ok(ToolExecutionResult {
                    value: json!({"error": "Tool execution denied by user", "denied": true}),
                    success: false,
                })
            }
        }
        Ok(Err(_)) => Ok(ToolExecutionResult {
            value: json!({"error": "Approval request cancelled", "cancelled": true}),
            success: false,
        }),
        Err(_) => {
            let mut pending = ctx.pending_approvals.write().await;
            pending.remove(tool_id);

            Ok(ToolExecutionResult {
                value: json!({"error": "Approval request timed out after 5 minutes", "timeout": true}),
                success: false,
            })
        }
    }
}

/// Execute a tool directly (after approval or auto-approved).
pub async fn execute_tool_direct(
    tool_name: &str,
    tool_args: &serde_json::Value,
    context: &SubAgentContext,
    model: &rig_anthropic_vertex::CompletionModel,
    ctx: &AgenticLoopContext<'_>,
) -> Result<ToolExecutionResult> {
    // Check if this is an indexer tool call
    if tool_name.starts_with("indexer_") {
        let (value, success) = execute_indexer_tool(ctx.indexer_state, tool_name, tool_args).await;
        return Ok(ToolExecutionResult { value, success });
    }

    // Check if this is a web search (Tavily) tool call
    if tool_name.starts_with("web_search") || tool_name == "web_extract" {
        let (value, success) = execute_tavily_tool(ctx.tavily_state, tool_name, tool_args).await;
        return Ok(ToolExecutionResult { value, success });
    }

    // Check if this is a sub-agent call
    if tool_name.starts_with("sub_agent_") {
        let agent_id = tool_name.strip_prefix("sub_agent_").unwrap_or("");

        // Get the agent definition
        let registry = ctx.sub_agent_registry.read().await;
        let agent_def = match registry.get(agent_id) {
            Some(def) => def.clone(),
            None => {
                return Ok(ToolExecutionResult {
                    value: json!({ "error": format!("Sub-agent '{}' not found", agent_id) }),
                    success: false,
                });
            }
        };
        drop(registry);

        let sub_ctx = SubAgentExecutorContext {
            event_tx: ctx.event_tx,
            tavily_state: ctx.tavily_state,
            pty_manager: ctx.pty_manager,
            current_session_id: ctx.current_session_id,
            tool_registry: ctx.tool_registry,
        };

        match execute_sub_agent(&agent_def, tool_args, context, model, sub_ctx).await {
            Ok(result) => {
                return Ok(ToolExecutionResult {
                    value: json!({
                        "agent_id": result.agent_id,
                        "response": result.response,
                        "success": result.success,
                        "duration_ms": result.duration_ms
                    }),
                    success: result.success,
                });
            }
            Err(e) => {
                return Ok(ToolExecutionResult {
                    value: json!({ "error": e.to_string() }),
                    success: false,
                });
            }
        }
    }

    // Check if this is a terminal command that should be intercepted
    if tool_name == "run_pty_cmd"
        && ctx.pty_manager.is_some()
        && ctx.current_session_id.read().await.is_some()
    {
        let command = tool_args
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match execute_in_terminal(ctx.pty_manager, ctx.current_session_id, command).await {
            Ok(v) => {
                return Ok(ToolExecutionResult {
                    value: v,
                    success: true,
                })
            }
            Err(e) => {
                return Ok(ToolExecutionResult {
                    value: json!({"error": e.to_string()}),
                    success: false,
                })
            }
        }
    }

    // Execute regular tool via registry
    let mut registry = ctx.tool_registry.write().await;
    let result = registry.execute_tool(tool_name, tool_args.clone()).await;

    match &result {
        Ok(v) => {
            let is_success = v
                .get("exit_code")
                .and_then(|ec| ec.as_i64())
                .map(|ec| ec == 0)
                .unwrap_or(true);
            Ok(ToolExecutionResult {
                value: v.clone(),
                success: is_success,
            })
        }
        Err(e) => Ok(ToolExecutionResult {
            value: json!({"error": e.to_string()}),
            success: false,
        }),
    }
}

/// Handle loop detection result and create appropriate tool result if blocked.
pub fn handle_loop_detection(
    loop_result: &LoopDetectionResult,
    tool_id: &str,
    event_tx: &mpsc::UnboundedSender<AiEvent>,
) -> Option<UserContent> {
    match loop_result {
        LoopDetectionResult::Blocked {
            tool_name,
            repeat_count,
            max_count,
            message,
        } => {
            let _ = event_tx.send(AiEvent::LoopBlocked {
                tool_name: tool_name.clone(),
                repeat_count: *repeat_count,
                max_count: *max_count,
                message: message.clone(),
            });
            let result_text = serde_json::to_string(&json!({
                "error": message,
                "loop_detected": true,
                "repeat_count": repeat_count,
                "suggestion": "Try a different approach or modify the arguments"
            }))
            .unwrap_or_default();
            Some(UserContent::ToolResult(ToolResult {
                id: tool_id.to_string(),
                call_id: Some(tool_id.to_string()),
                content: OneOrMany::one(ToolResultContent::Text(Text { text: result_text })),
            }))
        }
        LoopDetectionResult::MaxIterationsReached {
            iterations,
            max_iterations,
            message,
        } => {
            let _ = event_tx.send(AiEvent::MaxIterationsReached {
                iterations: *iterations,
                max_iterations: *max_iterations,
                message: message.clone(),
            });
            let result_text = serde_json::to_string(&json!({
                "error": message,
                "max_iterations_reached": true,
                "suggestion": "Provide a final response to the user"
            }))
            .unwrap_or_default();
            Some(UserContent::ToolResult(ToolResult {
                id: tool_id.to_string(),
                call_id: Some(tool_id.to_string()),
                content: OneOrMany::one(ToolResultContent::Text(Text { text: result_text })),
            }))
        }
        LoopDetectionResult::Warning {
            tool_name,
            current_count,
            max_count,
            message,
        } => {
            let _ = event_tx.send(AiEvent::LoopWarning {
                tool_name: tool_name.clone(),
                current_count: *current_count,
                max_count: *max_count,
                message: message.clone(),
            });
            None // Warning doesn't block execution
        }
        LoopDetectionResult::Allowed => None,
    }
}

/// Execute the main agentic loop with tool calling.
///
/// This function runs the LLM completion loop, handling:
/// - Tool calls and results
/// - Loop detection
/// - Context window management
/// - HITL approval
pub async fn run_agentic_loop(
    model: &rig_anthropic_vertex::CompletionModel,
    system_prompt: &str,
    initial_history: Vec<Message>,
    context: SubAgentContext,
    ctx: &AgenticLoopContext<'_>,
) -> Result<(String, Vec<Message>)> {
    // Reset loop detector for new turn
    {
        let mut detector = ctx.loop_detector.write().await;
        detector.reset();
    }

    // Get all available tools (standard + sub-agents + web search)
    let mut tools = get_all_tool_definitions();

    // Add web search tools if Tavily is available
    tools.extend(get_tavily_tool_definitions(ctx.tavily_state));

    // Only add sub-agent tools if we're not at max depth
    if context.depth < MAX_AGENT_DEPTH - 1 {
        let registry = ctx.sub_agent_registry.read().await;
        tools.extend(get_sub_agent_tool_definitions(&registry).await);
    }

    let original_history_len = initial_history.len();
    let mut chat_history = initial_history;

    // Update context manager with current history
    ctx.context_manager
        .update_from_messages(&chat_history)
        .await;

    // Enforce context window limits if needed
    let alert_level = ctx.context_manager.alert_level().await;
    if matches!(
        alert_level,
        TokenAlertLevel::Alert | TokenAlertLevel::Critical
    ) {
        let utilization_before = ctx.context_manager.utilization().await;
        tracing::info!(
            "Context alert level {:?} ({:.1}% utilization), enforcing context window",
            alert_level,
            utilization_before * 100.0
        );
        chat_history = ctx
            .context_manager
            .enforce_context_window(&chat_history)
            .await;

        // Update stats after pruning
        ctx.context_manager
            .update_from_messages(&chat_history)
            .await;
        let utilization_after = ctx.context_manager.utilization().await;

        // Emit context event to frontend
        let _ = ctx.event_tx.send(AiEvent::ContextPruned {
            messages_removed: original_history_len.saturating_sub(chat_history.len()),
            utilization_before,
            utilization_after,
        });
    }

    let mut accumulated_response = String::new();
    let mut iteration = 0;

    loop {
        iteration += 1;
        if iteration > MAX_TOOL_ITERATIONS {
            let _ = ctx.event_tx.send(AiEvent::Error {
                message: "Maximum tool iterations reached".to_string(),
                error_type: "max_iterations".to_string(),
            });
            break;
        }

        // Build request
        let request = rig::completion::CompletionRequest {
            preamble: Some(system_prompt.to_string()),
            chat_history: OneOrMany::many(chat_history.clone())
                .unwrap_or_else(|_| OneOrMany::one(chat_history[0].clone())),
            documents: vec![],
            tools: tools.clone(),
            temperature: Some(0.7),
            max_tokens: Some(8192),
            tool_choice: None,
            additional_params: None,
        };

        // Make completion request
        let response = model
            .completion(request)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        // Process response
        let mut has_tool_calls = false;
        let mut tool_calls_to_execute: Vec<ToolCall> = vec![];
        let mut text_content = String::new();

        for content in response.choice.iter() {
            match content {
                AssistantContent::Text(text) => {
                    text_content.push_str(&text.text);
                }
                AssistantContent::ToolCall(tool_call) => {
                    has_tool_calls = true;
                    tool_calls_to_execute.push(tool_call.clone());
                }
                _ => {}
            }
        }

        // Emit text delta if we have text
        if !text_content.is_empty() {
            accumulated_response.push_str(&text_content);
            let _ = ctx.event_tx.send(AiEvent::TextDelta {
                delta: text_content.clone(),
                accumulated: accumulated_response.clone(),
            });
        }

        // If no tool calls, we're done
        if !has_tool_calls {
            break;
        }

        // Add assistant response to history
        let assistant_content: Vec<AssistantContent> = response.choice.iter().cloned().collect();
        chat_history.push(Message::Assistant {
            id: None,
            content: OneOrMany::many(assistant_content).unwrap_or_else(|_| {
                OneOrMany::one(AssistantContent::Text(Text {
                    text: String::new(),
                }))
            }),
        });

        // Execute tool calls and collect results
        let mut tool_results: Vec<UserContent> = vec![];

        for tool_call in tool_calls_to_execute {
            let tool_name = &tool_call.function.name;
            // Normalize run_pty_cmd args to convert array commands to strings
            let tool_args = if tool_name == "run_pty_cmd" {
                normalize_run_pty_cmd_args(tool_call.function.arguments.clone())
            } else {
                tool_call.function.arguments.clone()
            };
            let tool_id = tool_call.id.clone();

            // Check for loop detection
            let loop_result = {
                let mut detector = ctx.loop_detector.write().await;
                detector.record_tool_call(tool_name, &tool_args)
            };

            // Handle loop detection (may add a blocked result)
            if let Some(blocked_result) =
                handle_loop_detection(&loop_result, &tool_id, ctx.event_tx)
            {
                tool_results.push(blocked_result);
                continue;
            }

            // Execute tool with HITL approval check
            let result = match execute_with_hitl(
                tool_name, &tool_args, &tool_id, &context, model, ctx,
            )
            .await
            {
                Ok(r) => r,
                Err(e) => ToolExecutionResult {
                    value: json!({ "error": e.to_string() }),
                    success: false,
                },
            };

            // Emit tool result event
            let _ = ctx.event_tx.send(AiEvent::ToolResult {
                tool_name: tool_name.clone(),
                result: result.value.clone(),
                success: result.success,
                request_id: tool_id.clone(),
            });

            // Add to tool results for LLM
            let result_text = serde_json::to_string(&result.value).unwrap_or_default();
            tool_results.push(UserContent::ToolResult(ToolResult {
                id: tool_id.clone(),
                call_id: Some(tool_id),
                content: OneOrMany::one(ToolResultContent::Text(Text { text: result_text })),
            }));
        }

        // Add tool results as user message
        chat_history.push(Message::User {
            content: OneOrMany::many(tool_results).unwrap_or_else(|_| {
                OneOrMany::one(UserContent::Text(Text {
                    text: "Tool executed".to_string(),
                }))
            }),
        });
    }

    Ok((accumulated_response, chat_history))
}

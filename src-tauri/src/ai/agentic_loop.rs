//! Agentic tool loop for LLM execution.
//!
//! This module contains the main agentic loop that handles:
//! - Tool execution with HITL approval
//! - Loop detection and prevention
//! - Context window management
//! - Message history management
//! - Extended thinking (streaming reasoning content)

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use futures::StreamExt;
use rig::completion::{AssistantContent, CompletionModel as RigCompletionModel, Message};
use rig::message::{Reasoning, Text, ToolCall, ToolResult, ToolResultContent, UserContent};
use rig::one_or_many::OneOrMany;
use rig::streaming::StreamedAssistantContent;
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
    get_all_tool_definitions_with_config, get_sub_agent_tool_definitions,
    get_tavily_tool_definitions, ToolConfig,
};
use super::tool_executors::{
    execute_indexer_tool, execute_tavily_tool, execute_web_fetch_tool, normalize_run_pty_cmd_args,
};
use super::tool_policy::{PolicyConstraintResult, ToolPolicy, ToolPolicyManager};
use crate::indexer::IndexerState;
use crate::tavily::TavilyState;

/// Maximum number of tool call iterations before stopping
pub const MAX_TOOL_ITERATIONS: usize = 100;

/// Timeout for approval requests in seconds (5 minutes)
pub const APPROVAL_TIMEOUT_SECS: u64 = 300;

/// Maximum tokens for a single completion request
pub const MAX_COMPLETION_TOKENS: u32 = 10_000;

/// Context for the agentic loop execution.
pub struct AgenticLoopContext<'a> {
    pub event_tx: &'a mpsc::UnboundedSender<AiEvent>,
    pub tool_registry: &'a Arc<RwLock<ToolRegistry>>,
    pub sub_agent_registry: &'a Arc<RwLock<SubAgentRegistry>>,
    pub indexer_state: Option<&'a Arc<IndexerState>>,
    pub tavily_state: Option<&'a Arc<TavilyState>>,
    pub approval_recorder: &'a Arc<ApprovalRecorder>,
    pub pending_approvals: &'a Arc<RwLock<HashMap<String, oneshot::Sender<ApprovalDecision>>>>,
    pub tool_policy_manager: &'a Arc<ToolPolicyManager>,
    pub context_manager: &'a Arc<ContextManager>,
    pub loop_detector: &'a Arc<RwLock<LoopDetector>>,
    /// Tool configuration for filtering available tools
    pub tool_config: &'a ToolConfig,
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

    // Wait for approval response (with timeout)
    match tokio::time::timeout(std::time::Duration::from_secs(APPROVAL_TIMEOUT_SECS), rx).await {
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
                value: json!({"error": format!("Approval request timed out after {} seconds", APPROVAL_TIMEOUT_SECS), "timeout": true}),
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

    // Check if this is our custom web_fetch tool (with readability extraction)
    if tool_name == "web_fetch" {
        let (value, success) = execute_web_fetch_tool(tool_name, tool_args).await;
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

    // Execute regular tool via registry
    let mut registry = ctx.tool_registry.write().await;
    let result = registry.execute_tool(tool_name, tool_args.clone()).await;

    match &result {
        Ok(v) => {
            // Check for failure: exit_code != 0 OR presence of "error" field
            let is_failure_by_exit_code = v
                .get("exit_code")
                .and_then(|ec| ec.as_i64())
                .map(|ec| ec != 0)
                .unwrap_or(false);
            let has_error_field = v.get("error").is_some();
            let is_success = !is_failure_by_exit_code && !has_error_field;
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
/// - Extended thinking (streaming reasoning content)
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

    // Get all available tools (filtered by config + sub-agents + web search)
    let mut tools = get_all_tool_definitions_with_config(ctx.tool_config);

    // print list of tool names to the console
    tracing::debug!(
        "Available tools: {:?}",
        tools.iter().map(|t| t.name.clone()).collect::<Vec<_>>()
    );

    // Add web search tools if Tavily is available and not disabled by config
    tools.extend(
        get_tavily_tool_definitions(ctx.tavily_state)
            .into_iter()
            .filter(|t| ctx.tool_config.is_tool_enabled(&t.name)),
    );

    // Only add sub-agent tools if we're not at max depth
    // Sub-agents are controlled by the registry, not the tool config
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
    let mut accumulated_thinking = String::new();
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
            temperature: Some(0.5),
            max_tokens: Some(MAX_COMPLETION_TOKENS as u64),
            tool_choice: None,
            additional_params: None,
        };

        // Make streaming completion request to capture thinking content
        tracing::info!("Starting streaming completion request");
        let mut stream = model.stream(request).await.map_err(|e| {
            tracing::error!("Failed to start stream: {}", e);
            anyhow::anyhow!("{}", e)
        })?;
        tracing::info!("Stream started successfully");

        // Process streaming response
        let mut has_tool_calls = false;
        let mut tool_calls_to_execute: Vec<ToolCall> = vec![];
        let mut text_content = String::new();
        let mut thinking_content = String::new();
        let mut thinking_signature: Option<String> = None;
        let mut chunk_count = 0;

        // Track tool call state for streaming
        let mut current_tool_id: Option<String> = None;
        let mut current_tool_name: Option<String> = None;
        let mut current_tool_args = String::new();

        while let Some(chunk_result) = stream.next().await {
            chunk_count += 1;
            match chunk_result {
                Ok(chunk) => {
                    match chunk {
                        StreamedAssistantContent::Text(text_msg) => {
                            // Check if this is thinking content (prefixed by our streaming impl)
                            // This handles the case where thinking is sent as a [Thinking] prefixed message
                            if let Some(thinking) = text_msg.text.strip_prefix("[Thinking] ") {
                                tracing::debug!("Text chunk is [Thinking] prefixed");
                                thinking_content.push_str(thinking);
                                accumulated_thinking.push_str(thinking);
                                // Emit reasoning event for frontend
                                let _ = ctx.event_tx.send(AiEvent::Reasoning {
                                    content: thinking.to_string(),
                                });
                            } else {
                                // Regular text content
                                text_content.push_str(&text_msg.text);
                                accumulated_response.push_str(&text_msg.text);
                                let _ = ctx.event_tx.send(AiEvent::TextDelta {
                                    delta: text_msg.text,
                                    accumulated: accumulated_response.clone(),
                                });
                            }
                        }
                        StreamedAssistantContent::Reasoning(reasoning) => {
                            // Native reasoning/thinking content from extended thinking models
                            let reasoning_text = reasoning.reasoning.join("");
                            tracing::debug!(
                                "Received reasoning chunk #{}: {} chars",
                                chunk_count,
                                reasoning_text.len()
                            );
                            thinking_content.push_str(&reasoning_text);
                            accumulated_thinking.push_str(&reasoning_text);
                            // Capture the signature (needed for API when sending back history)
                            if reasoning.signature.is_some() {
                                thinking_signature = reasoning.signature.clone();
                            }
                            // Emit reasoning event for frontend
                            let _ = ctx.event_tx.send(AiEvent::Reasoning {
                                content: reasoning_text,
                            });
                        }
                        StreamedAssistantContent::ToolCall(tool_call) => {
                            tracing::info!(
                                "Received tool call chunk #{}: {}",
                                chunk_count,
                                tool_call.function.name
                            );
                            has_tool_calls = true;

                            // Finalize any previous pending tool call first
                            if let (Some(prev_id), Some(prev_name)) =
                                (current_tool_id.take(), current_tool_name.take())
                            {
                                let args: serde_json::Value =
                                    serde_json::from_str(&current_tool_args)
                                        .unwrap_or(serde_json::Value::Null);
                                tracing::info!(
                                    "Finalizing previous tool call: {} with args: {}",
                                    prev_name,
                                    current_tool_args
                                );
                                tool_calls_to_execute.push(ToolCall {
                                    id: prev_id.clone(),
                                    call_id: Some(prev_id),
                                    function: rig::message::ToolFunction {
                                        name: prev_name,
                                        arguments: args,
                                    },
                                });
                                current_tool_args.clear();
                            }

                            // Check if this tool call has complete args (non-streaming case)
                            // If args are empty object {}, we'll wait for deltas
                            let has_complete_args = !tool_call.function.arguments.is_null()
                                && tool_call.function.arguments != serde_json::json!({});

                            if has_complete_args {
                                // Tool call came complete, add directly
                                tracing::info!("Tool call has complete args, adding directly");
                                tool_calls_to_execute.push(tool_call);
                            } else {
                                // Tool call has empty args, wait for deltas
                                tracing::info!(
                                    "Tool call has empty args, tracking for delta accumulation"
                                );
                                current_tool_id = Some(tool_call.id.clone());
                                current_tool_name = Some(tool_call.function.name.clone());
                                // Start with any existing args (might be empty object serialized)
                                if !tool_call.function.arguments.is_null()
                                    && tool_call.function.arguments != serde_json::json!({})
                                {
                                    current_tool_args = tool_call.function.arguments.to_string();
                                }
                            }
                        }
                        StreamedAssistantContent::ToolCallDelta { id, delta } => {
                            tracing::debug!(
                                "Received tool call delta #{}: id={}, {} chars",
                                chunk_count,
                                id,
                                delta.len()
                            );
                            // If we don't have a current tool ID but the delta has one, use it
                            if current_tool_id.is_none() && !id.is_empty() {
                                current_tool_id = Some(id);
                            }
                            // Accumulate tool call argument deltas
                            current_tool_args.push_str(&delta);
                        }
                        StreamedAssistantContent::Final(ref resp) => {
                            tracing::info!(
                                "Received final response chunk #{}: {:?}",
                                chunk_count,
                                resp
                            );
                            // Finalize any pending tool call from deltas
                            if let (Some(id), Some(name)) =
                                (current_tool_id.take(), current_tool_name.take())
                            {
                                let args: serde_json::Value =
                                    serde_json::from_str(&current_tool_args)
                                        .unwrap_or(serde_json::Value::Null);
                                tool_calls_to_execute.push(ToolCall {
                                    id: id.clone(),
                                    call_id: Some(id),
                                    function: rig::message::ToolFunction {
                                        name,
                                        arguments: args,
                                    },
                                });
                                current_tool_args.clear();
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Stream chunk error at #{}: {}", chunk_count, e);
                }
            }
        }

        tracing::info!(
            "Stream completed: {} chunks, {} chars text, {} chars thinking, {} tool calls",
            chunk_count,
            text_content.len(),
            thinking_content.len(),
            tool_calls_to_execute.len()
        );

        // Finalize any remaining tool call that wasn't closed by FinalResponse
        if let (Some(id), Some(name)) = (current_tool_id.take(), current_tool_name.take()) {
            let args: serde_json::Value =
                serde_json::from_str(&current_tool_args).unwrap_or(serde_json::Value::Null);
            tool_calls_to_execute.push(ToolCall {
                id: id.clone(),
                call_id: Some(id),
                function: rig::message::ToolFunction {
                    name,
                    arguments: args,
                },
            });
            has_tool_calls = true;
        }

        // Log thinking content if present (for debugging)
        if !thinking_content.is_empty() {
            tracing::debug!("Model thinking: {} chars", thinking_content.len());
        }

        // If no tool calls, we're done
        if !has_tool_calls {
            break;
        }

        // Build assistant content for history (thinking + text + tool calls)
        // IMPORTANT: Thinking blocks MUST come first when extended thinking is enabled
        let mut assistant_content: Vec<AssistantContent> = vec![];

        // Add thinking content first (required by Anthropic API when thinking is enabled)
        if !thinking_content.is_empty() {
            assistant_content.push(AssistantContent::Reasoning(
                Reasoning::multi(vec![thinking_content.clone()])
                    .with_signature(thinking_signature.clone()),
            ));
        }

        if !text_content.is_empty() {
            assistant_content.push(AssistantContent::Text(Text {
                text: text_content.clone(),
            }));
        }
        for tool_call in &tool_calls_to_execute {
            assistant_content.push(AssistantContent::ToolCall(tool_call.clone()));
        }

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
            let result = execute_with_hitl(tool_name, &tool_args, &tool_id, &context, model, ctx)
                .await
                .unwrap_or_else(|e| ToolExecutionResult {
                    value: json!({ "error": e.to_string() }),
                    success: false,
                });

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

    // Log total thinking if any was accumulated
    if !accumulated_thinking.is_empty() {
        tracing::info!(
            "Total thinking content: {} chars",
            accumulated_thinking.len()
        );
    }

    Ok((accumulated_response, chat_history))
}

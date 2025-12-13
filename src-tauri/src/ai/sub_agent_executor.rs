//! Sub-agent execution logic.
//!
//! This module handles the execution of sub-agents, which are specialized
//! agents that can be invoked by the main agent to handle specific tasks.

use std::sync::Arc;

use anyhow::Result;
use rig::completion::{AssistantContent, CompletionModel as RigCompletionModel, Message};
use rig::message::{Text, ToolCall, ToolResult, ToolResultContent, UserContent};
use rig::one_or_many::OneOrMany;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use vtcode_core::tools::ToolRegistry;

use super::events::AiEvent;
use super::sub_agent::{SubAgentContext, SubAgentDefinition, SubAgentResult};
use super::tool_definitions::{
    filter_tools_by_allowed, get_all_tool_definitions, get_tavily_tool_definitions,
};
use super::tool_executors::{
    execute_tavily_tool, execute_web_fetch_tool, normalize_run_pty_cmd_args,
};
use crate::tavily::TavilyState;

/// Context needed for sub-agent execution.
pub struct SubAgentExecutorContext<'a> {
    pub event_tx: &'a mpsc::UnboundedSender<AiEvent>,
    pub tavily_state: Option<&'a Arc<TavilyState>>,
    pub tool_registry: &'a Arc<RwLock<ToolRegistry>>,
}

/// Execute a sub-agent with the given task and context.
///
/// # Arguments
/// * `agent_def` - The sub-agent definition
/// * `args` - Arguments containing the task and optional context
/// * `parent_context` - The context from the parent agent
/// * `model` - The LLM model to use for completion
/// * `ctx` - Execution context with shared resources
///
/// # Returns
/// The result of the sub-agent execution
pub async fn execute_sub_agent(
    agent_def: &SubAgentDefinition,
    args: &serde_json::Value,
    parent_context: &SubAgentContext,
    model: &rig_anthropic_vertex::CompletionModel,
    ctx: SubAgentExecutorContext<'_>,
) -> Result<SubAgentResult> {
    let start_time = std::time::Instant::now();
    let agent_id = &agent_def.id;

    // Track files modified by this sub-agent
    let mut files_modified: Vec<String> = vec![];

    // Extract task and additional context from args
    let task = args
        .get("task")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Sub-agent call missing 'task' parameter"))?;
    let additional_context = args.get("context").and_then(|v| v.as_str()).unwrap_or("");

    // Build the sub-agent context with incremented depth
    let sub_context = SubAgentContext {
        original_request: parent_context.original_request.clone(),
        conversation_summary: parent_context.conversation_summary.clone(),
        variables: parent_context.variables.clone(),
        depth: parent_context.depth + 1,
    };

    // Build the prompt for the sub-agent
    let sub_prompt = if additional_context.is_empty() {
        task.to_string()
    } else {
        format!("{}\n\nAdditional context: {}", task, additional_context)
    };

    // Emit sub-agent start event
    let _ = ctx.event_tx.send(AiEvent::SubAgentStarted {
        agent_id: agent_id.to_string(),
        agent_name: agent_def.name.clone(),
        task: task.to_string(),
        depth: sub_context.depth,
    });

    // Build filtered tools based on agent's allowed tools
    let mut all_tools = get_all_tool_definitions();
    all_tools.extend(get_tavily_tool_definitions(ctx.tavily_state));
    let tools = filter_tools_by_allowed(all_tools, &agent_def.allowed_tools);

    // Build chat history for sub-agent
    let mut chat_history: Vec<Message> = vec![Message::User {
        content: OneOrMany::one(UserContent::Text(Text {
            text: sub_prompt.clone(),
        })),
    }];

    let mut accumulated_response = String::new();
    let mut iteration = 0;

    loop {
        iteration += 1;
        if iteration > agent_def.max_iterations {
            let _ = ctx.event_tx.send(AiEvent::SubAgentError {
                agent_id: agent_id.to_string(),
                error: "Maximum iterations reached".to_string(),
            });
            break;
        }

        // Build request with sub-agent's system prompt
        let request = rig::completion::CompletionRequest {
            preamble: Some(agent_def.system_prompt.clone()),
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
        let response = match model.completion(request).await {
            Ok(r) => r,
            Err(e) => {
                let _ = ctx.event_tx.send(AiEvent::SubAgentError {
                    agent_id: agent_id.to_string(),
                    error: e.to_string(),
                });
                return Ok(SubAgentResult {
                    agent_id: agent_id.to_string(),
                    response: format!("Error: {}", e),
                    context: sub_context,
                    success: false,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                    files_modified: files_modified.clone(),
                });
            }
        };

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

        if !text_content.is_empty() {
            accumulated_response.push_str(&text_content);
        }

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

        // Execute tool calls
        let mut tool_results: Vec<UserContent> = vec![];

        for tool_call in tool_calls_to_execute {
            let tool_name = &tool_call.function.name;
            let tool_args = if tool_name == "run_pty_cmd" {
                normalize_run_pty_cmd_args(tool_call.function.arguments.clone())
            } else {
                tool_call.function.arguments.clone()
            };
            let tool_id = tool_call.id.clone();

            // Execute the tool
            let (result_value, success) = if tool_name == "web_fetch" {
                execute_web_fetch_tool(tool_name, &tool_args).await
            } else if tool_name.starts_with("web_search") || tool_name == "web_extract" {
                execute_tavily_tool(ctx.tavily_state, tool_name, &tool_args).await
            } else {
                let mut registry = ctx.tool_registry.write().await;
                let result = registry.execute_tool(tool_name, tool_args.clone()).await;

                match &result {
                    Ok(v) => (v.clone(), true),
                    Err(e) => (serde_json::json!({ "error": e.to_string() }), false),
                }
            };

            // Track files modified by write tools
            if success && is_write_tool(tool_name) {
                if let Some(file_path) = extract_file_path(tool_name, &tool_args) {
                    if !files_modified.contains(&file_path) {
                        tracing::debug!(
                            "[sub-agent] Tracking modified file: {} (tool: {})",
                            file_path,
                            tool_name
                        );
                        files_modified.push(file_path);
                    }
                }
            }

            let result_text = serde_json::to_string(&result_value).unwrap_or_default();
            tool_results.push(UserContent::ToolResult(ToolResult {
                id: tool_id.clone(),
                call_id: Some(tool_id),
                content: OneOrMany::one(ToolResultContent::Text(Text { text: result_text })),
            }));
        }

        chat_history.push(Message::User {
            content: OneOrMany::many(tool_results).unwrap_or_else(|_| {
                OneOrMany::one(UserContent::Text(Text {
                    text: "Tool executed".to_string(),
                }))
            }),
        });
    }

    let duration_ms = start_time.elapsed().as_millis() as u64;

    let _ = ctx.event_tx.send(AiEvent::SubAgentCompleted {
        agent_id: agent_id.to_string(),
        response: accumulated_response.clone(),
        duration_ms,
    });

    if !files_modified.is_empty() {
        tracing::info!(
            "[sub-agent] {} modified {} files: {:?}",
            agent_id,
            files_modified.len(),
            files_modified
        );
    }

    Ok(SubAgentResult {
        agent_id: agent_id.to_string(),
        response: accumulated_response,
        context: sub_context,
        success: true,
        duration_ms,
        files_modified,
    })
}

/// Check if a tool modifies files
fn is_write_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "write_file"
            | "create_file"
            | "edit_file"
            | "delete_file"
            | "delete_path"
            | "rename_file"
            | "move_file"
            | "move_path"
            | "copy_path"
            | "create_directory"
            | "apply_patch"
    )
}

/// Extract file path from tool arguments
fn extract_file_path(tool_name: &str, args: &serde_json::Value) -> Option<String> {
    match tool_name {
        "write_file" | "create_file" | "edit_file" | "read_file" | "delete_file" => args
            .get("path")
            .or_else(|| args.get("file_path"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        "apply_patch" => {
            // Extract file paths from patch content
            args.get("patch")
                .and_then(|v| v.as_str())
                .and_then(|patch| {
                    // Look for "*** Update File:" or "*** Add File:" lines
                    for line in patch.lines() {
                        if let Some(path) = line.strip_prefix("*** Update File:") {
                            return Some(path.trim().to_string());
                        }
                        if let Some(path) = line.strip_prefix("*** Add File:") {
                            return Some(path.trim().to_string());
                        }
                    }
                    None
                })
        }
        "rename_file" | "move_file" | "move_path" | "copy_path" => args
            .get("destination")
            .or_else(|| args.get("to"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        "delete_path" => args
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        "create_directory" => args
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        _ => None,
    }
}

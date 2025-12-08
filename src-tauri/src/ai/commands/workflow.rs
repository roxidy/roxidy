//! Generic workflow execution commands for Tauri.
//!
//! These commands provide a workflow-agnostic interface for starting
//! and running any registered workflow.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use graph_flow::{InMemorySessionStorage, SessionStorage};
use rig::completion::{AssistantContent, CompletionModel as _, CompletionRequest, Message};
use rig::message::{Text, ToolResult, ToolResultContent, UserContent};
use rig::one_or_many::OneOrMany;
use serde_json::json;
use tauri::State;
use tokio::sync::RwLock;
use vtcode_core::tools::ToolRegistry;

use crate::ai::events::AiEvent;
use crate::ai::llm_client::LlmClient;
use crate::ai::tool_definitions::{
    get_all_tool_definitions_with_config, get_indexer_tool_definitions, ToolConfig, ToolPreset,
};
use crate::ai::tool_executors::{
    execute_indexer_tool, execute_tavily_tool, execute_web_fetch_tool,
};
use crate::ai::workflow::models::{
    StartWorkflowResponse, WorkflowAgentConfig, WorkflowAgentResult, WorkflowStateResponse,
    WorkflowStepResponse, WorkflowToolCall,
};
use crate::ai::workflow::{
    create_default_registry, WorkflowInfo, WorkflowLlmExecutor, WorkflowRegistry, WorkflowRunner,
    WorkflowStatus,
};
use crate::indexer::IndexerState;
use crate::state::AppState;
use crate::tavily::TavilyState;

use super::AI_NOT_INITIALIZED_ERROR;

/// State for workflow management.
pub struct WorkflowState {
    /// Registry of workflow definitions
    pub registry: RwLock<WorkflowRegistry>,
    /// Session storage for workflows
    pub storage: Arc<dyn SessionStorage + Send + Sync>,
    /// Active workflow runners keyed by session_id
    pub runners: RwLock<HashMap<String, WorkflowSession>>,
}

/// Active workflow session with runner and metadata.
pub struct WorkflowSession {
    pub runner: Arc<WorkflowRunner>,
    pub workflow_name: String,
    pub state_key: String,
}

impl Default for WorkflowState {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowState {
    pub fn new() -> Self {
        Self {
            registry: RwLock::new(create_default_registry()),
            storage: Arc::new(InMemorySessionStorage::new()),
            runners: RwLock::new(HashMap::new()),
        }
    }
}

/// Adapter that implements WorkflowLlmExecutor using the LLM client.
///
/// This executor can operate in two modes:
/// 1. **Simple mode**: Just client + event_tx - only supports `complete()`
/// 2. **Agent mode**: With tool infrastructure - supports full `run_agent()`
pub struct BridgeLlmExecutor {
    // Required for all operations
    client: Arc<RwLock<LlmClient>>,
    event_tx: tokio::sync::mpsc::UnboundedSender<AiEvent>,

    // Optional: enables agent mode with tool execution
    tool_registry: Option<Arc<RwLock<ToolRegistry>>>,
    indexer_state: Option<Arc<IndexerState>>,
    tavily_state: Option<Arc<TavilyState>>,

    // Workflow context (for source tracking in events)
    workflow_id: Option<String>,
    workflow_name: Option<String>,
}

impl BridgeLlmExecutor {
    /// Create a full executor with workflow context (for source tracking).
    #[allow(clippy::too_many_arguments)]
    pub fn with_workflow_context(
        client: Arc<RwLock<LlmClient>>,
        event_tx: tokio::sync::mpsc::UnboundedSender<AiEvent>,
        tool_registry: Arc<RwLock<ToolRegistry>>,
        _workspace: Arc<RwLock<PathBuf>>,
        indexer_state: Option<Arc<IndexerState>>,
        tavily_state: Option<Arc<TavilyState>>,
        workflow_id: String,
        workflow_name: String,
    ) -> Self {
        Self {
            client,
            event_tx,
            tool_registry: Some(tool_registry),
            indexer_state,
            tavily_state,
            workflow_id: Some(workflow_id),
            workflow_name: Some(workflow_name),
        }
    }

    /// Get the tool source with step context for events.
    fn get_tool_source_with_step(
        &self,
        step_name: Option<&str>,
        step_index: Option<usize>,
    ) -> crate::ai::events::ToolSource {
        match (&self.workflow_id, &self.workflow_name) {
            (Some(id), Some(name)) => crate::ai::events::ToolSource::Workflow {
                workflow_id: id.clone(),
                workflow_name: name.clone(),
                step_name: step_name.map(|s| s.to_string()),
                step_index,
            },
            _ => crate::ai::events::ToolSource::Main,
        }
    }
}

#[async_trait::async_trait]
impl WorkflowLlmExecutor for BridgeLlmExecutor {
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        _context: HashMap<String, serde_json::Value>,
    ) -> anyhow::Result<String> {
        tracing::debug!(
            "WorkflowLlmExecutor: system_prompt={:.100}..., user_prompt={:.100}...",
            system_prompt,
            user_prompt
        );

        // Build the completion request
        let chat_history = vec![Message::User {
            content: OneOrMany::one(UserContent::Text(Text {
                text: user_prompt.to_string(),
            })),
        }];

        let request = CompletionRequest {
            preamble: Some(system_prompt.to_string()),
            chat_history: OneOrMany::many(chat_history.clone())
                .unwrap_or_else(|_| OneOrMany::one(chat_history[0].clone())),
            documents: vec![],
            tools: vec![], // No tools for workflow tasks
            temperature: Some(0.7),
            max_tokens: Some(8192),
            tool_choice: None,
            additional_params: None,
        };

        // Make the completion call
        let client = self.client.read().await;
        let response = match &*client {
            LlmClient::VertexAnthropic(model) => model.completion(request).await?,
            LlmClient::Vtcode(_) => {
                return Err(anyhow::anyhow!(
                    "Vtcode client not yet supported for workflow completions"
                ));
            }
        };

        // Extract text from response
        let mut result = String::new();
        for content in response.choice.iter() {
            if let rig::completion::AssistantContent::Text(text) = content {
                result.push_str(&text.text);
            }
        }

        if result.is_empty() {
            return Err(anyhow::anyhow!("LLM returned empty response"));
        }

        Ok(result)
    }

    async fn run_agent(&self, config: WorkflowAgentConfig) -> anyhow::Result<WorkflowAgentResult> {
        // Check if we have agent capabilities
        let tool_registry = match &self.tool_registry {
            Some(r) => r.clone(),
            None => {
                // Fall back to simple completion without tools
                tracing::warn!("run_agent called without tool infrastructure, falling back to simple completion");
                let text = self
                    .complete(&config.system_prompt, &config.task, HashMap::new())
                    .await?;
                return Ok(WorkflowAgentResult {
                    response: text,
                    tool_history: vec![],
                    iterations: 1,
                    tokens_used: None,
                    completed: true,
                    error: None,
                });
            }
        };

        let max_iterations = config.max_iterations.unwrap_or(25);
        let emit_events = config.emit_events.unwrap_or(true);
        let step_name = config.step_name.as_deref();
        let step_index = config.step_index;
        let start_time = std::time::Instant::now();

        // Emit WorkflowStepStarted event
        if emit_events {
            if let (Some(wf_id), Some(step), Some(idx)) = (&self.workflow_id, step_name, step_index)
            {
                let _ = self.event_tx.send(AiEvent::WorkflowStepStarted {
                    workflow_id: wf_id.clone(),
                    step_name: step.to_string(),
                    step_index: idx,
                    total_steps: 4, // TODO: get actual total from workflow definition
                });
            }
        }

        // Build tool definitions based on config
        let tool_defs = self.build_tool_definitions(&config).await;

        tracing::info!(
            "Workflow agent starting with {} tools, max {} iterations",
            tool_defs.len(),
            max_iterations
        );

        // Initialize chat history with user task
        let mut chat_history = vec![Message::User {
            content: OneOrMany::one(UserContent::Text(Text {
                text: config.task.clone(),
            })),
        }];

        let mut tool_history: Vec<WorkflowToolCall> = vec![];
        let mut iterations = 0;
        let mut final_response = String::new();

        // Run the agent loop
        loop {
            iterations += 1;

            if iterations > max_iterations {
                tracing::warn!("Workflow agent hit max iterations ({})", max_iterations);
                // Emit step completed event (with error)
                if emit_events {
                    if let (Some(wf_id), Some(step)) = (&self.workflow_id, step_name) {
                        let _ = self.event_tx.send(AiEvent::WorkflowStepCompleted {
                            workflow_id: wf_id.clone(),
                            step_name: step.to_string(),
                            output: Some(format!("Hit max iterations ({})", max_iterations)),
                            duration_ms: start_time.elapsed().as_millis() as u64,
                        });
                    }
                }
                return Ok(WorkflowAgentResult {
                    response: final_response,
                    tool_history,
                    iterations,
                    tokens_used: None,
                    completed: false,
                    error: Some(format!("Hit max iterations ({})", max_iterations)),
                });
            }

            // Build completion request
            let request = CompletionRequest {
                preamble: Some(config.system_prompt.clone()),
                chat_history: OneOrMany::many(chat_history.clone())
                    .unwrap_or_else(|_| OneOrMany::one(chat_history[0].clone())),
                documents: vec![],
                tools: tool_defs.clone(),
                temperature: config.temperature.map(|t| t as f64).or(Some(0.7)),
                max_tokens: Some(8192),
                tool_choice: None,
                additional_params: None,
            };

            // Make LLM call
            let client = self.client.read().await;
            let response = match &*client {
                LlmClient::VertexAnthropic(model) => model.completion(request).await?,
                LlmClient::Vtcode(_) => {
                    return Err(anyhow::anyhow!(
                        "Vtcode client not supported for workflow agents"
                    ));
                }
            };
            drop(client);

            // Process response
            let mut has_tool_calls = false;
            let mut tool_calls_to_execute: Vec<(String, String, serde_json::Value)> = vec![];
            let mut assistant_content: Vec<AssistantContent> = vec![];

            for content in response.choice.iter() {
                match content {
                    AssistantContent::Text(text) => {
                        final_response.push_str(&text.text);
                        assistant_content.push(content.clone());
                    }
                    AssistantContent::ToolCall(tool_call) => {
                        has_tool_calls = true;
                        let args = tool_call.function.arguments.clone();
                        tool_calls_to_execute.push((
                            tool_call.id.clone(),
                            tool_call.function.name.clone(),
                            args,
                        ));
                        assistant_content.push(content.clone());
                    }
                    _ => {}
                }
            }

            // If no tool calls, we're done
            if !has_tool_calls {
                tracing::info!(
                    "Workflow agent completed after {} iterations, {} tool calls",
                    iterations,
                    tool_history.len()
                );
                // Emit step completed event
                if emit_events {
                    if let (Some(wf_id), Some(step)) = (&self.workflow_id, step_name) {
                        let _ = self.event_tx.send(AiEvent::WorkflowStepCompleted {
                            workflow_id: wf_id.clone(),
                            step_name: step.to_string(),
                            output: Some(final_response.clone()),
                            duration_ms: start_time.elapsed().as_millis() as u64,
                        });
                    }
                }
                return Ok(WorkflowAgentResult {
                    response: final_response,
                    tool_history,
                    iterations,
                    tokens_used: None,
                    completed: true,
                    error: None,
                });
            }

            // Add assistant message to history (only if we have content)
            if !assistant_content.is_empty() {
                chat_history.push(Message::Assistant {
                    id: None,
                    content: OneOrMany::many(assistant_content).unwrap_or_else(|_| {
                        OneOrMany::one(AssistantContent::Text(Text {
                            text: String::new(),
                        }))
                    }),
                });
            }

            // Execute tools and collect results
            let mut tool_results: Vec<ToolResult> = vec![];

            for (tool_id, tool_name, tool_args) in tool_calls_to_execute {
                if emit_events {
                    let _ = self.event_tx.send(AiEvent::ToolRequest {
                        request_id: tool_id.clone(),
                        tool_name: tool_name.clone(),
                        args: tool_args.clone(),
                        source: self.get_tool_source_with_step(step_name, step_index),
                    });
                }

                // Execute the tool
                let (result, success) = self
                    .execute_tool(&tool_name, &tool_args, &tool_registry)
                    .await;

                // Record tool call
                tool_history.push(WorkflowToolCall {
                    id: tool_id.clone(),
                    name: tool_name.clone(),
                    arguments: tool_args.clone(),
                });

                if emit_events {
                    let _ = self.event_tx.send(AiEvent::ToolResult {
                        request_id: tool_id.clone(),
                        tool_name: tool_name.clone(),
                        result: result.clone(),
                        success,
                        source: self.get_tool_source_with_step(step_name, step_index),
                    });
                }

                // Add to results for LLM
                let result_str = serde_json::to_string(&result).unwrap_or_default();
                tool_results.push(ToolResult {
                    id: tool_id.clone(),
                    call_id: Some(tool_id),
                    content: OneOrMany::one(ToolResultContent::Text(Text { text: result_str })),
                });
            }

            // Add tool results to history (only if we have results)
            if !tool_results.is_empty() {
                chat_history.push(Message::User {
                    content: OneOrMany::many(
                        tool_results
                            .into_iter()
                            .map(UserContent::ToolResult)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap_or_else(|_| {
                        OneOrMany::one(UserContent::Text(Text {
                            text: String::new(),
                        }))
                    }),
                });
            }
        }
    }

    fn emit_step_started(&self, step_name: &str, step_index: usize, total_steps: usize) {
        if let Some(workflow_id) = &self.workflow_id {
            let _ = self.event_tx.send(AiEvent::WorkflowStepStarted {
                workflow_id: workflow_id.clone(),
                step_name: step_name.to_string(),
                step_index,
                total_steps,
            });
        }
    }

    fn emit_step_completed(&self, step_name: &str, output: Option<&str>, duration_ms: u64) {
        if let Some(workflow_id) = &self.workflow_id {
            let _ = self.event_tx.send(AiEvent::WorkflowStepCompleted {
                workflow_id: workflow_id.clone(),
                step_name: step_name.to_string(),
                output: output.map(String::from),
                duration_ms,
            });
        }
    }
}

impl BridgeLlmExecutor {
    /// Build tool definitions based on the agent config.
    async fn build_tool_definitions(
        &self,
        config: &WorkflowAgentConfig,
    ) -> Vec<rig::completion::ToolDefinition> {
        match &config.tools {
            None => {
                // No tools
                vec![]
            }
            Some(allowed) if allowed.is_empty() => {
                // All tools - use Standard preset
                let mut tools = get_all_tool_definitions_with_config(&ToolConfig::with_preset(
                    ToolPreset::Standard,
                ));
                // Add indexer tools
                tools.extend(get_indexer_tool_definitions());
                tools
            }
            Some(allowed) => {
                // Specific tools only
                let mut tools = get_all_tool_definitions_with_config(&ToolConfig::with_preset(
                    ToolPreset::Full,
                ));
                tools.extend(get_indexer_tool_definitions());

                // Filter to allowed tools
                tools
                    .into_iter()
                    .filter(|t| allowed.contains(&t.name))
                    .collect()
            }
        }
    }

    /// Execute a single tool call.
    async fn execute_tool(
        &self,
        tool_name: &str,
        tool_args: &serde_json::Value,
        tool_registry: &Arc<RwLock<ToolRegistry>>,
    ) -> (serde_json::Value, bool) {
        // Indexer tools
        if tool_name.starts_with("indexer_") {
            return execute_indexer_tool(self.indexer_state.as_ref(), tool_name, tool_args).await;
        }

        // Web fetch
        if tool_name == "web_fetch" {
            return execute_web_fetch_tool(tool_name, tool_args).await;
        }

        // Tavily tools
        if tool_name.starts_with("web_search") || tool_name == "web_extract" {
            return execute_tavily_tool(self.tavily_state.as_ref(), tool_name, tool_args).await;
        }

        // vtcode-core tools
        let mut registry = tool_registry.write().await;

        match registry.execute_tool(tool_name, tool_args.clone()).await {
            Ok(result) => (result, true),
            Err(e) => (json!({"error": e.to_string()}), false),
        }
    }
}

/// List all available workflows.
#[tauri::command]
pub async fn list_workflows(state: State<'_, AppState>) -> Result<Vec<WorkflowInfo>, String> {
    let registry = state.workflow_state.registry.read().await;
    Ok(registry.list_info())
}

/// Start a workflow by name.
///
/// # Arguments
/// * `workflow_name` - Name of the workflow to start (e.g., "git_commit")
/// * `input` - Workflow-specific input as JSON
#[tauri::command]
pub async fn start_workflow(
    state: State<'_, AppState>,
    workflow_name: String,
    input: serde_json::Value,
) -> Result<StartWorkflowResponse, String> {
    // Check that AI is initialized (we need the event channel)
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard.as_ref().ok_or(AI_NOT_INITIALIZED_ERROR)?;

    // Get the workflow definition
    let registry = state.workflow_state.registry.read().await;
    let workflow = registry
        .get(&workflow_name)
        .ok_or_else(|| format!("Unknown workflow: {}", workflow_name))?;

    // Generate session_id first so we can pass it to the executor for source tracking
    let session_id = uuid::Uuid::new_v4().to_string();

    // Create the LLM executor with full agent capabilities AND workflow context
    // Use get_or_create_event_tx() to support both legacy and runtime paths
    let event_tx = bridge.get_or_create_event_tx();
    let executor: Arc<dyn WorkflowLlmExecutor> =
        Arc::new(BridgeLlmExecutor::with_workflow_context(
            bridge.client.clone(),
            event_tx,
            bridge.tool_registry.clone(),
            bridge.workspace.clone(),
            bridge.indexer_state.clone(),
            bridge.tavily_state.clone(),
            session_id.clone(),
            workflow_name.clone(),
        ));

    // Build the workflow graph
    let graph = workflow.build_graph(executor);

    // Create a runner
    let runner = WorkflowRunner::new(graph, state.workflow_state.storage.clone());

    // Initialize state
    let initial_state = workflow.init_state(input).map_err(|e| e.to_string())?;

    // Start the session with our pre-generated session_id
    runner
        .start_session_with_id(&session_id, "", workflow.start_task())
        .await
        .map_err(|e| e.to_string())?;

    // Set initial state in session context
    if let Ok(Some(session)) = state.workflow_state.storage.get(&session_id).await {
        session
            .context
            .set(workflow.state_key(), initial_state)
            .await;
        state
            .workflow_state
            .storage
            .save(session)
            .await
            .map_err(|e| format!("Failed to save session: {}", e))?;
    }

    // Store the runner with metadata
    let workflow_session = WorkflowSession {
        runner: Arc::new(runner),
        workflow_name: workflow_name.clone(),
        state_key: workflow.state_key().to_string(),
    };
    state
        .workflow_state
        .runners
        .write()
        .await
        .insert(session_id.clone(), workflow_session);

    // Emit workflow started event
    bridge.emit_event(AiEvent::WorkflowStarted {
        workflow_id: session_id.clone(),
        workflow_name: workflow_name.clone(),
        session_id: session_id.clone(),
    });

    Ok(StartWorkflowResponse {
        session_id,
        workflow_name,
    })
}

/// Execute the next step in a workflow.
#[tauri::command]
pub async fn step_workflow(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<WorkflowStepResponse, String> {
    let runners = state.workflow_state.runners.read().await;

    let session = runners
        .get(&session_id)
        .ok_or_else(|| format!("No workflow found with session_id: {}", session_id))?;

    let result = session
        .runner
        .step(&session_id)
        .await
        .map_err(|e| e.to_string())?;

    let (status, next_task_id, error) = match &result.status {
        WorkflowStatus::Paused { next_task_id } => {
            ("paused".to_string(), Some(next_task_id.clone()), None)
        }
        WorkflowStatus::WaitingForInput => ("waiting_for_input".to_string(), None, None),
        WorkflowStatus::Completed => ("completed".to_string(), None, None),
        WorkflowStatus::Error(e) => ("error".to_string(), None, Some(e.clone())),
    };

    Ok(WorkflowStepResponse {
        output: result.output,
        status,
        next_task_id,
        error,
    })
}

/// Run a workflow to completion.
#[tauri::command]
pub async fn run_workflow_to_completion(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<String, String> {
    let runners = state.workflow_state.runners.read().await;

    let session = runners
        .get(&session_id)
        .ok_or_else(|| format!("No workflow found with session_id: {}", session_id))?;

    let workflow_name = session.workflow_name.clone();
    let runner = session.runner.clone();
    drop(runners);

    let result = runner
        .run_to_completion(&session_id)
        .await
        .map_err(|e| e.to_string())?;

    // Emit workflow completed event
    if let Ok(bridge_guard) = state.ai_state.bridge.try_read() {
        if let Some(bridge) = bridge_guard.as_ref() {
            bridge.emit_event(AiEvent::WorkflowCompleted {
                workflow_id: session_id.clone(),
                final_output: result.clone(),
                total_duration_ms: 0, // TODO: track duration
            });
        }
    }

    // Cleanup the runner
    state
        .workflow_state
        .runners
        .write()
        .await
        .remove(&session_id);

    tracing::info!("Workflow '{}' completed: {}", workflow_name, session_id);

    Ok(result)
}

/// Get the current state of a workflow.
#[tauri::command]
pub async fn get_workflow_state(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<WorkflowStateResponse, String> {
    let runners = state.workflow_state.runners.read().await;

    let session_info = runners
        .get(&session_id)
        .ok_or_else(|| format!("No workflow found with session_id: {}", session_id))?;

    let state_key = session_info.state_key.clone();
    drop(runners);

    let session = state
        .workflow_state
        .storage
        .get(&session_id)
        .await
        .map_err(|e| format!("Failed to get session: {}", e))?
        .ok_or_else(|| format!("No session found with id: {}", session_id))?;

    let workflow_state: serde_json::Value = session
        .context
        .get(&state_key)
        .await
        .unwrap_or(serde_json::Value::Null);

    Ok(WorkflowStateResponse {
        state: workflow_state,
        status: "active".to_string(),
        current_task: session.current_task_id.clone(),
    })
}

/// List active workflow sessions.
#[tauri::command]
pub async fn list_workflow_sessions(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let runners = state.workflow_state.runners.read().await;
    Ok(runners.keys().cloned().collect())
}

/// Cancel a workflow session.
#[tauri::command]
pub async fn cancel_workflow(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    // Get workflow name for logging
    let workflow_name = {
        let runners = state.workflow_state.runners.read().await;
        runners.get(&session_id).map(|s| s.workflow_name.clone())
    };

    // Emit workflow error/cancelled event
    if let Ok(bridge_guard) = state.ai_state.bridge.try_read() {
        if let Some(bridge) = bridge_guard.as_ref() {
            bridge.emit_event(AiEvent::WorkflowError {
                workflow_id: session_id.clone(),
                step_name: None,
                error: "Workflow cancelled by user".to_string(),
            });
        }
    }

    // Remove the runner
    state
        .workflow_state
        .runners
        .write()
        .await
        .remove(&session_id);

    if let Some(name) = workflow_name {
        tracing::info!("Workflow '{}' cancelled: {}", name, session_id);
    }

    Ok(())
}

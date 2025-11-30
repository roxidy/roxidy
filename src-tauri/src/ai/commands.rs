use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::{mpsc, RwLock};

use super::agent_bridge::AgentBridge;
use super::events::AiEvent;
use crate::state::AppState;

/// Shared AI state.
/// Uses tokio RwLock for async compatibility with AgentBridge methods.
pub struct AiState {
    pub bridge: Arc<RwLock<Option<AgentBridge>>>,
}

impl AiState {
    pub fn new() -> Self {
        Self {
            bridge: Arc::new(RwLock::new(None)),
        }
    }
}

impl Default for AiState {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize the AI agent with the specified configuration.
///
/// # Arguments
/// * `workspace` - Path to the workspace directory
/// * `provider` - LLM provider name (e.g., "openrouter", "anthropic")
/// * `model` - Model identifier (e.g., "anthropic/claude-3.5-sonnet")
/// * `api_key` - API key for the provider
#[tauri::command]
pub async fn init_ai_agent(
    state: State<'_, AppState>,
    app: AppHandle,
    workspace: String,
    provider: String,
    model: String,
    api_key: String,
) -> Result<(), String> {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AiEvent>();

    // Spawn event forwarder to frontend
    let app_clone = app.clone();
    tokio::spawn(async move {
        while let Some(ai_event) = event_rx.recv().await {
            if let Err(e) = app_clone.emit("ai-event", &ai_event) {
                tracing::error!("Failed to emit AI event: {}", e);
            }
        }
    });

    // Create the agent bridge (async constructor)
    let mut bridge = AgentBridge::new(workspace.into(), &provider, &model, &api_key, event_tx)
        .await
        .map_err(|e| e.to_string())?;

    // Set PtyManager so commands can be executed in user's terminal
    bridge.set_pty_manager(state.pty_manager.clone());

    // Set IndexerState so code analysis tools are available
    bridge.set_indexer_state(state.indexer_state.clone());

    // Set TavilyState so web search tools are available
    bridge.set_tavily_state(state.tavily_state.clone());

    *state.ai_state.bridge.write().await = Some(bridge);

    tracing::info!(
        "AI agent initialized with provider: {}, model: {}",
        provider,
        model
    );
    Ok(())
}

/// Context information to inject into user messages.
/// This context is prepended as XML tags and not shown to the user.
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct PromptContext {
    /// The current working directory in the terminal
    pub working_directory: Option<String>,
    /// The session ID of the user's terminal (for running commands in the same terminal)
    pub session_id: Option<String>,
}

impl PromptContext {
    /// Format the context as XML tags to prepend to the user message.
    pub fn to_xml(&self) -> String {
        let mut xml = String::new();

        if let Some(cwd) = &self.working_directory {
            xml.push_str(&format!("<cwd>{}</cwd>\n", cwd));
        }

        if let Some(sid) = &self.session_id {
            xml.push_str(&format!("<session_id>{}</session_id>\n", sid));
        }

        if !xml.is_empty() {
            format!("<context>\n{}</context>\n\n", xml)
        } else {
            String::new()
        }
    }
}

/// Send a prompt to the AI agent and receive streaming response via events.
///
/// # Arguments
/// * `prompt` - The user's message
/// * `context` - Optional context to inject (working directory, etc.)
#[tauri::command]
pub async fn send_ai_prompt(
    state: State<'_, AppState>,
    prompt: String,
    context: Option<PromptContext>,
) -> Result<String, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    // Extract session_id and inject context as XML prefix if provided
    let (full_prompt, session_id) = match context {
        Some(ctx) => {
            let session_id = ctx.session_id.clone();
            let xml_context = ctx.to_xml();
            let prompt = if xml_context.is_empty() {
                prompt
            } else {
                format!("{}{}", xml_context, prompt)
            };
            (prompt, session_id)
        }
        None => (prompt, None),
    };

    // Set the session_id on the bridge for terminal command execution
    bridge.set_session_id(session_id).await;

    bridge
        .execute(&full_prompt)
        .await
        .map_err(|e| e.to_string())
}

/// Execute a specific tool with the given arguments.
#[tauri::command]
pub async fn execute_ai_tool(
    state: State<'_, AppState>,
    tool_name: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge
        .execute_tool(&tool_name, args)
        .await
        .map_err(|e| e.to_string())
}

/// Get the list of available tools.
#[tauri::command]
pub async fn get_available_tools(
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    // available_tools now returns Vec<serde_json::Value> directly
    let tools = bridge.available_tools().await;

    Ok(tools)
}

/// Shutdown the AI agent and cleanup resources.
#[tauri::command]
pub async fn shutdown_ai_agent(state: State<'_, AppState>) -> Result<(), String> {
    let mut bridge_guard = state.ai_state.bridge.write().await;
    *bridge_guard = None;
    tracing::info!("AI agent shut down");
    Ok(())
}

/// Check if the AI agent is initialized.
#[tauri::command]
pub async fn is_ai_initialized(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.ai_state.bridge.read().await.is_some())
}

/// Get the OpenRouter API key from environment.
/// Looks for OPENROUTER_API_KEY in environment variables.
#[tauri::command]
pub fn get_openrouter_api_key() -> Result<Option<String>, String> {
    Ok(std::env::var("OPENROUTER_API_KEY").ok())
}

/// Initialize the AI agent with Anthropic on Google Cloud Vertex AI.
///
/// # Arguments
/// * `workspace` - Path to the workspace directory
/// * `credentials_path` - Path to the service account JSON file
/// * `project_id` - Google Cloud project ID
/// * `location` - Vertex AI location (e.g., "us-east5")
/// * `model` - Model identifier (e.g., "claude-opus-4-5@20251101")
#[tauri::command]
pub async fn init_ai_agent_vertex(
    state: State<'_, AppState>,
    app: AppHandle,
    workspace: String,
    credentials_path: String,
    project_id: String,
    location: String,
    model: String,
) -> Result<(), String> {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AiEvent>();

    // Spawn event forwarder to frontend
    let app_clone = app.clone();
    tokio::spawn(async move {
        while let Some(ai_event) = event_rx.recv().await {
            if let Err(e) = app_clone.emit("ai-event", &ai_event) {
                tracing::error!("Failed to emit AI event: {}", e);
            }
        }
    });

    // Create the agent bridge for Vertex AI (async constructor)
    let mut bridge = AgentBridge::new_vertex_anthropic(
        workspace.into(),
        &credentials_path,
        &project_id,
        &location,
        &model,
        event_tx,
    )
    .await
    .map_err(|e| e.to_string())?;

    // Set PtyManager so commands can be executed in user's terminal
    bridge.set_pty_manager(state.pty_manager.clone());

    // Set IndexerState so code analysis tools are available
    bridge.set_indexer_state(state.indexer_state.clone());

    // Set TavilyState so web search tools are available
    bridge.set_tavily_state(state.tavily_state.clone());

    *state.ai_state.bridge.write().await = Some(bridge);

    tracing::info!(
        "AI agent initialized with Vertex AI Anthropic, project: {}, model: {}",
        project_id,
        model
    );
    Ok(())
}

/// Update the AI agent's workspace/working directory.
/// This allows the agent to stay in sync with the user's terminal directory.
///
/// # Arguments
/// * `workspace` - New workspace/working directory path
#[tauri::command]
pub async fn update_ai_workspace(
    state: State<'_, AppState>,
    workspace: String,
) -> Result<(), String> {
    tracing::info!("[cwd-sync] update_ai_workspace called with: {}", workspace);
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard.as_ref().ok_or_else(|| {
        tracing::warn!("[cwd-sync] AI agent not initialized, cannot update workspace");
        "AI agent not initialized. Call init_ai_agent first.".to_string()
    })?;

    bridge.set_workspace(workspace.into()).await;
    tracing::info!("[cwd-sync] AI workspace successfully updated");
    Ok(())
}

/// Load environment variables from a .env file.
/// Returns the number of variables loaded.
#[tauri::command]
pub fn load_env_file(path: String) -> Result<usize, String> {
    match dotenvy::from_path(&path) {
        Ok(_) => {
            // Count how many vars we can read
            let count = dotenvy::from_path_iter(&path)
                .map(|iter| iter.count())
                .unwrap_or(0);
            tracing::info!("Loaded {} environment variables from {}", count, path);
            Ok(count)
        }
        Err(e) => Err(format!("Failed to load .env file: {}", e)),
    }
}

/// Vertex AI configuration from environment variables.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VertexAiEnvConfig {
    pub credentials_path: Option<String>,
    pub project_id: Option<String>,
    pub location: Option<String>,
}

/// Get Vertex AI configuration from environment variables.
/// Looks for:
/// - GOOGLE_APPLICATION_CREDENTIALS or VERTEX_AI_CREDENTIALS_PATH
/// - VERTEX_AI_PROJECT_ID or GOOGLE_CLOUD_PROJECT
/// - VERTEX_AI_LOCATION (defaults to "us-east5" if not set)
#[tauri::command]
pub fn get_vertex_ai_config() -> VertexAiEnvConfig {
    let credentials_path = std::env::var("VERTEX_AI_CREDENTIALS_PATH")
        .or_else(|_| std::env::var("GOOGLE_APPLICATION_CREDENTIALS"))
        .ok();

    let project_id = std::env::var("VERTEX_AI_PROJECT_ID")
        .or_else(|_| std::env::var("GOOGLE_CLOUD_PROJECT"))
        .ok();

    let location = std::env::var("VERTEX_AI_LOCATION").ok();

    VertexAiEnvConfig {
        credentials_path,
        project_id,
        location,
    }
}

/// Clear the AI agent's conversation history.
/// Call this when starting a new conversation or when the user wants to reset context.
#[tauri::command]
pub async fn clear_ai_conversation(state: State<'_, AppState>) -> Result<(), String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge.clear_conversation_history().await;
    tracing::info!("AI conversation history cleared");
    Ok(())
}

/// Get the current conversation history length.
/// Useful for debugging or showing context status in the UI.
#[tauri::command]
pub async fn get_ai_conversation_length(state: State<'_, AppState>) -> Result<usize, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    Ok(bridge.conversation_history_len().await)
}

// ============================================================================
// Session Persistence Commands
// ============================================================================

use super::session::{self, QbitSessionSnapshot, SessionListingInfo};

/// List recent AI conversation sessions.
///
/// # Arguments
/// * `limit` - Maximum number of sessions to return (0 for all)
#[tauri::command]
pub async fn list_ai_sessions(limit: Option<usize>) -> Result<Vec<SessionListingInfo>, String> {
    session::list_recent_sessions(limit.unwrap_or(20))
        .await
        .map_err(|e| e.to_string())
}

/// Find a specific session by its identifier.
///
/// # Arguments
/// * `identifier` - The session identifier (file stem)
#[tauri::command]
pub async fn find_ai_session(identifier: String) -> Result<Option<SessionListingInfo>, String> {
    session::find_session(&identifier)
        .await
        .map_err(|e| e.to_string())
}

/// Load a full session with all messages by its identifier.
///
/// # Arguments
/// * `identifier` - The session identifier (file stem)
#[tauri::command]
pub async fn load_ai_session(identifier: String) -> Result<Option<QbitSessionSnapshot>, String> {
    session::load_session(&identifier)
        .await
        .map_err(|e| e.to_string())
}

/// Enable or disable session persistence.
///
/// When enabled, AI conversations are automatically saved to disk.
#[tauri::command]
pub async fn set_ai_session_persistence(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge.set_session_persistence_enabled(enabled).await;
    Ok(())
}

/// Check if session persistence is enabled.
#[tauri::command]
pub async fn is_ai_session_persistence_enabled(state: State<'_, AppState>) -> Result<bool, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    Ok(bridge.is_session_persistence_enabled().await)
}

/// Manually finalize and save the current session.
///
/// Returns the path to the saved session file, if any.
#[tauri::command]
pub async fn finalize_ai_session(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    let path = bridge.finalize_session().await;
    Ok(path.map(|p| p.display().to_string()))
}

/// Export a session transcript to a file.
///
/// # Arguments
/// * `identifier` - The session identifier (file stem)
/// * `output_path` - Path where the transcript should be saved
#[tauri::command]
pub async fn export_ai_session_transcript(
    identifier: String,
    output_path: String,
) -> Result<(), String> {
    let session = session::load_session(&identifier)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Session '{}' not found", identifier))?;

    // Format as markdown transcript
    let mut transcript = format!(
        "# Session Transcript\n\n\
         - **Workspace**: {}\n\
         - **Model**: {}\n\
         - **Provider**: {}\n\
         - **Started**: {}\n\
         - **Ended**: {}\n\
         - **Messages**: {}\n\
         - **Tools Used**: {}\n\n\
         ---\n\n",
        session.workspace_label,
        session.model,
        session.provider,
        session.started_at.format("%Y-%m-%d %H:%M:%S UTC"),
        session.ended_at.format("%Y-%m-%d %H:%M:%S UTC"),
        session.total_messages,
        session.distinct_tools.join(", ")
    );

    for msg in &session.messages {
        let role_label = match msg.role {
            session::QbitMessageRole::User => "**User**",
            session::QbitMessageRole::Assistant => "**Assistant**",
            session::QbitMessageRole::System => "**System**",
            session::QbitMessageRole::Tool => "**Tool**",
        };
        transcript.push_str(&format!("{}\n\n{}\n\n---\n\n", role_label, msg.content));
    }

    std::fs::write(&output_path, transcript)
        .map_err(|e| format!("Failed to write transcript: {}", e))?;

    tracing::info!("Session transcript exported to {}", output_path);
    Ok(())
}

/// Restore a previous session by loading its conversation history.
///
/// This loads the session's messages into the AI agent's conversation history,
/// allowing the user to continue from where they left off.
///
/// # Arguments
/// * `identifier` - The session identifier (file stem)
#[tauri::command]
pub async fn restore_ai_session(
    state: State<'_, AppState>,
    identifier: String,
) -> Result<session::QbitSessionSnapshot, String> {
    // First load the session
    let session = session::load_session(&identifier)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Session '{}' not found", identifier))?;

    // Get the bridge and restore the conversation history
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    // Restore the messages to the agent's conversation history
    bridge.restore_session(session.messages.clone()).await;

    tracing::info!(
        "Restored session '{}' with {} messages",
        identifier,
        session.messages.len()
    );

    // Return the session so the frontend can display the restored messages
    Ok(session)
}

// ============================================================================
// HITL (Human-in-the-Loop) Commands
// ============================================================================

use super::hitl::{ApprovalDecision, ApprovalPattern, ToolApprovalConfig};

/// Get approval patterns for all tools.
#[tauri::command]
pub async fn get_approval_patterns(
    state: State<'_, AppState>,
) -> Result<Vec<ApprovalPattern>, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    let patterns = bridge.get_approval_patterns().await;
    Ok(patterns)
}

/// Get the approval pattern for a specific tool.
#[tauri::command]
pub async fn get_tool_approval_pattern(
    state: State<'_, AppState>,
    tool_name: String,
) -> Result<Option<ApprovalPattern>, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    let pattern = bridge.get_tool_approval_pattern(&tool_name).await;
    Ok(pattern)
}

/// Get the HITL configuration.
#[tauri::command]
pub async fn get_hitl_config(state: State<'_, AppState>) -> Result<ToolApprovalConfig, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    let config = bridge.get_hitl_config().await;
    Ok(config)
}

/// Update the HITL configuration.
#[tauri::command]
pub async fn set_hitl_config(
    state: State<'_, AppState>,
    config: ToolApprovalConfig,
) -> Result<(), String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge
        .set_hitl_config(config)
        .await
        .map_err(|e| e.to_string())
}

/// Add a tool to the always-allow list.
#[tauri::command]
pub async fn add_tool_always_allow(
    state: State<'_, AppState>,
    tool_name: String,
) -> Result<(), String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge
        .add_tool_always_allow(&tool_name)
        .await
        .map_err(|e| e.to_string())
}

/// Remove a tool from the always-allow list.
#[tauri::command]
pub async fn remove_tool_always_allow(
    state: State<'_, AppState>,
    tool_name: String,
) -> Result<(), String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge
        .remove_tool_always_allow(&tool_name)
        .await
        .map_err(|e| e.to_string())
}

/// Reset all approval patterns (does not reset configuration).
#[tauri::command]
pub async fn reset_approval_patterns(state: State<'_, AppState>) -> Result<(), String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge
        .reset_approval_patterns()
        .await
        .map_err(|e| e.to_string())
}

/// Respond to a tool approval request.
///
/// This is called by the frontend after the user makes a decision in the approval dialog.
#[tauri::command]
pub async fn respond_to_tool_approval(
    state: State<'_, AppState>,
    decision: ApprovalDecision,
) -> Result<(), String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge
        .respond_to_approval(decision)
        .await
        .map_err(|e| e.to_string())
}

// ============================================================================
// Tool Policy Commands
// ============================================================================

use super::tool_policy::{ToolPolicy, ToolPolicyConfig};

/// Get the current tool policy configuration.
#[tauri::command]
pub async fn get_tool_policy_config(
    state: State<'_, AppState>,
) -> Result<ToolPolicyConfig, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    let config = bridge.get_tool_policy_config().await;
    Ok(config)
}

/// Update the tool policy configuration.
#[tauri::command]
pub async fn set_tool_policy_config(
    state: State<'_, AppState>,
    config: ToolPolicyConfig,
) -> Result<(), String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge
        .set_tool_policy_config(config)
        .await
        .map_err(|e| e.to_string())
}

/// Get the policy for a specific tool.
#[tauri::command]
pub async fn get_tool_policy(
    state: State<'_, AppState>,
    tool_name: String,
) -> Result<ToolPolicy, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    let policy = bridge.get_tool_policy(&tool_name).await;
    Ok(policy)
}

/// Set the policy for a specific tool.
#[tauri::command]
pub async fn set_tool_policy(
    state: State<'_, AppState>,
    tool_name: String,
    policy: ToolPolicy,
) -> Result<(), String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge
        .set_tool_policy(&tool_name, policy)
        .await
        .map_err(|e| e.to_string())
}

/// Reset tool policies to defaults.
#[tauri::command]
pub async fn reset_tool_policies(state: State<'_, AppState>) -> Result<(), String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge
        .reset_tool_policies()
        .await
        .map_err(|e| e.to_string())
}

/// Enable full-auto mode for tool execution.
///
/// In full-auto mode, tools in the allowed list execute without any approval.
#[tauri::command]
pub async fn enable_full_auto_mode(
    state: State<'_, AppState>,
    allowed_tools: Vec<String>,
) -> Result<(), String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge.enable_full_auto_mode(allowed_tools).await;
    Ok(())
}

/// Disable full-auto mode for tool execution.
#[tauri::command]
pub async fn disable_full_auto_mode(state: State<'_, AppState>) -> Result<(), String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge.disable_full_auto_mode().await;
    Ok(())
}

/// Check if full-auto mode is enabled.
#[tauri::command]
pub async fn is_full_auto_mode_enabled(state: State<'_, AppState>) -> Result<bool, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    Ok(bridge.is_full_auto_mode_enabled().await)
}

// ============================================================================
// Context Management Commands
// ============================================================================

use super::context_manager::{ContextSummary, ContextTrimConfig};
use super::token_budget::{TokenAlertLevel, TokenUsageStats};

/// Get the current context summary including token usage and alert level.
#[tauri::command]
pub async fn get_context_summary(state: State<'_, AppState>) -> Result<ContextSummary, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    Ok(bridge.get_context_summary().await)
}

/// Get detailed token usage statistics.
#[tauri::command]
pub async fn get_token_usage_stats(state: State<'_, AppState>) -> Result<TokenUsageStats, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    Ok(bridge.get_token_usage_stats().await)
}

/// Get the current token alert level.
#[tauri::command]
pub async fn get_token_alert_level(state: State<'_, AppState>) -> Result<TokenAlertLevel, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    Ok(bridge.get_token_alert_level().await)
}

/// Get the context utilization percentage (0.0 - 1.0+).
#[tauri::command]
pub async fn get_context_utilization(state: State<'_, AppState>) -> Result<f64, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    Ok(bridge.get_context_utilization().await)
}

/// Get remaining available tokens in the context window.
#[tauri::command]
pub async fn get_remaining_tokens(state: State<'_, AppState>) -> Result<usize, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    Ok(bridge.get_remaining_tokens().await)
}

/// Manually enforce context window limits by pruning old messages.
/// Returns the number of messages that were pruned.
#[tauri::command]
pub async fn enforce_context_window(state: State<'_, AppState>) -> Result<usize, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    Ok(bridge.enforce_context_window().await)
}

/// Reset the context manager (clear all token tracking).
/// This does not clear the conversation history, only the token stats.
#[tauri::command]
pub async fn reset_context_manager(state: State<'_, AppState>) -> Result<(), String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge.reset_context_manager().await;
    Ok(())
}

/// Get the context trim configuration.
#[tauri::command]
pub async fn get_context_trim_config(
    state: State<'_, AppState>,
) -> Result<ContextTrimConfig, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    Ok(bridge.get_context_trim_config())
}

/// Check if context management is enabled.
#[tauri::command]
pub async fn is_context_management_enabled(state: State<'_, AppState>) -> Result<bool, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    Ok(bridge.is_context_management_enabled())
}

// ============================================================================
// Loop Protection Commands
// ============================================================================

use super::loop_detection::{LoopDetectorStats, LoopProtectionConfig};

/// Get the current loop protection configuration.
#[tauri::command]
pub async fn get_loop_protection_config(
    state: State<'_, AppState>,
) -> Result<LoopProtectionConfig, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    Ok(bridge.get_loop_protection_config().await)
}

/// Set the loop protection configuration.
#[tauri::command]
pub async fn set_loop_protection_config(
    state: State<'_, AppState>,
    config: LoopProtectionConfig,
) -> Result<(), String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge.set_loop_protection_config(config).await;
    Ok(())
}

/// Get current loop detector statistics.
#[tauri::command]
pub async fn get_loop_detector_stats(
    state: State<'_, AppState>,
) -> Result<LoopDetectorStats, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    Ok(bridge.get_loop_detector_stats().await)
}

/// Check if loop detection is currently enabled.
#[tauri::command]
pub async fn is_loop_detection_enabled(state: State<'_, AppState>) -> Result<bool, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    Ok(bridge.is_loop_detection_enabled().await)
}

/// Disable loop detection for the current session.
/// This allows the agent to continue even if loops are detected.
#[tauri::command]
pub async fn disable_loop_detection(state: State<'_, AppState>) -> Result<(), String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge.disable_loop_detection_for_session().await;
    Ok(())
}

/// Re-enable loop detection.
#[tauri::command]
pub async fn enable_loop_detection(state: State<'_, AppState>) -> Result<(), String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge.enable_loop_detection().await;
    Ok(())
}

/// Reset the loop detector (clears all tracking).
#[tauri::command]
pub async fn reset_loop_detector(state: State<'_, AppState>) -> Result<(), String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge.reset_loop_detector().await;
    Ok(())
}

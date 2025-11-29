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
    let mut bridge = AgentBridge::new(
        workspace.into(),
        &provider,
        &model,
        &api_key,
        event_tx,
    )
    .await
    .map_err(|e| e.to_string())?;

    // Set PtyManager so commands can be executed in user's terminal
    bridge.set_pty_manager(state.pty_manager.clone());

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

    bridge.execute(&full_prompt).await.map_err(|e| e.to_string())
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
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    tracing::debug!("AI workspace updated to: {}", workspace);
    bridge.set_workspace(workspace.into()).await;
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

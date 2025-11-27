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
    let bridge = AgentBridge::new(
        workspace.into(),
        &provider,
        &model,
        &api_key,
        event_tx,
    )
    .await
    .map_err(|e| e.to_string())?;

    *state.ai_state.bridge.write().await = Some(bridge);

    tracing::info!(
        "AI agent initialized with provider: {}, model: {}",
        provider,
        model
    );
    Ok(())
}

/// Send a prompt to the AI agent and receive streaming response via events.
#[tauri::command]
pub async fn send_ai_prompt(
    state: State<'_, AppState>,
    prompt: String,
) -> Result<String, String> {
    let bridge_guard = state.ai_state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge.execute(&prompt).await.map_err(|e| e.to_string())
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
    let bridge = AgentBridge::new_vertex_anthropic(
        workspace.into(),
        &credentials_path,
        &project_id,
        &location,
        &model,
        event_tx,
    )
    .await
    .map_err(|e| e.to_string())?;

    *state.ai_state.bridge.write().await = Some(bridge);

    tracing::info!(
        "AI agent initialized with Vertex AI Anthropic, project: {}, model: {}",
        project_id,
        model
    );
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

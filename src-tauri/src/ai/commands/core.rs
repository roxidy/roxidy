// Core AI agent commands for initialization and execution.

use std::sync::Arc;
use tauri::{AppHandle, State};

use super::super::agent_bridge::AgentBridge;
use super::configure_bridge;
use crate::runtime::{QbitRuntime, TauriRuntime};
use crate::state::AppState;

/// Initialize the AI agent with the specified configuration.
///
/// If an existing AI agent is running, its session will be finalized and the
/// sidecar session will be ended before the new agent is initialized.
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
    // Clean up existing session before replacing the bridge
    // This ensures sessions are properly finalized when switching models/providers
    {
        let bridge_guard = state.ai_state.bridge.read().await;
        if bridge_guard.is_some() {
            // End the sidecar session (the bridge's Drop impl will finalize its session)
            if let Err(e) = state.sidecar_state.end_session() {
                tracing::warn!("Failed to end sidecar session during agent reinit: {}", e);
            } else {
                tracing::debug!("Sidecar session ended during agent reinit");
            }
        }
    }

    // Phase 5: Use runtime-based constructor
    // TauriRuntime handles event emission via Tauri's event system
    let runtime: Arc<dyn QbitRuntime> = Arc::new(TauriRuntime::new(app));

    // Store runtime in AiState (for potential future use by other components)
    *state.ai_state.runtime.write().await = Some(runtime.clone());

    // Create bridge with runtime (Phase 5 - new path)
    let mut bridge =
        AgentBridge::new_with_runtime(workspace.into(), &provider, &model, &api_key, runtime)
            .await
            .map_err(|e| e.to_string())?;

    configure_bridge(&mut bridge, &state);

    // Replace the bridge (old bridge's Drop impl will finalize its session)
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
    let bridge_guard = state.ai_state.get_bridge().await?;
    let bridge = bridge_guard.as_ref().unwrap();

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
    let bridge_guard = state.ai_state.get_bridge().await?;
    let bridge = bridge_guard.as_ref().unwrap();

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
    let bridge_guard = state.ai_state.get_bridge().await?;
    let bridge = bridge_guard.as_ref().unwrap();
    Ok(bridge.available_tools().await)
}

/// Sub-agent information for the frontend.
#[derive(serde::Serialize)]
pub struct SubAgentInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// Get the list of available sub-agents.
#[tauri::command]
pub async fn list_sub_agents(state: State<'_, AppState>) -> Result<Vec<SubAgentInfo>, String> {
    let bridge_guard = state.ai_state.get_bridge().await?;
    let bridge = bridge_guard.as_ref().unwrap();
    let registry = bridge.sub_agent_registry.read().await;

    Ok(registry
        .all()
        .map(|agent| SubAgentInfo {
            id: agent.id.clone(),
            name: agent.name.clone(),
            description: agent.description.clone(),
        })
        .collect())
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

// Session and conversation management commands.

use tauri::State;

use super::super::session::{self, QbitSessionSnapshot, SessionListingInfo};
use crate::state::AppState;

/// Clear the AI agent's conversation history.
/// Call this when starting a new conversation or when the user wants to reset context.
///
/// This also ends the current sidecar session (if any) so that a new session
/// will be started with the next prompt.
#[tauri::command]
pub async fn clear_ai_conversation(state: State<'_, AppState>) -> Result<(), String> {
    let bridge_guard = state.ai_state.get_bridge().await?;
    let bridge = bridge_guard.as_ref().unwrap();
    bridge.clear_conversation_history().await;

    // End the sidecar session so a new one starts with the next prompt
    if let Err(e) = state.sidecar_state.end_session() {
        tracing::warn!(
            "Failed to end sidecar session during conversation clear: {}",
            e
        );
        // Don't fail the whole operation - sidecar is optional
    } else {
        tracing::debug!("Sidecar session ended during conversation clear");
    }

    tracing::info!("AI conversation history cleared");
    Ok(())
}

/// Get the current conversation history length.
/// Useful for debugging or showing context status in the UI.
#[tauri::command]
pub async fn get_ai_conversation_length(state: State<'_, AppState>) -> Result<usize, String> {
    let bridge_guard = state.ai_state.get_bridge().await?;
    let bridge = bridge_guard.as_ref().unwrap();
    Ok(bridge.conversation_history_len().await)
}

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
    let bridge_guard = state.ai_state.get_bridge().await?;
    let bridge = bridge_guard.as_ref().unwrap();

    bridge.set_session_persistence_enabled(enabled).await;
    Ok(())
}

/// Check if session persistence is enabled.
#[tauri::command]
pub async fn is_ai_session_persistence_enabled(state: State<'_, AppState>) -> Result<bool, String> {
    let bridge_guard = state.ai_state.get_bridge().await?;
    let bridge = bridge_guard.as_ref().unwrap();

    Ok(bridge.is_session_persistence_enabled().await)
}

/// Manually finalize and save the current session.
///
/// Returns the path to the saved session file, if any.
#[tauri::command]
pub async fn finalize_ai_session(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let bridge_guard = state.ai_state.get_bridge().await?;
    let bridge = bridge_guard.as_ref().unwrap();

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
            super::super::session::QbitMessageRole::User => "**User**",
            super::super::session::QbitMessageRole::Assistant => "**Assistant**",
            super::super::session::QbitMessageRole::System => "**System**",
            super::super::session::QbitMessageRole::Tool => "**Tool**",
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
) -> Result<QbitSessionSnapshot, String> {
    // First load the session
    let session = session::load_session(&identifier)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Session '{}' not found", identifier))?;

    // Get the bridge and restore the conversation history
    let bridge_guard = state.ai_state.get_bridge().await?;
    let bridge = bridge_guard.as_ref().unwrap();

    // Restore the messages to the agent's conversation history
    bridge.restore_session(session.messages.clone()).await;

    tracing::info!(
        "Restored session '{}' with {} messages",
        identifier,
        session.messages.len()
    );

    // Start a sidecar session for context capture
    // Extract the first user message as the initial request
    let initial_request = session
        .messages
        .iter()
        .find(|m| m.role == super::super::session::QbitMessageRole::User)
        .map(|m| m.content.clone())
        .unwrap_or_else(|| format!("Restored session: {}", identifier));

    // End any existing sidecar session first
    if let Err(e) = state.sidecar_state.end_session() {
        tracing::debug!("No existing sidecar session to end: {}", e);
    }

    // Start a new sidecar session for this restored session
    match state.sidecar_state.start_session(&initial_request) {
        Ok(sid) => {
            tracing::info!("Started sidecar session {} for restored session", sid);
        }
        Err(e) => {
            tracing::warn!(
                "Failed to start sidecar session for restored session: {}",
                e
            );
        }
    }

    // Return the session so the frontend can display the restored messages
    Ok(session)
}

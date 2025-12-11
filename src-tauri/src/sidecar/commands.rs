//! Tauri commands for the simplified markdown-based sidecar system.
//!
//! These commands provide the interface between the frontend and the sidecar
//! session management system.

use crate::state::AppState;
use tauri::State;

use super::config::SidecarConfig;
use super::session::SessionMeta;
use super::state::SidecarStatus;

// =============================================================================
// Status & Initialization
// =============================================================================

/// Get the current sidecar status
#[tauri::command]
pub async fn sidecar_status(state: State<'_, AppState>) -> Result<SidecarStatus, String> {
    Ok(state.sidecar_state.status())
}

/// Initialize the sidecar for a workspace
#[tauri::command]
pub async fn sidecar_initialize(
    state: State<'_, AppState>,
    workspace_path: String,
) -> Result<(), String> {
    state
        .sidecar_state
        .initialize(workspace_path.into())
        .await
        .map_err(|e| e.to_string())
}

// =============================================================================
// Session Lifecycle
// =============================================================================

/// Start a new session
#[tauri::command]
pub async fn sidecar_start_session(
    state: State<'_, AppState>,
    initial_request: String,
) -> Result<String, String> {
    state
        .sidecar_state
        .start_session(&initial_request)
        .map_err(|e| e.to_string())
}

/// End the current session
#[tauri::command]
pub async fn sidecar_end_session(
    state: State<'_, AppState>,
) -> Result<Option<SessionMeta>, String> {
    state.sidecar_state.end_session().map_err(|e| e.to_string())
}

/// Get the current session ID
#[tauri::command]
pub async fn sidecar_current_session(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.sidecar_state.current_session_id())
}

// =============================================================================
// Session Content
// =============================================================================

/// Get the state.md content for a session (injectable context)
#[tauri::command]
pub async fn sidecar_get_session_state(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<String, String> {
    state
        .sidecar_state
        .get_session_state(&session_id)
        .await
        .map_err(|e| e.to_string())
}

/// Get the injectable context for the current session
#[tauri::command]
pub async fn sidecar_get_injectable_context(
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    state
        .sidecar_state
        .get_injectable_context()
        .await
        .map_err(|e| e.to_string())
}

/// Get the log.md content for a session
#[tauri::command]
pub async fn sidecar_get_session_log(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<String, String> {
    state
        .sidecar_state
        .get_session_log(&session_id)
        .await
        .map_err(|e| e.to_string())
}

/// Get the metadata for a session
#[tauri::command]
pub async fn sidecar_get_session_meta(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<SessionMeta, String> {
    state
        .sidecar_state
        .get_session_meta(&session_id)
        .await
        .map_err(|e| e.to_string())
}

/// List all sessions
#[tauri::command]
pub async fn sidecar_list_sessions(state: State<'_, AppState>) -> Result<Vec<SessionMeta>, String> {
    state
        .sidecar_state
        .list_sessions()
        .await
        .map_err(|e| e.to_string())
}

// =============================================================================
// Configuration
// =============================================================================

/// Get the sidecar configuration
#[tauri::command]
pub async fn sidecar_get_config(state: State<'_, AppState>) -> Result<SidecarConfig, String> {
    Ok(state.sidecar_state.config())
}

/// Update the sidecar configuration
#[tauri::command]
pub async fn sidecar_set_config(
    state: State<'_, AppState>,
    config: SidecarConfig,
) -> Result<(), String> {
    state.sidecar_state.set_config(config);
    Ok(())
}

// =============================================================================
// Lifecycle
// =============================================================================

/// Shutdown the sidecar
#[tauri::command]
pub async fn sidecar_shutdown(state: State<'_, AppState>) -> Result<(), String> {
    state.sidecar_state.shutdown();
    Ok(())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    // Commands are tested via integration tests
}

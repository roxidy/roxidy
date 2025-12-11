//! Tauri commands for the simplified sidecar system.
//!
//! Provides interface between frontend and sidecar session/patch management.

use crate::state::AppState;
use tauri::State;

use super::commits::{PatchManager, StagedPatch};
use super::config::SidecarConfig;
use super::session::{Session, SessionMeta};
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

/// Get the state.md content for a session (body only)
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
// L2: Staged Patches
// =============================================================================

/// Get all staged patches for a session
#[tauri::command]
pub async fn sidecar_get_staged_patches(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<StagedPatch>, String> {
    let sessions_dir = state.sidecar_state.config().sessions_dir();
    let session = Session::load(&sessions_dir, &session_id)
        .await
        .map_err(|e| e.to_string())?;

    let manager = PatchManager::new(session.dir().to_path_buf());
    manager.list_staged().await.map_err(|e| e.to_string())
}

/// Get all applied patches for a session
#[tauri::command]
pub async fn sidecar_get_applied_patches(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<StagedPatch>, String> {
    let sessions_dir = state.sidecar_state.config().sessions_dir();
    let session = Session::load(&sessions_dir, &session_id)
        .await
        .map_err(|e| e.to_string())?;

    let manager = PatchManager::new(session.dir().to_path_buf());
    manager.list_applied().await.map_err(|e| e.to_string())
}

/// Get a specific patch by ID
#[tauri::command]
pub async fn sidecar_get_patch(
    state: State<'_, AppState>,
    session_id: String,
    patch_id: u32,
) -> Result<Option<StagedPatch>, String> {
    let sessions_dir = state.sidecar_state.config().sessions_dir();
    let session = Session::load(&sessions_dir, &session_id)
        .await
        .map_err(|e| e.to_string())?;

    let manager = PatchManager::new(session.dir().to_path_buf());
    manager
        .get_staged(patch_id)
        .await
        .map_err(|e| e.to_string())
}

/// Discard a staged patch
#[tauri::command]
pub async fn sidecar_discard_patch(
    state: State<'_, AppState>,
    session_id: String,
    patch_id: u32,
) -> Result<bool, String> {
    let sessions_dir = state.sidecar_state.config().sessions_dir();
    let session = Session::load(&sessions_dir, &session_id)
        .await
        .map_err(|e| e.to_string())?;

    let manager = PatchManager::new(session.dir().to_path_buf());
    manager
        .discard_patch(patch_id)
        .await
        .map_err(|e| e.to_string())
}

/// Apply a staged patch using git am
#[tauri::command]
pub async fn sidecar_apply_patch(
    state: State<'_, AppState>,
    session_id: String,
    patch_id: u32,
) -> Result<String, String> {
    let sessions_dir = state.sidecar_state.config().sessions_dir();
    let session = Session::load(&sessions_dir, &session_id)
        .await
        .map_err(|e| e.to_string())?;

    let git_root = session
        .meta()
        .git_root
        .clone()
        .or_else(|| {
            std::process::Command::new("git")
                .args(["rev-parse", "--show-toplevel"])
                .current_dir(&session.meta().cwd)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| {
                    std::path::PathBuf::from(String::from_utf8_lossy(&o.stdout).trim().to_string())
                })
        })
        .ok_or_else(|| "No git repository found".to_string())?;

    let manager = PatchManager::new(session.dir().to_path_buf());
    manager
        .apply_patch(patch_id, &git_root)
        .await
        .map_err(|e| e.to_string())
}

/// Apply all staged patches in order
#[tauri::command]
pub async fn sidecar_apply_all_patches(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<(u32, String)>, String> {
    let sessions_dir = state.sidecar_state.config().sessions_dir();
    let session = Session::load(&sessions_dir, &session_id)
        .await
        .map_err(|e| e.to_string())?;

    let git_root = session
        .meta()
        .git_root
        .clone()
        .or_else(|| {
            std::process::Command::new("git")
                .args(["rev-parse", "--show-toplevel"])
                .current_dir(&session.meta().cwd)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| {
                    std::path::PathBuf::from(String::from_utf8_lossy(&o.stdout).trim().to_string())
                })
        })
        .ok_or_else(|| "No git repository found".to_string())?;

    let manager = PatchManager::new(session.dir().to_path_buf());
    manager
        .apply_all_patches(&git_root)
        .await
        .map_err(|e| e.to_string())
}

/// Get staged patches for the current session
#[tauri::command]
pub async fn sidecar_get_current_staged_patches(
    state: State<'_, AppState>,
) -> Result<Vec<StagedPatch>, String> {
    let session_id = state
        .sidecar_state
        .current_session_id()
        .ok_or_else(|| "No active session".to_string())?;

    let sessions_dir = state.sidecar_state.config().sessions_dir();
    let session = Session::load(&sessions_dir, &session_id)
        .await
        .map_err(|e| e.to_string())?;

    let manager = PatchManager::new(session.dir().to_path_buf());
    manager.list_staged().await.map_err(|e| e.to_string())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    // Commands are tested via integration tests
}

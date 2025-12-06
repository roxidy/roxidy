//! Tauri commands for the sidecar system.
//!
//! These commands expose sidecar functionality to the frontend.

use std::path::PathBuf;

use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::state::AppState;

use super::events::SidecarSession;
use super::models::ModelsStatus;
use super::state::SidecarStatus;
use super::storage::{IndexStatus, StorageStats};
use super::synthesis::{CommitDraft, HistoryResponse, SessionSummary};

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
        .initialize(PathBuf::from(workspace_path))
        .await
        .map_err(|e| e.to_string())
}

/// Start a new capture session
#[tauri::command]
pub async fn sidecar_start_session(
    state: State<'_, AppState>,
    initial_request: String,
) -> Result<String, String> {
    let session_id = state
        .sidecar_state
        .start_session(&initial_request)
        .map_err(|e| e.to_string())?;

    Ok(session_id.to_string())
}

/// End the current session
#[tauri::command]
pub async fn sidecar_end_session(
    state: State<'_, AppState>,
) -> Result<Option<SidecarSession>, String> {
    let session = state.sidecar_state.end_session().map_err(|e| e.to_string())?;

    // Save the session to storage if we have one
    if let Some(ref session) = session {
        if let Some(storage) = state.sidecar_state.storage() {
            if let Err(e) = storage.save_session(session).await {
                tracing::error!("Failed to save session to storage: {}", e);
            } else {
                tracing::info!("Session {} saved to storage", session.id);
            }
        }
    }

    Ok(session)
}

/// Get the current session ID
#[tauri::command]
pub async fn sidecar_current_session(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.sidecar_state.current_session_id().map(|id| id.to_string()))
}

/// Generate a commit message for the current or specified session
#[tauri::command]
pub async fn sidecar_generate_commit(
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> Result<CommitDraft, String> {
    let session_id = if let Some(id) = session_id {
        Uuid::parse_str(&id).map_err(|e| e.to_string())?
    } else {
        state
            .sidecar_state
            .current_session_id()
            .ok_or_else(|| "No active session".to_string())?
    };

    // Create synthesizer
    let storage = state
        .sidecar_state
        .storage()
        .ok_or_else(|| "Storage not initialized".to_string())?;

    let config = state.sidecar_state.config();
    let model_manager = std::sync::Arc::new(super::models::ModelManager::new(config.models_dir));

    let synthesizer = super::synthesis::Synthesizer::new(
        storage,
        model_manager,
        config.synthesis_enabled && state.sidecar_state.llm_ready(),
    );

    synthesizer
        .synthesize_commit(session_id)
        .await
        .map_err(|e| e.to_string())
}

/// Generate a summary for a session
#[tauri::command]
pub async fn sidecar_generate_summary(
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> Result<SessionSummary, String> {
    let session_id = if let Some(id) = session_id {
        Uuid::parse_str(&id).map_err(|e| e.to_string())?
    } else {
        state
            .sidecar_state
            .current_session_id()
            .ok_or_else(|| "No active session".to_string())?
    };

    let storage = state
        .sidecar_state
        .storage()
        .ok_or_else(|| "Storage not initialized".to_string())?;

    let config = state.sidecar_state.config();
    let model_manager = std::sync::Arc::new(super::models::ModelManager::new(config.models_dir));

    let synthesizer = super::synthesis::Synthesizer::new(
        storage,
        model_manager,
        config.synthesis_enabled && state.sidecar_state.llm_ready(),
    );

    synthesizer
        .synthesize_summary(session_id)
        .await
        .map_err(|e| e.to_string())
}

/// Query session history
#[tauri::command]
pub async fn sidecar_query_history(
    state: State<'_, AppState>,
    question: String,
    limit: Option<usize>,
) -> Result<HistoryResponse, String> {
    let storage = state
        .sidecar_state
        .storage()
        .ok_or_else(|| "Storage not initialized".to_string())?;

    let config = state.sidecar_state.config();
    let model_manager = std::sync::Arc::new(super::models::ModelManager::new(config.models_dir));

    let synthesizer = super::synthesis::Synthesizer::new(
        storage,
        model_manager,
        config.synthesis_enabled && state.sidecar_state.llm_ready(),
    );

    synthesizer
        .query_history(&question, limit.unwrap_or(10))
        .await
        .map_err(|e| e.to_string())
}

/// Search events by keyword
#[tauri::command]
pub async fn sidecar_search_events(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<super::events::SessionEvent>, String> {
    state
        .sidecar_state
        .search_events(&query, limit.unwrap_or(20))
        .await
        .map_err(|e| e.to_string())
}

/// Get events for a session
#[tauri::command]
pub async fn sidecar_get_session_events(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<super::events::SessionEvent>, String> {
    let session_id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;

    state
        .sidecar_state
        .get_session_events(session_id)
        .await
        .map_err(|e| e.to_string())
}

/// Get checkpoints for a session
#[tauri::command]
pub async fn sidecar_get_session_checkpoints(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<super::events::Checkpoint>, String> {
    let session_id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;

    state
        .sidecar_state
        .get_session_checkpoints(session_id)
        .await
        .map_err(|e| e.to_string())
}

/// List all sessions
#[tauri::command]
pub async fn sidecar_list_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<SidecarSession>, String> {
    let storage = state
        .sidecar_state
        .storage()
        .ok_or_else(|| "Storage not initialized".to_string())?;

    storage.list_sessions().await.map_err(|e| e.to_string())
}

/// Get storage statistics
#[tauri::command]
pub async fn sidecar_storage_stats(state: State<'_, AppState>) -> Result<StorageStats, String> {
    let storage = state
        .sidecar_state
        .storage()
        .ok_or_else(|| "Storage not initialized".to_string())?;

    storage.stats().await.map_err(|e| e.to_string())
}

/// Get model status
#[tauri::command]
pub async fn sidecar_models_status(state: State<'_, AppState>) -> Result<ModelsStatus, String> {
    let config = state.sidecar_state.config();
    let model_manager = super::models::ModelManager::new(config.models_dir);

    Ok(model_manager.status())
}

/// Download models
#[tauri::command]
pub async fn sidecar_download_models(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let config = state.sidecar_state.config();
    let model_manager = super::models::ModelManager::new(config.models_dir);

    // Download embedding model
    model_manager
        .download_embedding_model(|progress| {
            let _ = app.emit("sidecar-download-progress", &progress);
        })
        .await
        .map_err(|e| e.to_string())?;

    // Download LLM model
    model_manager
        .download_llm_model(|progress| {
            let _ = app.emit("sidecar-download-progress", &progress);
        })
        .await
        .map_err(|e| e.to_string())?;

    // Update ready status
    state.sidecar_state.set_embeddings_ready(model_manager.embedding_available());
    state.sidecar_state.set_llm_ready(model_manager.llm_available());

    Ok(())
}

/// Get sidecar configuration
#[tauri::command]
pub async fn sidecar_get_config(
    state: State<'_, AppState>,
) -> Result<super::config::SidecarConfig, String> {
    Ok(state.sidecar_state.config())
}

/// Update sidecar configuration
#[tauri::command]
pub async fn sidecar_set_config(
    state: State<'_, AppState>,
    config: super::config::SidecarConfig,
) -> Result<(), String> {
    state.sidecar_state.set_config(config);
    Ok(())
}

/// Shutdown the sidecar
#[tauri::command]
pub async fn sidecar_shutdown(state: State<'_, AppState>) -> Result<(), String> {
    state.sidecar_state.shutdown();
    Ok(())
}

/// Export a session to JSON
#[tauri::command]
pub async fn sidecar_export_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<String, String> {
    let session_id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;

    let storage = state
        .sidecar_state
        .storage()
        .ok_or_else(|| "Storage not initialized".to_string())?;

    // Get session
    let session = storage
        .get_session(session_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    // Get events
    let events = storage
        .get_session_events(session_id)
        .await
        .map_err(|e| e.to_string())?;

    // Get checkpoints
    let checkpoints = storage
        .get_session_checkpoints(session_id)
        .await
        .map_err(|e| e.to_string())?;

    // Create export
    let export = super::events::SessionExport::new(session, events, checkpoints);

    export.to_json().map_err(|e| e.to_string())
}

/// Export a session to a file
#[tauri::command]
pub async fn sidecar_export_session_to_file(
    state: State<'_, AppState>,
    session_id: String,
    output_path: String,
) -> Result<(), String> {
    let json = sidecar_export_session(state, session_id).await?;

    tokio::fs::write(&output_path, json)
        .await
        .map_err(|e| format!("Failed to write export file: {}", e))
}

/// Import a session from JSON
#[tauri::command]
pub async fn sidecar_import_session(
    state: State<'_, AppState>,
    json: String,
) -> Result<String, String> {
    let export =
        super::events::SessionExport::from_json(&json).map_err(|e| e.to_string())?;

    let storage = state
        .sidecar_state
        .storage()
        .ok_or_else(|| "Storage not initialized".to_string())?;

    // Save session
    storage
        .save_session(&export.session)
        .await
        .map_err(|e| e.to_string())?;

    // Save events
    storage
        .save_events(&export.events)
        .await
        .map_err(|e| e.to_string())?;

    // Save checkpoints
    for checkpoint in &export.checkpoints {
        storage
            .save_checkpoint(checkpoint)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(export.session.id.to_string())
}

/// Import a session from a file
#[tauri::command]
pub async fn sidecar_import_session_from_file(
    state: State<'_, AppState>,
    input_path: String,
) -> Result<String, String> {
    let json = tokio::fs::read_to_string(&input_path)
        .await
        .map_err(|e| format!("Failed to read import file: {}", e))?;

    sidecar_import_session(state, json).await
}

/// Get pending files for commit boundary detection
#[tauri::command]
pub async fn sidecar_pending_files(
    state: State<'_, AppState>,
) -> Result<Vec<PathBuf>, String> {
    Ok(state.sidecar_state.pending_commit_files())
}

/// Clear commit boundary tracking (after manual commit)
#[tauri::command]
pub async fn sidecar_clear_commit_boundary(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.sidecar_state.clear_commit_boundary();
    Ok(())
}

/// Delete old events based on retention policy
#[tauri::command]
pub async fn sidecar_cleanup(
    state: State<'_, AppState>,
    max_age_days: Option<u32>,
) -> Result<usize, String> {
    let storage = state
        .sidecar_state
        .storage()
        .ok_or_else(|| "Storage not initialized".to_string())?;

    let config = state.sidecar_state.config();
    let days = max_age_days.unwrap_or(config.retention_days);

    storage
        .cleanup_old_events(days)
        .await
        .map_err(|e| e.to_string())
}

/// Get vector index status
#[tauri::command]
pub async fn sidecar_index_status(state: State<'_, AppState>) -> Result<IndexStatus, String> {
    let storage = state
        .sidecar_state
        .storage()
        .ok_or_else(|| "Storage not initialized".to_string())?;

    storage.indexes_exist().await.map_err(|e| e.to_string())
}

/// Create vector indexes for faster search (if enough data exists)
#[tauri::command]
pub async fn sidecar_create_indexes(state: State<'_, AppState>) -> Result<IndexStatus, String> {
    let storage = state
        .sidecar_state
        .storage()
        .ok_or_else(|| "Storage not initialized".to_string())?;

    // Try to create events index
    let events_indexed = storage
        .create_events_index()
        .await
        .map_err(|e| e.to_string())?;

    // Try to create checkpoints index
    let checkpoints_indexed = storage
        .create_checkpoints_index()
        .await
        .map_err(|e| e.to_string())?;

    // Get final status
    let mut status = storage.indexes_exist().await.map_err(|e| e.to_string())?;

    // If we just created indexes, update the status
    if events_indexed {
        status.events_indexed = true;
    }
    if checkpoints_indexed {
        status.checkpoints_indexed = true;
    }

    Ok(status)
}

#[cfg(test)]
mod tests {
    // Command tests require Tauri test harness
    // Basic functionality is tested in other modules
}

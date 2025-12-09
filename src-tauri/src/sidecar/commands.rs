//! Tauri commands for the sidecar system.
//!
//! These commands expose sidecar functionality to the frontend.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::state::AppState;

use super::config::SynthesisBackend;
use super::events::SidecarSession;
use super::models::ModelsStatus;
use super::state::SidecarStatus;
use super::storage::{IndexStatus, StorageStats};
use super::synthesis::{CommitDraft, HistoryResponse, SessionSummary};
use super::synthesis_llm;

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
    // Initialize main sidecar storage
    state
        .sidecar_state
        .initialize(PathBuf::from(workspace_path))
        .await
        .map_err(|e| e.to_string())?;

    // Initialize Layer1 processor for session state tracking
    state.sidecar_state.initialize_layer1().await.map_err(|e| {
        tracing::warn!("Failed to initialize Layer1 processor: {}", e);
        e.to_string()
    })?;

    tracing::info!("[sidecar] Initialization complete (storage + Layer1)");
    Ok(())
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
    let session = state
        .sidecar_state
        .end_session()
        .map_err(|e| e.to_string())?;

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
    Ok(state
        .sidecar_state
        .current_session_id()
        .map(|id| id.to_string()))
}

/// Generate a commit message for the current or specified session
#[tauri::command]
pub async fn sidecar_generate_commit(
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> Result<CommitDraft, String> {
    tracing::info!(
        "[sidecar-cmd] generate_commit called with session_id: {:?}",
        session_id
    );

    // Create synthesizer - need storage first
    let storage = state
        .sidecar_state
        .storage()
        .ok_or_else(|| "Storage not initialized".to_string())?;

    let session_id = if let Some(id) = session_id {
        tracing::debug!("[sidecar-cmd] Using provided session_id: {}", id);
        Uuid::parse_str(&id).map_err(|e| e.to_string())?
    } else if let Some(id) = state.sidecar_state.current_session_id() {
        // Use current active session if available
        tracing::debug!("[sidecar-cmd] Using current active session: {}", id);
        id
    } else {
        // Fall back to most recent completed session
        tracing::debug!("[sidecar-cmd] No active session, looking for recent completed session");
        let sessions = storage.list_sessions().await.map_err(|e| e.to_string())?;
        tracing::debug!("[sidecar-cmd] Found {} sessions in storage", sessions.len());
        let id = sessions
            .first()
            .map(|s| s.id)
            .ok_or_else(|| "No sessions found. Complete an AI interaction first.".to_string())?;
        tracing::debug!("[sidecar-cmd] Using most recent session: {}", id);
        id
    };

    let config = state.sidecar_state.config();
    let model_manager = Arc::new(super::models::ModelManager::new(config.models_dir.clone()));

    // Create the LLM backend based on config
    let llm =
        match synthesis_llm::create_synthesis_llm(&config.synthesis_backend, model_manager.clone())
            .await
        {
            Ok(llm) => llm,
            Err(e) => {
                tracing::warn!(
                    "[sidecar-cmd] Failed to create LLM backend, falling back to template: {}",
                    e
                );
                Arc::new(synthesis_llm::TemplateLlm) as Arc<dyn synthesis_llm::SynthesisLlm>
            }
        };

    let llm_enabled = config.synthesis_enabled && llm.is_available();
    tracing::info!(
        "[sidecar-cmd] Config: synthesis_enabled={}, backend={}, llm_enabled={}",
        config.synthesis_enabled,
        llm.description(),
        llm_enabled
    );

    let synthesizer = super::synthesis::Synthesizer::new(storage, model_manager, llm, llm_enabled);

    tracing::info!(
        "[sidecar-cmd] Synthesizing commit for session: {}",
        session_id
    );
    let result = synthesizer
        .synthesize_commit(session_id)
        .await
        .map_err(|e| {
            tracing::error!("[sidecar-cmd] Synthesis failed: {}", e);
            e.to_string()
        });

    if let Ok(ref draft) = result {
        tracing::info!(
            "[sidecar-cmd] Commit draft generated: scope='{}', message='{}', files={}",
            draft.scope,
            truncate_str(&draft.message, 60),
            draft.files.len()
        );
    }

    result
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
    let model_manager = Arc::new(super::models::ModelManager::new(config.models_dir.clone()));

    let llm =
        match synthesis_llm::create_synthesis_llm(&config.synthesis_backend, model_manager.clone())
            .await
        {
            Ok(llm) => llm,
            Err(e) => {
                tracing::warn!("[sidecar-cmd] Failed to create LLM backend: {}", e);
                Arc::new(synthesis_llm::TemplateLlm) as Arc<dyn synthesis_llm::SynthesisLlm>
            }
        };

    let synthesizer = super::synthesis::Synthesizer::new(
        storage,
        model_manager,
        llm.clone(),
        config.synthesis_enabled && llm.is_available(),
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
    let model_manager = Arc::new(super::models::ModelManager::new(config.models_dir.clone()));

    let llm =
        match synthesis_llm::create_synthesis_llm(&config.synthesis_backend, model_manager.clone())
            .await
        {
            Ok(llm) => llm,
            Err(e) => {
                tracing::warn!("[sidecar-cmd] Failed to create LLM backend: {}", e);
                Arc::new(synthesis_llm::TemplateLlm) as Arc<dyn synthesis_llm::SynthesisLlm>
            }
        };

    let synthesizer = super::synthesis::Synthesizer::new(
        storage,
        model_manager,
        llm.clone(),
        config.synthesis_enabled && llm.is_available(),
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
    state
        .sidecar_state
        .set_embeddings_ready(model_manager.embedding_available());
    state
        .sidecar_state
        .set_llm_ready(model_manager.llm_available());

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

/// Set the synthesis backend
#[tauri::command]
pub async fn sidecar_set_backend(
    state: State<'_, AppState>,
    backend: SynthesisBackend,
) -> Result<String, String> {
    let mut config = state.sidecar_state.config();
    config.synthesis_backend = backend.clone();
    state.sidecar_state.set_config(config.clone());

    // Verify the backend can be created
    let model_manager = Arc::new(super::models::ModelManager::new(config.models_dir.clone()));
    let llm = synthesis_llm::create_synthesis_llm(&backend, model_manager)
        .await
        .map_err(|e| format!("Failed to create backend: {}", e))?;

    tracing::info!(
        "[sidecar-cmd] Switched synthesis backend to: {}",
        llm.description()
    );
    Ok(llm.description())
}

/// Get available synthesis backend options
#[tauri::command]
pub async fn sidecar_available_backends() -> Result<Vec<String>, String> {
    Ok(vec![
        "local".to_string(),
        "vertex-anthropic".to_string(),
        "openai".to_string(),
        "grok".to_string(),
        "openai-compatible".to_string(),
        "template".to_string(),
    ])
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
    let export = super::events::SessionExport::from_json(&json).map_err(|e| e.to_string())?;

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
pub async fn sidecar_pending_files(state: State<'_, AppState>) -> Result<Vec<PathBuf>, String> {
    Ok(state.sidecar_state.pending_commit_files())
}

/// Clear commit boundary tracking (after manual commit)
#[tauri::command]
pub async fn sidecar_clear_commit_boundary(state: State<'_, AppState>) -> Result<(), String> {
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

/// Get the current Layer 1 session state
#[tauri::command]
pub async fn sidecar_get_session_state(
    session_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<super::layer1::SessionState>, String> {
    // Get session_id from param or current session
    let session_uuid = if let Some(id) = session_id {
        Uuid::parse_str(&id).map_err(|e| e.to_string())?
    } else {
        state
            .sidecar_state
            .current_session_id()
            .ok_or_else(|| "No active session".to_string())?
    };

    // Call sidecar_state.get_layer1_state(uuid)
    state
        .sidecar_state
        .get_layer1_state(session_uuid)
        .await
        .map_err(|e| e.to_string())
}

/// Get injectable context string for debugging/preview
#[tauri::command]
pub async fn sidecar_get_injectable_context(
    session_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Get the session state first
    let session_state = sidecar_get_session_state(session_id, state).await?;

    match session_state {
        Some(state) => Ok(super::layer1::api::get_injectable_context(&state)),
        None => Ok(String::new()),
    }
}

/// Get current goal stack
#[tauri::command]
pub async fn sidecar_get_goals(
    session_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<super::layer1::Goal>, String> {
    let session_state = sidecar_get_session_state(session_id, state).await?;

    match session_state {
        Some(state) => Ok(state.goal_stack),
        None => Ok(Vec::new()),
    }
}

/// Get file context map
#[tauri::command]
pub async fn sidecar_get_file_contexts(
    session_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<std::collections::HashMap<String, super::layer1::FileContext>, String> {
    let session_state = sidecar_get_session_state(session_id, state).await?;

    match session_state {
        Some(state) => {
            // Convert PathBuf keys to String for JSON serialization
            let mut result = std::collections::HashMap::new();
            for (path, context) in state.file_contexts {
                result.insert(path.display().to_string(), context);
            }
            Ok(result)
        }
        None => Ok(std::collections::HashMap::new()),
    }
}

/// Get decision log
#[tauri::command]
pub async fn sidecar_get_decisions(
    session_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<super::layer1::Decision>, String> {
    let session_state = sidecar_get_session_state(session_id, state).await?;

    match session_state {
        Some(state) => Ok(state.decisions),
        None => Ok(Vec::new()),
    }
}

/// Get error journal
#[tauri::command]
pub async fn sidecar_get_errors(
    session_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<super::layer1::ErrorEntry>, String> {
    let session_state = sidecar_get_session_state(session_id, state).await?;

    match session_state {
        Some(state) => Ok(state.errors),
        None => Ok(Vec::new()),
    }
}

/// Get open questions
#[tauri::command]
pub async fn sidecar_get_open_questions(
    session_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<super::layer1::OpenQuestion>, String> {
    let session_state = sidecar_get_session_state(session_id, state).await?;

    match session_state {
        Some(state) => Ok(state.open_questions),
        None => Ok(Vec::new()),
    }
}

/// Answer an open question
#[tauri::command]
pub async fn sidecar_answer_question(
    question_id: String,
    answer: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Parse question_id as UUID
    let question_uuid = Uuid::parse_str(&question_id).map_err(|e| e.to_string())?;

    // Get the current session's state
    let session_id = state
        .sidecar_state
        .current_session_id()
        .ok_or_else(|| "No active session".to_string())?;

    // Get the layer1 storage
    let mut session_state = state
        .sidecar_state
        .get_layer1_state(session_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Session state not found".to_string())?;

    // Answer the question
    session_state.answer_question_by_id(question_uuid, answer);

    // Note: The state is updated in-memory. For persistence, we would need to
    // save it back to storage, but that requires access to Layer1Storage which
    // isn't exposed in the current SidecarState API. This will be handled by
    // the Layer1Processor when it processes events.

    Ok(())
}

/// Mark a goal as completed manually
#[tauri::command]
pub async fn sidecar_complete_goal(
    goal_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Parse goal_id as UUID
    let goal_uuid = Uuid::parse_str(&goal_id).map_err(|e| e.to_string())?;

    // Get the current session's state
    let session_id = state
        .sidecar_state
        .current_session_id()
        .ok_or_else(|| "No active session".to_string())?;

    // Get the layer1 storage
    let mut session_state = state
        .sidecar_state
        .get_layer1_state(session_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Session state not found".to_string())?;

    // Complete the goal
    session_state.complete_goal(goal_uuid);

    // Note: The state is updated in-memory. For persistence, we would need to
    // save it back to storage, but that requires access to Layer1Storage which
    // isn't exposed in the current SidecarState API. This will be handled by
    // the Layer1Processor when it processes events.

    Ok(())
}

/// Truncate a string to a maximum length
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let mut result: String = s.chars().take(max_len.saturating_sub(3)).collect();
        result.push_str("...");
        result
    }
}

#[cfg(test)]
mod tests {
    // Command tests require Tauri test harness
    // Basic functionality is tested in other modules
}

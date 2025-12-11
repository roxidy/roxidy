//! Tauri commands for the simplified sidecar system.
//!
//! Provides interface between frontend and sidecar session/patch/artifact management.

use crate::state::AppState;
use tauri::State;

use super::artifacts::{ArtifactFile, ArtifactManager};
use super::commits::{PatchManager, StagedPatch};
use super::config::SidecarConfig;
use super::events::SidecarEvent;
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
    let discarded = manager
        .discard_patch(patch_id)
        .await
        .map_err(|e| e.to_string())?;

    // Emit patch discarded event if successful
    if discarded {
        state
            .sidecar_state
            .emit_event(SidecarEvent::PatchDiscarded {
                session_id: session_id.clone(),
                patch_id,
            });
    }

    Ok(discarded)
}

/// Apply a staged patch using git am
///
/// After successful application, triggers L3 artifact regeneration (L2 -> L3 cascade).
/// Uses the configured artifact synthesis backend from settings.
#[tauri::command]
pub async fn sidecar_apply_patch(
    state: State<'_, AppState>,
    session_id: String,
    patch_id: u32,
) -> Result<String, String> {
    use super::artifacts::ArtifactSynthesisConfig;

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

    let patch_manager = PatchManager::new(session.dir().to_path_buf());

    // Get the patch subject before applying (for artifact regeneration)
    let patch = patch_manager
        .get_staged(patch_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Patch {} not found", patch_id))?;
    let patch_subject = patch.subject.clone();

    // Apply the patch
    let sha = patch_manager
        .apply_patch(patch_id, &git_root)
        .await
        .map_err(|e| e.to_string())?;

    // Emit patch applied event
    state.sidecar_state.emit_event(SidecarEvent::PatchApplied {
        session_id: session_id.clone(),
        patch_id,
        commit_sha: sha.clone(),
    });

    // L2 -> L3 Cascade: Trigger artifact regeneration with configured backend
    let artifact_manager = ArtifactManager::new(session.dir().to_path_buf());
    let session_context = session.read_state().await.unwrap_or_default();

    // Get artifact synthesis config from settings
    let settings = state.settings_manager.get().await;
    let artifact_config = ArtifactSynthesisConfig::from_sidecar_settings(&settings.sidecar);

    if let Err(e) = artifact_manager
        .regenerate_from_patches_with_config(
            &git_root,
            &[patch_subject],
            &session_context,
            &artifact_config,
        )
        .await
    {
        // Log but don't fail - artifact regeneration is non-critical
        tracing::warn!("Failed to regenerate artifacts after patch apply: {}", e);
    }

    Ok(sha)
}

/// Apply all staged patches in order
///
/// After successful application of all patches, triggers L3 artifact regeneration (L2 -> L3 cascade).
/// Uses the configured artifact synthesis backend from settings.
#[tauri::command]
pub async fn sidecar_apply_all_patches(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<(u32, String)>, String> {
    use super::artifacts::ArtifactSynthesisConfig;

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

    let patch_manager = PatchManager::new(session.dir().to_path_buf());

    // Get all patch subjects before applying (for artifact regeneration)
    let staged = patch_manager
        .list_staged()
        .await
        .map_err(|e| e.to_string())?;
    let patch_subjects: Vec<String> = staged.iter().map(|p| p.subject.clone()).collect();

    // Apply all patches
    let results = patch_manager
        .apply_all_patches(&git_root)
        .await
        .map_err(|e| e.to_string())?;

    // Emit patch applied events for each applied patch
    for (patch_id, sha) in &results {
        state.sidecar_state.emit_event(SidecarEvent::PatchApplied {
            session_id: session_id.clone(),
            patch_id: *patch_id,
            commit_sha: sha.clone(),
        });
    }

    // L2 -> L3 Cascade: Trigger artifact regeneration if patches were applied
    if !results.is_empty() {
        let artifact_manager = ArtifactManager::new(session.dir().to_path_buf());
        let session_context = session.read_state().await.unwrap_or_default();

        // Get artifact synthesis config from settings
        let settings = state.settings_manager.get().await;
        let artifact_config = ArtifactSynthesisConfig::from_sidecar_settings(&settings.sidecar);

        if let Err(e) = artifact_manager
            .regenerate_from_patches_with_config(
                &git_root,
                &patch_subjects,
                &session_context,
                &artifact_config,
            )
            .await
        {
            // Log but don't fail - artifact regeneration is non-critical
            tracing::warn!(
                "Failed to regenerate artifacts after applying {} patches: {}",
                results.len(),
                e
            );
        }
    }

    Ok(results)
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

/// Regenerate a patch's commit message using LLM synthesis
///
/// Uses the configured synthesis backend to generate a new commit message
/// based on the patch diff and session context.
#[tauri::command]
pub async fn sidecar_regenerate_patch(
    state: State<'_, AppState>,
    session_id: String,
    patch_id: u32,
) -> Result<StagedPatch, String> {
    use super::synthesis::{create_synthesizer, SynthesisConfig, SynthesisInput};

    let sessions_dir = state.sidecar_state.config().sessions_dir();
    let session = Session::load(&sessions_dir, &session_id)
        .await
        .map_err(|e| e.to_string())?;

    let manager = PatchManager::new(session.dir().to_path_buf());

    // Get the patch and its diff
    let patch = manager
        .get_staged(patch_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Patch {} not found", patch_id))?;

    let diff = manager
        .get_patch_diff(patch_id)
        .await
        .map_err(|e| e.to_string())?;

    // Get session context
    let session_context = session.read_state().await.ok();

    // Get synthesis config from settings
    let settings = state.settings_manager.get().await;
    let synthesis_config = SynthesisConfig::from_sidecar_settings(&settings.sidecar);

    // Create synthesizer
    let synthesizer = create_synthesizer(&synthesis_config)
        .map_err(|e| format!("Failed to create synthesizer: {}", e))?;

    // Build input
    let files: Vec<std::path::PathBuf> = patch.files.iter().map(std::path::PathBuf::from).collect();
    let mut input = SynthesisInput::new(diff, files);
    if let Some(ctx) = session_context {
        input = input.with_context(ctx);
    }

    // Generate new message
    let result = synthesizer
        .synthesize(&input)
        .await
        .map_err(|e| format!("Synthesis failed: {}", e))?;

    // Update the patch with the new message
    let updated_patch = manager
        .update_patch_message(patch_id, &result.message)
        .await
        .map_err(|e| e.to_string())?;

    tracing::info!(
        "Regenerated patch {} message using {} backend",
        patch_id,
        result.backend
    );

    Ok(updated_patch)
}

/// Update a patch's commit message manually (without LLM)
#[tauri::command]
pub async fn sidecar_update_patch_message(
    state: State<'_, AppState>,
    session_id: String,
    patch_id: u32,
    new_message: String,
) -> Result<StagedPatch, String> {
    let sessions_dir = state.sidecar_state.config().sessions_dir();
    let session = Session::load(&sessions_dir, &session_id)
        .await
        .map_err(|e| e.to_string())?;

    let manager = PatchManager::new(session.dir().to_path_buf());
    let updated_patch = manager
        .update_patch_message(patch_id, &new_message)
        .await
        .map_err(|e| e.to_string())?;

    // Emit patch message updated event
    state
        .sidecar_state
        .emit_event(SidecarEvent::PatchMessageUpdated {
            session_id: session_id.clone(),
            patch_id,
            new_subject: updated_patch.subject.clone(),
        });

    Ok(updated_patch)
}

// =============================================================================
// L3: Project Artifacts
// =============================================================================

/// Get all pending artifacts for a session
#[tauri::command]
pub async fn sidecar_get_pending_artifacts(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<ArtifactFile>, String> {
    let sessions_dir = state.sidecar_state.config().sessions_dir();
    let session = Session::load(&sessions_dir, &session_id)
        .await
        .map_err(|e| e.to_string())?;

    let manager = ArtifactManager::new(session.dir().to_path_buf());
    manager.list_pending().await.map_err(|e| e.to_string())
}

/// Get all applied artifacts for a session
#[tauri::command]
pub async fn sidecar_get_applied_artifacts(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<ArtifactFile>, String> {
    let sessions_dir = state.sidecar_state.config().sessions_dir();
    let session = Session::load(&sessions_dir, &session_id)
        .await
        .map_err(|e| e.to_string())?;

    let manager = ArtifactManager::new(session.dir().to_path_buf());
    manager.list_applied().await.map_err(|e| e.to_string())
}

/// Get a specific pending artifact by filename
#[tauri::command]
pub async fn sidecar_get_artifact(
    state: State<'_, AppState>,
    session_id: String,
    filename: String,
) -> Result<Option<ArtifactFile>, String> {
    let sessions_dir = state.sidecar_state.config().sessions_dir();
    let session = Session::load(&sessions_dir, &session_id)
        .await
        .map_err(|e| e.to_string())?;

    let manager = ArtifactManager::new(session.dir().to_path_buf());
    manager
        .get_pending(&filename)
        .await
        .map_err(|e| e.to_string())
}

/// Discard a pending artifact
#[tauri::command]
pub async fn sidecar_discard_artifact(
    state: State<'_, AppState>,
    session_id: String,
    filename: String,
) -> Result<bool, String> {
    let sessions_dir = state.sidecar_state.config().sessions_dir();
    let session = Session::load(&sessions_dir, &session_id)
        .await
        .map_err(|e| e.to_string())?;

    let manager = ArtifactManager::new(session.dir().to_path_buf());
    let discarded = manager
        .discard_artifact(&filename)
        .await
        .map_err(|e| e.to_string())?;

    // Emit artifact discarded event if successful
    if discarded {
        state
            .sidecar_state
            .emit_event(SidecarEvent::ArtifactDiscarded {
                session_id: session_id.clone(),
                filename: filename.clone(),
            });
    }

    Ok(discarded)
}

/// Preview an artifact (show diff against current target file)
#[tauri::command]
pub async fn sidecar_preview_artifact(
    state: State<'_, AppState>,
    session_id: String,
    filename: String,
) -> Result<String, String> {
    let sessions_dir = state.sidecar_state.config().sessions_dir();
    let session = Session::load(&sessions_dir, &session_id)
        .await
        .map_err(|e| e.to_string())?;

    let manager = ArtifactManager::new(session.dir().to_path_buf());
    manager
        .preview_artifact(&filename)
        .await
        .map_err(|e| e.to_string())
}

/// Get pending artifacts for the current session
#[tauri::command]
pub async fn sidecar_get_current_pending_artifacts(
    state: State<'_, AppState>,
) -> Result<Vec<ArtifactFile>, String> {
    let session_id = state
        .sidecar_state
        .current_session_id()
        .ok_or_else(|| "No active session".to_string())?;

    let sessions_dir = state.sidecar_state.config().sessions_dir();
    let session = Session::load(&sessions_dir, &session_id)
        .await
        .map_err(|e| e.to_string())?;

    let manager = ArtifactManager::new(session.dir().to_path_buf());
    manager.list_pending().await.map_err(|e| e.to_string())
}

/// Apply a pending artifact (copy to target, git add, move to applied)
#[tauri::command]
pub async fn sidecar_apply_artifact(
    state: State<'_, AppState>,
    session_id: String,
    filename: String,
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

    let manager = ArtifactManager::new(session.dir().to_path_buf());
    let target_path = manager
        .apply_artifact(&filename, &git_root)
        .await
        .map_err(|e| e.to_string())?;

    // Emit artifact applied event
    state
        .sidecar_state
        .emit_event(SidecarEvent::ArtifactApplied {
            session_id: session_id.clone(),
            filename: filename.clone(),
            target: target_path.display().to_string(),
        });

    Ok(target_path.display().to_string())
}

/// Apply all pending artifacts
#[tauri::command]
pub async fn sidecar_apply_all_artifacts(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<(String, String)>, String> {
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

    let manager = ArtifactManager::new(session.dir().to_path_buf());
    let results = manager
        .apply_all_artifacts(&git_root)
        .await
        .map_err(|e| e.to_string())?;

    // Emit artifact applied events for each applied artifact
    for (filename, path) in &results {
        state
            .sidecar_state
            .emit_event(SidecarEvent::ArtifactApplied {
                session_id: session_id.clone(),
                filename: filename.clone(),
                target: path.display().to_string(),
            });
    }

    // Convert PathBuf to String for serialization
    Ok(results
        .into_iter()
        .map(|(filename, path)| (filename, path.display().to_string()))
        .collect())
}

/// Regenerate artifacts using LLM synthesis
///
/// Triggers artifact regeneration for README.md and CLAUDE.md based on
/// applied patches and session context. Uses the configured synthesis backend.
///
/// # Arguments
/// * `session_id` - The session to regenerate artifacts for
/// * `backend_override` - Optional backend override (uses config default if None)
#[tauri::command]
pub async fn sidecar_regenerate_artifacts(
    state: State<'_, AppState>,
    session_id: String,
    backend_override: Option<String>,
) -> Result<Vec<String>, String> {
    use super::artifacts::{ArtifactSynthesisBackend, ArtifactSynthesisConfig};
    use super::commits::PatchManager;

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

    // Get session context
    let session_context = session.read_state().await.unwrap_or_default();

    // Get patch subjects from applied patches
    let patch_manager = PatchManager::new(session.dir().to_path_buf());
    let applied_patches = patch_manager
        .list_applied()
        .await
        .map_err(|e| e.to_string())?;
    let patch_subjects: Vec<String> = applied_patches.iter().map(|p| p.subject.clone()).collect();

    // Build synthesis config
    let settings = state.settings_manager.get().await;
    let mut config = ArtifactSynthesisConfig::from_sidecar_settings(&settings.sidecar);

    // Apply backend override if provided
    if let Some(backend_str) = backend_override {
        config.backend = backend_str
            .parse::<ArtifactSynthesisBackend>()
            .map_err(|e| e.to_string())?;
    }

    // Regenerate artifacts
    let artifact_manager = ArtifactManager::new(session.dir().to_path_buf());
    let created = artifact_manager
        .regenerate_from_patches_with_config(&git_root, &patch_subjects, &session_context, &config)
        .await
        .map_err(|e| e.to_string())?;

    // Emit artifact created events for each new artifact
    // Load pending artifacts to get their metadata
    let pending_artifacts = artifact_manager.list_pending().await.unwrap_or_default();
    for artifact in &pending_artifacts {
        state
            .sidecar_state
            .emit_event(SidecarEvent::ArtifactCreated {
                session_id: session_id.clone(),
                filename: artifact.filename.clone(),
                target: artifact.meta.target.display().to_string(),
            });
    }

    tracing::info!(
        "Regenerated {} artifacts for session {} using {} backend",
        created.len(),
        session_id,
        config.backend
    );

    // Return the created artifact paths as strings
    Ok(created
        .into_iter()
        .map(|p| p.display().to_string())
        .collect())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    // Commands are tested via integration tests
}

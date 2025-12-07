//! Application state for Tauri commands.
//!
//! This module is only compiled when the `tauri` feature is enabled.

#![cfg(feature = "tauri")]

use std::sync::Arc;

use crate::ai::commands::WorkflowState;
use crate::ai::AiState;
use crate::indexer::IndexerState;
use crate::pty::PtyManager;
use crate::settings::SettingsManager;
use crate::sidecar::SidecarState;
use crate::tavily::TavilyState;

pub struct AppState {
    pub pty_manager: Arc<PtyManager>,
    pub ai_state: AiState,
    pub workflow_state: Arc<WorkflowState>,
    pub indexer_state: Arc<IndexerState>,
    pub tavily_state: Arc<TavilyState>,
    pub settings_manager: Arc<SettingsManager>,
    pub sidecar_state: Arc<SidecarState>,
}

impl AppState {
    /// Create a new AppState with all subsystems initialized.
    ///
    /// This is async because SettingsManager needs to load from disk.
    pub async fn new() -> Self {
        // Initialize settings manager first (needed by TavilyState in the future)
        let settings_manager = Arc::new(
            SettingsManager::new()
                .await
                .expect("Failed to initialize settings manager"),
        );

        // Ensure settings file exists (creates template on first run)
        if let Err(e) = settings_manager.ensure_settings_file().await {
            tracing::warn!("Failed to create settings template: {}", e);
        }

        Self {
            pty_manager: Arc::new(PtyManager::new()),
            ai_state: AiState::new(),
            workflow_state: Arc::new(WorkflowState::new()),
            indexer_state: Arc::new(IndexerState::new()),
            tavily_state: Arc::new(TavilyState::new()),
            settings_manager,
            sidecar_state: Arc::new(SidecarState::new()),
        }
    }
}

use std::sync::Arc;

use crate::ai::commands::WorkflowState;
use crate::ai::AiState;
use crate::indexer::IndexerState;
use crate::pty::PtyManager;
use crate::tavily::TavilyState;

pub struct AppState {
    pub pty_manager: Arc<PtyManager>,
    pub ai_state: AiState,
    pub workflow_state: Arc<WorkflowState>,
    pub indexer_state: Arc<IndexerState>,
    pub tavily_state: Arc<TavilyState>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            pty_manager: Arc::new(PtyManager::new()),
            ai_state: AiState::new(),
            workflow_state: Arc::new(WorkflowState::new()),
            indexer_state: Arc::new(IndexerState::new()),
            tavily_state: Arc::new(TavilyState::new()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

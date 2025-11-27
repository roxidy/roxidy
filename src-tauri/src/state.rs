use crate::ai::AiState;
use crate::pty::PtyManager;

pub struct AppState {
    pub pty_manager: PtyManager,
    pub ai_state: AiState,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            pty_manager: PtyManager::new(),
            ai_state: AiState::new(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

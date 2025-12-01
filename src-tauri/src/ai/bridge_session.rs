//! Session persistence extension for AgentBridge.
//!
//! This module contains methods for managing conversation session persistence.

use std::path::PathBuf;

use rig::completion::Message;

use super::agent_bridge::AgentBridge;
use super::session::QbitSessionManager;

impl AgentBridge {
    // ========================================================================
    // Session Persistence Methods
    // ========================================================================

    /// Enable or disable session persistence.
    pub async fn set_session_persistence_enabled(&self, enabled: bool) {
        *self.session_persistence_enabled.write().await = enabled;
        tracing::debug!("Session persistence enabled: {}", enabled);
    }

    /// Check if session persistence is enabled.
    pub async fn is_session_persistence_enabled(&self) -> bool {
        *self.session_persistence_enabled.read().await
    }

    /// Start a new session for persistence.
    pub(crate) async fn start_session(&self) {
        if !*self.session_persistence_enabled.read().await {
            return;
        }

        let mut manager_guard = self.session_manager.write().await;
        if manager_guard.is_some() {
            return;
        }

        let workspace = self.workspace.read().await.clone();
        match QbitSessionManager::new(workspace, &self.model_name, &self.provider_name).await {
            Ok(manager) => {
                *manager_guard = Some(manager);
                tracing::debug!("Session started for persistence");
            }
            Err(e) => {
                tracing::warn!("Failed to start session for persistence: {}", e);
            }
        }
    }

    /// Execute an operation with the session manager if available.
    async fn with_session_manager<F>(&self, f: F)
    where
        F: FnOnce(&mut QbitSessionManager),
    {
        let mut guard = self.session_manager.write().await;
        if let Some(ref mut manager) = *guard {
            f(manager);
        }
    }

    /// Record a user message in the current session.
    pub(crate) async fn record_user_message(&self, content: &str) {
        self.with_session_manager(|m| m.add_user_message(content))
            .await;
    }

    /// Record an assistant message in the current session.
    pub(crate) async fn record_assistant_message(&self, content: &str) {
        self.with_session_manager(|m| m.add_assistant_message(content))
            .await;
    }

    /// Record a tool use in the current session.
    pub(crate) async fn record_tool_use(&self, tool_name: &str, result: &str) {
        self.with_session_manager(|m| m.add_tool_use(tool_name, result))
            .await;
    }

    /// Save the current session to disk.
    pub(crate) async fn save_session(&self) {
        let manager_guard = self.session_manager.read().await;
        if let Some(ref manager) = *manager_guard {
            match manager.save() {
                Ok(path) => {
                    tracing::debug!("Session saved to: {}", path.display());
                }
                Err(e) => {
                    tracing::warn!("Failed to save session: {}", e);
                }
            }
        }
    }

    /// Finalize and save the current session.
    pub async fn finalize_session(&self) -> Option<PathBuf> {
        let mut manager_guard = self.session_manager.write().await;
        if let Some(ref mut manager) = manager_guard.take() {
            match manager.finalize() {
                Ok(path) => {
                    tracing::info!("Session finalized: {}", path.display());
                    return Some(path);
                }
                Err(e) => {
                    tracing::warn!("Failed to finalize session: {}", e);
                }
            }
        }
        None
    }

    // ========================================================================
    // Conversation History Methods
    // ========================================================================

    /// Clear the conversation history.
    pub async fn clear_conversation_history(&self) {
        self.finalize_session().await;

        let mut history = self.conversation_history.write().await;
        history.clear();
        tracing::debug!("Conversation history cleared");
    }

    /// Get the current conversation history length.
    pub async fn conversation_history_len(&self) -> usize {
        self.conversation_history.read().await.len()
    }

    /// Restore conversation history from a previous session.
    pub async fn restore_session(&self, messages: Vec<super::session::QbitSessionMessage>) {
        self.finalize_session().await;

        let rig_messages: Vec<Message> =
            messages.iter().filter_map(|m| m.to_rig_message()).collect();

        let mut history = self.conversation_history.write().await;
        *history = rig_messages;

        tracing::info!(
            "Restored session with {} messages ({} in history)",
            messages.len(),
            history.len()
        );
    }
}

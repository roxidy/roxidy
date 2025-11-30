//! Context management extension for AgentBridge.
//!
//! This module contains methods for managing context window and token budgeting.

use std::sync::Arc;

use super::agent_bridge::AgentBridge;
use super::context_manager::{ContextManager, ContextSummary, ContextTrimConfig};
use super::token_budget::{TokenAlertLevel, TokenUsageStats};

impl AgentBridge {
    // ========================================================================
    // Context Management Methods
    // ========================================================================

    /// Get the context manager reference.
    #[allow(dead_code)]
    pub fn context_manager(&self) -> Arc<ContextManager> {
        Arc::clone(&self.context_manager)
    }

    /// Get current context summary.
    pub async fn get_context_summary(&self) -> ContextSummary {
        self.context_manager.get_summary().await
    }

    /// Get current token usage statistics.
    pub async fn get_token_usage_stats(&self) -> TokenUsageStats {
        self.context_manager.stats().await
    }

    /// Get current token alert level.
    pub async fn get_token_alert_level(&self) -> TokenAlertLevel {
        self.context_manager.alert_level().await
    }

    /// Get context utilization percentage.
    pub async fn get_context_utilization(&self) -> f64 {
        self.context_manager.utilization().await
    }

    /// Get remaining available tokens.
    pub async fn get_remaining_tokens(&self) -> usize {
        self.context_manager.remaining_tokens().await
    }

    /// Update token budget from current conversation history.
    #[allow(dead_code)]
    pub async fn update_context_from_history(&self) {
        let history = self.conversation_history.read().await;
        self.context_manager.update_from_messages(&history).await;
    }

    /// Enforce context window limits by pruning old messages if needed.
    pub async fn enforce_context_window(&self) -> usize {
        let mut history = self.conversation_history.write().await;
        let original_len = history.len();
        let pruned = self.context_manager.enforce_context_window(&history).await;
        let pruned_count = original_len.saturating_sub(pruned.len());
        *history = pruned;
        pruned_count
    }

    /// Reset the context manager.
    pub async fn reset_context_manager(&self) {
        self.context_manager.reset().await;
    }

    /// Get the context trim configuration.
    pub fn get_context_trim_config(&self) -> ContextTrimConfig {
        self.context_manager.trim_config().clone()
    }

    /// Check if context management is enabled.
    pub fn is_context_management_enabled(&self) -> bool {
        self.context_manager.is_enabled()
    }

    /// Truncate a tool response if it exceeds limits.
    #[allow(dead_code)]
    pub async fn truncate_tool_response(&self, content: &str, tool_name: &str) -> String {
        let result = self
            .context_manager
            .truncate_tool_response(content, tool_name)
            .await;
        result.content
    }
}

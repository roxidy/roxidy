//! Context management orchestration
//!
//! Coordinates token budgeting, context pruning, and truncation strategies.

use rig::message::Message;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{
    context_pruner::{ContextPruner, ContextPrunerConfig, PruneResult, SemanticScore},
    token_budget::{TokenAlertLevel, TokenBudgetConfig, TokenBudgetManager, TokenUsageStats},
    token_trunc::{aggregate_tool_output, truncate_by_tokens, TruncationResult},
};

/// Configuration for context trimming behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextTrimConfig {
    /// Enable automatic context trimming
    pub enabled: bool,
    /// Target utilization ratio (0.0-1.0) when trimming
    pub target_utilization: f64,
    /// Enable aggressive trimming when critically low on space
    pub aggressive_on_critical: bool,
    /// Maximum tool response tokens before truncation
    pub max_tool_response_tokens: usize,
    /// Enable semantic-aware pruning
    pub semantic_pruning: bool,
}

impl Default for ContextTrimConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            target_utilization: 0.7,
            aggressive_on_critical: true,
            max_tool_response_tokens: 25_000,
            semantic_pruning: true,
        }
    }
}

/// Efficiency metrics after context operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEfficiency {
    /// Utilization before operation
    pub utilization_before: f64,
    /// Utilization after operation
    pub utilization_after: f64,
    /// Tokens freed
    pub tokens_freed: usize,
    /// Messages pruned
    pub messages_pruned: usize,
    /// Tool responses truncated
    pub tool_responses_truncated: usize,
    /// Timestamp of operation
    pub timestamp: u64,
}

/// Events emitted during context management
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContextEvent {
    /// Warning threshold exceeded
    WarningThreshold {
        utilization: f64,
        total_tokens: usize,
        max_tokens: usize,
    },
    /// Alert threshold exceeded
    AlertThreshold {
        utilization: f64,
        total_tokens: usize,
        max_tokens: usize,
    },
    /// Context was pruned
    ContextPruned {
        messages_removed: usize,
        tokens_freed: usize,
        utilization_after: f64,
    },
    /// Tool response was truncated
    ToolResponseTruncated {
        original_tokens: usize,
        truncated_tokens: usize,
        tool_name: String,
    },
    /// Context window exceeded (critical)
    ContextExceeded {
        total_tokens: usize,
        max_tokens: usize,
    },
}

/// Central manager for context window management
#[derive(Debug)]
pub struct ContextManager {
    /// Token budget manager
    token_budget: Arc<TokenBudgetManager>,
    /// Context pruner
    pruner: Arc<RwLock<ContextPruner>>,
    /// Trim configuration
    trim_config: ContextTrimConfig,
    /// Whether token budgeting is enabled
    token_budget_enabled: bool,
    /// Last recorded efficiency metrics
    last_efficiency: Arc<RwLock<Option<ContextEfficiency>>>,
    /// Event channel for notifications
    event_tx: Option<tokio::sync::mpsc::Sender<ContextEvent>>,
}

impl ContextManager {
    /// Create a new context manager
    pub fn new(budget_config: TokenBudgetConfig, trim_config: ContextTrimConfig) -> Self {
        let pruner_config = ContextPrunerConfig {
            max_tokens: budget_config.available_tokens(),
            ..Default::default()
        };

        Self {
            token_budget: Arc::new(TokenBudgetManager::new(budget_config)),
            pruner: Arc::new(RwLock::new(ContextPruner::new(pruner_config))),
            trim_config,
            token_budget_enabled: true,
            last_efficiency: Arc::new(RwLock::new(None)),
            event_tx: None,
        }
    }

    /// Create with default configuration for a model
    pub fn for_model(model: &str) -> Self {
        Self::new(
            TokenBudgetConfig::for_model(model),
            ContextTrimConfig::default(),
        )
    }

    /// Set event channel for notifications
    pub fn set_event_channel(&mut self, tx: tokio::sync::mpsc::Sender<ContextEvent>) {
        self.event_tx = Some(tx);
    }

    /// Get reference to token budget manager
    pub fn token_budget(&self) -> Arc<TokenBudgetManager> {
        Arc::clone(&self.token_budget)
    }

    /// Get current trim configuration
    pub fn trim_config(&self) -> &ContextTrimConfig {
        &self.trim_config
    }

    /// Update trim configuration
    pub fn set_trim_config(&mut self, config: ContextTrimConfig) {
        self.trim_config = config;
    }

    /// Check if token budgeting is enabled
    pub fn is_enabled(&self) -> bool {
        self.token_budget_enabled
    }

    /// Enable/disable token budgeting
    pub fn set_enabled(&mut self, enabled: bool) {
        self.token_budget_enabled = enabled;
    }

    /// Get current token usage stats
    pub async fn stats(&self) -> TokenUsageStats {
        self.token_budget.stats().await
    }

    /// Get current alert level
    pub async fn alert_level(&self) -> TokenAlertLevel {
        self.token_budget.alert_level().await
    }

    /// Get utilization percentage
    pub async fn utilization(&self) -> f64 {
        self.token_budget.usage_percentage().await
    }

    /// Get remaining tokens
    pub async fn remaining_tokens(&self) -> usize {
        self.token_budget.remaining_tokens().await
    }

    /// Get last efficiency metrics
    pub async fn last_efficiency(&self) -> Option<ContextEfficiency> {
        self.last_efficiency.read().await.clone()
    }

    /// Reset token budget
    pub async fn reset(&self) {
        self.token_budget.reset().await;
        *self.last_efficiency.write().await = None;
    }

    /// Update budget from message history
    pub async fn update_from_messages(&self, messages: &[Message]) {
        let mut stats = TokenUsageStats::new();

        for message in messages {
            let tokens = TokenBudgetManager::estimate_tokens(&message_to_text(message));
            match message {
                Message::User { content } => {
                    // Check if this contains tool results
                    let has_tool_result = content
                        .iter()
                        .any(|c| matches!(c, rig::message::UserContent::ToolResult(_)));
                    if has_tool_result {
                        stats.tool_results_tokens += tokens;
                    } else {
                        stats.user_messages_tokens += tokens;
                    }
                }
                Message::Assistant { .. } => stats.assistant_messages_tokens += tokens,
            }
        }

        stats.total_tokens = stats.system_prompt_tokens
            + stats.user_messages_tokens
            + stats.assistant_messages_tokens
            + stats.tool_results_tokens;

        self.token_budget.set_stats(stats).await;

        // Check thresholds and emit events
        self.check_and_emit_alerts().await;
    }

    /// Check thresholds and emit alert events
    async fn check_and_emit_alerts(&self) {
        if let Some(ref tx) = self.event_tx {
            let alert_level = self.token_budget.alert_level().await;
            let stats = self.token_budget.stats().await;
            let utilization = self.token_budget.usage_percentage().await;
            let max_tokens = self.token_budget.config().max_context_tokens;

            let event = match alert_level {
                TokenAlertLevel::Critical => Some(ContextEvent::ContextExceeded {
                    total_tokens: stats.total_tokens,
                    max_tokens,
                }),
                TokenAlertLevel::Alert => Some(ContextEvent::AlertThreshold {
                    utilization,
                    total_tokens: stats.total_tokens,
                    max_tokens,
                }),
                TokenAlertLevel::Warning => Some(ContextEvent::WarningThreshold {
                    utilization,
                    total_tokens: stats.total_tokens,
                    max_tokens,
                }),
                TokenAlertLevel::Normal => None,
            };

            if let Some(event) = event {
                let _ = tx.send(event).await;
            }
        }
    }

    /// Enforce context window by pruning if necessary
    pub async fn enforce_context_window(&self, messages: &[Message]) -> Vec<Message> {
        if !self.token_budget_enabled || !self.trim_config.enabled {
            return messages.to_vec();
        }

        let utilization_before = self.token_budget.usage_percentage().await;
        let alert_level = self.token_budget.alert_level().await;

        // Determine if we need to prune
        let should_prune = matches!(
            alert_level,
            TokenAlertLevel::Alert | TokenAlertLevel::Critical
        );

        if !should_prune {
            return messages.to_vec();
        }

        // Calculate target tokens
        let target_utilization = if matches!(alert_level, TokenAlertLevel::Critical)
            && self.trim_config.aggressive_on_critical
        {
            self.trim_config.target_utilization * 0.8
        } else {
            self.trim_config.target_utilization
        };

        let target_tokens =
            (self.token_budget.config().available_tokens() as f64 * target_utilization) as usize;

        // Enable aggressive mode if critical
        {
            let mut pruner = self.pruner.write().await;
            pruner.set_aggressive(
                matches!(alert_level, TokenAlertLevel::Critical)
                    && self.trim_config.aggressive_on_critical,
            );
        }

        // Prune messages
        let pruner = self.pruner.read().await;
        let result = pruner.prune_messages(messages, target_tokens);

        if !result.pruned {
            return messages.to_vec();
        }

        // Apply pruning
        let kept_messages: Vec<Message> = result
            .kept_indices
            .iter()
            .filter_map(|&i| messages.get(i).cloned())
            .collect();

        // Update stats
        self.update_from_messages(&kept_messages).await;
        let utilization_after = self.token_budget.usage_percentage().await;

        // Record efficiency
        let efficiency = ContextEfficiency {
            utilization_before,
            utilization_after,
            tokens_freed: result.pruned_tokens,
            messages_pruned: result.pruned_indices.len(),
            tool_responses_truncated: 0,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        *self.last_efficiency.write().await = Some(efficiency);

        // Emit event
        if let Some(ref tx) = self.event_tx {
            let _ = tx
                .send(ContextEvent::ContextPruned {
                    messages_removed: result.pruned_indices.len(),
                    tokens_freed: result.pruned_tokens,
                    utilization_after,
                })
                .await;
        }

        tracing::info!(
            "Context pruned: {} messages removed, {} tokens freed, utilization {:.1}% -> {:.1}%",
            result.pruned_indices.len(),
            result.pruned_tokens,
            utilization_before * 100.0,
            utilization_after * 100.0
        );

        kept_messages
    }

    /// Truncate tool response if it exceeds limits
    pub async fn truncate_tool_response(&self, content: &str, tool_name: &str) -> TruncationResult {
        let result = aggregate_tool_output(content, self.trim_config.max_tool_response_tokens);

        if result.truncated {
            // Emit event
            if let Some(ref tx) = self.event_tx {
                let _ = tx
                    .send(ContextEvent::ToolResponseTruncated {
                        original_tokens: TokenBudgetManager::estimate_tokens(content),
                        truncated_tokens: TokenBudgetManager::estimate_tokens(&result.content),
                        tool_name: tool_name.to_string(),
                    })
                    .await;
            }

            tracing::debug!(
                "Tool response '{}' truncated: {} -> {} tokens",
                tool_name,
                TokenBudgetManager::estimate_tokens(content),
                TokenBudgetManager::estimate_tokens(&result.content)
            );
        }

        result
    }

    /// Check if there's room for a new message
    pub async fn can_add_message(&self, estimated_tokens: usize) -> bool {
        !self
            .token_budget
            .would_exceed_budget(estimated_tokens)
            .await
    }

    /// Get prune result without applying it
    pub async fn preview_prune(&self, messages: &[Message], target_tokens: usize) -> PruneResult {
        let pruner = self.pruner.read().await;
        pruner.prune_messages(messages, target_tokens)
    }

    /// Score a message's semantic importance
    pub async fn score_message(&self, message: &Message) -> SemanticScore {
        let pruner = self.pruner.read().await;
        pruner.score_message(message)
    }

    /// Get context summary for diagnostics
    pub async fn get_summary(&self) -> ContextSummary {
        let stats = self.token_budget.stats().await;
        let config = self.token_budget.config();

        ContextSummary {
            total_tokens: stats.total_tokens,
            max_tokens: config.max_context_tokens,
            available_tokens: config.available_tokens(),
            utilization: self.token_budget.usage_percentage().await,
            alert_level: self.token_budget.alert_level().await,
            system_prompt_tokens: stats.system_prompt_tokens,
            user_messages_tokens: stats.user_messages_tokens,
            assistant_messages_tokens: stats.assistant_messages_tokens,
            tool_results_tokens: stats.tool_results_tokens,
            warning_threshold: config.warning_threshold,
            alert_threshold: config.alert_threshold,
        }
    }
}

/// Summary of current context state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSummary {
    pub total_tokens: usize,
    pub max_tokens: usize,
    pub available_tokens: usize,
    pub utilization: f64,
    pub alert_level: TokenAlertLevel,
    pub system_prompt_tokens: usize,
    pub user_messages_tokens: usize,
    pub assistant_messages_tokens: usize,
    pub tool_results_tokens: usize,
    pub warning_threshold: f64,
    pub alert_threshold: f64,
}

/// Convert message to text for token estimation
fn message_to_text(message: &Message) -> String {
    use rig::completion::AssistantContent;
    use rig::message::UserContent;

    match message {
        Message::User { content } => content
            .iter()
            .map(|c| match c {
                UserContent::Text(t) => t.text.clone(),
                UserContent::Image(_) => "[image]".to_string(),
                UserContent::Document(_) => "[document]".to_string(),
                UserContent::ToolResult(result) => result
                    .content
                    .iter()
                    .map(|tc| format!("{:?}", tc))
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => "[media]".to_string(), // Audio, Video, etc.
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Message::Assistant { content, .. } => content
            .iter()
            .map(|c| match c {
                AssistantContent::Text(t) => t.text.clone(),
                AssistantContent::ToolCall(call) => {
                    format!("[tool: {}]", call.function.name)
                }
                _ => "[reasoning]".to_string(), // Reasoning, etc.
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::message::Text;
    use rig::one_or_many::OneOrMany;

    fn create_user_message(text: &str) -> Message {
        Message::User {
            content: OneOrMany::one(rig::message::UserContent::Text(Text {
                text: text.to_string(),
            })),
        }
    }

    #[tokio::test]
    async fn test_context_manager_creation() {
        let manager = ContextManager::for_model("claude-3-5-sonnet");
        assert!(manager.is_enabled());
        assert_eq!(manager.alert_level().await, TokenAlertLevel::Normal);
    }

    #[tokio::test]
    async fn test_update_from_messages() {
        let manager = ContextManager::for_model("claude-3-5-sonnet");
        let messages = vec![
            create_user_message("Hello, how are you?"),
            create_user_message("I need help with something."),
        ];

        manager.update_from_messages(&messages).await;
        let stats = manager.stats().await;
        assert!(stats.user_messages_tokens > 0);
    }

    #[tokio::test]
    async fn test_tool_response_truncation() {
        let manager = ContextManager::new(
            TokenBudgetConfig::default(),
            ContextTrimConfig {
                max_tool_response_tokens: 10, // Very small for testing
                ..Default::default()
            },
        );

        // Create long content that exceeds MIN_TRUNCATION_LENGTH (100 chars) and is much larger than 10 tokens
        let long_content = "This is a very long tool response that contains a lot of text. \
            We need to ensure that it exceeds the minimum truncation length of 100 characters. \
            This additional text should push us well over that threshold and trigger actual truncation.";
        let result = manager
            .truncate_tool_response(long_content, "test_tool")
            .await;

        // With only 10 max tokens (~40 chars), this should be truncated
        assert!(result.truncated);
        assert!(result.result_chars < long_content.len());
    }

    #[tokio::test]
    async fn test_context_summary() {
        let manager = ContextManager::for_model("claude-3-5-sonnet");
        let summary = manager.get_summary().await;

        assert!(summary.max_tokens > 0);
        assert_eq!(summary.utilization, 0.0);
        assert_eq!(summary.alert_level, TokenAlertLevel::Normal);
    }
}

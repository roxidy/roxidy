//! Context pruning for intelligent conversation trimming
//!
//! Implements semantic-aware message pruning based on VTCode's design.

use rig::completion::{AssistantContent, Message};
use rig::message::UserContent;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::token_budget::TokenBudgetManager;

/// Semantic importance scores for different message types
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SemanticScore(pub u32);

impl SemanticScore {
    /// System messages - highest priority (never pruned)
    pub const SYSTEM: SemanticScore = SemanticScore(950);
    /// User queries - high importance
    pub const USER_QUERY: SemanticScore = SemanticScore(850);
    /// Tool responses - medium importance
    pub const TOOL_RESPONSE: SemanticScore = SemanticScore(600);
    /// Assistant responses - moderate importance
    pub const ASSISTANT: SemanticScore = SemanticScore(500);
    /// Context/filler messages - lower importance
    pub const CONTEXT: SemanticScore = SemanticScore(300);
    /// Minimum value
    pub const MIN: SemanticScore = SemanticScore(0);
    /// Maximum value
    pub const MAX: SemanticScore = SemanticScore(1000);

    /// Create a new score (clamped to 0-1000)
    pub fn new(score: u32) -> Self {
        Self(score.min(1000))
    }

    /// Convert to ratio (0.0 - 1.0)
    pub fn as_ratio(&self) -> f64 {
        self.0 as f64 / 1000.0
    }

    /// Get the raw score value
    pub fn value(&self) -> u32 {
        self.0
    }
}

impl Default for SemanticScore {
    fn default() -> Self {
        Self::CONTEXT
    }
}

/// A message with associated metadata for pruning decisions
#[derive(Debug, Clone)]
pub struct ScoredMessage {
    /// Original message index in conversation
    pub index: usize,
    /// The message content
    pub message: Message,
    /// Semantic importance score
    pub semantic_score: SemanticScore,
    /// Estimated token count
    pub token_count: usize,
    /// Adjusted priority (semantic + recency)
    pub priority: f64,
    /// Whether this message should always be kept
    pub protected: bool,
}

impl ScoredMessage {
    /// Create a new scored message
    pub fn new(index: usize, message: Message, token_count: usize) -> Self {
        let semantic_score = Self::compute_semantic_score(&message);
        let protected = semantic_score.0 >= SemanticScore::SYSTEM.0;

        Self {
            index,
            message,
            semantic_score,
            token_count,
            priority: semantic_score.as_ratio(),
            protected,
        }
    }

    /// Compute semantic score based on message type and content
    fn compute_semantic_score(message: &Message) -> SemanticScore {
        match message {
            Message::User { content } => {
                // Check if this is a tool result
                for item in content.iter() {
                    if matches!(item, UserContent::ToolResult(_)) {
                        return SemanticScore::TOOL_RESPONSE;
                    }
                }
                SemanticScore::USER_QUERY
            }
            Message::Assistant { content, .. } => {
                // Check if this contains tool calls (higher importance)
                for item in content.iter() {
                    if matches!(item, AssistantContent::ToolCall(_)) {
                        return SemanticScore::new(650); // Tool calls slightly higher
                    }
                }
                SemanticScore::ASSISTANT
            }
        }
    }

    /// Update priority with recency bonus
    pub fn apply_recency_bonus(&mut self, turns_from_end: usize, bonus_per_turn: f64) {
        let recency = 1.0 / (1.0 + turns_from_end as f64 * 0.1);
        self.priority = self.semantic_score.as_ratio() * 0.6
            + recency * 0.3
            + (1.0 - self.token_count as f64 / 500.0).max(0.0) * 0.1;

        // Add explicit recency bonus
        self.priority += recency * bonus_per_turn;
    }
}

/// Configuration for context pruning behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPrunerConfig {
    /// Maximum tokens to allow in context
    pub max_tokens: usize,
    /// Minimum semantic score to always keep (0-1000)
    pub semantic_threshold: u32,
    /// Bonus added per turn for recent messages
    pub recency_bonus_per_turn: f64,
    /// Minimum score for messages that should never be pruned
    pub min_keep_semantic: u32,
    /// Number of recent turns to always protect
    pub protected_recent_turns: usize,
    /// Whether to enable aggressive mode (removes more)
    pub aggressive: bool,
}

impl Default for ContextPrunerConfig {
    fn default() -> Self {
        Self {
            max_tokens: 8192,
            semantic_threshold: 300,
            recency_bonus_per_turn: 0.05,
            min_keep_semantic: 400,
            protected_recent_turns: 2,
            aggressive: false,
        }
    }
}

/// Result of a pruning operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruneResult {
    /// Messages to keep (in original order)
    pub kept_indices: Vec<usize>,
    /// Messages that were pruned
    pub pruned_indices: Vec<usize>,
    /// Total tokens in kept messages
    pub kept_tokens: usize,
    /// Total tokens that were pruned
    pub pruned_tokens: usize,
    /// Whether pruning was needed at all
    pub pruned: bool,
}

/// Context pruner for intelligent message trimming
#[derive(Debug)]
pub struct ContextPruner {
    config: ContextPrunerConfig,
}

impl ContextPruner {
    /// Create a new context pruner
    pub fn new(config: ContextPrunerConfig) -> Self {
        Self { config }
    }

    /// Create with default configuration
    pub fn with_max_tokens(max_tokens: usize) -> Self {
        Self::new(ContextPrunerConfig {
            max_tokens,
            ..Default::default()
        })
    }

    /// Get the current configuration
    pub fn config(&self) -> &ContextPrunerConfig {
        &self.config
    }

    /// Update configuration
    pub fn set_config(&mut self, config: ContextPrunerConfig) {
        self.config = config;
    }

    /// Set aggressive mode
    pub fn set_aggressive(&mut self, aggressive: bool) {
        self.config.aggressive = aggressive;
    }

    /// Prune messages to fit within token budget
    pub fn prune_messages(&self, messages: &[Message], budget: usize) -> PruneResult {
        if messages.is_empty() {
            return PruneResult {
                kept_indices: vec![],
                pruned_indices: vec![],
                kept_tokens: 0,
                pruned_tokens: 0,
                pruned: false,
            };
        }

        // Score all messages
        let mut scored: Vec<ScoredMessage> = messages
            .iter()
            .enumerate()
            .map(|(i, msg)| {
                let token_count =
                    TokenBudgetManager::estimate_tokens(&Self::message_to_string(msg));
                ScoredMessage::new(i, msg.clone(), token_count)
            })
            .collect();

        // Calculate total tokens
        let total_tokens: usize = scored.iter().map(|s| s.token_count).sum();

        // If we're under budget, keep everything
        if total_tokens <= budget {
            return PruneResult {
                kept_indices: (0..messages.len()).collect(),
                pruned_indices: vec![],
                kept_tokens: total_tokens,
                pruned_tokens: 0,
                pruned: false,
            };
        }

        // Apply recency bonuses
        let message_count = scored.len();
        for (i, msg) in scored.iter_mut().enumerate() {
            let turns_from_end = message_count - 1 - i;
            msg.apply_recency_bonus(turns_from_end, self.config.recency_bonus_per_turn);

            // Protect recent messages
            if turns_from_end < self.config.protected_recent_turns {
                msg.protected = true;
            }
        }

        // Separate protected and prunable messages
        let (protected, mut prunable): (Vec<_>, Vec<_>) =
            scored.into_iter().partition(|m| m.protected);

        // Calculate tokens from protected messages
        let protected_tokens: usize = protected.iter().map(|m| m.token_count).sum();

        // If protected messages exceed budget, we can't do anything useful
        if protected_tokens >= budget {
            tracing::warn!(
                "Protected messages ({} tokens) exceed budget ({} tokens)",
                protected_tokens,
                budget
            );
            return PruneResult {
                kept_indices: protected.iter().map(|m| m.index).collect(),
                pruned_indices: prunable.iter().map(|m| m.index).collect(),
                kept_tokens: protected_tokens,
                pruned_tokens: prunable.iter().map(|m| m.token_count).sum(),
                pruned: true,
            };
        }

        // Sort prunable by priority (highest first)
        prunable.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Greedily add messages until budget is exhausted
        let remaining_budget = budget - protected_tokens;
        let mut kept_prunable = Vec::new();
        let mut current_tokens = 0;

        for msg in prunable {
            if current_tokens + msg.token_count <= remaining_budget {
                current_tokens += msg.token_count;
                kept_prunable.push(msg);
            }
        }

        // Combine kept messages and sort by original index
        let mut kept: Vec<_> = protected.into_iter().chain(kept_prunable).collect();
        kept.sort_by_key(|m| m.index);

        let kept_indices: Vec<usize> = kept.iter().map(|m| m.index).collect();
        let kept_tokens: usize = kept.iter().map(|m| m.token_count).sum();

        // Determine pruned indices
        let kept_set: HashSet<usize> = kept_indices.iter().copied().collect();
        let pruned_indices: Vec<usize> = (0..messages.len())
            .filter(|i| !kept_set.contains(i))
            .collect();
        let pruned_tokens = total_tokens - kept_tokens;
        let was_pruned = !pruned_indices.is_empty();

        PruneResult {
            kept_indices,
            pruned_indices,
            kept_tokens,
            pruned_tokens,
            pruned: was_pruned,
        }
    }

    /// Apply pruning result to get filtered messages
    pub fn apply_prune_result<'a>(
        &self,
        messages: &'a [Message],
        result: &PruneResult,
    ) -> Vec<&'a Message> {
        result
            .kept_indices
            .iter()
            .filter_map(|&i| messages.get(i))
            .collect()
    }

    /// Convert message to string for token estimation
    fn message_to_string(message: &Message) -> String {
        match message {
            Message::User { content } => {
                content
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
                    .join("\n")
            }
            Message::Assistant { content, .. } => {
                content
                    .iter()
                    .map(|c| match c {
                        AssistantContent::Text(t) => t.text.clone(),
                        AssistantContent::ToolCall(call) => {
                            format!("[tool: {}]", call.function.name)
                        }
                        _ => "[reasoning]".to_string(), // Reasoning, etc.
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
    }

    /// Get semantic score for a message
    pub fn score_message(&self, message: &Message) -> SemanticScore {
        ScoredMessage::compute_semantic_score(message)
    }
}

impl Default for ContextPruner {
    fn default() -> Self {
        Self::new(ContextPrunerConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::message::Text;
    use rig::one_or_many::OneOrMany;

    fn create_user_message(text: &str) -> Message {
        Message::User {
            content: OneOrMany::one(UserContent::Text(Text {
                text: text.to_string(),
            })),
        }
    }

    fn create_assistant_message(text: &str) -> Message {
        Message::Assistant {
            id: None,
            content: OneOrMany::one(AssistantContent::Text(Text {
                text: text.to_string(),
            })),
        }
    }

    #[test]
    fn test_semantic_scoring() {
        let pruner = ContextPruner::default();

        let user = create_user_message("User query");
        let assistant = create_assistant_message("Assistant response");

        assert_eq!(pruner.score_message(&user), SemanticScore::USER_QUERY);
        assert_eq!(pruner.score_message(&assistant), SemanticScore::ASSISTANT);
    }

    #[test]
    fn test_no_pruning_under_budget() {
        let pruner = ContextPruner::with_max_tokens(10000);
        let messages = vec![
            create_user_message("Hello"),
            create_assistant_message("Hi there"),
        ];

        let result = pruner.prune_messages(&messages, 10000);
        assert!(!result.pruned);
        assert_eq!(result.kept_indices.len(), 2);
        assert!(result.pruned_indices.is_empty());
    }

    #[test]
    fn test_pruning_over_budget() {
        let pruner = ContextPruner::new(ContextPrunerConfig {
            protected_recent_turns: 0, // Don't protect recent for this test
            ..Default::default()
        });

        // Create messages where total exceeds budget
        let messages: Vec<Message> = (0..10)
            .map(|i| create_user_message(&format!("Message {} with some content to add tokens", i)))
            .collect();

        // Set a very small budget to force pruning
        let result = pruner.prune_messages(&messages, 50);
        assert!(result.pruned);
        assert!(result.kept_indices.len() < messages.len());
    }

    #[test]
    fn test_user_messages_kept_by_score() {
        let pruner = ContextPruner::new(ContextPrunerConfig {
            protected_recent_turns: 0,
            ..Default::default()
        });

        let messages = vec![
            create_user_message("User message 1"),
            create_user_message("User message 2"),
            create_assistant_message("Assistant message"),
        ];

        // Very small budget - should keep at least one message
        let result = pruner.prune_messages(&messages, 20);
        // At least some messages should be kept
        assert!(!result.kept_indices.is_empty());
    }
}

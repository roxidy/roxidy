//! Request and response types for Anthropic Vertex AI API.

use serde::{Deserialize, Serialize};

/// Anthropic API version for Vertex AI
pub const ANTHROPIC_VERSION: &str = "vertex-2023-10-16";

/// Maximum tokens default
pub const DEFAULT_MAX_TOKENS: u32 = 4096;

/// Configuration for extended thinking (reasoning) mode.
/// When enabled, the model will show its reasoning process before responding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingConfig {
    /// Must be "enabled" to activate extended thinking
    #[serde(rename = "type")]
    pub thinking_type: String,
    /// Token budget for thinking (must be >= 1024)
    pub budget_tokens: u32,
}

impl ThinkingConfig {
    /// Create a new thinking configuration with the specified budget.
    /// Budget must be at least 1024 tokens.
    pub fn new(budget_tokens: u32) -> Self {
        Self {
            thinking_type: "enabled".to_string(),
            budget_tokens: budget_tokens.max(1024),
        }
    }

    /// Create a thinking config with a default budget of 10,000 tokens
    pub fn default_budget() -> Self {
        Self::new(10_000)
    }
}

/// Content block in a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Text content
    Text { text: String },
    /// Image content (base64 encoded)
    Image {
        source: ImageSource,
    },
    /// Tool use request from the model
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Tool result from execution
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    /// Thinking/reasoning content from extended thinking mode
    Thinking {
        thinking: String,
        /// Signature for verification (provided by API)
        signature: String,
    },
}

/// Image source for image content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

/// Role in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

impl Message {
    /// Create a user message with text content
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    /// Create an assistant message with text content
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }
}

/// Tool definition for the API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Request body for the Anthropic Vertex AI API
#[derive(Debug, Clone, Serialize)]
pub struct CompletionRequest {
    /// Anthropic API version
    pub anthropic_version: String,
    /// Messages in the conversation
    pub messages: Vec<Message>,
    /// Maximum tokens to generate
    pub max_tokens: u32,
    /// System prompt (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Temperature for sampling (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p sampling (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Top-k sampling (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Stop sequences (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// Tools available to the model (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    /// Whether to stream the response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Extended thinking configuration (optional)
    /// When enabled, temperature must be 1 and budget_tokens >= 1024
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            anthropic_version: ANTHROPIC_VERSION.to_string(),
            messages: Vec::new(),
            max_tokens: DEFAULT_MAX_TOKENS,
            system: None,
            temperature: None,
            top_p: None,
            top_k: None,
            stop_sequences: None,
            tools: None,
            stream: None,
            thinking: None,
        }
    }
}

/// Usage statistics in the response
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Usage {
    /// Input tokens (may be missing in message_delta events)
    #[serde(default)]
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Stop reason for completion
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    StopSequence,
    ToolUse,
}

/// Response from the Anthropic Vertex AI API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// Unique ID for the response
    pub id: String,
    /// Type of response (always "message")
    #[serde(rename = "type")]
    pub response_type: String,
    /// Role (always "assistant")
    pub role: String,
    /// Content blocks
    pub content: Vec<ContentBlock>,
    /// Model that generated the response
    pub model: String,
    /// Reason the model stopped generating
    pub stop_reason: Option<StopReason>,
    /// Stop sequence that triggered stopping (if applicable)
    pub stop_sequence: Option<String>,
    /// Token usage statistics
    pub usage: Usage,
}

impl CompletionResponse {
    /// Extract text content from the response
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Extract tool use blocks from the response
    pub fn tool_uses(&self) -> Vec<(&str, &str, &serde_json::Value)> {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolUse { id, name, input } => Some((id.as_str(), name.as_str(), input)),
                _ => None,
            })
            .collect()
    }

    /// Extract thinking/reasoning content from the response
    pub fn thinking(&self) -> Option<&str> {
        self.content.iter().find_map(|block| match block {
            ContentBlock::Thinking { thinking, .. } => Some(thinking.as_str()),
            _ => None,
        })
    }
}

/// Streaming event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// Initial message start event
    MessageStart { message: StreamMessageStart },
    /// Content block started
    ContentBlockStart {
        index: usize,
        content_block: ContentBlock,
    },
    /// Delta for content block
    ContentBlockDelta {
        index: usize,
        delta: ContentDelta,
    },
    /// Content block finished
    ContentBlockStop { index: usize },
    /// Final message delta with usage
    MessageDelta {
        delta: MessageDeltaContent,
        usage: Usage,
    },
    /// Message complete
    MessageStop,
    /// Ping event (keep-alive)
    Ping,
    /// Error event
    Error { error: StreamError },
}

/// Message start in streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMessageStart {
    pub id: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub role: String,
    pub model: String,
    pub usage: Usage,
}

/// Content delta in streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
    /// Thinking content delta (streamed reasoning)
    ThinkingDelta { thinking: String },
    /// Signature delta for thinking blocks
    SignatureDelta { signature: String },
}

/// Message delta content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDeltaContent {
    pub stop_reason: Option<StopReason>,
    pub stop_sequence: Option<String>,
}

/// Error in streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

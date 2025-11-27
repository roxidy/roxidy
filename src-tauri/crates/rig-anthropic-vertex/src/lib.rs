//! Anthropic Claude models on Google Cloud Vertex AI provider for rig.
//!
//! This crate provides integration with Anthropic's Claude models deployed on
//! Google Cloud Vertex AI. It implements rig-core's `CompletionModel` trait.
//!
//! # Example
//!
//! ```rust,no_run
//! use rig_anthropic_vertex::Client;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create client from service account JSON file
//!     let client = Client::from_service_account(
//!         "/path/to/service-account.json",
//!         "your-project-id",
//!         "us-east5",
//!     ).await?;
//!
//!     // Get a Claude model
//!     let model = client.completion_model("claude-sonnet-4-20250514");
//!
//!     // Use with rig's agent or completion request builders
//!     Ok(())
//! }
//! ```

mod client;
mod completion;
mod error;
mod streaming;
mod types;

pub use client::Client;
pub use completion::CompletionModel;
pub use error::AnthropicVertexError;
pub use types::*;

/// Available Claude models on Vertex AI
pub mod models {
    /// Claude Opus 4.5 - Most powerful model
    pub const CLAUDE_OPUS_4_5: &str = "claude-opus-4-5@20251101";
    /// Claude Sonnet 4 - Balanced performance
    pub const CLAUDE_SONNET_4: &str = "claude-sonnet-4-20250514";
    /// Claude 3.5 Sonnet - Previous generation
    pub const CLAUDE_3_5_SONNET: &str = "claude-3-5-sonnet-v2@20241022";
    /// Claude 3.5 Haiku - Fast and efficient
    pub const CLAUDE_3_5_HAIKU: &str = "claude-3-5-haiku@20241022";
}

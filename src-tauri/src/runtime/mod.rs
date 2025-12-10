// Runtime abstraction for CLI vs Tauri environments
//
// The `tauri` and `cli` features are mutually exclusive. Each provides a different
// implementation of the QbitRuntime trait for their respective environments.

// Compile-time guard: ensure tauri and cli features are mutually exclusive
#[cfg(all(feature = "tauri", feature = "cli"))]
compile_error!("Features 'tauri' and 'cli' are mutually exclusive. Use --features tauri OR --features cli, not both.");

use std::any::Any;

use async_trait::async_trait;
use serde::Serialize;
use thiserror::Error;

/// Runtime-specific errors
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Failed to emit event: {0}")]
    EmitFailed(String),

    #[error("Event receiver closed")]
    ReceiverClosed,

    #[error("Approval request timed out after {0}s")]
    ApprovalTimeout(u64),

    #[error("Approval denied by user")]
    ApprovalDenied,

    #[error("Not running in interactive mode (no TTY)")]
    NotInteractive,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Other error: {0}")]
    Other(String),
}

/// Events that can be emitted to the frontend/CLI
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum RuntimeEvent {
    /// Terminal output from PTY
    TerminalOutput { session_id: String, data: Vec<u8> },

    /// Terminal exited
    TerminalExit {
        session_id: String,
        code: Option<i32>,
    },

    /// AI agent event (re-export from AiEvent)
    Ai(Box<crate::ai::events::AiEvent>),

    /// Generic extensibility
    Custom {
        name: String,
        payload: serde_json::Value,
    },
}

/// Approval decision from user
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalResult {
    Approved,
    Denied,
    AlwaysAllow,
    AlwaysDeny,
    Timeout,
}

/// Runtime abstraction for Tauri vs CLI vs other environments
///
/// This trait provides platform-specific functionality for:
/// - Emitting events (UI updates, logs, JSON output)
/// - Requesting user approval for tool execution
/// - Querying runtime capabilities
///
/// # Object Safety
/// This trait is object-safe and intended to be used as `Arc<dyn QbitRuntime>`.
///
/// # Performance
/// `request_approval()` uses `#[async_trait]` which boxes the future. This is
/// acceptable since approval is infrequent and IO-bound.
#[async_trait]
pub trait QbitRuntime: Send + Sync + 'static {
    /// Emit an event to the frontend/output
    ///
    /// # Errors
    /// Returns `RuntimeError::EmitFailed` if the event cannot be delivered
    /// (e.g., receiver dropped, channel full).
    fn emit(&self, event: RuntimeEvent) -> Result<(), RuntimeError>;

    /// Request approval for a tool execution (blocks until decision or timeout)
    ///
    /// # Arguments
    /// - `request_id`: Unique identifier for this approval request
    /// - `tool_name`: Name of the tool requesting approval
    /// - `args`: Tool arguments (for display to user)
    /// - `risk_level`: Risk assessment ("low", "medium", "high")
    ///
    /// # Returns
    /// - `Ok(ApprovalResult)` with user's decision
    /// - `Err(RuntimeError::ApprovalTimeout)` if no response within timeout
    /// - `Err(RuntimeError::NotInteractive)` if approval required but no UI/TTY
    ///
    /// # Ownership
    /// Takes owned `String` to avoid lifetime issues with `#[async_trait]`.
    async fn request_approval(
        &self,
        request_id: String,
        tool_name: String,
        args: serde_json::Value,
        risk_level: String,
    ) -> Result<ApprovalResult, RuntimeError>;

    /// Check if running in interactive mode (has UI or TTY)
    fn is_interactive(&self) -> bool;

    /// Check if auto-approve mode is enabled
    fn auto_approve(&self) -> bool;

    /// Graceful shutdown - flush events, close channels, etc.
    ///
    /// Called during application exit to ensure all events are processed.
    async fn shutdown(&self) -> Result<(), RuntimeError>;

    /// Get as Any for downcasting to concrete type.
    ///
    /// This enables runtime-specific operations like updating channels.
    fn as_any(&self) -> &dyn Any;
}

// Feature-gated runtime implementations
#[cfg(feature = "cli")]
pub mod cli;
#[cfg(feature = "tauri")]
pub mod tauri;

// Re-exports for convenience (feature-gated)
#[cfg(feature = "cli")]
pub use cli::CliRuntime;
#[cfg(feature = "tauri")]
pub use tauri::TauriRuntime;

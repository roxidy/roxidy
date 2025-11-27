use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RoxidyError {
    #[error("PTY error: {0}")]
    Pty(String),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

// Implement Serialize for Tauri
impl Serialize for RoxidyError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// Convert to Tauri-compatible result
pub type Result<T> = std::result::Result<T, RoxidyError>;

//! Configuration for the simplified markdown-based sidecar.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Sidecar configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarConfig {
    /// Enable the sidecar system
    pub enabled: bool,

    /// Directory for session storage (default: ~/.qbit/sessions)
    pub sessions_dir: Option<PathBuf>,

    /// Days to retain session data (0 = unlimited)
    pub retention_days: u32,

    /// Maximum size for state.md in bytes (context budget)
    pub max_state_size: usize,

    /// Whether to write raw events to events.jsonl
    pub write_raw_events: bool,

    /// Whether to use LLM for state updates (false = rule-based only)
    pub use_llm_for_state: bool,

    /// Capture tool call events
    pub capture_tool_calls: bool,

    /// Capture agent reasoning events
    pub capture_reasoning: bool,
}

impl Default for SidecarConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sessions_dir: None, // Will use default_sessions_dir()
            retention_days: 30,
            max_state_size: 16 * 1024, // 16KB
            write_raw_events: true,
            use_llm_for_state: false, // Start with rule-based, enable LLM later
            capture_tool_calls: true,
            capture_reasoning: true,
        }
    }
}

impl SidecarConfig {
    /// Get the sessions directory, falling back to default
    pub fn sessions_dir(&self) -> PathBuf {
        self.sessions_dir
            .clone()
            .unwrap_or_else(super::session::default_sessions_dir)
    }

    /// Create config from QbitSettings
    pub fn from_qbit_settings(settings: &crate::settings::schema::SidecarSettings) -> Self {
        Self {
            enabled: settings.enabled,
            sessions_dir: None, // Use default
            retention_days: settings.retention_days,
            max_state_size: 16 * 1024,
            write_raw_events: true,
            use_llm_for_state: false,
            capture_tool_calls: settings.capture_tool_calls,
            capture_reasoning: settings.capture_reasoning,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SidecarConfig::default();
        assert!(config.enabled);
        assert_eq!(config.retention_days, 30);
        assert!(config.capture_tool_calls);
        assert!(config.capture_reasoning);
    }

    #[test]
    fn test_sessions_dir_default() {
        let config = SidecarConfig::default();
        let dir = config.sessions_dir();
        assert!(dir.to_string_lossy().contains(".qbit"));
        assert!(dir.to_string_lossy().contains("sessions"));
    }

    #[test]
    fn test_sessions_dir_custom() {
        let mut config = SidecarConfig::default();
        config.sessions_dir = Some(PathBuf::from("/custom/sessions"));
        assert_eq!(config.sessions_dir(), PathBuf::from("/custom/sessions"));
    }
}

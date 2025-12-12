//! Configuration for the simplified markdown-based sidecar.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::artifacts::ArtifactSynthesisBackend;
use super::synthesis::SynthesisBackend;
use crate::settings::schema::{
    SynthesisGrokSettings, SynthesisOpenAiSettings, SynthesisVertexSettings,
};

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

    /// Enable synthesis for commit messages
    pub synthesis_enabled: bool,

    /// Which synthesis backend to use for commit messages
    pub synthesis_backend: SynthesisBackend,

    /// Which synthesis backend to use for artifact generation (README.md, CLAUDE.md)
    /// Defaults to the same backend as synthesis_backend
    pub artifact_synthesis_backend: ArtifactSynthesisBackend,

    /// Vertex AI settings for synthesis
    pub synthesis_vertex: SynthesisVertexSettings,

    /// OpenAI settings for synthesis
    pub synthesis_openai: SynthesisOpenAiSettings,

    /// Grok settings for synthesis
    pub synthesis_grok: SynthesisGrokSettings,
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
            synthesis_enabled: true,
            synthesis_backend: SynthesisBackend::Template,
            artifact_synthesis_backend: ArtifactSynthesisBackend::Template,
            synthesis_vertex: SynthesisVertexSettings::default(),
            synthesis_openai: SynthesisOpenAiSettings::default(),
            synthesis_grok: SynthesisGrokSettings::default(),
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
        let backend = settings
            .synthesis_backend
            .parse()
            .unwrap_or(SynthesisBackend::Template);

        // Artifact synthesis uses the same backend as commit message synthesis by default
        let artifact_backend = settings
            .synthesis_backend
            .parse()
            .unwrap_or(ArtifactSynthesisBackend::Template);

        Self {
            enabled: settings.enabled,
            sessions_dir: None, // Use default
            retention_days: settings.retention_days,
            max_state_size: 16 * 1024,
            write_raw_events: true,
            use_llm_for_state: false,
            capture_tool_calls: settings.capture_tool_calls,
            capture_reasoning: settings.capture_reasoning,
            synthesis_enabled: settings.synthesis_enabled,
            synthesis_backend: backend,
            artifact_synthesis_backend: artifact_backend,
            synthesis_vertex: settings.synthesis_vertex.clone(),
            synthesis_openai: settings.synthesis_openai.clone(),
            synthesis_grok: settings.synthesis_grok.clone(),
        }
    }

    /// Get artifact synthesis config from this config
    #[allow(dead_code)]
    pub fn artifact_synthesis_config(&self) -> super::artifacts::ArtifactSynthesisConfig {
        super::artifacts::ArtifactSynthesisConfig {
            backend: self.artifact_synthesis_backend,
            ..Default::default()
        }
    }

    /// Check if synthesis is enabled and using LLM (not template)
    #[allow(dead_code)]
    pub fn use_llm_synthesis(&self) -> bool {
        self.synthesis_enabled && self.synthesis_backend != SynthesisBackend::Template
    }

    /// Check if artifact synthesis is enabled and using LLM (not template)
    #[allow(dead_code)]
    pub fn use_llm_artifact_synthesis(&self) -> bool {
        self.synthesis_enabled
            && self.artifact_synthesis_backend != ArtifactSynthesisBackend::Template
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

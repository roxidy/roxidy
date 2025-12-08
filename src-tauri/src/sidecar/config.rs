//! Configuration for the sidecar system.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::settings::schema::{SidecarSettings, SynthesisVertexSettings, SynthesisOpenAiSettings, SynthesisGrokSettings};

/// LLM provider configuration for remote backends
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum LlmProvider {
    /// Anthropic Claude via Google Vertex AI
    VertexAnthropic {
        project_id: String,
        location: String,
        #[serde(default = "default_vertex_model")]
        model: String,
        /// Path to service account credentials JSON
        credentials_path: Option<String>,
    },
    /// OpenAI API (including Azure OpenAI)
    OpenAI {
        #[serde(default = "default_openai_model")]
        model: String,
        /// API key (or use OPENAI_API_KEY env var)
        api_key: Option<String>,
        /// Optional base URL for Azure or proxies
        base_url: Option<String>,
    },
    /// xAI Grok
    Grok {
        #[serde(default = "default_grok_model")]
        model: String,
        /// API key (or use GROK_API_KEY env var)
        api_key: Option<String>,
    },
    /// Generic OpenAI-compatible API
    OpenAICompatible {
        model: String,
        base_url: String,
        api_key: Option<String>,
    },
}

fn default_vertex_model() -> String {
    "claude-sonnet-4-5-20250514".to_string()
}

fn default_openai_model() -> String {
    "gpt-4o-mini".to_string()
}

fn default_grok_model() -> String {
    "grok-2".to_string()
}

impl LlmProvider {
    /// Get the model name for this provider
    pub fn model_name(&self) -> &str {
        match self {
            LlmProvider::VertexAnthropic { model, .. } => model,
            LlmProvider::OpenAI { model, .. } => model,
            LlmProvider::Grok { model, .. } => model,
            LlmProvider::OpenAICompatible { model, .. } => model,
        }
    }

    /// Get a display name for the provider
    pub fn provider_name(&self) -> &str {
        match self {
            LlmProvider::VertexAnthropic { .. } => "Vertex AI (Claude)",
            LlmProvider::OpenAI { .. } => "OpenAI",
            LlmProvider::Grok { .. } => "xAI Grok",
            LlmProvider::OpenAICompatible { .. } => "OpenAI Compatible",
        }
    }
}

/// Synthesis backend configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "backend")]
pub enum SynthesisBackend {
    /// Local LLM via mistral.rs (Qwen, etc.)
    Local,
    /// Remote LLM provider
    Remote { provider: LlmProvider },
    /// Template-only mode (no LLM)
    Template,
}

impl Default for SynthesisBackend {
    fn default() -> Self {
        // Default to Local if the feature is enabled, otherwise Template
        #[cfg(feature = "local-llm")]
        {
            SynthesisBackend::Local
        }
        #[cfg(not(feature = "local-llm"))]
        {
            SynthesisBackend::Template
        }
    }
}

impl SynthesisBackend {
    /// Check if the local LLM feature is compiled in
    pub fn local_llm_available() -> bool {
        cfg!(feature = "local-llm")
    }
}

/// Configuration for the sidecar context capture system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarConfig {
    /// Number of events before generating a checkpoint
    pub checkpoint_event_threshold: usize,

    /// Seconds of inactivity before generating a checkpoint
    pub checkpoint_time_threshold_secs: u64,

    /// Maximum events in memory buffer before flushing to disk
    pub buffer_flush_threshold: usize,

    /// Enable/disable LLM synthesis (can run without models in template mode)
    pub synthesis_enabled: bool,

    /// Enable/disable embedding generation
    pub embeddings_enabled: bool,

    /// Which LLM backend to use for synthesis
    #[serde(default)]
    pub synthesis_backend: SynthesisBackend,

    /// Path to store sidecar data (defaults to ~/.qbit/sidecar/)
    pub data_dir: PathBuf,

    /// Path to store models (defaults to ~/.qbit/models/)
    pub models_dir: PathBuf,

    /// Maximum age of events to keep (in days, 0 = unlimited)
    pub retention_days: u32,

    /// Whether to capture tool call events (can be noisy)
    pub capture_tool_calls: bool,

    /// Whether to capture agent reasoning events
    pub capture_reasoning: bool,

    /// Minimum content length for an event to be captured
    pub min_content_length: usize,
}

impl Default for SidecarConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let qbit_dir = home.join(".qbit");

        Self {
            checkpoint_event_threshold: 20,
            checkpoint_time_threshold_secs: 300, // 5 minutes
            buffer_flush_threshold: 100,
            synthesis_enabled: true,
            embeddings_enabled: true,
            synthesis_backend: SynthesisBackend::default(),
            data_dir: qbit_dir.join("sidecar"),
            models_dir: qbit_dir.join("models"),
            retention_days: 30,
            capture_tool_calls: true,
            capture_reasoning: true,
            min_content_length: 10,
        }
    }
}

#[allow(dead_code)]
impl SidecarConfig {
    /// Create a new config with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a config for testing (uses temp directories)
    #[cfg(test)]
    pub fn test_config(temp_dir: &std::path::Path) -> Self {
        Self {
            data_dir: temp_dir.join("sidecar"),
            models_dir: temp_dir.join("models"),
            checkpoint_event_threshold: 5,
            checkpoint_time_threshold_secs: 10,
            buffer_flush_threshold: 10,
            synthesis_enabled: false,
            embeddings_enabled: false,
            synthesis_backend: SynthesisBackend::Template,
            retention_days: 0,
            capture_tool_calls: true,
            capture_reasoning: true,
            min_content_length: 1,
        }
    }

    /// Set the data directory
    pub fn with_data_dir(mut self, path: PathBuf) -> Self {
        self.data_dir = path;
        self
    }

    /// Set the models directory
    pub fn with_models_dir(mut self, path: PathBuf) -> Self {
        self.models_dir = path;
        self
    }

    /// Disable synthesis (for running without models)
    pub fn without_synthesis(mut self) -> Self {
        self.synthesis_enabled = false;
        self
    }

    /// Disable embeddings (for running without models)
    pub fn without_embeddings(mut self) -> Self {
        self.embeddings_enabled = false;
        self
    }

    /// Load config from file, or return default if file doesn't exist
    pub fn load_or_default(path: &std::path::Path) -> Self {
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(config) => return config,
                    Err(e) => {
                        tracing::warn!("Failed to parse sidecar config: {}", e);
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read sidecar config: {}", e);
                }
            }
        }
        Self::default()
    }

    /// Save config to file
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get the path to the LanceDB database
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("sidecar.lance")
    }

    /// Get the path to the embedding model (fastembed cache directory)
    pub fn embedding_model_path(&self) -> PathBuf {
        // fastembed caches models with this naming convention
        self.models_dir.join("models--Qdrant--all-MiniLM-L6-v2-onnx")
    }

    /// Get the path to the LLM model
    pub fn llm_model_path(&self) -> PathBuf {
        self.models_dir.join("qwen2.5-0.5b-instruct-q4_k_m.gguf")
    }

    /// Check if the embedding model is available
    pub fn embedding_model_available(&self) -> bool {
        self.embedding_model_path().exists()
    }

    /// Check if the LLM model is available
    pub fn llm_model_available(&self) -> bool {
        self.llm_model_path().exists()
    }

    /// Ensure data directories exist
    pub fn ensure_directories(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.models_dir)?;
        Ok(())
    }

    /// Create a SidecarConfig from QbitSettings sidecar section
    pub fn from_qbit_settings(settings: &SidecarSettings, ai_vertex: Option<&crate::settings::schema::VertexAiSettings>) -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let qbit_dir = home.join(".qbit");

        // Convert synthesis backend string to enum
        let synthesis_backend = match settings.synthesis_backend.as_str() {
            "local" => SynthesisBackend::Local,
            "template" => SynthesisBackend::Template,
            "vertex_anthropic" => {
                // Get project_id and location from sidecar settings or fall back to AI settings
                let project_id = settings.synthesis_vertex.project_id.clone()
                    .or_else(|| ai_vertex.and_then(|v| v.project_id.clone()))
                    .unwrap_or_default();
                let location = settings.synthesis_vertex.location.clone()
                    .or_else(|| ai_vertex.and_then(|v| v.location.clone()))
                    .unwrap_or_else(|| "us-east5".to_string());
                let credentials_path = settings.synthesis_vertex.credentials_path.clone()
                    .or_else(|| ai_vertex.and_then(|v| v.credentials_path.clone()));

                SynthesisBackend::Remote {
                    provider: LlmProvider::VertexAnthropic {
                        project_id,
                        location,
                        model: settings.synthesis_vertex.model.clone(),
                        credentials_path,
                    },
                }
            }
            "openai" => SynthesisBackend::Remote {
                provider: LlmProvider::OpenAI {
                    model: settings.synthesis_openai.model.clone(),
                    api_key: settings.synthesis_openai.api_key.clone(),
                    base_url: settings.synthesis_openai.base_url.clone(),
                },
            },
            "grok" => SynthesisBackend::Remote {
                provider: LlmProvider::Grok {
                    model: settings.synthesis_grok.model.clone(),
                    api_key: settings.synthesis_grok.api_key.clone(),
                },
            },
            _ => SynthesisBackend::Template, // Default fallback
        };

        Self {
            checkpoint_event_threshold: 20,
            checkpoint_time_threshold_secs: 300,
            buffer_flush_threshold: 100,
            synthesis_enabled: settings.synthesis_enabled,
            embeddings_enabled: true,
            synthesis_backend,
            data_dir: qbit_dir.join("sidecar"),
            models_dir: qbit_dir.join("models"),
            retention_days: settings.retention_days,
            capture_tool_calls: settings.capture_tool_calls,
            capture_reasoning: settings.capture_reasoning,
            min_content_length: 10,
        }
    }

    /// Update QbitSettings sidecar section from this config
    pub fn to_qbit_settings(&self) -> SidecarSettings {
        let (synthesis_backend, synthesis_vertex, synthesis_openai, synthesis_grok) = match &self.synthesis_backend {
            SynthesisBackend::Local => (
                "local".to_string(),
                SynthesisVertexSettings::default(),
                SynthesisOpenAiSettings::default(),
                SynthesisGrokSettings::default(),
            ),
            SynthesisBackend::Template => (
                "template".to_string(),
                SynthesisVertexSettings::default(),
                SynthesisOpenAiSettings::default(),
                SynthesisGrokSettings::default(),
            ),
            SynthesisBackend::Remote { provider } => match provider {
                LlmProvider::VertexAnthropic { project_id, location, model, credentials_path } => (
                    "vertex_anthropic".to_string(),
                    SynthesisVertexSettings {
                        project_id: Some(project_id.clone()),
                        location: Some(location.clone()),
                        model: model.clone(),
                        credentials_path: credentials_path.clone(),
                    },
                    SynthesisOpenAiSettings::default(),
                    SynthesisGrokSettings::default(),
                ),
                LlmProvider::OpenAI { model, api_key, base_url } => (
                    "openai".to_string(),
                    SynthesisVertexSettings::default(),
                    SynthesisOpenAiSettings {
                        api_key: api_key.clone(),
                        model: model.clone(),
                        base_url: base_url.clone(),
                    },
                    SynthesisGrokSettings::default(),
                ),
                LlmProvider::Grok { model, api_key } => (
                    "grok".to_string(),
                    SynthesisVertexSettings::default(),
                    SynthesisOpenAiSettings::default(),
                    SynthesisGrokSettings {
                        api_key: api_key.clone(),
                        model: model.clone(),
                    },
                ),
                LlmProvider::OpenAICompatible { model, base_url, api_key } => (
                    "openai".to_string(),
                    SynthesisVertexSettings::default(),
                    SynthesisOpenAiSettings {
                        api_key: api_key.clone(),
                        model: model.clone(),
                        base_url: Some(base_url.clone()),
                    },
                    SynthesisGrokSettings::default(),
                ),
            },
        };

        SidecarSettings {
            enabled: true, // Always enabled if we have a config
            synthesis_enabled: self.synthesis_enabled,
            synthesis_backend,
            synthesis_vertex,
            synthesis_openai,
            synthesis_grok,
            retention_days: self.retention_days,
            capture_tool_calls: self.capture_tool_calls,
            capture_reasoning: self.capture_reasoning,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = SidecarConfig::default();
        assert_eq!(config.checkpoint_event_threshold, 20);
        assert!(config.synthesis_enabled);
        assert!(config.embeddings_enabled);
    }

    #[test]
    fn test_config_builders() {
        let config = SidecarConfig::new()
            .with_data_dir(PathBuf::from("/custom/data"))
            .without_synthesis()
            .without_embeddings();

        assert_eq!(config.data_dir, PathBuf::from("/custom/data"));
        assert!(!config.synthesis_enabled);
        assert!(!config.embeddings_enabled);
    }

    #[test]
    fn test_config_serialization() {
        let config = SidecarConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: SidecarConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(
            deserialized.checkpoint_event_threshold,
            config.checkpoint_event_threshold
        );
    }

    #[test]
    fn test_config_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let config = SidecarConfig::new().with_data_dir(PathBuf::from("/test/path"));

        config.save(&config_path).unwrap();
        let loaded = SidecarConfig::load_or_default(&config_path);

        assert_eq!(loaded.data_dir, PathBuf::from("/test/path"));
    }

    #[test]
    fn test_db_path() {
        let config = SidecarConfig::new().with_data_dir(PathBuf::from("/data"));
        assert_eq!(config.db_path(), PathBuf::from("/data/sidecar.lance"));
    }
}

//! Configuration for the sidecar system.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
            data_dir: qbit_dir.join("sidecar"),
            models_dir: qbit_dir.join("models"),
            retention_days: 30,
            capture_tool_calls: true,
            capture_reasoning: true,
            min_content_length: 10,
        }
    }
}

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

    /// Get the path to the embedding model
    pub fn embedding_model_path(&self) -> PathBuf {
        self.models_dir.join("all-minilm-l6-v2")
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

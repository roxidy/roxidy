//! Settings schema definitions for Qbit configuration.
//!
//! All settings structs use `#[serde(default)]` to allow partial configuration files.
//! Missing fields are filled with sensible defaults.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Root settings structure for Qbit.
///
/// Loaded from `~/.qbit/settings.toml` with environment variable interpolation support.
/// Version field enables future migrations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QbitSettings {
    /// Schema version for migrations
    pub version: u32,

    /// AI provider configuration
    pub ai: AiSettings,

    /// API keys for external services
    pub api_keys: ApiKeysSettings,

    /// User interface preferences
    pub ui: UiSettings,

    /// Terminal configuration
    pub terminal: TerminalSettings,

    /// Agent behavior settings
    pub agent: AgentSettings,

    /// MCP server definitions
    #[serde(default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,

    /// Repository trust levels
    #[serde(default)]
    pub trust: TrustSettings,

    /// Privacy and telemetry settings
    pub privacy: PrivacySettings,

    /// Advanced/debug settings
    pub advanced: AdvancedSettings,
}

/// AI provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    /// Default AI provider: "vertex_ai" | "openrouter" | "anthropic" | "openai" | "ollama"
    pub default_provider: String,

    /// Default model for the selected provider
    pub default_model: String,

    /// Vertex AI specific settings
    pub vertex_ai: VertexAiSettings,

    /// OpenRouter specific settings
    pub openrouter: OpenRouterSettings,

    /// Direct Anthropic API settings
    pub anthropic: AnthropicSettings,

    /// OpenAI settings
    pub openai: OpenAiSettings,

    /// Ollama settings
    pub ollama: OllamaSettings,
}

/// Vertex AI (Anthropic on Google Cloud) settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct VertexAiSettings {
    /// Path to service account JSON credentials
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credentials_path: Option<String>,

    /// Google Cloud project ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,

    /// Vertex AI region (e.g., "us-east5")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

/// OpenRouter API settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct OpenRouterSettings {
    /// OpenRouter API key (supports $ENV_VAR syntax)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

/// Direct Anthropic API settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AnthropicSettings {
    /// Anthropic API key (supports $ENV_VAR syntax)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

/// OpenAI API settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct OpenAiSettings {
    /// OpenAI API key (supports $ENV_VAR syntax)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Custom base URL for OpenAI-compatible APIs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

/// Ollama local LLM settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OllamaSettings {
    /// Ollama server URL
    pub base_url: String,
}

/// API keys for external services.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ApiKeysSettings {
    /// Tavily API key for web search
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tavily: Option<String>,

    /// GitHub token for repository access
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github: Option<String>,
}

/// User interface preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    /// Theme: "dark" | "light" | "system"
    pub theme: String,

    /// Show tips on startup
    pub show_tips: bool,

    /// Hide banner/welcome message
    pub hide_banner: bool,
}

/// Terminal configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalSettings {
    /// Default shell override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,

    /// Font family
    pub font_family: String,

    /// Font size in pixels
    pub font_size: u32,

    /// Scrollback buffer lines
    pub scrollback: u32,
}

/// Agent behavior settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentSettings {
    /// Auto-save conversations
    pub session_persistence: bool,

    /// Session retention in days (0 = forever)
    pub session_retention_days: u32,

    /// Enable pattern learning for auto-approval
    pub pattern_learning: bool,

    /// Minimum approvals before auto-approve
    pub min_approvals_for_auto: u32,

    /// Approval rate threshold (0.0 - 1.0)
    pub approval_threshold: f64,
}

/// MCP (Model Context Protocol) server configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct McpServerConfig {
    /// Command to start the server
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Arguments for the command
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables for the server
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// URL for HTTP-based MCP servers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Repository trust settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TrustSettings {
    /// Paths with full trust (all tools allowed)
    #[serde(default)]
    pub full_trust: Vec<String>,

    /// Paths with read-only trust
    #[serde(default)]
    pub read_only_trust: Vec<String>,

    /// Paths that are never trusted
    #[serde(default)]
    pub never_trust: Vec<String>,
}

/// Privacy and telemetry settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PrivacySettings {
    /// Enable anonymous usage statistics
    pub usage_statistics: bool,

    /// Log prompts for debugging (local only)
    pub log_prompts: bool,
}

/// Advanced/debug settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AdvancedSettings {
    /// Enable experimental features
    pub enable_experimental: bool,

    /// Log level: "error" | "warn" | "info" | "debug" | "trace"
    pub log_level: String,
}

// =============================================================================
// Default implementations
// =============================================================================

impl Default for QbitSettings {
    fn default() -> Self {
        Self {
            version: 1,
            ai: AiSettings::default(),
            api_keys: ApiKeysSettings::default(),
            ui: UiSettings::default(),
            terminal: TerminalSettings::default(),
            agent: AgentSettings::default(),
            mcp_servers: HashMap::new(),
            trust: TrustSettings::default(),
            privacy: PrivacySettings::default(),
            advanced: AdvancedSettings::default(),
        }
    }
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            default_provider: "vertex_ai".to_string(),
            default_model: "claude-opus-4-5@20251101".to_string(),
            vertex_ai: VertexAiSettings::default(),
            openrouter: OpenRouterSettings::default(),
            anthropic: AnthropicSettings::default(),
            openai: OpenAiSettings::default(),
            ollama: OllamaSettings::default(),
        }
    }
}

impl Default for OllamaSettings {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
        }
    }
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            show_tips: true,
            hide_banner: false,
        }
    }
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            shell: None,
            font_family: "JetBrains Mono".to_string(),
            font_size: 14,
            scrollback: 10000,
        }
    }
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            session_persistence: true,
            session_retention_days: 30,
            pattern_learning: true,
            min_approvals_for_auto: 3,
            approval_threshold: 0.8,
        }
    }
}

impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            enable_experimental: false,
            log_level: "info".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = QbitSettings::default();
        assert_eq!(settings.version, 1);
        assert_eq!(settings.ai.default_provider, "vertex_ai");
        assert_eq!(settings.ai.default_model, "claude-opus-4-5@20251101");
        assert_eq!(settings.ui.theme, "dark");
        assert_eq!(settings.terminal.font_size, 14);
        assert!(settings.agent.session_persistence);
    }

    #[test]
    fn test_parse_minimal_toml() {
        let toml = r#"
            version = 1
            [ai]
            default_provider = "openrouter"
        "#;

        let settings: QbitSettings = toml::from_str(toml).unwrap();
        assert_eq!(settings.ai.default_provider, "openrouter");
        // Defaults should fill in missing fields
        assert_eq!(settings.terminal.font_size, 14);
    }

    #[test]
    fn test_serialize_settings() {
        let settings = QbitSettings::default();
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        assert!(toml_str.contains("version = 1"));
        assert!(toml_str.contains("[ai]"));
    }
}

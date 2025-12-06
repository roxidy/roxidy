//! Settings loading, saving, and environment variable interpolation.
//!
//! The `SettingsManager` handles:
//! - Loading settings from `~/.qbit/settings.toml`
//! - Resolving `$VAR` and `${VAR}` environment variable references
//! - Atomic file writes with temp file + rename
//! - First-run template generation

use std::path::PathBuf;

use anyhow::{Context, Result};
use tokio::sync::RwLock;

use super::schema::QbitSettings;

/// Embedded template for first-run generation.
const TEMPLATE: &str = include_str!("template.toml");

/// Get the path to the global settings file.
pub fn settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".qbit")
        .join("settings.toml")
}

/// Manages settings loading, interpolation, and persistence.
pub struct SettingsManager {
    /// Cached settings (with env vars resolved)
    settings: RwLock<QbitSettings>,

    /// Path to the settings file
    path: PathBuf,
}

impl SettingsManager {
    /// Create a new SettingsManager, loading from disk if available.
    pub async fn new() -> Result<Self> {
        let path = settings_path();
        let settings = Self::load_from_path(&path).await?;

        Ok(Self {
            settings: RwLock::new(settings),
            path,
        })
    }

    /// Load settings from a specific path.
    async fn load_from_path(path: &PathBuf) -> Result<QbitSettings> {
        if !path.exists() {
            tracing::debug!("Settings file not found at {:?}, using defaults", path);
            return Ok(QbitSettings::default());
        }

        let contents = tokio::fs::read_to_string(path)
            .await
            .context("Failed to read settings file")?;

        // Parse into typed struct
        let mut settings: QbitSettings =
            toml::from_str(&contents).context("Failed to deserialize settings")?;

        // Resolve environment variable references
        Self::resolve_env_vars(&mut settings);

        tracing::info!("Loaded settings from {:?}", path);
        Ok(settings)
    }

    /// Resolve $ENV_VAR references in string fields.
    fn resolve_env_vars(settings: &mut QbitSettings) {
        // Helper to resolve a single optional string
        fn resolve_opt(value: &mut Option<String>) {
            if let Some(v) = value {
                if let Some(resolved) = resolve_env_ref(v) {
                    *v = resolved;
                }
            }
        }

        // AI settings
        resolve_opt(&mut settings.ai.vertex_ai.credentials_path);
        resolve_opt(&mut settings.ai.vertex_ai.project_id);
        resolve_opt(&mut settings.ai.vertex_ai.location);
        resolve_opt(&mut settings.ai.openrouter.api_key);
        resolve_opt(&mut settings.ai.anthropic.api_key);
        resolve_opt(&mut settings.ai.openai.api_key);
        resolve_opt(&mut settings.ai.openai.base_url);

        // API keys
        resolve_opt(&mut settings.api_keys.tavily);
        resolve_opt(&mut settings.api_keys.github);

        // MCP server env vars
        for config in settings.mcp_servers.values_mut() {
            for v in config.env.values_mut() {
                if let Some(resolved) = resolve_env_ref(v) {
                    *v = resolved;
                }
            }
        }
    }

    /// Get the current settings (read-only).
    pub async fn get(&self) -> QbitSettings {
        self.settings.read().await.clone()
    }

    /// Update settings and persist to disk.
    pub async fn update(&self, new_settings: QbitSettings) -> Result<()> {
        // Update cached settings
        *self.settings.write().await = new_settings.clone();

        // Serialize to TOML
        let toml_string =
            toml::to_string_pretty(&new_settings).context("Failed to serialize settings")?;

        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Atomic write: write to temp file, then rename
        let temp_path = self.path.with_extension("toml.tmp");
        tokio::fs::write(&temp_path, &toml_string).await?;
        tokio::fs::rename(&temp_path, &self.path).await?;

        tracing::info!("Saved settings to {:?}", self.path);
        Ok(())
    }

    /// Get a specific setting by dot-notation key (e.g., "ai.vertex_ai.project_id").
    pub async fn get_value(&self, key: &str) -> Result<serde_json::Value> {
        let settings = self.settings.read().await;
        let json = serde_json::to_value(&*settings)?;

        // Navigate by key path
        let mut current = &json;
        for part in key.split('.') {
            current = current
                .get(part)
                .ok_or_else(|| anyhow::anyhow!("Setting '{}' not found", key))?;
        }

        Ok(current.clone())
    }

    /// Set a specific setting by dot-notation key.
    pub async fn set_value(&self, key: &str, value: serde_json::Value) -> Result<()> {
        let mut settings = self.settings.write().await;
        let mut json = serde_json::to_value(&*settings)?;

        // Navigate and set by key path
        let parts: Vec<&str> = key.split('.').collect();
        set_nested_value(&mut json, &parts, value)?;

        // Deserialize back
        *settings = serde_json::from_value(json)?;
        drop(settings);

        // Persist
        self.update(self.get().await).await
    }

    /// Reset to defaults and persist.
    pub async fn reset(&self) -> Result<()> {
        self.update(QbitSettings::default()).await
    }

    /// Check if settings file exists.
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Get the settings file path.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Ensure settings file exists, creating from template if needed.
    ///
    /// Returns `true` if a new file was created.
    pub async fn ensure_settings_file(&self) -> Result<bool> {
        if self.path.exists() {
            return Ok(false); // Already exists
        }

        // Create parent directory
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Write template
        tokio::fs::write(&self.path, TEMPLATE).await?;
        tracing::info!("Generated settings template at {:?}", self.path);
        Ok(true) // Created new file
    }

    /// Reload settings from disk.
    pub async fn reload(&self) -> Result<()> {
        let settings = Self::load_from_path(&self.path).await?;
        *self.settings.write().await = settings;
        Ok(())
    }
}

/// Set a value in a nested JSON object using a key path.
fn set_nested_value(
    json: &mut serde_json::Value,
    parts: &[&str],
    value: serde_json::Value,
) -> Result<()> {
    if parts.is_empty() {
        return Err(anyhow::anyhow!("Empty key path"));
    }

    let mut current = json;
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            // Last part: set the value
            if let Some(obj) = current.as_object_mut() {
                obj.insert((*part).to_string(), value);
                return Ok(());
            } else {
                return Err(anyhow::anyhow!("Cannot set value on non-object"));
            }
        } else {
            // Navigate deeper
            current = current
                .get_mut(*part)
                .ok_or_else(|| anyhow::anyhow!("Setting path '{}' not found", parts.join(".")))?;
        }
    }

    Ok(())
}

/// Resolve a $ENV_VAR or ${ENV_VAR} reference.
///
/// Returns `Some(resolved)` if the value starts with `$` and the env var exists.
/// Returns `None` if no env var reference or env var not set.
fn resolve_env_ref(value: &str) -> Option<String> {
    let trimmed = value.trim();

    // Check for $VAR_NAME format
    if trimmed.starts_with('$') {
        let var_name = if trimmed.starts_with("${") && trimmed.ends_with('}') {
            // ${VAR_NAME} format
            &trimmed[2..trimmed.len() - 1]
        } else {
            // $VAR_NAME format
            &trimmed[1..]
        };

        return std::env::var(var_name).ok();
    }

    None
}

/// Get a setting value with environment variable fallback.
///
/// Priority order:
/// 1. Settings value (if set and non-empty)
/// 2. Environment variable (first match from list)
/// 3. Default value
pub fn get_with_env_fallback(
    setting: &Option<String>,
    env_vars: &[&str],
    default: Option<String>,
) -> Option<String> {
    // 1. Check settings value
    if let Some(v) = setting {
        if !v.is_empty() {
            return Some(v.clone());
        }
    }

    // 2. Check environment variables
    for env_var in env_vars {
        if let Ok(v) = std::env::var(env_var) {
            if !v.is_empty() {
                return Some(v);
            }
        }
    }

    // 3. Return default
    default
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_env_ref_dollar_format() {
        std::env::set_var("TEST_VAR_1", "test_value_1");

        assert_eq!(
            resolve_env_ref("$TEST_VAR_1"),
            Some("test_value_1".to_string())
        );

        std::env::remove_var("TEST_VAR_1");
    }

    #[test]
    fn test_resolve_env_ref_braces_format() {
        std::env::set_var("TEST_VAR_2", "test_value_2");

        assert_eq!(
            resolve_env_ref("${TEST_VAR_2}"),
            Some("test_value_2".to_string())
        );

        std::env::remove_var("TEST_VAR_2");
    }

    #[test]
    fn test_resolve_env_ref_no_match() {
        assert_eq!(resolve_env_ref("regular_value"), None);
        assert_eq!(resolve_env_ref("$NONEXISTENT_VAR_XYZ_12345"), None);
    }

    #[test]
    fn test_get_with_env_fallback_from_setting() {
        let setting = Some("from_settings".to_string());
        let result = get_with_env_fallback(&setting, &["SOME_VAR"], None);
        assert_eq!(result, Some("from_settings".to_string()));
    }

    #[test]
    fn test_get_with_env_fallback_from_env() {
        std::env::set_var("FALLBACK_TEST_VAR", "from_env");

        let setting = None;
        let result = get_with_env_fallback(&setting, &["FALLBACK_TEST_VAR"], None);
        assert_eq!(result, Some("from_env".to_string()));

        std::env::remove_var("FALLBACK_TEST_VAR");
    }

    #[test]
    fn test_get_with_env_fallback_default() {
        let setting = None;
        let result = get_with_env_fallback(
            &setting,
            &["NONEXISTENT_VAR_ABC"],
            Some("default_value".to_string()),
        );
        assert_eq!(result, Some("default_value".to_string()));
    }

    #[test]
    fn test_get_with_env_fallback_empty_setting() {
        std::env::set_var("EMPTY_SETTING_TEST", "from_env");

        // Empty string in setting should fall through to env var
        let setting = Some("".to_string());
        let result = get_with_env_fallback(&setting, &["EMPTY_SETTING_TEST"], None);
        assert_eq!(result, Some("from_env".to_string()));

        std::env::remove_var("EMPTY_SETTING_TEST");
    }

    #[tokio::test]
    async fn test_settings_manager_defaults() {
        // Create a temp path that doesn't exist
        let manager = SettingsManager {
            settings: RwLock::new(QbitSettings::default()),
            path: PathBuf::from("/nonexistent/settings.toml"),
        };

        let settings = manager.get().await;
        assert_eq!(settings.version, 1);
        assert_eq!(settings.ai.default_provider, "vertex_ai");
    }

    #[tokio::test]
    async fn test_settings_manager_get_value() {
        let manager = SettingsManager {
            settings: RwLock::new(QbitSettings::default()),
            path: PathBuf::from("/nonexistent/settings.toml"),
        };

        let value = manager.get_value("ai.default_provider").await.unwrap();
        assert_eq!(value, serde_json::json!("vertex_ai"));

        let value = manager.get_value("terminal.font_size").await.unwrap();
        assert_eq!(value, serde_json::json!(14));
    }
}

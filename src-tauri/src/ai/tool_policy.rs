//! Tool Policy System for AI Agent
//!
//! This module provides policy-based access control for AI tool execution:
//! - `ToolPolicy` enum: allow/prompt/deny policies
//! - `ToolPolicyConfig`: Configuration loaded from `.qbit/tool-policy.json`
//! - `ToolConstraints`: Per-tool execution limits
//! - `ToolPolicyManager`: Manages policy loading, saving, and evaluation
//!
//! Based on the VTCode implementation pattern.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Policy for a tool determining whether it can be executed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolPolicy {
    /// Execute without prompting
    Allow,
    /// Request user confirmation (HITL)
    #[default]
    Prompt,
    /// Prevent execution entirely
    Deny,
}

impl std::fmt::Display for ToolPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolPolicy::Allow => write!(f, "allow"),
            ToolPolicy::Prompt => write!(f, "prompt"),
            ToolPolicy::Deny => write!(f, "deny"),
        }
    }
}

/// Constraints that can be applied to tool execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolConstraints {
    /// Maximum number of items/results (e.g., for list operations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u32>,

    /// Maximum bytes for content operations (e.g., file read/write)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,

    /// Allowed modes for the tool (e.g., ["read", "write"])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_modes: Option<Vec<String>>,

    /// Blocked URL schemes (e.g., ["file://", "data://"])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_schemes: Option<Vec<String>>,

    /// Blocked domains/hosts (e.g., ["127.0.0.1", "localhost"])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_hosts: Option<Vec<String>>,

    /// Allowed file extensions (e.g., [".rs", ".ts", ".py"])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_extensions: Option<Vec<String>>,

    /// Blocked file patterns (e.g., ["*.env", "*.key"])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_patterns: Option<Vec<String>>,

    /// Maximum command execution time in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u32>,
}

impl ToolConstraints {
    /// Check if a URL is blocked based on schemes and hosts.
    pub fn is_url_blocked(&self, url: &str) -> Option<String> {
        // Check blocked schemes
        if let Some(schemes) = &self.blocked_schemes {
            for scheme in schemes {
                if url.starts_with(scheme) {
                    return Some(format!("URL scheme '{}' is blocked", scheme));
                }
            }
        }

        // Check blocked hosts
        if let Some(hosts) = &self.blocked_hosts {
            // Extract host from URL using simple parsing
            // Look for :// then extract until next / or end
            if let Some(scheme_end) = url.find("://") {
                let after_scheme = &url[scheme_end + 3..];
                // Find the end of the host (first /, :, or end of string)
                let host_end = after_scheme
                    .find(['/', ':', '?'])
                    .unwrap_or(after_scheme.len());
                let host = &after_scheme[..host_end];

                for blocked in hosts {
                    if host == blocked.as_str()
                        || host.ends_with(&format!(".{}", blocked))
                        || (blocked.starts_with('.') && host.ends_with(blocked))
                    {
                        return Some(format!("Host '{}' is blocked", host));
                    }
                }
            }
        }

        None
    }

    /// Check if a file path is blocked based on extensions and patterns.
    pub fn is_path_blocked(&self, path: &str) -> Option<String> {
        // Check blocked patterns using simple glob-like matching
        if let Some(patterns) = &self.blocked_patterns {
            for pattern in patterns {
                if Self::simple_glob_match(pattern, path) {
                    return Some(format!("Path matches blocked pattern '{}'", pattern));
                }
            }
        }

        // Check allowed extensions (if specified, only these are allowed)
        if let Some(extensions) = &self.allowed_extensions {
            if !extensions.is_empty() {
                let has_valid_ext = extensions
                    .iter()
                    .any(|ext| path.ends_with(ext) || path.ends_with(&ext[1..]));
                if !has_valid_ext {
                    return Some(format!(
                        "File extension not in allowed list: {:?}",
                        extensions
                    ));
                }
            }
        }

        None
    }

    /// Simple glob pattern matching (supports *, **, and ?)
    fn simple_glob_match(pattern: &str, path: &str) -> bool {
        // Handle ** patterns (match any path segment)
        if pattern.contains("**") {
            let parts: Vec<&str> = pattern.split("**").collect();
            if parts.len() == 2 {
                let prefix = parts[0];
                let suffix = parts[1];

                // If pattern is like "**/*.env", check if path ends with suffix pattern
                if prefix.is_empty() && suffix.starts_with('/') {
                    let suffix_pattern = &suffix[1..];
                    return Self::simple_glob_match(suffix_pattern, path)
                        || path
                            .split('/')
                            .any(|segment| Self::simple_glob_match(suffix_pattern, segment));
                }

                // Check if path starts with prefix and ends with suffix
                let matches_prefix = prefix.is_empty() || path.starts_with(prefix);
                let matches_suffix = suffix.is_empty() || Self::simple_glob_match(suffix, path);
                return matches_prefix && matches_suffix;
            }
        }

        // Simple * matching (matches any characters except /)
        if pattern.contains('*') {
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                let prefix = parts[0];
                let suffix = parts[1];
                return path.starts_with(prefix) && path.ends_with(suffix);
            }
        }

        // Exact match
        pattern == path
    }

    /// Check if a mode is allowed.
    pub fn is_mode_allowed(&self, mode: &str) -> bool {
        match &self.allowed_modes {
            Some(modes) => modes.iter().any(|m| m == mode),
            None => true, // No restriction
        }
    }

    /// Check if an item count exceeds the limit.
    pub fn exceeds_max_items(&self, count: u32) -> bool {
        self.max_items.map(|max| count > max).unwrap_or(false)
    }

    /// Check if a byte size exceeds the limit.
    pub fn exceeds_max_bytes(&self, bytes: u64) -> bool {
        self.max_bytes.map(|max| bytes > max).unwrap_or(false)
    }
}

/// Configuration for tool policies loaded from `.qbit/tool-policy.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPolicyConfig {
    /// Version for future migrations
    #[serde(default = "default_version")]
    pub version: u32,

    /// List of all known/available tools
    #[serde(default)]
    pub available_tools: Vec<String>,

    /// Per-tool policies
    #[serde(default)]
    pub policies: HashMap<String, ToolPolicy>,

    /// Per-tool constraints
    #[serde(default)]
    pub constraints: HashMap<String, ToolConstraints>,

    /// Default policy for unknown tools
    #[serde(default)]
    pub default_policy: ToolPolicy,
}

fn default_version() -> u32 {
    1
}

impl Default for ToolPolicyConfig {
    fn default() -> Self {
        let mut policies = HashMap::new();
        let mut constraints = HashMap::new();

        // Default allowed tools (safe read-only operations)
        let allow_tools = [
            "read_file",
            "grep_file",
            "list_files",
            "indexer_search_code",
            "indexer_search_files",
            "indexer_analyze_file",
            "indexer_extract_symbols",
            "indexer_get_metrics",
            "indexer_detect_language",
            "debug_agent",
            "analyze_agent",
            "get_errors",
            "update_plan",
            "list_skills",
            "search_skills",
            "load_skill",
            "search_tools",
        ];

        for tool in allow_tools {
            policies.insert(tool.to_string(), ToolPolicy::Allow);
        }

        // Default prompt tools (file modifications)
        let prompt_tools = [
            "write_file",
            "create_file",
            "edit_file",
            "apply_patch",
            "save_skill",
            "web_fetch",
            "create_pty_session",
            "send_pty_input",
        ];

        for tool in prompt_tools {
            policies.insert(tool.to_string(), ToolPolicy::Prompt);
        }

        // Default deny tools (dangerous operations)
        let deny_tools = ["delete_file", "execute_code"];

        for tool in deny_tools {
            policies.insert(tool.to_string(), ToolPolicy::Deny);
        }

        // Default constraints for network operations
        let web_fetch_constraints = ToolConstraints {
            max_bytes: Some(65536), // 64KB max response
            blocked_hosts: Some(vec![
                "127.0.0.1".to_string(),
                "::1".to_string(),
                "localhost".to_string(),
                ".local".to_string(),
                ".internal".to_string(),
                ".lan".to_string(),
            ]),
            ..Default::default()
        };
        constraints.insert("web_fetch".to_string(), web_fetch_constraints);

        // Constraints for file operations
        let write_file_constraints = ToolConstraints {
            blocked_patterns: Some(vec![
                "*.env".to_string(),
                "*.key".to_string(),
                "*.pem".to_string(),
                "**/credentials*".to_string(),
                "**/secrets*".to_string(),
            ]),
            ..Default::default()
        };
        constraints.insert("write_file".to_string(), write_file_constraints.clone());
        constraints.insert("edit_file".to_string(), write_file_constraints);

        Self {
            version: 1,
            available_tools: Vec::new(),
            policies,
            constraints,
            default_policy: ToolPolicy::Prompt,
        }
    }
}

/// Result of applying policy constraints to a tool call.
#[derive(Debug, Clone)]
pub enum PolicyConstraintResult {
    /// Constraints passed, tool can execute
    Allowed,
    /// A constraint was violated
    Violated(String),
    /// Arguments were modified to comply with constraints
    Modified(serde_json::Value, String),
}

/// Manages tool policies for the AI agent.
///
/// Supports a two-tier policy system:
/// 1. **Global policy** (`~/.qbit/tool-policy.json`) - User's default preferences
/// 2. **Project policy** (`{workspace}/.qbit/tool-policy.json`) - Project-specific overrides
///
/// Project policies override global policies for the same tool.
pub struct ToolPolicyManager {
    /// Merged configuration (global + project, project takes precedence)
    config: RwLock<ToolPolicyConfig>,
    /// Global config (from ~/.qbit/)
    global_config: RwLock<Option<ToolPolicyConfig>>,
    /// Project config (from workspace/.qbit/)
    project_config: RwLock<Option<ToolPolicyConfig>>,
    /// Path to the global policy file
    global_config_path: PathBuf,
    /// Path to the project policy file
    project_config_path: PathBuf,
    /// Tools that have been pre-approved this session
    preapproved: RwLock<HashSet<String>>,
    /// Whether full-auto mode is enabled
    full_auto_allowlist: RwLock<Option<HashSet<String>>>,
}

impl ToolPolicyManager {
    /// Create a new ToolPolicyManager for the given workspace.
    ///
    /// Loads policies from both global (~/.qbit/tool-policy.json) and
    /// project ({workspace}/.qbit/tool-policy.json) locations, merging them
    /// with project policies taking precedence.
    pub async fn new(workspace: &Path) -> Self {
        let global_config_path = Self::global_policy_path();
        let project_config_path = workspace.join(".qbit").join("tool-policy.json");

        // Load global config
        let global_config = Self::load_config_file(&global_config_path).await;
        if global_config.is_some() {
            tracing::debug!("Loaded global tool policy from {:?}", global_config_path);
        }

        // Load project config
        let project_config = Self::load_config_file(&project_config_path).await;
        if project_config.is_some() {
            tracing::debug!("Loaded project tool policy from {:?}", project_config_path);
        }

        // Merge configs (project overrides global)
        let merged_config = Self::merge_configs(&global_config, &project_config);

        Self {
            config: RwLock::new(merged_config),
            global_config: RwLock::new(global_config),
            project_config: RwLock::new(project_config),
            global_config_path,
            project_config_path,
            preapproved: RwLock::new(HashSet::new()),
            full_auto_allowlist: RwLock::new(None),
        }
    }

    /// Get the path to the global policy file (~/.qbit/tool-policy.json).
    pub fn global_policy_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".qbit")
            .join("tool-policy.json")
    }

    /// Load a config file if it exists.
    async fn load_config_file(path: &PathBuf) -> Option<ToolPolicyConfig> {
        if !path.exists() {
            return None;
        }

        match tokio::fs::read_to_string(path).await {
            Ok(contents) => match serde_json::from_str(&contents) {
                Ok(config) => Some(config),
                Err(e) => {
                    tracing::warn!("Failed to parse tool policy config {:?}: {}", path, e);
                    None
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read tool policy config {:?}: {}", path, e);
                None
            }
        }
    }

    /// Merge global and project configs.
    ///
    /// - Starts with defaults
    /// - Applies global config (if present)
    /// - Applies project config (if present), which overrides global
    fn merge_configs(
        global: &Option<ToolPolicyConfig>,
        project: &Option<ToolPolicyConfig>,
    ) -> ToolPolicyConfig {
        let mut merged = ToolPolicyConfig::default();

        // Apply global config
        if let Some(global_cfg) = global {
            // Merge policies (global overrides defaults)
            for (tool, policy) in &global_cfg.policies {
                merged.policies.insert(tool.clone(), policy.clone());
            }
            // Merge constraints (global overrides defaults)
            for (tool, constraints) in &global_cfg.constraints {
                merged.constraints.insert(tool.clone(), constraints.clone());
            }
            // Use global default_policy if set
            merged.default_policy = global_cfg.default_policy.clone();
            // Merge available_tools
            for tool in &global_cfg.available_tools {
                if !merged.available_tools.contains(tool) {
                    merged.available_tools.push(tool.clone());
                }
            }
        }

        // Apply project config (overrides global)
        if let Some(project_cfg) = project {
            // Merge policies (project overrides global)
            for (tool, policy) in &project_cfg.policies {
                merged.policies.insert(tool.clone(), policy.clone());
            }
            // Merge constraints (project overrides global)
            for (tool, constraints) in &project_cfg.constraints {
                merged.constraints.insert(tool.clone(), constraints.clone());
            }
            // Use project default_policy if set
            merged.default_policy = project_cfg.default_policy.clone();
            // Merge available_tools
            for tool in &project_cfg.available_tools {
                if !merged.available_tools.contains(tool) {
                    merged.available_tools.push(tool.clone());
                }
            }
        }

        merged
    }

    /// Create a new ToolPolicyManager with an explicit config (for testing).
    pub fn with_config(config: ToolPolicyConfig, project_config_path: PathBuf) -> Self {
        Self {
            config: RwLock::new(config.clone()),
            global_config: RwLock::new(None),
            project_config: RwLock::new(Some(config)),
            global_config_path: Self::global_policy_path(),
            project_config_path,
            preapproved: RwLock::new(HashSet::new()),
            full_auto_allowlist: RwLock::new(None),
        }
    }

    /// Check if global policy file exists.
    pub fn has_global_policy(&self) -> bool {
        self.global_config_path.exists()
    }

    /// Check if project policy file exists.
    pub fn has_project_policy(&self) -> bool {
        self.project_config_path.exists()
    }

    /// Get the global config (if loaded).
    pub async fn get_global_config(&self) -> Option<ToolPolicyConfig> {
        self.global_config.read().await.clone()
    }

    /// Get the project config (if loaded).
    pub async fn get_project_config(&self) -> Option<ToolPolicyConfig> {
        self.project_config.read().await.clone()
    }

    /// Get the policy for a tool.
    pub async fn get_policy(&self, tool_name: &str) -> ToolPolicy {
        let config = self.config.read().await;

        // Check full-auto mode first
        if let Some(ref allowlist) = *self.full_auto_allowlist.read().await {
            if allowlist.contains(tool_name) {
                return ToolPolicy::Allow;
            }
        }

        config
            .policies
            .get(tool_name)
            .cloned()
            .unwrap_or_else(|| config.default_policy.clone())
    }

    /// Set the policy for a tool and persist to disk.
    pub async fn set_policy(&self, tool_name: &str, policy: ToolPolicy) -> Result<()> {
        {
            let mut config = self.config.write().await;
            config.policies.insert(tool_name.to_string(), policy);
        }
        self.save().await
    }

    /// Get constraints for a tool.
    pub async fn get_constraints(&self, tool_name: &str) -> Option<ToolConstraints> {
        let config = self.config.read().await;
        config.constraints.get(tool_name).cloned()
    }

    /// Set constraints for a tool and persist to disk.
    pub async fn set_constraints(
        &self,
        tool_name: &str,
        constraints: ToolConstraints,
    ) -> Result<()> {
        {
            let mut config = self.config.write().await;
            config
                .constraints
                .insert(tool_name.to_string(), constraints);
        }
        self.save().await
    }

    /// Apply policy constraints to tool arguments.
    /// Returns the result indicating whether constraints pass, fail, or modify the args.
    pub async fn apply_constraints(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> PolicyConstraintResult {
        let config = self.config.read().await;

        let constraints = match config.constraints.get(tool_name) {
            Some(c) => c,
            None => return PolicyConstraintResult::Allowed,
        };

        // Check URL-based constraints
        if let Some(url) = args.get("url").and_then(|v| v.as_str()) {
            if let Some(reason) = constraints.is_url_blocked(url) {
                return PolicyConstraintResult::Violated(reason);
            }
        }

        // Check path-based constraints
        for path_field in &["path", "file_path", "file", "target"] {
            if let Some(path) = args.get(*path_field).and_then(|v| v.as_str()) {
                if let Some(reason) = constraints.is_path_blocked(path) {
                    return PolicyConstraintResult::Violated(reason);
                }
            }
        }

        // Check mode constraints
        if let Some(mode) = args.get("mode").and_then(|v| v.as_str()) {
            if !constraints.is_mode_allowed(mode) {
                return PolicyConstraintResult::Violated(format!("Mode '{}' is not allowed", mode));
            }
        }

        // Check item count constraints
        if let Some(max_items) = constraints.max_items {
            if let Some(limit) = args.get("limit").and_then(|v| v.as_u64()) {
                if limit > max_items as u64 {
                    // Modify args to comply with constraint
                    let mut modified_args = args.clone();
                    if let Some(obj) = modified_args.as_object_mut() {
                        obj.insert("limit".to_string(), serde_json::json!(max_items));
                    }
                    return PolicyConstraintResult::Modified(
                        modified_args,
                        format!(
                            "Limit reduced from {} to {} per policy constraint",
                            limit, max_items
                        ),
                    );
                }
            }
        }

        PolicyConstraintResult::Allowed
    }

    /// Check if a tool should be executed based on policy.
    /// Returns true if the tool can execute (Allow policy or pre-approved).
    pub async fn should_execute(&self, tool_name: &str) -> bool {
        // Check pre-approved
        if self.preapproved.read().await.contains(tool_name) {
            return true;
        }

        // Check policy
        matches!(self.get_policy(tool_name).await, ToolPolicy::Allow)
    }

    /// Check if a tool requires prompting.
    pub async fn requires_prompt(&self, tool_name: &str) -> bool {
        // Check pre-approved (no prompt needed)
        if self.preapproved.read().await.contains(tool_name) {
            return false;
        }

        matches!(self.get_policy(tool_name).await, ToolPolicy::Prompt)
    }

    /// Check if a tool is denied.
    pub async fn is_denied(&self, tool_name: &str) -> bool {
        matches!(self.get_policy(tool_name).await, ToolPolicy::Deny)
    }

    /// Pre-approve a tool for this session (one-time approval).
    pub async fn preapprove(&self, tool_name: &str) {
        self.preapproved.write().await.insert(tool_name.to_string());
    }

    /// Check and consume pre-approval status.
    pub async fn take_preapproved(&self, tool_name: &str) -> bool {
        self.preapproved.write().await.remove(tool_name)
    }

    /// Enable full-auto mode with the given allowed tools.
    pub async fn enable_full_auto(&self, allowed_tools: Vec<String>) {
        let allowlist: HashSet<String> = allowed_tools.into_iter().collect();
        *self.full_auto_allowlist.write().await = Some(allowlist);
        tracing::info!("Full-auto mode enabled");
    }

    /// Disable full-auto mode.
    pub async fn disable_full_auto(&self) {
        *self.full_auto_allowlist.write().await = None;
        tracing::info!("Full-auto mode disabled");
    }

    /// Check if full-auto mode is enabled.
    pub async fn is_full_auto_enabled(&self) -> bool {
        self.full_auto_allowlist.read().await.is_some()
    }

    /// Check if a tool is allowed in full-auto mode.
    pub async fn is_allowed_in_full_auto(&self, tool_name: &str) -> bool {
        if let Some(ref allowlist) = *self.full_auto_allowlist.read().await {
            allowlist.contains(tool_name)
        } else {
            false
        }
    }

    /// Update the list of available tools (synced from tool registry).
    pub async fn sync_available_tools(&self, tools: Vec<String>) -> Result<()> {
        {
            let mut config = self.config.write().await;
            config.available_tools = tools;
        }
        self.save().await
    }

    /// Get the current configuration.
    pub async fn get_config(&self) -> ToolPolicyConfig {
        self.config.read().await.clone()
    }

    /// Set the entire configuration and persist.
    pub async fn set_config(&self, config: ToolPolicyConfig) -> Result<()> {
        *self.config.write().await = config;
        self.save().await
    }

    /// Set all tools to Allow policy.
    pub async fn allow_all(&self) -> Result<()> {
        {
            let mut config = self.config.write().await;
            for tool in &config.available_tools.clone() {
                config.policies.insert(tool.clone(), ToolPolicy::Allow);
            }
            config.default_policy = ToolPolicy::Allow;
        }
        self.save().await
    }

    /// Set all tools to Deny policy.
    pub async fn deny_all(&self) -> Result<()> {
        {
            let mut config = self.config.write().await;
            for tool in &config.available_tools.clone() {
                config.policies.insert(tool.clone(), ToolPolicy::Deny);
            }
            config.default_policy = ToolPolicy::Deny;
        }
        self.save().await
    }

    /// Reset all tools to Prompt policy.
    pub async fn reset_to_prompt(&self) -> Result<()> {
        {
            let mut config = self.config.write().await;
            config.policies.clear();
            config.default_policy = ToolPolicy::Prompt;
        }
        self.save().await
    }

    /// Reset to default configuration.
    pub async fn reset_to_defaults(&self) -> Result<()> {
        *self.config.write().await = ToolPolicyConfig::default();
        self.save().await
    }

    /// Save configuration to project policy file.
    ///
    /// This saves changes to the project-level policy file (`{workspace}/.qbit/tool-policy.json`).
    /// The merged config is saved, so all current settings are persisted.
    pub async fn save(&self) -> Result<()> {
        self.save_project().await
    }

    /// Save configuration to project policy file.
    pub async fn save_project(&self) -> Result<()> {
        let config = self.config.read().await;

        // Ensure directory exists
        if let Some(parent) = self.project_config_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let json = serde_json::to_string_pretty(&*config)?;
        tokio::fs::write(&self.project_config_path, json).await?;

        // Update project_config cache
        *self.project_config.write().await = Some(config.clone());

        tracing::debug!(
            "Saved project tool policy config to {:?}",
            self.project_config_path
        );
        Ok(())
    }

    /// Save configuration to global policy file (~/.qbit/tool-policy.json).
    pub async fn save_global(&self) -> Result<()> {
        let config = self.config.read().await;

        // Ensure directory exists
        if let Some(parent) = self.global_config_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let json = serde_json::to_string_pretty(&*config)?;
        tokio::fs::write(&self.global_config_path, json).await?;

        // Update global_config cache
        *self.global_config.write().await = Some(config.clone());

        tracing::debug!(
            "Saved global tool policy config to {:?}",
            self.global_config_path
        );
        Ok(())
    }

    /// Reload configuration from both global and project files.
    pub async fn reload(&self) -> Result<()> {
        // Reload global config
        let global_config = Self::load_config_file(&self.global_config_path).await;
        *self.global_config.write().await = global_config.clone();

        // Reload project config
        let project_config = Self::load_config_file(&self.project_config_path).await;
        *self.project_config.write().await = project_config.clone();

        // Re-merge configs
        let merged = Self::merge_configs(&global_config, &project_config);
        *self.config.write().await = merged;

        tracing::debug!(
            "Reloaded tool policy configs (global: {}, project: {})",
            global_config.is_some(),
            project_config.is_some()
        );
        Ok(())
    }

    /// Get the path to the project policy file.
    pub fn project_policy_path(&self) -> &PathBuf {
        &self.project_config_path
    }

    /// Get the path to the global policy file.
    pub fn global_policy_path_ref(&self) -> &PathBuf {
        &self.global_config_path
    }

    /// Print a summary of policy status for debugging.
    pub async fn print_status(&self) {
        let config = self.config.read().await;
        let preapproved = self.preapproved.read().await;
        let full_auto = self.full_auto_allowlist.read().await;

        tracing::info!("=== Tool Policy Status ===");
        tracing::info!("Default policy: {}", config.default_policy);
        tracing::info!("Available tools: {}", config.available_tools.len());
        tracing::info!("Configured policies: {}", config.policies.len());
        tracing::info!("Configured constraints: {}", config.constraints.len());
        tracing::info!("Pre-approved this session: {}", preapproved.len());
        tracing::info!(
            "Full-auto mode: {}",
            if full_auto.is_some() {
                "enabled"
            } else {
                "disabled"
            }
        );

        // Count by policy type
        let allow_count = config
            .policies
            .values()
            .filter(|p| **p == ToolPolicy::Allow)
            .count();
        let prompt_count = config
            .policies
            .values()
            .filter(|p| **p == ToolPolicy::Prompt)
            .count();
        let deny_count = config
            .policies
            .values()
            .filter(|p| **p == ToolPolicy::Deny)
            .count();

        tracing::info!(
            "Policy distribution: {} allow, {} prompt, {} deny",
            allow_count,
            prompt_count,
            deny_count
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_policy_default() {
        assert_eq!(ToolPolicy::default(), ToolPolicy::Prompt);
    }

    #[test]
    fn test_tool_policy_display() {
        assert_eq!(format!("{}", ToolPolicy::Allow), "allow");
        assert_eq!(format!("{}", ToolPolicy::Prompt), "prompt");
        assert_eq!(format!("{}", ToolPolicy::Deny), "deny");
    }

    #[test]
    fn test_constraints_url_blocked() {
        let mut constraints = ToolConstraints::default();
        constraints.blocked_hosts = Some(vec![
            "localhost".to_string(),
            "127.0.0.1".to_string(),
            ".internal".to_string(),
        ]);
        constraints.blocked_schemes = Some(vec!["file://".to_string()]);

        // Blocked hosts
        assert!(constraints.is_url_blocked("http://localhost/api").is_some());
        assert!(constraints
            .is_url_blocked("http://127.0.0.1:8080/")
            .is_some());
        assert!(constraints
            .is_url_blocked("https://app.internal/")
            .is_some());

        // Blocked schemes
        assert!(constraints.is_url_blocked("file:///etc/passwd").is_some());

        // Allowed
        assert!(constraints
            .is_url_blocked("https://api.example.com/")
            .is_none());
    }

    #[test]
    fn test_constraints_path_blocked() {
        let mut constraints = ToolConstraints::default();
        constraints.blocked_patterns = Some(vec!["*.env".to_string(), "**/secrets/*".to_string()]);
        constraints.allowed_extensions = Some(vec![".rs".to_string(), ".ts".to_string()]);

        // Blocked patterns
        assert!(constraints.is_path_blocked(".env").is_some());
        assert!(constraints.is_path_blocked("config/.env").is_some());
        assert!(constraints
            .is_path_blocked("config/secrets/key.txt")
            .is_some());

        // Allowed extensions (only .rs and .ts allowed)
        assert!(constraints.is_path_blocked("main.py").is_some()); // .py not allowed
        assert!(constraints.is_path_blocked("main.rs").is_none()); // .rs allowed
        assert!(constraints.is_path_blocked("app.ts").is_none()); // .ts allowed
    }

    #[test]
    fn test_constraints_mode_allowed() {
        let mut constraints = ToolConstraints::default();
        constraints.allowed_modes = Some(vec!["read".to_string(), "list".to_string()]);

        assert!(constraints.is_mode_allowed("read"));
        assert!(constraints.is_mode_allowed("list"));
        assert!(!constraints.is_mode_allowed("write"));
        assert!(!constraints.is_mode_allowed("delete"));

        // No mode restriction
        let empty_constraints = ToolConstraints::default();
        assert!(empty_constraints.is_mode_allowed("anything"));
    }

    #[test]
    fn test_constraints_limits() {
        let mut constraints = ToolConstraints::default();
        constraints.max_items = Some(100);
        constraints.max_bytes = Some(65536);

        assert!(!constraints.exceeds_max_items(50));
        assert!(!constraints.exceeds_max_items(100));
        assert!(constraints.exceeds_max_items(101));

        assert!(!constraints.exceeds_max_bytes(32768));
        assert!(!constraints.exceeds_max_bytes(65536));
        assert!(constraints.exceeds_max_bytes(65537));
    }

    #[test]
    fn test_default_config() {
        let config = ToolPolicyConfig::default();

        // Check default policies
        assert_eq!(config.policies.get("read_file"), Some(&ToolPolicy::Allow));
        assert_eq!(config.policies.get("write_file"), Some(&ToolPolicy::Prompt));
        assert_eq!(config.policies.get("delete_file"), Some(&ToolPolicy::Deny));

        // Check default policy for unknown tools
        assert_eq!(config.default_policy, ToolPolicy::Prompt);

        // Check constraints exist for web_fetch
        assert!(config.constraints.contains_key("web_fetch"));
    }

    #[tokio::test]
    async fn test_policy_manager_get_set() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = ToolPolicyManager::new(&temp_dir.path().to_path_buf()).await;

        // Default policy for read_file should be Allow
        assert_eq!(manager.get_policy("read_file").await, ToolPolicy::Allow);

        // Default policy for unknown tool should be Prompt
        assert_eq!(manager.get_policy("unknown_tool").await, ToolPolicy::Prompt);

        // Set a policy
        manager
            .set_policy("custom_tool", ToolPolicy::Deny)
            .await
            .unwrap();
        assert_eq!(manager.get_policy("custom_tool").await, ToolPolicy::Deny);
    }

    #[tokio::test]
    async fn test_policy_manager_preapproval() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = ToolPolicyManager::new(&temp_dir.path().to_path_buf()).await;

        // Initially, write_file requires prompt
        assert!(manager.requires_prompt("write_file").await);
        assert!(!manager.should_execute("write_file").await);

        // Pre-approve
        manager.preapprove("write_file").await;

        // Now it should execute without prompt
        assert!(!manager.requires_prompt("write_file").await);
        assert!(manager.should_execute("write_file").await);

        // Take pre-approval (one-time use)
        assert!(manager.take_preapproved("write_file").await);
        assert!(!manager.take_preapproved("write_file").await); // Already consumed

        // Back to requiring prompt
        assert!(manager.requires_prompt("write_file").await);
    }

    #[tokio::test]
    async fn test_policy_manager_full_auto() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = ToolPolicyManager::new(&temp_dir.path().to_path_buf()).await;

        // Initially not in full-auto mode
        assert!(!manager.is_full_auto_enabled().await);
        assert!(!manager.is_allowed_in_full_auto("write_file").await);

        // Enable full-auto with specific tools
        manager
            .enable_full_auto(vec!["read_file".to_string(), "write_file".to_string()])
            .await;

        assert!(manager.is_full_auto_enabled().await);
        assert!(manager.is_allowed_in_full_auto("read_file").await);
        assert!(manager.is_allowed_in_full_auto("write_file").await);
        assert!(!manager.is_allowed_in_full_auto("delete_file").await);

        // Policy should return Allow for full-auto tools
        assert_eq!(manager.get_policy("write_file").await, ToolPolicy::Allow);

        // Disable full-auto
        manager.disable_full_auto().await;
        assert!(!manager.is_full_auto_enabled().await);

        // Back to normal policy
        assert_eq!(manager.get_policy("write_file").await, ToolPolicy::Prompt);
    }

    #[tokio::test]
    async fn test_apply_constraints() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = ToolPolicyManager::new(&temp_dir.path().to_path_buf()).await;

        // Test URL constraint violation
        let args = serde_json::json!({
            "url": "http://localhost:8080/api"
        });
        let result = manager.apply_constraints("web_fetch", &args).await;
        assert!(matches!(result, PolicyConstraintResult::Violated(_)));

        // Test allowed URL
        let args = serde_json::json!({
            "url": "https://api.example.com/"
        });
        let result = manager.apply_constraints("web_fetch", &args).await;
        assert!(matches!(result, PolicyConstraintResult::Allowed));
    }

    #[test]
    fn test_merge_configs() {
        // Test merging global and project configs

        // Global config: read_file=Allow, custom_tool=Deny
        let mut global = ToolPolicyConfig::default();
        global
            .policies
            .insert("custom_tool".to_string(), ToolPolicy::Deny);
        global
            .policies
            .insert("global_only_tool".to_string(), ToolPolicy::Allow);
        global.default_policy = ToolPolicy::Deny;

        // Project config: custom_tool=Allow (overrides global), project_tool=Prompt
        let mut project = ToolPolicyConfig {
            version: 1,
            available_tools: vec![],
            policies: HashMap::new(),
            constraints: HashMap::new(),
            default_policy: ToolPolicy::Prompt,
        };
        project
            .policies
            .insert("custom_tool".to_string(), ToolPolicy::Allow);
        project
            .policies
            .insert("project_tool".to_string(), ToolPolicy::Prompt);

        // Merge: project overrides global
        let merged = ToolPolicyManager::merge_configs(&Some(global), &Some(project));

        // custom_tool should be Allow (project overrides global's Deny)
        assert_eq!(merged.policies.get("custom_tool"), Some(&ToolPolicy::Allow));

        // global_only_tool should be Allow (from global, not in project)
        assert_eq!(
            merged.policies.get("global_only_tool"),
            Some(&ToolPolicy::Allow)
        );

        // project_tool should be Prompt (from project)
        assert_eq!(
            merged.policies.get("project_tool"),
            Some(&ToolPolicy::Prompt)
        );

        // default_policy should be Prompt (from project, overrides global's Deny)
        assert_eq!(merged.default_policy, ToolPolicy::Prompt);
    }

    #[test]
    fn test_merge_configs_global_only() {
        // Test when only global config exists
        let mut global = ToolPolicyConfig::default();
        global
            .policies
            .insert("my_tool".to_string(), ToolPolicy::Deny);
        global.default_policy = ToolPolicy::Allow;

        let merged = ToolPolicyManager::merge_configs(&Some(global), &None);

        assert_eq!(merged.policies.get("my_tool"), Some(&ToolPolicy::Deny));
        assert_eq!(merged.default_policy, ToolPolicy::Allow);
    }

    #[test]
    fn test_merge_configs_project_only() {
        // Test when only project config exists
        let mut project = ToolPolicyConfig::default();
        project
            .policies
            .insert("my_tool".to_string(), ToolPolicy::Allow);
        project.default_policy = ToolPolicy::Deny;

        let merged = ToolPolicyManager::merge_configs(&None, &Some(project));

        assert_eq!(merged.policies.get("my_tool"), Some(&ToolPolicy::Allow));
        assert_eq!(merged.default_policy, ToolPolicy::Deny);
    }

    #[test]
    fn test_merge_configs_neither() {
        // Test when neither global nor project config exists
        let merged = ToolPolicyManager::merge_configs(&None, &None);

        // Should use defaults
        assert_eq!(merged.policies.get("read_file"), Some(&ToolPolicy::Allow));
        assert_eq!(merged.default_policy, ToolPolicy::Prompt);
    }
}

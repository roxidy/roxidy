//! Approval recording and pattern learning for HITL.
//!
//! This module tracks tool approval decisions and learns patterns to enable
//! automatic approval for frequently-approved tools.

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Minimum number of approvals required before auto-approve is considered.
pub const HITL_AUTO_APPROVE_MIN_APPROVALS: u32 = 3;

/// Approval rate threshold for auto-approve (80%).
pub const HITL_AUTO_APPROVE_THRESHOLD: f64 = 0.8;

/// Configuration for tool approval behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolApprovalConfig {
    /// Tools that are always allowed without approval
    pub always_allow: Vec<String>,
    /// Tools that always require approval (cannot be auto-approved)
    pub always_require_approval: Vec<String>,
    /// Whether pattern learning is enabled
    pub pattern_learning_enabled: bool,
    /// Minimum approvals before auto-approve
    pub min_approvals: u32,
    /// Approval rate threshold (0.0 - 1.0)
    pub approval_threshold: f64,
}

impl Default for ToolApprovalConfig {
    fn default() -> Self {
        Self {
            // Safe read-only tools
            always_allow: vec![
                "read_file".to_string(),
                "grep_file".to_string(),
                "list_files".to_string(),
                "indexer_search_code".to_string(),
                "indexer_search_files".to_string(),
                "indexer_analyze_file".to_string(),
                "indexer_extract_symbols".to_string(),
                "indexer_get_metrics".to_string(),
                "indexer_detect_language".to_string(),
                "debug_agent".to_string(),
                "analyze_agent".to_string(),
                "get_errors".to_string(),
                "list_skills".to_string(),
                "search_skills".to_string(),
                "load_skill".to_string(),
                "search_tools".to_string(),
            ],
            // Dangerous tools that should always require approval
            always_require_approval: vec![
                "delete_file".to_string(),
                "run_pty_cmd".to_string(),
                "execute_code".to_string(),
            ],
            pattern_learning_enabled: true,
            min_approvals: HITL_AUTO_APPROVE_MIN_APPROVALS,
            approval_threshold: HITL_AUTO_APPROVE_THRESHOLD,
        }
    }
}

/// Request for tool approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Unique ID for this request
    pub request_id: String,
    /// Name of the tool requesting approval
    pub tool_name: String,
    /// Tool arguments
    pub args: serde_json::Value,
    /// Current approval stats for this tool
    pub current_stats: Option<ApprovalPattern>,
    /// Whether this tool can potentially be auto-approved in the future
    pub can_learn: bool,
    /// Risk level of this tool
    pub risk_level: RiskLevel,
}

/// Risk level for a tool operation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// Safe operations (read-only)
    Low,
    /// Operations that modify state but are recoverable
    Medium,
    /// Operations that can cause significant changes
    High,
    /// Destructive or irreversible operations
    Critical,
}

impl RiskLevel {
    /// Determine risk level for a tool based on its name.
    pub fn for_tool(tool_name: &str) -> Self {
        match tool_name {
            // Read-only operations
            "read_file" | "grep_file" | "list_files" => RiskLevel::Low,
            "indexer_search_code" | "indexer_search_files" | "indexer_analyze_file" => {
                RiskLevel::Low
            }
            "indexer_extract_symbols" | "indexer_get_metrics" | "indexer_detect_language" => {
                RiskLevel::Low
            }
            "debug_agent" | "analyze_agent" | "get_errors" => RiskLevel::Low,
            "list_skills" | "search_skills" | "load_skill" | "search_tools" => RiskLevel::Low,
            "update_plan" => RiskLevel::Low,
            "web_fetch" => RiskLevel::Low,

            // Write operations (recoverable)
            "write_file" | "create_file" | "edit_file" | "apply_patch" => RiskLevel::Medium,
            "save_skill" => RiskLevel::Medium,

            // Shell execution
            "run_pty_cmd" => RiskLevel::High,
            "create_pty_session" | "send_pty_input" => RiskLevel::High,

            // Destructive operations
            "delete_file" => RiskLevel::Critical,
            "execute_code" => RiskLevel::Critical,

            // Default for unknown tools
            _ => {
                // Sub-agents are medium risk
                if tool_name.starts_with("sub_agent_") {
                    RiskLevel::Medium
                } else {
                    RiskLevel::High
                }
            }
        }
    }
}

/// User's decision on an approval request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecision {
    /// The request ID this decision is for
    pub request_id: String,
    /// Whether the tool was approved
    pub approved: bool,
    /// Optional reason/justification for the decision
    pub reason: Option<String>,
    /// Whether to remember this decision for future auto-approval
    pub remember: bool,
    /// Whether to always allow this specific tool
    pub always_allow: bool,
}

/// Approval pattern/statistics for a specific tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalPattern {
    /// Name of the tool
    pub tool_name: String,
    /// Total number of approval requests
    pub total_requests: u32,
    /// Number of approvals
    pub approvals: u32,
    /// Number of denials
    pub denials: u32,
    /// Whether this tool has been marked as "always allow"
    pub always_allow: bool,
    /// Last time this pattern was updated
    pub last_updated: DateTime<Utc>,
    /// Justifications provided (for auditing)
    pub justifications: Vec<String>,
}

impl ApprovalPattern {
    /// Create a new pattern for a tool.
    pub fn new(tool_name: String) -> Self {
        Self {
            tool_name,
            total_requests: 0,
            approvals: 0,
            denials: 0,
            always_allow: false,
            last_updated: Utc::now(),
            justifications: Vec::new(),
        }
    }

    /// Calculate the approval rate (0.0 - 1.0).
    pub fn approval_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.approvals as f64 / self.total_requests as f64
        }
    }

    /// Check if this pattern qualifies for auto-approval based on thresholds.
    pub fn qualifies_for_auto_approve(&self, min_approvals: u32, threshold: f64) -> bool {
        self.approvals >= min_approvals && self.approval_rate() >= threshold
    }

    /// Record an approval decision.
    pub fn record_decision(&mut self, approved: bool, reason: Option<String>) {
        self.total_requests += 1;
        if approved {
            self.approvals += 1;
        } else {
            self.denials += 1;
        }
        self.last_updated = Utc::now();

        if let Some(r) = reason {
            if !r.is_empty() {
                // Keep last 10 justifications for auditing
                if self.justifications.len() >= 10 {
                    self.justifications.remove(0);
                }
                self.justifications.push(r);
            }
        }
    }
}

/// Persisted approval data.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApprovalData {
    /// Version for future migrations
    version: u32,
    /// Approval patterns by tool name
    patterns: HashMap<String, ApprovalPattern>,
    /// Configuration
    config: ToolApprovalConfig,
}

impl Default for ApprovalData {
    fn default() -> Self {
        Self {
            version: 1,
            patterns: HashMap::new(),
            config: ToolApprovalConfig::default(),
        }
    }
}

/// Records and manages tool approval patterns.
///
/// Thread-safe wrapper around approval data with persistence.
pub struct ApprovalRecorder {
    /// Approval data (patterns and config)
    data: Arc<RwLock<ApprovalData>>,
    /// Path to the persistence file
    storage_path: PathBuf,
}

impl ApprovalRecorder {
    /// Create a new ApprovalRecorder with the given storage directory.
    pub async fn new(storage_dir: PathBuf) -> Self {
        let storage_path = storage_dir.join("approval_patterns.json");

        // Try to load existing data
        let data = if storage_path.exists() {
            match tokio::fs::read_to_string(&storage_path).await {
                Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
                Err(e) => {
                    tracing::warn!("Failed to load approval data: {}", e);
                    ApprovalData::default()
                }
            }
        } else {
            ApprovalData::default()
        };

        Self {
            data: Arc::new(RwLock::new(data)),
            storage_path,
        }
    }

    /// Check if a tool should be auto-approved.
    ///
    /// Returns `true` if:
    /// - Tool is in the always_allow list, OR
    /// - Pattern learning is enabled AND the tool has enough approvals with high rate
    pub async fn should_auto_approve(&self, tool_name: &str) -> bool {
        let data = self.data.read().await;

        // Check always_allow list
        if data.config.always_allow.contains(&tool_name.to_string()) {
            return true;
        }

        // Check if tool is in always_require_approval list
        if data
            .config
            .always_require_approval
            .contains(&tool_name.to_string())
        {
            return false;
        }

        // Check if pattern learning is enabled
        if !data.config.pattern_learning_enabled {
            return false;
        }

        // Check the approval pattern
        if let Some(pattern) = data.patterns.get(tool_name) {
            // Check if explicitly marked as always_allow
            if pattern.always_allow {
                return true;
            }

            // Check if pattern qualifies
            pattern.qualifies_for_auto_approve(
                data.config.min_approvals,
                data.config.approval_threshold,
            )
        } else {
            false
        }
    }

    /// Record an approval decision.
    pub async fn record_approval(
        &self,
        tool_name: &str,
        approved: bool,
        reason: Option<String>,
        always_allow: bool,
    ) -> anyhow::Result<()> {
        let mut data = self.data.write().await;

        // Get or create pattern
        let pattern = data
            .patterns
            .entry(tool_name.to_string())
            .or_insert_with(|| ApprovalPattern::new(tool_name.to_string()));

        // Record the decision
        pattern.record_decision(approved, reason);

        // Handle always_allow
        if always_allow && approved {
            pattern.always_allow = true;
        }

        // Persist to disk
        drop(data);
        self.save().await
    }

    /// Get the approval pattern for a tool.
    pub async fn get_pattern(&self, tool_name: &str) -> Option<ApprovalPattern> {
        let data = self.data.read().await;
        data.patterns.get(tool_name).cloned()
    }

    /// Get all approval patterns.
    pub async fn get_all_patterns(&self) -> Vec<ApprovalPattern> {
        let data = self.data.read().await;
        data.patterns.values().cloned().collect()
    }

    /// Get the current configuration.
    pub async fn get_config(&self) -> ToolApprovalConfig {
        let data = self.data.read().await;
        data.config.clone()
    }

    /// Update the configuration.
    pub async fn set_config(&self, config: ToolApprovalConfig) -> anyhow::Result<()> {
        {
            let mut data = self.data.write().await;
            data.config = config;
        }
        self.save().await
    }

    /// Add a tool to the always_allow list.
    pub async fn add_always_allow(&self, tool_name: &str) -> anyhow::Result<()> {
        {
            let mut data = self.data.write().await;
            if !data.config.always_allow.contains(&tool_name.to_string()) {
                data.config.always_allow.push(tool_name.to_string());
            }
            // Also remove from always_require if present
            data.config
                .always_require_approval
                .retain(|t| t != tool_name);
        }
        self.save().await
    }

    /// Remove a tool from the always_allow list.
    pub async fn remove_always_allow(&self, tool_name: &str) -> anyhow::Result<()> {
        {
            let mut data = self.data.write().await;
            data.config.always_allow.retain(|t| t != tool_name);
            // Also clear the pattern's always_allow flag
            if let Some(pattern) = data.patterns.get_mut(tool_name) {
                pattern.always_allow = false;
            }
        }
        self.save().await
    }

    /// Reset all approval patterns (keep config).
    pub async fn reset_patterns(&self) -> anyhow::Result<()> {
        {
            let mut data = self.data.write().await;
            data.patterns.clear();
        }
        self.save().await
    }

    /// Save approval data to disk.
    async fn save(&self) -> anyhow::Result<()> {
        let data = self.data.read().await;

        // Ensure directory exists
        if let Some(parent) = self.storage_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let json = serde_json::to_string_pretty(&*data)?;
        tokio::fs::write(&self.storage_path, json).await?;

        tracing::debug!("Saved approval patterns to {:?}", self.storage_path);
        Ok(())
    }

    /// Create an approval request for a tool.
    pub async fn create_request(
        &self,
        request_id: String,
        tool_name: &str,
        args: serde_json::Value,
    ) -> ApprovalRequest {
        let data = self.data.read().await;
        let pattern = data.patterns.get(tool_name).cloned();
        let can_learn = !data
            .config
            .always_require_approval
            .contains(&tool_name.to_string());
        let risk_level = RiskLevel::for_tool(tool_name);

        ApprovalRequest {
            request_id,
            tool_name: tool_name.to_string(),
            args,
            current_stats: pattern,
            can_learn,
            risk_level,
        }
    }

    /// Get a suggestion message if a tool is close to auto-approval threshold.
    pub async fn get_suggestion(&self, tool_name: &str) -> Option<String> {
        let data = self.data.read().await;

        if !data.config.pattern_learning_enabled {
            return None;
        }

        if let Some(pattern) = data.patterns.get(tool_name) {
            let rate = pattern.approval_rate();
            let approvals = pattern.approvals;
            let min = data.config.min_approvals;
            let threshold = data.config.approval_threshold;

            // Already qualifies
            if pattern.qualifies_for_auto_approve(min, threshold) {
                return None;
            }

            // Close to threshold - suggest
            if approvals >= 2 && rate >= 0.6 {
                let needed = min.saturating_sub(approvals);
                if needed > 0 {
                    return Some(format!(
                        "You've approved '{}' {} times ({:.0}% approval rate). {} more approval(s) needed for auto-approve.",
                        tool_name, approvals, rate * 100.0, needed
                    ));
                } else if rate < threshold {
                    return Some(format!(
                        "Tool '{}' has {} approvals but only {:.0}% approval rate. Need {:.0}% for auto-approve.",
                        tool_name, approvals, rate * 100.0, threshold * 100.0
                    ));
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approval_pattern_rate() {
        let mut pattern = ApprovalPattern::new("test_tool".to_string());

        // No requests = 0% rate
        assert_eq!(pattern.approval_rate(), 0.0);

        // 3 approvals, 0 denials = 100%
        pattern.record_decision(true, None);
        pattern.record_decision(true, None);
        pattern.record_decision(true, None);
        assert_eq!(pattern.approval_rate(), 1.0);

        // 3 approvals, 1 denial = 75%
        pattern.record_decision(false, None);
        assert_eq!(pattern.approval_rate(), 0.75);
    }

    #[test]
    fn test_approval_pattern_qualification() {
        let mut pattern = ApprovalPattern::new("test_tool".to_string());

        // Not enough approvals
        pattern.record_decision(true, None);
        pattern.record_decision(true, None);
        assert!(!pattern.qualifies_for_auto_approve(3, 0.8));

        // Enough approvals but rate too low
        pattern.record_decision(true, None);
        pattern.record_decision(false, None);
        pattern.record_decision(false, None);
        // 3 approvals, 2 denials = 60%
        assert!(!pattern.qualifies_for_auto_approve(3, 0.8));

        // Meet both thresholds
        pattern.record_decision(true, None);
        pattern.record_decision(true, None);
        // 5 approvals, 2 denials = ~71%
        assert!(!pattern.qualifies_for_auto_approve(3, 0.8));

        pattern.record_decision(true, None);
        // 6 approvals, 2 denials = 75%
        assert!(!pattern.qualifies_for_auto_approve(3, 0.8));

        pattern.record_decision(true, None);
        // 7 approvals, 2 denials = ~78%
        assert!(!pattern.qualifies_for_auto_approve(3, 0.8));

        pattern.record_decision(true, None);
        // 8 approvals, 2 denials = 80%
        assert!(pattern.qualifies_for_auto_approve(3, 0.8));
    }

    #[test]
    fn test_risk_level_classification() {
        assert_eq!(RiskLevel::for_tool("read_file"), RiskLevel::Low);
        assert_eq!(RiskLevel::for_tool("write_file"), RiskLevel::Medium);
        assert_eq!(RiskLevel::for_tool("run_pty_cmd"), RiskLevel::High);
        assert_eq!(RiskLevel::for_tool("delete_file"), RiskLevel::Critical);
        assert_eq!(RiskLevel::for_tool("sub_agent_analyzer"), RiskLevel::Medium);
        assert_eq!(RiskLevel::for_tool("unknown_tool"), RiskLevel::High);
    }
}

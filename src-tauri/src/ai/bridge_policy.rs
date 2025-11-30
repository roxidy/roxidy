//! Tool policy extension for AgentBridge.
//!
//! This module contains methods for managing tool policies (allow/prompt/deny rules).

use anyhow::Result;

use super::agent_bridge::AgentBridge;
use super::loop_detection::{LoopDetectorStats, LoopProtectionConfig};
use super::tool_policy::{ToolPolicy, ToolPolicyConfig};

impl AgentBridge {
    // ========================================================================
    // Tool Policy Methods
    // ========================================================================

    /// Get the tool policy configuration.
    pub async fn get_tool_policy_config(&self) -> ToolPolicyConfig {
        self.tool_policy_manager.get_config().await
    }

    /// Set the tool policy configuration.
    pub async fn set_tool_policy_config(&self, config: ToolPolicyConfig) -> Result<()> {
        self.tool_policy_manager.set_config(config).await
    }

    /// Get the policy for a specific tool.
    pub async fn get_tool_policy(&self, tool_name: &str) -> ToolPolicy {
        self.tool_policy_manager.get_policy(tool_name).await
    }

    /// Set the policy for a specific tool.
    pub async fn set_tool_policy(&self, tool_name: &str, policy: ToolPolicy) -> Result<()> {
        self.tool_policy_manager.set_policy(tool_name, policy).await
    }

    /// Reset tool policies to defaults.
    pub async fn reset_tool_policies(&self) -> Result<()> {
        self.tool_policy_manager.reset_to_defaults().await
    }

    /// Enable full-auto mode with the given allowed tools.
    pub async fn enable_full_auto_mode(&self, allowed_tools: Vec<String>) {
        self.tool_policy_manager
            .enable_full_auto(allowed_tools)
            .await;
    }

    /// Disable full-auto mode.
    pub async fn disable_full_auto_mode(&self) {
        self.tool_policy_manager.disable_full_auto().await;
    }

    /// Check if full-auto mode is enabled.
    pub async fn is_full_auto_mode_enabled(&self) -> bool {
        self.tool_policy_manager.is_full_auto_enabled().await
    }

    // ========================================================================
    // Loop Protection Methods
    // ========================================================================

    /// Get the loop protection configuration.
    pub async fn get_loop_protection_config(&self) -> LoopProtectionConfig {
        self.loop_detector.read().await.config().clone()
    }

    /// Set the loop protection configuration.
    pub async fn set_loop_protection_config(&self, config: LoopProtectionConfig) {
        self.loop_detector.write().await.set_config(config);
    }

    /// Get current loop detector statistics.
    pub async fn get_loop_detector_stats(&self) -> LoopDetectorStats {
        self.loop_detector.read().await.stats()
    }

    /// Check if loop detection is currently enabled.
    pub async fn is_loop_detection_enabled(&self) -> bool {
        self.loop_detector.read().await.is_enabled()
    }

    /// Disable loop detection for the current session.
    pub async fn disable_loop_detection_for_session(&self) {
        self.loop_detector.write().await.disable_for_session();
    }

    /// Re-enable loop detection.
    pub async fn enable_loop_detection(&self) {
        self.loop_detector.write().await.enable();
    }

    /// Reset the loop detector.
    pub async fn reset_loop_detector(&self) {
        self.loop_detector.write().await.reset();
    }
}

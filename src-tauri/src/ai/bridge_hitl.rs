//! HITL (Human-in-the-Loop) extension for AgentBridge.
//!
//! This module contains methods for managing tool approval patterns and decisions.

use anyhow::Result;

use super::agent_bridge::AgentBridge;
use super::hitl::{ApprovalDecision, ApprovalPattern, ToolApprovalConfig};

impl AgentBridge {
    // ========================================================================
    // HITL (Human-in-the-Loop) Methods
    // ========================================================================

    /// Get all approval patterns.
    pub async fn get_approval_patterns(&self) -> Vec<ApprovalPattern> {
        self.approval_recorder.get_all_patterns().await
    }

    /// Get the approval pattern for a specific tool.
    pub async fn get_tool_approval_pattern(&self, tool_name: &str) -> Option<ApprovalPattern> {
        self.approval_recorder.get_pattern(tool_name).await
    }

    /// Get the HITL configuration.
    pub async fn get_hitl_config(&self) -> ToolApprovalConfig {
        self.approval_recorder.get_config().await
    }

    /// Set the HITL configuration.
    pub async fn set_hitl_config(&self, config: ToolApprovalConfig) -> Result<()> {
        self.approval_recorder.set_config(config).await
    }

    /// Add a tool to the always-allow list.
    pub async fn add_tool_always_allow(&self, tool_name: &str) -> Result<()> {
        self.approval_recorder.add_always_allow(tool_name).await
    }

    /// Remove a tool from the always-allow list.
    pub async fn remove_tool_always_allow(&self, tool_name: &str) -> Result<()> {
        self.approval_recorder.remove_always_allow(tool_name).await
    }

    /// Reset all approval patterns.
    pub async fn reset_approval_patterns(&self) -> Result<()> {
        self.approval_recorder.reset_patterns().await
    }

    /// Respond to a pending approval request.
    pub async fn respond_to_approval(&self, decision: ApprovalDecision) -> Result<()> {
        let sender = {
            let mut pending = self.pending_approvals.write().await;
            pending.remove(&decision.request_id)
        };

        self.approval_recorder
            .record_approval(
                decision
                    .request_id
                    .split('_')
                    .next_back()
                    .unwrap_or("unknown"),
                decision.approved,
                decision.reason.clone(),
                decision.always_allow,
            )
            .await?;

        if let Some(sender) = sender {
            let _ = sender.send(decision);
        } else {
            tracing::warn!(
                "No pending approval found for request_id: {}",
                decision.request_id
            );
        }

        Ok(())
    }
}

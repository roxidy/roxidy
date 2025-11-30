//! Human-in-the-Loop (HITL) module for tool approval management.
//!
//! This module provides:
//! - `ApprovalRecorder`: Tracks approval patterns for tools
//! - `ApprovalPattern`: Statistics for a specific tool
//! - Pattern learning: Auto-approve tools with high approval rates
//!
//! Based on the VTCode implementation pattern.

mod approval_recorder;

pub use approval_recorder::{
    ApprovalDecision, ApprovalPattern, ApprovalRecorder, ApprovalRequest, RiskLevel,
    ToolApprovalConfig,
};

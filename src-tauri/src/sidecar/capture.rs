//! Event capture bridge for the sidecar system.
//!
//! This module provides the integration point between the agentic loop
//! and the sidecar event capture system.

use std::path::PathBuf;
use std::sync::Arc;

use tracing::{debug, trace};

use crate::ai::events::AiEvent;

use super::events::{DecisionType, FeedbackType, FileOperation, SessionEvent};
use super::state::SidecarState;

/// Capture bridge that processes AI events and forwards them to the sidecar
pub struct CaptureContext {
    /// Reference to sidecar state
    sidecar: Arc<SidecarState>,
    /// Last tool name (for correlating requests with results)
    last_tool_name: Option<String>,
    /// Last tool args (for file operations)
    last_tool_args: Option<serde_json::Value>,
}

impl CaptureContext {
    /// Create a new capture context
    pub fn new(sidecar: Arc<SidecarState>) -> Self {
        Self {
            sidecar,
            last_tool_name: None,
            last_tool_args: None,
        }
    }

    /// Process an AI event and capture relevant information
    pub fn process(&mut self, event: &AiEvent) {
        // Skip if no active session
        let session_id = match self.sidecar.current_session_id() {
            Some(id) => id,
            None => {
                trace!("[sidecar-capture] No active session, skipping event");
                return;
            }
        };

        match event {
            AiEvent::ToolRequest {
                tool_name, args, ..
            } => {
                debug!("[sidecar-capture] Tool request: {}", tool_name);
                // Store for later correlation with result
                self.last_tool_name = Some(tool_name.clone());
                self.last_tool_args = Some(args.clone());
            }

            AiEvent::ToolResult {
                tool_name,
                success,
                ..
            } => {
                debug!("[sidecar-capture] Tool result: {} success={}", tool_name, success);

                // Capture file operations
                if let Some(event) = self.create_file_event(session_id, tool_name, *success) {
                    debug!("[sidecar-capture] Captured file event for {}", tool_name);
                    self.sidecar.capture(event);
                }

                // Capture tool call summary
                let args_summary = self
                    .last_tool_args
                    .as_ref()
                    .map(|a| summarize_args(a))
                    .unwrap_or_else(|| "{}".to_string());

                let event = SessionEvent::tool_call(
                    session_id,
                    tool_name,
                    &args_summary,
                    None, // Reasoning is captured separately
                    *success,
                );
                self.sidecar.capture(event);
                debug!("[sidecar-capture] Captured tool call event");

                // Clear last tool info
                self.last_tool_name = None;
                self.last_tool_args = None;
            }

            AiEvent::Reasoning { content } => {
                // High-signal event - capture agent's extended thinking/reasoning
                debug!(
                    "[sidecar-capture] Captured reasoning ({} chars)",
                    content.len()
                );
                let decision_type = infer_decision_type(content);
                let event = SessionEvent::reasoning(session_id, content, decision_type);
                self.sidecar.capture(event);
            }

            AiEvent::Completed {
                response,
                duration_ms,
                ..
            } => {
                // Capture the final AI response for searchability
                if !response.is_empty() {
                    debug!(
                        "[sidecar-capture] Captured AI response ({} chars, {:?}ms)",
                        response.len(),
                        duration_ms
                    );
                    let event = SessionEvent::ai_response(session_id, response, *duration_ms);
                    self.sidecar.capture(event);
                }
            }

            AiEvent::ToolAutoApproved {
                tool_name, reason, ..
            } => {
                debug!("[sidecar-capture] Tool auto-approved: {}", tool_name);
                let event = SessionEvent::feedback(
                    session_id,
                    FeedbackType::Approve,
                    Some(tool_name.clone()),
                    Some(format!("Auto-approved: {}", reason)),
                );
                self.sidecar.capture(event);
            }

            AiEvent::ToolDenied {
                tool_name, reason, ..
            } => {
                debug!("[sidecar-capture] Tool denied: {} - {}", tool_name, reason);
                let event = SessionEvent::feedback(
                    session_id,
                    FeedbackType::Deny,
                    Some(tool_name.clone()),
                    Some(reason.clone()),
                );
                self.sidecar.capture(event);
            }

            AiEvent::Error { message, .. } => {
                debug!("[sidecar-capture] Error captured: {}", message);
                let event = SessionEvent::error(session_id, message, None, false);
                self.sidecar.capture(event);
            }

            // Ignore streaming events and other low-signal events
            AiEvent::Started { .. }
            | AiEvent::TextDelta { .. }
            | AiEvent::ToolApprovalRequest { .. }
            | AiEvent::SubAgentStarted { .. }
            | AiEvent::SubAgentToolRequest { .. }
            | AiEvent::SubAgentToolResult { .. }
            | AiEvent::SubAgentCompleted { .. }
            | AiEvent::SubAgentError { .. }
            | AiEvent::ContextPruned { .. }
            | AiEvent::ContextWarning { .. }
            | AiEvent::ToolResponseTruncated { .. }
            | AiEvent::LoopWarning { .. }
            | AiEvent::LoopBlocked { .. }
            | AiEvent::MaxIterationsReached { .. }
            | AiEvent::WorkflowStarted { .. }
            | AiEvent::WorkflowStepStarted { .. }
            | AiEvent::WorkflowStepCompleted { .. }
            | AiEvent::WorkflowCompleted { .. }
            | AiEvent::WorkflowError { .. } => {
                trace!("[sidecar-capture] Ignoring low-signal event: {:?}", std::any::type_name::<AiEvent>());
            }
        }
    }

    /// Create a file event from tool info
    fn create_file_event(
        &self,
        session_id: uuid::Uuid,
        tool_name: &str,
        success: bool,
    ) -> Option<SessionEvent> {
        // Only create file events for successful file operations
        if !success {
            return None;
        }

        let args = self.last_tool_args.as_ref()?;

        let (path, operation) = match tool_name {
            "write" | "create_file" => {
                let path = args.get("file_path").or_else(|| args.get("path"))?;
                let path = PathBuf::from(path.as_str()?);
                // Check if it's a create or modify based on file existence
                // For simplicity, treat all writes as creates (can be improved)
                (path, FileOperation::Create)
            }
            "edit" | "edit_file" | "apply_patch" => {
                let path = args.get("file_path").or_else(|| args.get("path"))?;
                let path = PathBuf::from(path.as_str()?);
                (path, FileOperation::Modify)
            }
            "delete" | "delete_file" | "remove_file" => {
                let path = args.get("file_path").or_else(|| args.get("path"))?;
                let path = PathBuf::from(path.as_str()?);
                (path, FileOperation::Delete)
            }
            "rename" | "move_file" => {
                let from = args.get("from").or_else(|| args.get("source"))?;
                let to = args.get("to").or_else(|| args.get("destination"))?;
                let from_path = PathBuf::from(from.as_str()?);
                let to_path = PathBuf::from(to.as_str()?);
                (
                    to_path,
                    FileOperation::Rename { from: from_path },
                )
            }
            _ => return None,
        };

        Some(SessionEvent::file_edit(session_id, path, operation, None))
    }
}

/// Summarize tool arguments for logging
fn summarize_args(args: &serde_json::Value) -> String {
    match args {
        serde_json::Value::Object(map) => {
            let mut parts = Vec::new();

            // Extract key fields
            if let Some(path) = map.get("path").or_else(|| map.get("file_path")) {
                if let Some(s) = path.as_str() {
                    parts.push(format!("path={}", truncate_path(s)));
                }
            }

            if let Some(query) = map.get("query") {
                if let Some(s) = query.as_str() {
                    parts.push(format!("query={}", truncate(s, 30)));
                }
            }

            if let Some(cmd) = map.get("command") {
                if let Some(s) = cmd.as_str() {
                    parts.push(format!("cmd={}", truncate(s, 30)));
                }
            }

            if parts.is_empty() {
                // Just show field names
                let keys: Vec<_> = map.keys().take(3).cloned().collect();
                format!("{{{}}}", keys.join(", "))
            } else {
                parts.join(", ")
            }
        }
        _ => truncate(&args.to_string(), 50),
    }
}

/// Infer decision type from reasoning content
fn infer_decision_type(content: &str) -> Option<DecisionType> {
    let content_lower = content.to_lowercase();

    if content_lower.contains("instead of")
        || content_lower.contains("i'll use")
        || content_lower.contains("choosing")
        || content_lower.contains("approach")
    {
        Some(DecisionType::ApproachChoice)
    } else if content_lower.contains("tradeoff")
        || content_lower.contains("trade-off")
        || content_lower.contains("sacrific")
    {
        Some(DecisionType::Tradeoff)
    } else if content_lower.contains("didn't work")
        || content_lower.contains("failed")
        || content_lower.contains("trying")
        || content_lower.contains("fallback")
    {
        Some(DecisionType::Fallback)
    } else if content_lower.contains("assuming")
        || content_lower.contains("i assume")
        || content_lower.contains("presume")
    {
        Some(DecisionType::Assumption)
    } else {
        None
    }
}

/// Truncate a string
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let mut result: String = s.chars().take(max_len.saturating_sub(3)).collect();
        result.push_str("...");
        result
    }
}

/// Truncate a path, keeping the filename
fn truncate_path(path: &str) -> String {
    if path.len() <= 40 {
        return path.to_string();
    }

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 2 {
        return truncate(path, 40);
    }

    // Keep first and last parts
    format!("{}/.../{}",
        parts.first().unwrap_or(&""),
        parts.last().unwrap_or(&""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summarize_args() {
        let args = serde_json::json!({
            "path": "/src/lib.rs",
            "content": "some long content here"
        });

        let summary = summarize_args(&args);
        assert!(summary.contains("path="));
    }

    #[test]
    fn test_infer_decision_type() {
        assert_eq!(
            infer_decision_type("I'll use approach A instead of B"),
            Some(DecisionType::ApproachChoice)
        );

        assert_eq!(
            infer_decision_type("This is a tradeoff between speed and safety"),
            Some(DecisionType::Tradeoff)
        );

        // "approach" keyword triggers ApproachChoice before Fallback check
        assert_eq!(
            infer_decision_type("That didn't work, trying another approach"),
            Some(DecisionType::ApproachChoice)
        );

        // Test pure fallback case
        assert_eq!(
            infer_decision_type("That didn't work, trying something else"),
            Some(DecisionType::Fallback)
        );

        assert_eq!(
            infer_decision_type("Assuming the user wants JSON output"),
            Some(DecisionType::Assumption)
        );

        assert_eq!(
            infer_decision_type("Just a regular comment"),
            None
        );
    }

    #[test]
    fn test_truncate_path() {
        assert_eq!(truncate_path("/short/path.rs"), "/short/path.rs");
        // Path exactly at 40 chars doesn't get truncated
        assert_eq!(truncate_path("/this/is/exactly/forty/chars/file.rs"), "/this/is/exactly/forty/chars/file.rs");
        // Path over 40 chars with multiple segments gets truncated
        assert_eq!(
            truncate_path("/very/long/deeply/nested/path/to/some/file.rs"),
            "/.../file.rs"
        );
    }
}

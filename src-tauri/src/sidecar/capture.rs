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

/// Maximum length for tool output storage
const MAX_TOOL_OUTPUT_LEN: usize = 2000;
/// Maximum length for diff storage
const MAX_DIFF_LEN: usize = 4000;

/// Capture bridge that processes AI events and forwards them to the sidecar
pub struct CaptureContext {
    /// Reference to sidecar state
    sidecar: Arc<SidecarState>,
    /// Last tool name (for correlating requests with results)
    last_tool_name: Option<String>,
    /// Last tool args (for file operations)
    last_tool_args: Option<serde_json::Value>,
    /// Pending old content for generating diffs (path -> content)
    pending_old_content: Option<(PathBuf, String)>,
}

impl CaptureContext {
    /// Create a new capture context
    pub fn new(sidecar: Arc<SidecarState>) -> Self {
        Self {
            sidecar,
            last_tool_name: None,
            last_tool_args: None,
            pending_old_content: None,
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

                // For edit operations, try to capture old content for diff generation
                if is_edit_tool(tool_name) {
                    if let Some(path) = extract_path_from_args(args) {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            self.pending_old_content = Some((path, content));
                        }
                    }
                }
            }

            AiEvent::ToolResult {
                tool_name,
                result,
                success,
                ..
            } => {
                debug!(
                    "[sidecar-capture] Tool result: {} success={}",
                    tool_name, success
                );

                // Capture file operations (keep for backwards compat)
                if let Some(event) = self.create_file_event(session_id, tool_name, *success) {
                    debug!("[sidecar-capture] Captured file event for {}", tool_name);
                    self.sidecar.capture(event);
                }

                // Extract tool output
                let tool_output = extract_tool_output(result);

                // Extract files_accessed for read operations
                let files_accessed = if is_read_tool(tool_name) {
                    extract_files_from_result(tool_name, &self.last_tool_args, result)
                } else {
                    None
                };

                // Extract files_modified for write operations
                let files_modified = if is_write_tool(tool_name) && *success {
                    extract_files_modified(tool_name, self.last_tool_args.as_ref())
                } else {
                    vec![]
                };

                // Generate diff for edit operations
                let diff = if is_edit_tool(tool_name) && *success {
                    self.generate_diff(tool_name, &self.last_tool_args)
                } else {
                    None
                };

                // Capture tool call summary with enhanced data
                let args_summary = self
                    .last_tool_args
                    .as_ref()
                    .map(summarize_args)
                    .unwrap_or_else(|| "{}".to_string());

                let event = SessionEvent::tool_call_with_output(
                    session_id,
                    tool_name,
                    &args_summary,
                    *success,
                    tool_output,
                    files_accessed,
                    files_modified,
                    diff,
                );
                self.sidecar.capture(event);
                debug!("[sidecar-capture] Captured enhanced tool call event");

                // Clear last tool info
                self.last_tool_name = None;
                self.last_tool_args = None;
                self.pending_old_content = None;
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
                trace!(
                    "[sidecar-capture] Ignoring low-signal event: {:?}",
                    std::any::type_name::<AiEvent>()
                );
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
                (to_path, FileOperation::Rename { from: from_path })
            }
            _ => return None,
        };

        Some(SessionEvent::file_edit(session_id, path, operation, None))
    }

    /// Generate a unified diff for an edit operation
    fn generate_diff(
        &self,
        tool_name: &str,
        args: &Option<serde_json::Value>,
    ) -> Option<String> {
        let args = args.as_ref()?;

        match tool_name {
            "write" | "create_file" => {
                // For new files, show the full content as additions
                let path = extract_path_from_args(args)?;
                let new_content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let diff = format!(
                    "--- /dev/null\n+++ {}\n@@ -0,0 +1,{} @@\n{}",
                    path.display(),
                    new_content.lines().count(),
                    new_content
                        .lines()
                        .map(|l| format!("+{}", l))
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                Some(truncate(&diff, MAX_DIFF_LEN))
            }
            "edit" | "edit_file" | "str_replace" | "apply_patch" => {
                // For edits, try to generate a proper diff
                if let Some((path, old_content)) = &self.pending_old_content {
                    // Try to read the new content
                    if let Ok(new_content) = std::fs::read_to_string(path) {
                        let diff = generate_unified_diff(
                            &path.to_string_lossy(),
                            old_content,
                            &new_content,
                        );
                        return Some(truncate(&diff, MAX_DIFF_LEN));
                    }
                }

                // Fallback: show old/new from args if available
                let path = extract_path_from_args(args)?;
                let old = args
                    .get("old_string")
                    .or_else(|| args.get("old"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let new = args
                    .get("new_string")
                    .or_else(|| args.get("new"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if !old.is_empty() || !new.is_empty() {
                    let diff = format!(
                        "--- {}\n+++ {}\n@@ @@\n{}\n{}",
                        path.display(),
                        path.display(),
                        old.lines()
                            .map(|l| format!("-{}", l))
                            .collect::<Vec<_>>()
                            .join("\n"),
                        new.lines()
                            .map(|l| format!("+{}", l))
                            .collect::<Vec<_>>()
                            .join("\n")
                    );
                    Some(truncate(&diff, MAX_DIFF_LEN))
                } else {
                    None
                }
            }
            "delete" | "delete_file" | "remove_file" => {
                // For deletions, show removed content if we have it
                if let Some((path, old_content)) = &self.pending_old_content {
                    let diff = format!(
                        "--- {}\n+++ /dev/null\n@@ -1,{} +0,0 @@\n{}",
                        path.display(),
                        old_content.lines().count(),
                        old_content
                            .lines()
                            .map(|l| format!("-{}", l))
                            .collect::<Vec<_>>()
                            .join("\n")
                    );
                    Some(truncate(&diff, MAX_DIFF_LEN))
                } else {
                    let path = extract_path_from_args(args)?;
                    Some(format!("--- {}\n+++ /dev/null", path.display()))
                }
            }
            _ => None,
        }
    }
}

/// Check if a tool is a read-only file tool
fn is_read_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "list_files" | "read_file" | "read" | "glob" | "grep" | "search" | "list_directory"
    )
}

/// Check if a tool writes files (create/delete)
fn is_write_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "write" | "create_file" | "delete" | "delete_file" | "remove_file" | "rename" | "move_file"
    )
}

/// Check if a tool is an edit operation that modifies file content
fn is_edit_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "write"
            | "create_file"
            | "edit"
            | "edit_file"
            | "str_replace"
            | "apply_patch"
            | "delete"
            | "delete_file"
            | "remove_file"
    )
}

/// Extract tool output as a string
fn extract_tool_output(result: &serde_json::Value) -> Option<String> {
    // Try to get string content directly
    if let Some(s) = result.as_str() {
        return Some(truncate(s, MAX_TOOL_OUTPUT_LEN));
    }

    // Try to get content from common fields
    if let Some(content) = result.get("content").and_then(|v| v.as_str()) {
        return Some(truncate(content, MAX_TOOL_OUTPUT_LEN));
    }

    if let Some(output) = result.get("output").and_then(|v| v.as_str()) {
        return Some(truncate(output, MAX_TOOL_OUTPUT_LEN));
    }

    // For objects, serialize to string
    if result.is_object() || result.is_array() {
        let json_str = serde_json::to_string(result).ok()?;
        return Some(truncate(&json_str, MAX_TOOL_OUTPUT_LEN));
    }

    None
}

/// Extract file paths from tool result based on tool type
fn extract_files_from_result(
    tool_name: &str,
    args: &Option<serde_json::Value>,
    result: &serde_json::Value,
) -> Option<Vec<PathBuf>> {
    match tool_name {
        "read_file" | "read" => {
            // For read, the file is in the args
            args.as_ref()
                .and_then(extract_path_from_args)
                .map(|p| vec![p])
        }
        "list_files" | "list_directory" | "glob" => {
            // Parse file list from result
            let result_str = result.as_str().or_else(|| {
                result
                    .get("content")
                    .or_else(|| result.get("output"))
                    .and_then(|v| v.as_str())
            })?;

            let files: Vec<PathBuf> = result_str
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| {
                    // Clean up the line (remove leading spaces/tree characters)
                    let cleaned = line.trim_start_matches(|c: char| {
                        c.is_whitespace() || c == '│' || c == '├' || c == '└' || c == '─'
                    });
                    PathBuf::from(cleaned.trim())
                })
                .filter(|p| !p.as_os_str().is_empty())
                .collect();

            if files.is_empty() {
                None
            } else {
                Some(files)
            }
        }
        "grep" | "search" => {
            // Extract unique file paths from grep-style output (file:line: content)
            let result_str = result.as_str().or_else(|| {
                result
                    .get("content")
                    .or_else(|| result.get("output"))
                    .and_then(|v| v.as_str())
            })?;

            let files: Vec<PathBuf> = result_str
                .lines()
                .filter_map(|line| {
                    // Parse "file:line: content" format
                    let parts: Vec<&str> = line.splitn(3, ':').collect();
                    if parts.len() >= 2 {
                        Some(PathBuf::from(parts[0].trim()))
                    } else {
                        None
                    }
                })
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            if files.is_empty() {
                None
            } else {
                Some(files)
            }
        }
        _ => None,
    }
}

/// Extract file path from tool arguments
fn extract_path_from_args(args: &serde_json::Value) -> Option<PathBuf> {
    let path = args
        .get("file_path")
        .or_else(|| args.get("path"))
        .and_then(|v| v.as_str())?;
    Some(PathBuf::from(path))
}

/// Extract files_modified based on tool type
/// For rename/move, returns both source and destination paths
fn extract_files_modified(tool_name: &str, args: Option<&serde_json::Value>) -> Vec<PathBuf> {
    let args = match args {
        Some(a) => a,
        None => return vec![],
    };

    match tool_name {
        "rename" | "move_file" => {
            // For rename/move, include both source and destination
            let mut files = Vec::new();
            if let Some(from) = args
                .get("from")
                .or_else(|| args.get("source"))
                .and_then(|v| v.as_str())
            {
                files.push(PathBuf::from(from));
            }
            if let Some(to) = args
                .get("to")
                .or_else(|| args.get("destination"))
                .and_then(|v| v.as_str())
            {
                files.push(PathBuf::from(to));
            }
            files
        }
        _ => {
            // For other write tools, just the target path
            extract_path_from_args(args)
                .map(|p| vec![p])
                .unwrap_or_default()
        }
    }
}

/// Generate a simple unified diff between two strings
fn generate_unified_diff(path: &str, old: &str, new: &str) -> String {
    // Simple line-by-line diff (not a full diff algorithm, but captures changes)
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let mut diff_lines = vec![format!("--- {}", path), format!("+++ {}", path)];

    // Find changes using a simple approach
    let max_len = old_lines.len().max(new_lines.len());
    let mut in_hunk = false;
    let mut hunk_start = 0;

    for i in 0..max_len {
        let old_line = old_lines.get(i);
        let new_line = new_lines.get(i);

        match (old_line, new_line) {
            (Some(o), Some(n)) if o == n => {
                // Lines match, add context if in hunk
                if in_hunk {
                    diff_lines.push(format!(" {}", o));
                }
            }
            (Some(o), Some(n)) => {
                // Lines differ
                if !in_hunk {
                    in_hunk = true;
                    hunk_start = i + 1;
                    diff_lines.push(format!("@@ -{},{} +{},{} @@", hunk_start, 1, hunk_start, 1));
                }
                diff_lines.push(format!("-{}", o));
                diff_lines.push(format!("+{}", n));
            }
            (Some(o), None) => {
                // Old line removed
                if !in_hunk {
                    in_hunk = true;
                    hunk_start = i + 1;
                    diff_lines.push(format!("@@ -{} @@", hunk_start));
                }
                diff_lines.push(format!("-{}", o));
            }
            (None, Some(n)) => {
                // New line added
                if !in_hunk {
                    in_hunk = true;
                    hunk_start = old_lines.len() + 1;
                    diff_lines.push(format!("@@ +{} @@", i + 1));
                }
                diff_lines.push(format!("+{}", n));
            }
            (None, None) => break,
        }
    }

    diff_lines.join("\n")
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
    format!(
        "{}/.../{}",
        parts.first().unwrap_or(&""),
        parts.last().unwrap_or(&"")
    )
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

        assert_eq!(infer_decision_type("Just a regular comment"), None);
    }

    #[test]
    fn test_truncate_path() {
        assert_eq!(truncate_path("/short/path.rs"), "/short/path.rs");
        // Path exactly at 40 chars doesn't get truncated
        assert_eq!(
            truncate_path("/this/is/exactly/forty/chars/file.rs"),
            "/this/is/exactly/forty/chars/file.rs"
        );
        // Path over 40 chars with multiple segments gets truncated
        assert_eq!(
            truncate_path("/very/long/deeply/nested/path/to/some/file.rs"),
            "/.../file.rs"
        );
    }

    #[test]
    fn test_is_read_tool() {
        // Should be read tools
        assert!(is_read_tool("list_files"));
        assert!(is_read_tool("read_file"));
        assert!(is_read_tool("read"));
        assert!(is_read_tool("glob"));
        assert!(is_read_tool("grep"));
        assert!(is_read_tool("search"));
        assert!(is_read_tool("list_directory"));

        // Should NOT be read tools
        assert!(!is_read_tool("write"));
        assert!(!is_read_tool("edit_file"));
        assert!(!is_read_tool("bash"));
        assert!(!is_read_tool("unknown_tool"));
    }

    #[test]
    fn test_is_write_tool() {
        // Should be write tools
        assert!(is_write_tool("write"));
        assert!(is_write_tool("create_file"));
        assert!(is_write_tool("delete"));
        assert!(is_write_tool("delete_file"));
        assert!(is_write_tool("remove_file"));
        assert!(is_write_tool("rename"));
        assert!(is_write_tool("move_file"));

        // Should NOT be write tools
        assert!(!is_write_tool("read_file"));
        assert!(!is_write_tool("edit"));
        assert!(!is_write_tool("bash"));
    }

    #[test]
    fn test_is_edit_tool() {
        // Should be edit tools
        assert!(is_edit_tool("write"));
        assert!(is_edit_tool("create_file"));
        assert!(is_edit_tool("edit"));
        assert!(is_edit_tool("edit_file"));
        assert!(is_edit_tool("str_replace"));
        assert!(is_edit_tool("apply_patch"));
        assert!(is_edit_tool("delete"));
        assert!(is_edit_tool("delete_file"));
        assert!(is_edit_tool("remove_file"));

        // Should NOT be edit tools
        assert!(!is_edit_tool("read_file"));
        assert!(!is_edit_tool("list_files"));
        assert!(!is_edit_tool("bash"));
    }

    #[test]
    fn test_extract_tool_output_string() {
        let result = serde_json::json!("Hello, world!");
        let output = extract_tool_output(&result);
        assert_eq!(output, Some("Hello, world!".to_string()));
    }

    #[test]
    fn test_extract_tool_output_content_field() {
        let result = serde_json::json!({
            "content": "File contents here",
            "other": "ignored"
        });
        let output = extract_tool_output(&result);
        assert_eq!(output, Some("File contents here".to_string()));
    }

    #[test]
    fn test_extract_tool_output_output_field() {
        let result = serde_json::json!({
            "output": "Command output",
            "exitCode": 0
        });
        let output = extract_tool_output(&result);
        assert_eq!(output, Some("Command output".to_string()));
    }

    #[test]
    fn test_extract_tool_output_object() {
        let result = serde_json::json!({
            "success": true,
            "data": [1, 2, 3]
        });
        let output = extract_tool_output(&result);
        assert!(output.is_some());
        // Should be JSON serialized
        assert!(output.unwrap().contains("success"));
    }

    #[test]
    fn test_extract_tool_output_truncation() {
        // Create a very long string
        let long_content = "x".repeat(3000);
        let result = serde_json::json!(long_content);
        let output = extract_tool_output(&result);
        assert!(output.is_some());
        // Use char count, not byte len (ellipsis is 3 bytes)
        let char_count = output.unwrap().chars().count();
        assert!(char_count <= MAX_TOOL_OUTPUT_LEN, "Expected <= {} chars, got {}", MAX_TOOL_OUTPUT_LEN, char_count);
    }

    #[test]
    fn test_extract_path_from_args() {
        let args = serde_json::json!({"path": "/src/main.rs"});
        assert_eq!(
            extract_path_from_args(&args),
            Some(PathBuf::from("/src/main.rs"))
        );

        let args = serde_json::json!({"file_path": "/src/lib.rs"});
        assert_eq!(
            extract_path_from_args(&args),
            Some(PathBuf::from("/src/lib.rs"))
        );

        let args = serde_json::json!({"other": "value"});
        assert_eq!(extract_path_from_args(&args), None);
    }

    #[test]
    fn test_extract_files_from_result_read_file() {
        let args = Some(serde_json::json!({"path": "src/main.rs"}));
        let result = serde_json::json!("fn main() {}");
        let files = extract_files_from_result("read_file", &args, &result);
        assert_eq!(files, Some(vec![PathBuf::from("src/main.rs")]));
    }

    #[test]
    fn test_extract_files_from_result_list_files() {
        let args = Some(serde_json::json!({"path": "."}));
        let result = serde_json::json!("src/\n  main.rs\n  lib.rs\nCargo.toml");
        let files = extract_files_from_result("list_files", &args, &result);
        assert!(files.is_some());
        let files = files.unwrap();
        assert!(files.contains(&PathBuf::from("src/")));
        assert!(files.contains(&PathBuf::from("main.rs")));
        assert!(files.contains(&PathBuf::from("lib.rs")));
        assert!(files.contains(&PathBuf::from("Cargo.toml")));
    }

    #[test]
    fn test_extract_files_from_result_grep() {
        let args = Some(serde_json::json!({"pattern": "fn main"}));
        let result = serde_json::json!("src/main.rs:10: fn main() {\nsrc/bin/app.rs:5: fn main() {");
        let files = extract_files_from_result("grep", &args, &result);
        assert!(files.is_some());
        let files = files.unwrap();
        assert!(files.contains(&PathBuf::from("src/main.rs")));
        assert!(files.contains(&PathBuf::from("src/bin/app.rs")));
        // Should be deduplicated
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_extract_files_from_result_unknown_tool() {
        let args = Some(serde_json::json!({}));
        let result = serde_json::json!("some output");
        let files = extract_files_from_result("bash", &args, &result);
        assert!(files.is_none());
    }

    #[test]
    fn test_generate_unified_diff_simple() {
        let old = "line1\nline2\nline3";
        let new = "line1\nmodified\nline3";
        let diff = generate_unified_diff("test.txt", old, new);

        assert!(diff.contains("--- test.txt"));
        assert!(diff.contains("+++ test.txt"));
        assert!(diff.contains("-line2"));
        assert!(diff.contains("+modified"));
    }

    #[test]
    fn test_generate_unified_diff_addition() {
        let old = "line1";
        let new = "line1\nline2";
        let diff = generate_unified_diff("test.txt", old, new);

        assert!(diff.contains("+line2"));
    }

    #[test]
    fn test_generate_unified_diff_removal() {
        let old = "line1\nline2";
        let new = "line1";
        let diff = generate_unified_diff("test.txt", old, new);

        assert!(diff.contains("-line2"));
    }

    #[test]
    fn test_generate_unified_diff_no_changes() {
        let content = "line1\nline2\nline3";
        let diff = generate_unified_diff("test.txt", content, content);

        assert!(diff.contains("--- test.txt"));
        assert!(diff.contains("+++ test.txt"));
        // No actual changes should result in minimal diff
        assert!(!diff.contains("-line"));
        assert!(!diff.contains("+line"));
    }

    #[test]
    fn test_extract_files_modified_single_path() {
        let args = serde_json::json!({"path": "src/main.rs"});
        let files = extract_files_modified("write", Some(&args));
        assert_eq!(files, vec![PathBuf::from("src/main.rs")]);
    }

    #[test]
    fn test_extract_files_modified_rename() {
        let args = serde_json::json!({
            "from": "old_name.rs",
            "to": "new_name.rs"
        });
        let files = extract_files_modified("rename", Some(&args));
        assert_eq!(files.len(), 2);
        assert!(files.contains(&PathBuf::from("old_name.rs")));
        assert!(files.contains(&PathBuf::from("new_name.rs")));
    }

    #[test]
    fn test_extract_files_modified_move_file() {
        let args = serde_json::json!({
            "source": "src/old.rs",
            "destination": "lib/new.rs"
        });
        let files = extract_files_modified("move_file", Some(&args));
        assert_eq!(files.len(), 2);
        assert!(files.contains(&PathBuf::from("src/old.rs")));
        assert!(files.contains(&PathBuf::from("lib/new.rs")));
    }

    #[test]
    fn test_extract_files_modified_none_args() {
        let files = extract_files_modified("write", None);
        assert!(files.is_empty());
    }
}

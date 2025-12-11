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

                // Capture file operations
                if let Some(event) = self.create_file_event(&session_id, tool_name, *success) {
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
                let args_summary = self.last_tool_args.as_ref().map(summarize_args);
                let mut event = SessionEvent::tool_call_with_output(
                    session_id.clone(),
                    tool_name.clone(),
                    args_summary,
                    None,
                    *success,
                    tool_output,
                    diff,
                );

                // Add files_accessed if present
                if let Some(files) = files_accessed {
                    event.files_accessed = Some(files);
                }

                // Add files_modified if present
                if !files_modified.is_empty() {
                    event.files_modified = files_modified;
                }

                self.sidecar.capture(event);

                // Clear pending state
                self.last_tool_name = None;
                self.last_tool_args = None;
                self.pending_old_content = None;
            }

            AiEvent::Reasoning { content } => {
                debug!("[sidecar-capture] Reasoning event");
                // Try to detect decisions in reasoning
                let decision_type = infer_decision_type(content);
                let event = SessionEvent::reasoning(session_id, content, decision_type);
                self.sidecar.capture(event);
            }

            AiEvent::ToolApprovalRequest { tool_name, .. } => {
                debug!("[sidecar-capture] Tool approval request: {}", tool_name);
                // We'll capture the actual feedback when user responds
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
                debug!("[sidecar-capture] Tool denied: {}", tool_name);
                let event = SessionEvent::feedback(
                    session_id,
                    FeedbackType::Deny,
                    Some(tool_name.clone()),
                    Some(reason.clone()),
                );
                self.sidecar.capture(event);
            }

            AiEvent::Error { message, .. } => {
                debug!("[sidecar-capture] Error: {}", message);
                let event = SessionEvent::error(session_id, message, None);
                self.sidecar.capture(event);
            }

            AiEvent::Completed { response, .. } => {
                debug!("[sidecar-capture] Turn completed");
                if !response.is_empty() {
                    let event = SessionEvent::ai_response(session_id, response);
                    self.sidecar.capture(event);
                }
            }

            // Events we don't capture
            AiEvent::Started { .. }
            | AiEvent::TextDelta { .. }
            | AiEvent::ContextPruned { .. }
            | AiEvent::ContextWarning { .. }
            | AiEvent::ToolResponseTruncated { .. }
            | AiEvent::LoopWarning { .. }
            | AiEvent::LoopBlocked { .. }
            | AiEvent::MaxIterationsReached { .. }
            | AiEvent::SubAgentStarted { .. }
            | AiEvent::SubAgentToolRequest { .. }
            | AiEvent::SubAgentToolResult { .. }
            | AiEvent::SubAgentCompleted { .. }
            | AiEvent::SubAgentError { .. }
            | AiEvent::WorkflowStarted { .. }
            | AiEvent::WorkflowStepStarted { .. }
            | AiEvent::WorkflowStepCompleted { .. }
            | AiEvent::WorkflowCompleted { .. }
            | AiEvent::WorkflowError { .. } => {
                // These events are not captured
            }
        }
    }

    /// Create a file event from a tool result
    fn create_file_event(
        &self,
        session_id: &str,
        tool_name: &str,
        success: bool,
    ) -> Option<SessionEvent> {
        if !success {
            return None;
        }

        let args = self.last_tool_args.as_ref()?;

        match tool_name {
            "write_file" | "create_file" => {
                let path = extract_path_from_args(args)?;
                let summary = args
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| truncate(s, 100).to_string());
                Some(SessionEvent::file_edit(
                    session_id.to_string(),
                    path,
                    FileOperation::Create,
                    summary,
                ))
            }
            "edit_file" => {
                let path = extract_path_from_args(args)?;
                let summary = args
                    .get("display_description")
                    .and_then(|v| v.as_str())
                    .map(|s| truncate(s, 100).to_string());

                // Generate diff if we have old content
                let diff = self.generate_diff(tool_name, &self.last_tool_args);

                let mut event = SessionEvent::file_edit(
                    session_id.to_string(),
                    path,
                    FileOperation::Modify,
                    summary,
                );
                event.diff = diff;
                Some(event)
            }
            "delete_file" | "delete_path" => {
                let path = extract_path_from_args(args)?;
                Some(SessionEvent::file_edit(
                    session_id.to_string(),
                    path,
                    FileOperation::Delete,
                    None,
                ))
            }
            "rename_file" | "move_file" | "move_path" => {
                let from_path = args
                    .get("source_path")
                    .or_else(|| args.get("from"))
                    .and_then(|v| v.as_str())
                    .map(PathBuf::from)?;
                let to_path = args
                    .get("destination_path")
                    .or_else(|| args.get("to"))
                    .and_then(|v| v.as_str())
                    .map(PathBuf::from)?;
                Some(SessionEvent::file_edit(
                    session_id.to_string(),
                    to_path,
                    FileOperation::Rename { from: from_path },
                    None,
                ))
            }
            _ => None,
        }
    }

    /// Generate a diff for edit operations
    fn generate_diff(&self, tool_name: &str, args: &Option<serde_json::Value>) -> Option<String> {
        if tool_name != "edit_file" {
            return None;
        }

        let args = args.as_ref()?;
        let path = extract_path_from_args(args)?;

        // Get old content from pending or read current file
        let old_content = if let Some((pending_path, content)) = &self.pending_old_content {
            if pending_path == &path {
                content.clone()
            } else {
                return None;
            }
        } else {
            return None;
        };

        // Get new content by reading the file
        let new_content = std::fs::read_to_string(&path).ok()?;

        // Generate unified diff
        let diff = generate_unified_diff(&old_content, &new_content, &path.display().to_string());

        // Truncate if too long
        Some(truncate(&diff, MAX_DIFF_LEN).to_string())
    }
}

/// Check if tool is a read operation
fn is_read_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "read_file" | "list_files" | "list_directory" | "grep" | "find_path" | "diagnostics"
    )
}

/// Check if tool is a write operation
fn is_write_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "write_file"
            | "create_file"
            | "edit_file"
            | "delete_file"
            | "delete_path"
            | "rename_file"
            | "move_file"
            | "move_path"
            | "copy_path"
            | "create_directory"
    )
}

/// Check if tool is an edit operation (for diff generation)
fn is_edit_tool(tool_name: &str) -> bool {
    matches!(tool_name, "edit_file" | "write_file" | "create_file")
}

/// Extract tool output from result
fn extract_tool_output(result: &serde_json::Value) -> Option<String> {
    // Try different output formats
    let output = if let Some(s) = result.as_str() {
        s.to_string()
    } else if let Some(content) = result.get("content").and_then(|v| v.as_str()) {
        content.to_string()
    } else if let Some(output) = result.get("output").and_then(|v| v.as_str()) {
        output.to_string()
    } else {
        // Serialize the whole thing
        serde_json::to_string(result).ok()?
    };

    // Truncate if needed
    Some(truncate(&output, MAX_TOOL_OUTPUT_LEN).to_string())
}

/// Extract files from a tool result
fn extract_files_from_result(
    tool_name: &str,
    args: &Option<serde_json::Value>,
    _result: &serde_json::Value,
) -> Option<Vec<PathBuf>> {
    match tool_name {
        "read_file" => {
            let args = args.as_ref()?;
            let path = extract_path_from_args(args)?;
            Some(vec![path])
        }
        "list_files" | "list_directory" => {
            let args = args.as_ref()?;
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)?;
            Some(vec![path])
        }
        "grep" | "find_path" => {
            // These tools return multiple files in results, but we just track the search
            None
        }
        _ => None,
    }
}

/// Extract path from tool args
fn extract_path_from_args(args: &serde_json::Value) -> Option<PathBuf> {
    args.get("path")
        .or_else(|| args.get("file_path"))
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
}

/// Extract files modified from tool args
fn extract_files_modified(tool_name: &str, args: Option<&serde_json::Value>) -> Vec<PathBuf> {
    let args = match args {
        Some(a) => a,
        None => return vec![],
    };

    match tool_name {
        "write_file" | "create_file" | "edit_file" | "delete_file" | "delete_path" => {
            if let Some(path) = extract_path_from_args(args) {
                vec![path]
            } else {
                vec![]
            }
        }
        "rename_file" | "move_file" | "move_path" => {
            let mut files = vec![];
            if let Some(from) = args
                .get("source_path")
                .or_else(|| args.get("from"))
                .and_then(|v| v.as_str())
            {
                files.push(PathBuf::from(from));
            }
            if let Some(to) = args
                .get("destination_path")
                .or_else(|| args.get("to"))
                .and_then(|v| v.as_str())
            {
                files.push(PathBuf::from(to));
            }
            files
        }
        "copy_path" => {
            if let Some(dest) = args.get("destination_path").and_then(|v| v.as_str()) {
                vec![PathBuf::from(dest)]
            } else {
                vec![]
            }
        }
        "create_directory" => {
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                vec![PathBuf::from(path)]
            } else {
                vec![]
            }
        }
        _ => vec![],
    }
}

/// Generate unified diff between two strings
fn generate_unified_diff(old: &str, new: &str, filename: &str) -> String {
    use std::fmt::Write;

    let diff = similar::TextDiff::from_lines(old, new);
    let mut output = String::new();

    writeln!(output, "--- a/{}", filename).unwrap();
    writeln!(output, "+++ b/{}", filename).unwrap();

    for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
        writeln!(output, "{}", hunk.header()).unwrap();
        for change in hunk.iter_changes() {
            let sign = match change.tag() {
                similar::ChangeTag::Delete => "-",
                similar::ChangeTag::Insert => "+",
                similar::ChangeTag::Equal => " ",
            };
            write!(output, "{}{}", sign, change.value()).unwrap();
            if !change.value().ends_with('\n') {
                writeln!(output).unwrap();
            }
        }
    }

    output
}

/// Summarize tool args for logging
fn summarize_args(args: &serde_json::Value) -> String {
    let mut parts = Vec::new();

    if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
        parts.push(format!("path={}", truncate_path(path, 50)));
    }
    if let Some(desc) = args.get("display_description").and_then(|v| v.as_str()) {
        parts.push(format!("desc={}", truncate(desc, 40)));
    }
    if let Some(query) = args.get("query").and_then(|v| v.as_str()) {
        parts.push(format!("query={}", truncate(query, 30)));
    }
    if let Some(regex) = args.get("regex").and_then(|v| v.as_str()) {
        parts.push(format!("regex={}", truncate(regex, 30)));
    }

    if parts.is_empty() {
        // Fallback: show keys
        if let Some(obj) = args.as_object() {
            let keys: Vec<&str> = obj.keys().map(|s| s.as_str()).take(3).collect();
            format!("keys=[{}]", keys.join(", "))
        } else {
            "...".to_string()
        }
    } else {
        parts.join(", ")
    }
}

/// Infer decision type from reasoning content
fn infer_decision_type(content: &str) -> Option<DecisionType> {
    let lower = content.to_lowercase();

    // Check for approach/strategy decisions
    if lower.contains("i'll use")
        || lower.contains("i will use")
        || lower.contains("let's use")
        || lower.contains("going with")
        || lower.contains("choosing")
        || lower.contains("decided to")
    {
        return Some(DecisionType::ApproachChoice);
    }

    // Check for tradeoff decisions
    if lower.contains("tradeoff")
        || lower.contains("trade-off")
        || lower.contains("balance between")
        || lower.contains("weighing")
    {
        return Some(DecisionType::Tradeoff);
    }

    // Check for fallback decisions
    if lower.contains("instead")
        || lower.contains("fallback")
        || lower.contains("alternative")
        || lower.contains("workaround")
    {
        return Some(DecisionType::Fallback);
    }

    // Check for assumptions
    if lower.contains("assuming") || lower.contains("i assume") || lower.contains("presumably") {
        return Some(DecisionType::Assumption);
    }

    None
}

/// Truncate a string to a maximum length
fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        let mut end = max_len;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &s[..end]
    }
}

/// Truncate a path string, keeping the end
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        path.to_string()
    } else {
        let keep = max_len.saturating_sub(3);
        format!("...{}", &path[path.len() - keep..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summarize_args() {
        let args = serde_json::json!({
            "path": "src/main.rs",
            "display_description": "Add main function"
        });
        let summary = summarize_args(&args);
        assert!(summary.contains("path="));
        assert!(summary.contains("desc="));
    }

    #[test]
    fn test_infer_decision_type() {
        assert_eq!(
            infer_decision_type("I'll use the tokio runtime for async"),
            Some(DecisionType::ApproachChoice)
        );
        assert_eq!(
            infer_decision_type("There's a tradeoff between speed and safety"),
            Some(DecisionType::Tradeoff)
        );
        assert_eq!(
            infer_decision_type("Using a fallback approach"),
            Some(DecisionType::Fallback)
        );
        assert_eq!(
            infer_decision_type("Assuming the API returns JSON"),
            Some(DecisionType::Assumption)
        );
        assert_eq!(infer_decision_type("Just reading the file"), None);
    }

    #[test]
    fn test_truncate_path() {
        assert_eq!(truncate_path("short.rs", 20), "short.rs");
        assert_eq!(
            truncate_path("very/long/path/to/file.rs", 15),
            "...h/to/file.rs"
        );
    }

    #[test]
    fn test_is_read_tool() {
        assert!(is_read_tool("read_file"));
        assert!(is_read_tool("grep"));
        assert!(!is_read_tool("write_file"));
    }

    #[test]
    fn test_is_write_tool() {
        assert!(is_write_tool("write_file"));
        assert!(is_write_tool("edit_file"));
        assert!(!is_write_tool("read_file"));
    }

    #[test]
    fn test_extract_path_from_args() {
        let args = serde_json::json!({"path": "src/main.rs"});
        assert_eq!(
            extract_path_from_args(&args),
            Some(PathBuf::from("src/main.rs"))
        );

        let args = serde_json::json!({"file_path": "lib.rs"});
        assert_eq!(extract_path_from_args(&args), Some(PathBuf::from("lib.rs")));

        let args = serde_json::json!({"other": "value"});
        assert_eq!(extract_path_from_args(&args), None);
    }

    #[test]
    fn test_extract_files_modified_single_path() {
        let args = serde_json::json!({"path": "src/main.rs"});
        let files = extract_files_modified("write_file", Some(&args));
        assert_eq!(files, vec![PathBuf::from("src/main.rs")]);
    }

    #[test]
    fn test_extract_files_modified_rename() {
        let args = serde_json::json!({
            "source_path": "old.rs",
            "destination_path": "new.rs"
        });
        let files = extract_files_modified("move_file", Some(&args));
        assert_eq!(files.len(), 2);
        assert!(files.contains(&PathBuf::from("old.rs")));
        assert!(files.contains(&PathBuf::from("new.rs")));
    }

    #[test]
    fn test_generate_unified_diff_simple() {
        let old = "line1\nline2\nline3\n";
        let new = "line1\nmodified\nline3\n";
        let diff = generate_unified_diff(old, new, "test.txt");
        assert!(diff.contains("--- a/test.txt"));
        assert!(diff.contains("+++ b/test.txt"));
        assert!(diff.contains("-line2"));
        assert!(diff.contains("+modified"));
    }

    // =========================================================================
    // Integration tests for AI event -> SessionEvent flow
    // =========================================================================

    mod integration {
        use super::*;
        use crate::sidecar::config::SidecarConfig;
        use crate::sidecar::state::SidecarState;
        use tempfile::TempDir;

        fn test_config(temp_dir: &std::path::Path) -> SidecarConfig {
            SidecarConfig {
                enabled: true,
                sessions_dir: Some(temp_dir.to_path_buf()),
                retention_days: 0,
                max_state_size: 16 * 1024,
                write_raw_events: false,
                use_llm_for_state: false,
                capture_tool_calls: true,
                capture_reasoning: true,
                synthesis_enabled: false,
                synthesis_backend: crate::sidecar::synthesis::SynthesisBackend::Template,
                artifact_synthesis_backend:
                    crate::sidecar::artifacts::ArtifactSynthesisBackend::Template,
            }
        }

        #[tokio::test]
        async fn test_tool_request_stores_state_for_result() {
            let temp = TempDir::new().unwrap();
            let config = test_config(temp.path());
            let sidecar = Arc::new(SidecarState::with_config(config));

            sidecar.initialize(temp.path().to_path_buf()).await.unwrap();
            let _session_id = sidecar.start_session("Test session").unwrap();

            // Give time for async session creation
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

            let mut capture = CaptureContext::new(sidecar.clone());

            // Process tool request first
            capture.process(&AiEvent::ToolRequest {
                request_id: "test-1".to_string(),
                tool_name: "write_file".to_string(),
                args: serde_json::json!({"path": "src/test.rs", "content": "fn main() {}"}),
                source: crate::ai::events::ToolSource::Main,
            });

            // Verify state was stored
            assert_eq!(capture.last_tool_name, Some("write_file".to_string()));
            assert!(capture.last_tool_args.is_some());
        }

        #[tokio::test]
        async fn test_tool_result_clears_state() {
            let temp = TempDir::new().unwrap();
            let config = test_config(temp.path());
            let sidecar = Arc::new(SidecarState::with_config(config));

            sidecar.initialize(temp.path().to_path_buf()).await.unwrap();
            let _session_id = sidecar.start_session("Test session").unwrap();

            // Give time for async session creation
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

            let mut capture = CaptureContext::new(sidecar.clone());

            // Process tool request then result
            capture.process(&AiEvent::ToolRequest {
                request_id: "test-1".to_string(),
                tool_name: "read_file".to_string(),
                args: serde_json::json!({"path": "src/test.rs"}),
                source: crate::ai::events::ToolSource::Main,
            });

            capture.process(&AiEvent::ToolResult {
                tool_name: "read_file".to_string(),
                result: serde_json::json!({"content": "file contents"}),
                success: true,
                request_id: "test-1".to_string(),
                source: crate::ai::events::ToolSource::Main,
            });

            // State should be cleared after result
            assert!(capture.last_tool_name.is_none());
            assert!(capture.last_tool_args.is_none());
        }

        #[tokio::test]
        async fn test_write_tool_captures_file_edit_event() {
            let temp = TempDir::new().unwrap();
            let config = test_config(temp.path());
            let sidecar = Arc::new(SidecarState::with_config(config));

            sidecar.initialize(temp.path().to_path_buf()).await.unwrap();
            let _session_id = sidecar.start_session("Test session").unwrap();

            // Give time for async session creation
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

            let mut capture = CaptureContext::new(sidecar.clone());

            // Process write_file tool
            capture.process(&AiEvent::ToolRequest {
                request_id: "test-1".to_string(),
                tool_name: "write_file".to_string(),
                args: serde_json::json!({"path": "/tmp/test.rs", "content": "fn main() {}"}),
                source: crate::ai::events::ToolSource::Main,
            });

            capture.process(&AiEvent::ToolResult {
                tool_name: "write_file".to_string(),
                result: serde_json::json!({"success": true}),
                success: true,
                request_id: "test-1".to_string(),
                source: crate::ai::events::ToolSource::Main,
            });

            // The capture should have processed both events
            // State should be cleared
            assert!(capture.last_tool_name.is_none());
        }

        #[tokio::test]
        async fn test_reasoning_event_captured() {
            let temp = TempDir::new().unwrap();
            let config = test_config(temp.path());
            let sidecar = Arc::new(SidecarState::with_config(config));

            sidecar.initialize(temp.path().to_path_buf()).await.unwrap();
            let _session_id = sidecar.start_session("Test session").unwrap();

            // Give time for async session creation
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

            let mut capture = CaptureContext::new(sidecar.clone());

            // Process reasoning event with completion signal
            capture.process(&AiEvent::Reasoning {
                content: "I've completed the implementation.".to_string(),
            });

            // The capture should have processed the event
            // (We can't easily verify the event was captured without mocking sidecar.capture)
            // but we can verify no panic occurred
        }

        #[tokio::test]
        async fn test_edit_tool_captures_diff() {
            let temp = TempDir::new().unwrap();
            let config = test_config(temp.path());
            let sidecar = Arc::new(SidecarState::with_config(config));

            sidecar.initialize(temp.path().to_path_buf()).await.unwrap();
            let _session_id = sidecar.start_session("Test session").unwrap();

            // Give time for async session creation
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

            // Create a test file to edit
            let test_file = temp.path().join("test.rs");
            std::fs::write(&test_file, "fn original() {}").unwrap();

            let mut capture = CaptureContext::new(sidecar.clone());

            // Process edit_file tool request (captures old content)
            capture.process(&AiEvent::ToolRequest {
                request_id: "test-1".to_string(),
                tool_name: "edit_file".to_string(),
                args: serde_json::json!({
                    "path": test_file.to_string_lossy(),
                    "display_description": "Update function"
                }),
                source: crate::ai::events::ToolSource::Main,
            });

            // Verify old content was captured for diff
            assert!(capture.pending_old_content.is_some());
            let (path, content) = capture.pending_old_content.as_ref().unwrap();
            assert_eq!(*path, test_file);
            assert_eq!(content, "fn original() {}");

            // Simulate file modification
            std::fs::write(&test_file, "fn modified() {}").unwrap();

            // Process result
            capture.process(&AiEvent::ToolResult {
                tool_name: "edit_file".to_string(),
                result: serde_json::json!({"success": true}),
                success: true,
                request_id: "test-1".to_string(),
                source: crate::ai::events::ToolSource::Main,
            });

            // State should be cleared
            assert!(capture.pending_old_content.is_none());
        }
    }
}

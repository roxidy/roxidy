//! Event types captured by the sidecar system.
//!
//! These types represent the semantic information extracted from agent interactions
//! that we want to persist and query later.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Types of events captured by the sidecar
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventType {
    /// User prompt with their stated intent
    UserPrompt {
        /// What the user asked for
        intent: String,
    },

    /// File modification with context
    FileEdit {
        /// Path to the file
        path: PathBuf,
        /// Type of operation performed
        operation: FileOperation,
        /// One-line description if available
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },

    /// Tool call with reasoning
    ToolCall {
        /// Name of the tool invoked
        tool_name: String,
        /// Truncated/summarized args
        args_summary: String,
        /// Why the agent made this call
        #[serde(skip_serializing_if = "Option::is_none")]
        reasoning: Option<String>,
        /// Whether the tool call succeeded
        success: bool,
    },

    /// Agent's explicit reasoning (from extended thinking or text)
    AgentReasoning {
        /// The reasoning content
        content: String,
        /// Type of decision being made
        #[serde(skip_serializing_if = "Option::is_none")]
        decision_type: Option<DecisionType>,
    },

    /// User feedback on agent action
    UserFeedback {
        /// Type of feedback
        feedback_type: FeedbackType,
        /// Tool that was being approved/denied
        #[serde(skip_serializing_if = "Option::is_none")]
        target_tool: Option<String>,
        /// User's comment if any
        #[serde(skip_serializing_if = "Option::is_none")]
        comment: Option<String>,
    },

    /// Error with recovery attempt
    ErrorRecovery {
        /// The error message
        error_message: String,
        /// What action was taken to recover
        #[serde(skip_serializing_if = "Option::is_none")]
        recovery_action: Option<String>,
        /// Whether the error was resolved
        resolved: bool,
    },

    /// Commit boundary marker (detected or explicit)
    CommitBoundary {
        /// Suggested commit message if available
        #[serde(skip_serializing_if = "Option::is_none")]
        suggested_message: Option<String>,
        /// Files that should be included in this commit
        files_in_scope: Vec<PathBuf>,
    },

    /// Session started
    SessionStart {
        /// Initial user request
        initial_request: String,
    },

    /// Session ended
    SessionEnd {
        /// Final summary if available
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },

    /// AI response (final accumulated text)
    AiResponse {
        /// The response content (truncated for storage)
        content: String,
        /// Whether this was a complete response or truncated
        truncated: bool,
        /// Duration in milliseconds
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
}

impl EventType {
    /// Get a short name for this event type
    pub fn name(&self) -> &'static str {
        match self {
            EventType::UserPrompt { .. } => "user_prompt",
            EventType::FileEdit { .. } => "file_edit",
            EventType::ToolCall { .. } => "tool_call",
            EventType::AgentReasoning { .. } => "reasoning",
            EventType::UserFeedback { .. } => "feedback",
            EventType::ErrorRecovery { .. } => "error",
            EventType::CommitBoundary { .. } => "commit_boundary",
            EventType::SessionStart { .. } => "session_start",
            EventType::SessionEnd { .. } => "session_end",
            EventType::AiResponse { .. } => "ai_response",
        }
    }

    /// Check if this is a high-signal event worth embedding
    pub fn is_high_signal(&self) -> bool {
        matches!(
            self,
            EventType::UserPrompt { .. }
                | EventType::FileEdit { .. }
                | EventType::AgentReasoning { .. }
                | EventType::UserFeedback { .. }
                | EventType::CommitBoundary { .. }
                | EventType::AiResponse { .. }
        )
    }
}

/// File operation types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileOperation {
    /// File was created
    Create,
    /// File was modified
    Modify,
    /// File was deleted
    Delete,
    /// File was renamed
    Rename {
        /// Original path before rename
        from: PathBuf,
    },
}

/// Types of decisions the agent makes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionType {
    /// "I'll use X instead of Y because..."
    ApproachChoice,
    /// "This sacrifices A for B"
    Tradeoff,
    /// "Since X didn't work, trying Y"
    Fallback,
    /// "Assuming the user wants..."
    Assumption,
}

/// Types of user feedback
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackType {
    /// User approved the action
    Approve,
    /// User denied the action
    Deny,
    /// User modified the action
    Modify,
    /// User added a comment/annotation
    Annotate,
}

/// A captured session event with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    /// Unique identifier for this event
    pub id: Uuid,
    /// Session this event belongs to
    pub session_id: Uuid,
    /// When this event occurred
    pub timestamp: DateTime<Utc>,
    /// The type and details of this event
    pub event_type: EventType,
    /// Full content for embedding (human-readable summary)
    pub content: String,
    /// Related files (for filtering/grouping)
    pub files: Vec<PathBuf>,
    /// 384-dimensional embedding vector (computed async)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}

impl SessionEvent {
    /// Create a new session event
    pub fn new(session_id: Uuid, event_type: EventType, content: String) -> Self {
        let files = Self::extract_files(&event_type);
        Self {
            id: Uuid::new_v4(),
            session_id,
            timestamp: Utc::now(),
            event_type,
            content,
            files,
            embedding: None,
        }
    }

    /// Create a user prompt event
    pub fn user_prompt(session_id: Uuid, prompt: &str) -> Self {
        Self::new(
            session_id,
            EventType::UserPrompt {
                intent: prompt.to_string(),
            },
            format!("User asked: {}", truncate(prompt, 500)),
        )
    }

    /// Create a file edit event
    pub fn file_edit(
        session_id: Uuid,
        path: PathBuf,
        operation: FileOperation,
        summary: Option<String>,
    ) -> Self {
        let content = match &operation {
            FileOperation::Create => format!(
                "File created: {}{}",
                path.display(),
                summary.as_ref().map(|s| format!(" - {}", s)).unwrap_or_default()
            ),
            FileOperation::Modify => format!(
                "File modified: {}{}",
                path.display(),
                summary.as_ref().map(|s| format!(" - {}", s)).unwrap_or_default()
            ),
            FileOperation::Delete => format!(
                "File deleted: {}{}",
                path.display(),
                summary.as_ref().map(|s| format!(" - {}", s)).unwrap_or_default()
            ),
            FileOperation::Rename { from } => format!(
                "Renamed {} to {}{}",
                from.display(),
                path.display(),
                summary.as_ref().map(|s| format!(": {}", s)).unwrap_or_default()
            ),
        };

        Self::new(
            session_id,
            EventType::FileEdit {
                path: path.clone(),
                operation,
                summary,
            },
            content,
        )
    }

    /// Create a tool call event
    pub fn tool_call(
        session_id: Uuid,
        tool_name: &str,
        args_summary: &str,
        reasoning: Option<String>,
        success: bool,
    ) -> Self {
        let status = if success { "succeeded" } else { "failed" };
        Self::new(
            session_id,
            EventType::ToolCall {
                tool_name: tool_name.to_string(),
                args_summary: args_summary.to_string(),
                reasoning: reasoning.clone(),
                success,
            },
            format!(
                "Tool {} {}: {}{}",
                tool_name,
                status,
                truncate(args_summary, 200),
                reasoning
                    .map(|r| format!(" ({})", truncate(&r, 100)))
                    .unwrap_or_default()
            ),
        )
    }

    /// Create an agent reasoning event
    pub fn reasoning(
        session_id: Uuid,
        content: &str,
        decision_type: Option<DecisionType>,
    ) -> Self {
        Self::new(
            session_id,
            EventType::AgentReasoning {
                content: content.to_string(),
                decision_type,
            },
            format!("Agent reasoning: {}", truncate(content, 500)),
        )
    }

    /// Create a user feedback event
    pub fn feedback(
        session_id: Uuid,
        feedback_type: FeedbackType,
        target_tool: Option<String>,
        comment: Option<String>,
    ) -> Self {
        let action = match feedback_type {
            FeedbackType::Approve => "approved",
            FeedbackType::Deny => "denied",
            FeedbackType::Modify => "modified",
            FeedbackType::Annotate => "annotated",
        };

        Self::new(
            session_id,
            EventType::UserFeedback {
                feedback_type,
                target_tool: target_tool.clone(),
                comment: comment.clone(),
            },
            format!(
                "User {} {}{}",
                action,
                target_tool.unwrap_or_else(|| "action".to_string()),
                comment.map(|c| format!(": {}", c)).unwrap_or_default()
            ),
        )
    }

    /// Create an error event
    pub fn error(
        session_id: Uuid,
        error_message: &str,
        recovery_action: Option<String>,
        resolved: bool,
    ) -> Self {
        Self::new(
            session_id,
            EventType::ErrorRecovery {
                error_message: error_message.to_string(),
                recovery_action: recovery_action.clone(),
                resolved,
            },
            format!(
                "Error{}: {}{}",
                if resolved { " (resolved)" } else { "" },
                truncate(error_message, 200),
                recovery_action
                    .map(|r| format!(" → {}", r))
                    .unwrap_or_default()
            ),
        )
    }

    /// Create a commit boundary event
    pub fn commit_boundary(
        session_id: Uuid,
        files: Vec<PathBuf>,
        suggested_message: Option<String>,
    ) -> Self {
        let file_count = files.len();
        Self::new(
            session_id,
            EventType::CommitBoundary {
                suggested_message: suggested_message.clone(),
                files_in_scope: files,
            },
            format!(
                "Commit boundary detected: {} file(s){}",
                file_count,
                suggested_message
                    .map(|m| format!(" - {}", truncate(&m, 100)))
                    .unwrap_or_default()
            ),
        )
    }

    /// Create an AI response event
    pub fn ai_response(
        session_id: Uuid,
        response: &str,
        duration_ms: Option<u64>,
    ) -> Self {
        // Truncate for storage but mark if truncated
        const MAX_RESPONSE_LEN: usize = 2000;
        let truncated = response.chars().count() > MAX_RESPONSE_LEN;
        let content = truncate(response, MAX_RESPONSE_LEN);

        Self::new(
            session_id,
            EventType::AiResponse {
                content: content.clone(),
                truncated,
                duration_ms,
            },
            format!("AI responded: {}", truncate(response, 500)),
        )
    }

    /// Extract file paths from event type
    fn extract_files(event_type: &EventType) -> Vec<PathBuf> {
        match event_type {
            EventType::FileEdit { path, .. } => vec![path.clone()],
            EventType::CommitBoundary { files_in_scope, .. } => files_in_scope.clone(),
            _ => vec![],
        }
    }

    /// Set the embedding for this event
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }
}

/// Periodic checkpoint summarizing a batch of events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unique identifier
    pub id: Uuid,
    /// Session this checkpoint belongs to
    pub session_id: Uuid,
    /// When this checkpoint was generated
    pub timestamp: DateTime<Utc>,
    /// LLM-generated summary of the events
    pub summary: String,
    /// Event IDs covered by this checkpoint
    pub event_ids: Vec<Uuid>,
    /// Files touched in these events
    pub files_touched: Vec<PathBuf>,
    /// Embedding of the summary
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}

impl Checkpoint {
    /// Create a new checkpoint
    pub fn new(
        session_id: Uuid,
        summary: String,
        event_ids: Vec<Uuid>,
        files_touched: Vec<PathBuf>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            timestamp: Utc::now(),
            summary,
            event_ids,
            files_touched,
            embedding: None,
        }
    }

    /// Create an empty checkpoint (no events to summarize)
    pub fn empty(session_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            timestamp: Utc::now(),
            summary: String::new(),
            event_ids: vec![],
            files_touched: vec![],
            embedding: None,
        }
    }

    /// Set the embedding for this checkpoint
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }
}

/// Active session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarSession {
    /// Unique identifier
    pub id: Uuid,
    /// When the session started
    pub started_at: DateTime<Utc>,
    /// When the session ended (None if still active)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    /// The user's initial request
    pub initial_request: String,
    /// Workspace path for this session
    pub workspace_path: PathBuf,
    /// Number of events captured
    pub event_count: usize,
    /// Number of checkpoints generated
    pub checkpoint_count: usize,
    /// All files touched during the session
    pub files_touched: Vec<PathBuf>,
    /// Final summary (generated at session end)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_summary: Option<String>,
}

impl SidecarSession {
    /// Create a new session
    pub fn new(workspace_path: PathBuf, initial_request: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            started_at: Utc::now(),
            ended_at: None,
            initial_request,
            workspace_path,
            event_count: 0,
            checkpoint_count: 0,
            files_touched: vec![],
            final_summary: None,
        }
    }

    /// Check if the session is still active
    pub fn is_active(&self) -> bool {
        self.ended_at.is_none()
    }

    /// End the session
    pub fn end(&mut self, summary: Option<String>) {
        self.ended_at = Some(Utc::now());
        self.final_summary = summary;
    }

    /// Record that a file was touched
    pub fn touch_file(&mut self, path: PathBuf) {
        if !self.files_touched.contains(&path) {
            self.files_touched.push(path);
        }
    }

    /// Increment event count
    pub fn increment_events(&mut self) {
        self.event_count += 1;
    }

    /// Increment checkpoint count
    pub fn increment_checkpoints(&mut self) {
        self.checkpoint_count += 1;
    }
}

/// Commit boundary detector
///
/// Detects logical boundaries where a commit would make sense based on:
/// - File save patterns (many edits followed by pause)
/// - Completion signals in reasoning
/// - User feedback events
pub struct CommitBoundaryDetector {
    /// File edits since last boundary
    recent_edits: Vec<PathBuf>,
    /// Last event timestamp
    last_event_time: Option<DateTime<Utc>>,
    /// Minimum events before considering a boundary
    min_events: usize,
    /// Pause threshold in seconds
    pause_threshold_secs: u64,
}

impl Default for CommitBoundaryDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl CommitBoundaryDetector {
    /// Create a new detector with default settings
    pub fn new() -> Self {
        Self {
            recent_edits: Vec::new(),
            last_event_time: None,
            min_events: 3,
            pause_threshold_secs: 60,
        }
    }

    /// Create with custom thresholds
    pub fn with_thresholds(min_events: usize, pause_threshold_secs: u64) -> Self {
        Self {
            recent_edits: Vec::new(),
            last_event_time: None,
            min_events,
            pause_threshold_secs,
        }
    }

    /// Check if an event suggests a commit boundary
    pub fn check_boundary(&mut self, event: &SessionEvent) -> Option<CommitBoundaryInfo> {
        let now = event.timestamp;

        // Track file edits
        if let EventType::FileEdit { path, .. } = &event.event_type {
            if !self.recent_edits.contains(path) {
                self.recent_edits.push(path.clone());
            }
        }

        // Check for explicit completion signals
        if let Some(boundary) = self.check_completion_signals(event) {
            return Some(boundary);
        }

        // Check for pause-based boundary
        if let Some(boundary) = self.check_pause_boundary(now) {
            return Some(boundary);
        }

        self.last_event_time = Some(now);
        None
    }

    /// Check for completion signals in reasoning
    fn check_completion_signals(&mut self, event: &SessionEvent) -> Option<CommitBoundaryInfo> {
        match &event.event_type {
            EventType::AgentReasoning { content, .. } => {
                let lower = content.to_lowercase();
                let is_completion = lower.contains("done")
                    || lower.contains("complete")
                    || lower.contains("finished")
                    || lower.contains("implemented")
                    || lower.contains("ready to commit")
                    || lower.contains("ready for review");

                if is_completion && self.recent_edits.len() >= self.min_events {
                    return Some(self.create_boundary("Completion signal detected"));
                }
            }
            EventType::UserFeedback {
                feedback_type: FeedbackType::Approve,
                ..
            } => {
                if self.recent_edits.len() >= self.min_events {
                    return Some(self.create_boundary("User approved changes"));
                }
            }
            EventType::SessionEnd { .. } => {
                if !self.recent_edits.is_empty() {
                    return Some(self.create_boundary("Session ended"));
                }
            }
            _ => {}
        }
        None
    }

    /// Check for pause-based boundary
    fn check_pause_boundary(&mut self, now: DateTime<Utc>) -> Option<CommitBoundaryInfo> {
        if let Some(last) = self.last_event_time {
            let pause_duration = (now - last).num_seconds() as u64;

            if pause_duration >= self.pause_threshold_secs
                && self.recent_edits.len() >= self.min_events
            {
                return Some(self.create_boundary("Pause in activity detected"));
            }
        }
        None
    }

    /// Create a boundary info and reset state
    fn create_boundary(&mut self, reason: &str) -> CommitBoundaryInfo {
        let files = std::mem::take(&mut self.recent_edits);
        CommitBoundaryInfo {
            files_in_scope: files,
            reason: reason.to_string(),
            timestamp: Utc::now(),
        }
    }

    /// Get files edited since last boundary (without creating a boundary)
    pub fn pending_files(&self) -> &[PathBuf] {
        &self.recent_edits
    }

    /// Clear pending edits (e.g., after user commits manually)
    pub fn clear(&mut self) {
        self.recent_edits.clear();
        self.last_event_time = None;
    }
}

/// Information about a detected commit boundary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitBoundaryInfo {
    /// Files that should be in this commit
    pub files_in_scope: Vec<PathBuf>,
    /// Why this boundary was detected
    pub reason: String,
    /// When the boundary was detected
    pub timestamp: DateTime<Utc>,
}

/// Export format for session data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionExport {
    /// Export version for compatibility
    pub version: u32,
    /// When the export was created
    pub exported_at: DateTime<Utc>,
    /// Session metadata
    pub session: SidecarSession,
    /// All events in the session
    pub events: Vec<SessionEvent>,
    /// All checkpoints in the session
    pub checkpoints: Vec<Checkpoint>,
}

impl SessionExport {
    /// Current export version
    pub const VERSION: u32 = 1;

    /// Create a new export
    pub fn new(session: SidecarSession, events: Vec<SessionEvent>, checkpoints: Vec<Checkpoint>) -> Self {
        Self {
            version: Self::VERSION,
            exported_at: Utc::now(),
            session,
            events,
            checkpoints,
        }
    }

    /// Export to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Export to JSON bytes
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec_pretty(self)
    }

    /// Import from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Truncate a string to a maximum length
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let mut result: String = s.chars().take(max_len.saturating_sub(1)).collect();
        result.push('…');
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_names() {
        assert_eq!(
            EventType::UserPrompt {
                intent: "test".into()
            }
            .name(),
            "user_prompt"
        );
        assert_eq!(
            EventType::FileEdit {
                path: PathBuf::from("test"),
                operation: FileOperation::Create,
                summary: None
            }
            .name(),
            "file_edit"
        );
    }

    #[test]
    fn test_event_type_high_signal() {
        assert!(EventType::UserPrompt {
            intent: "test".into()
        }
        .is_high_signal());
        assert!(EventType::AgentReasoning {
            content: "test".into(),
            decision_type: None
        }
        .is_high_signal());
        assert!(!EventType::ToolCall {
            tool_name: "test".into(),
            args_summary: "test".into(),
            reasoning: None,
            success: true
        }
        .is_high_signal());
    }

    #[test]
    fn test_session_event_creation() {
        let session_id = Uuid::new_v4();

        let event = SessionEvent::user_prompt(session_id, "Add authentication");
        assert_eq!(event.session_id, session_id);
        assert!(event.content.contains("authentication"));
        assert!(event.embedding.is_none());
    }

    #[test]
    fn test_file_edit_event() {
        let session_id = Uuid::new_v4();
        let path = PathBuf::from("/src/lib.rs");

        let event =
            SessionEvent::file_edit(session_id, path.clone(), FileOperation::Modify, None);

        assert_eq!(event.files, vec![path]);
        assert!(event.content.contains("modified"));
    }

    #[test]
    fn test_session_lifecycle() {
        let mut session = SidecarSession::new(PathBuf::from("/project"), "Initial request".into());

        assert!(session.is_active());
        assert_eq!(session.event_count, 0);

        session.increment_events();
        session.touch_file(PathBuf::from("/src/lib.rs"));
        session.touch_file(PathBuf::from("/src/lib.rs")); // Duplicate

        assert_eq!(session.event_count, 1);
        assert_eq!(session.files_touched.len(), 1);

        session.end(Some("Summary".into()));
        assert!(!session.is_active());
        assert!(session.final_summary.is_some());
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("a longer string here", 10), "a longer …");
    }

    #[test]
    fn test_checkpoint_creation() {
        let session_id = Uuid::new_v4();
        let checkpoint = Checkpoint::new(
            session_id,
            "Summary".into(),
            vec![Uuid::new_v4()],
            vec![PathBuf::from("/src/lib.rs")],
        );

        assert_eq!(checkpoint.session_id, session_id);
        assert!(checkpoint.embedding.is_none());
    }

    #[test]
    fn test_event_serialization() {
        let session_id = Uuid::new_v4();
        let event = SessionEvent::reasoning(session_id, "Choosing approach A", Some(DecisionType::ApproachChoice));

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: SessionEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, event.id);
        assert_eq!(deserialized.session_id, event.session_id);
    }

    #[test]
    fn test_commit_boundary_detector_file_tracking() {
        let mut detector = CommitBoundaryDetector::new();
        let session_id = Uuid::new_v4();

        // Add some file edits
        let event1 = SessionEvent::file_edit(
            session_id,
            PathBuf::from("/src/lib.rs"),
            FileOperation::Modify,
            None,
        );
        let event2 = SessionEvent::file_edit(
            session_id,
            PathBuf::from("/src/main.rs"),
            FileOperation::Modify,
            None,
        );

        detector.check_boundary(&event1);
        detector.check_boundary(&event2);

        assert_eq!(detector.pending_files().len(), 2);
    }

    #[test]
    fn test_commit_boundary_completion_signal() {
        let mut detector = CommitBoundaryDetector::with_thresholds(2, 60);
        let session_id = Uuid::new_v4();

        // Add file edits
        detector.check_boundary(&SessionEvent::file_edit(
            session_id,
            PathBuf::from("/src/a.rs"),
            FileOperation::Modify,
            None,
        ));
        detector.check_boundary(&SessionEvent::file_edit(
            session_id,
            PathBuf::from("/src/b.rs"),
            FileOperation::Create,
            None,
        ));

        // Add completion signal
        let boundary = detector.check_boundary(&SessionEvent::reasoning(
            session_id,
            "Implementation is complete",
            None,
        ));

        assert!(boundary.is_some());
        let boundary = boundary.unwrap();
        assert_eq!(boundary.files_in_scope.len(), 2);
        assert!(boundary.reason.contains("Completion"));
    }

    #[test]
    fn test_commit_boundary_user_approval() {
        let mut detector = CommitBoundaryDetector::with_thresholds(1, 60);
        let session_id = Uuid::new_v4();

        detector.check_boundary(&SessionEvent::file_edit(
            session_id,
            PathBuf::from("/src/lib.rs"),
            FileOperation::Modify,
            None,
        ));

        let boundary = detector.check_boundary(&SessionEvent::feedback(
            session_id,
            FeedbackType::Approve,
            Some("edit".into()),
            None,
        ));

        assert!(boundary.is_some());
        assert!(boundary.unwrap().reason.contains("approved"));
    }

    #[test]
    fn test_commit_boundary_clear() {
        let mut detector = CommitBoundaryDetector::new();
        let session_id = Uuid::new_v4();

        detector.check_boundary(&SessionEvent::file_edit(
            session_id,
            PathBuf::from("/src/lib.rs"),
            FileOperation::Modify,
            None,
        ));

        assert!(!detector.pending_files().is_empty());

        detector.clear();

        assert!(detector.pending_files().is_empty());
    }

    #[test]
    fn test_session_export() {
        let session = SidecarSession::new(PathBuf::from("/project"), "Test request".into());
        let session_id = session.id;

        let events = vec![
            SessionEvent::user_prompt(session_id, "Add feature"),
            SessionEvent::file_edit(
                session_id,
                PathBuf::from("/src/lib.rs"),
                FileOperation::Modify,
                None,
            ),
        ];

        let checkpoints = vec![Checkpoint::new(
            session_id,
            "Test checkpoint".into(),
            vec![events[0].id],
            vec![],
        )];

        let export = SessionExport::new(session, events, checkpoints);

        // Test JSON serialization
        let json = export.to_json().unwrap();
        assert!(json.contains("Test request"));

        // Test deserialization
        let imported = SessionExport::from_json(&json).unwrap();
        assert_eq!(imported.version, SessionExport::VERSION);
        assert_eq!(imported.session.id, session_id);
        assert_eq!(imported.events.len(), 2);
        assert_eq!(imported.checkpoints.len(), 1);
    }
}

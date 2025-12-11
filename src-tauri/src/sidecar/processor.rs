//! Event processor for markdown-based sidecar.
//!
//! Processes events asynchronously, updating:
//! - `state.md` via LLM interpretation
//! - `log.md` with chronological entries
//! - `events.jsonl` with raw events

use anyhow::{Context, Result};
use chrono::Utc;
use std::path::PathBuf;

use tokio::sync::mpsc;

use super::events::SessionEvent;
use super::formats::{format_log_entry, format_log_entry_with_diff, STATE_UPDATE_PROMPT};
use super::session::Session;

/// Event sent to the processor
#[derive(Debug)]
pub enum ProcessorTask {
    /// Process a new event
    ProcessEvent {
        session_id: String,
        event: Box<SessionEvent>,
    },
    /// End a session
    EndSession { session_id: String },
    /// Shutdown the processor
    Shutdown,
}

/// Configuration for the processor
#[derive(Debug, Clone)]
pub struct ProcessorConfig {
    /// Directory containing sessions
    pub sessions_dir: PathBuf,
    /// Whether to use LLM for state updates (false = rule-based only)
    pub use_llm: bool,
    /// Whether to write raw events to events.jsonl
    pub write_raw_events: bool,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            sessions_dir: super::session::default_sessions_dir(),
            use_llm: true,
            write_raw_events: true,
        }
    }
}

/// Markdown-based event processor
pub struct Processor {
    task_tx: mpsc::Sender<ProcessorTask>,
}

impl Processor {
    /// Create a new processor and spawn its background task
    pub fn spawn(config: ProcessorConfig) -> Self {
        let (task_tx, task_rx) = mpsc::channel(256);

        let processor_config = config.clone();
        tokio::spawn(async move {
            run_processor(processor_config, task_rx).await;
        });

        Self { task_tx }
    }

    /// Process an event (non-blocking, queues for async processing)
    pub fn process_event(&self, session_id: String, event: SessionEvent) {
        let task = ProcessorTask::ProcessEvent {
            session_id,
            event: Box::new(event),
        };
        if let Err(e) = self.task_tx.try_send(task) {
            tracing::warn!("Failed to queue event for processing: {}", e);
        }
    }

    /// Signal session end
    pub fn end_session(&self, session_id: String) {
        let task = ProcessorTask::EndSession { session_id };
        if let Err(e) = self.task_tx.try_send(task) {
            tracing::warn!("Failed to queue session end: {}", e);
        }
    }

    /// Shutdown the processor
    pub async fn shutdown(&self) {
        let _ = self.task_tx.send(ProcessorTask::Shutdown).await;
    }
}

/// Main processor loop
async fn run_processor(config: ProcessorConfig, mut task_rx: mpsc::Receiver<ProcessorTask>) {
    tracing::info!("Sidecar processor started");

    while let Some(task) = task_rx.recv().await {
        match task {
            ProcessorTask::ProcessEvent { session_id, event } => {
                if let Err(e) = handle_event(&config, &session_id, &event).await {
                    tracing::error!("Failed to process event for {}: {}", session_id, e);
                }
            }
            ProcessorTask::EndSession { session_id } => {
                if let Err(e) = handle_end_session(&config, &session_id).await {
                    tracing::error!("Failed to end session {}: {}", session_id, e);
                }
            }
            ProcessorTask::Shutdown => {
                tracing::info!("Sidecar processor shutting down");
                break;
            }
        }
    }
}

/// Handle a single event
async fn handle_event(
    config: &ProcessorConfig,
    session_id: &str,
    event: &SessionEvent,
) -> Result<()> {
    // Load session
    let session = Session::load(&config.sessions_dir, session_id)
        .await
        .context("Failed to load session")?;

    // Write raw event to events.jsonl
    if config.write_raw_events {
        let event_json = serde_json::to_string(event)?;
        session.append_event(&event_json).await?;
    }

    // Generate and append log entry
    let log_entry = format_event_for_log(event);
    session.append_log(&log_entry).await?;

    // Update state.md
    if config.use_llm {
        // TODO: Implement LLM-based state update
        // For now, use rule-based update
        update_state_rule_based(&session, event).await?;
    } else {
        update_state_rule_based(&session, event).await?;
    }

    tracing::debug!(
        "Processed event for session {}: {:?}",
        session_id,
        event.event_type.name()
    );
    Ok(())
}

/// Handle session end
async fn handle_end_session(config: &ProcessorConfig, session_id: &str) -> Result<()> {
    let mut session = Session::load(&config.sessions_dir, session_id).await?;
    session.complete().await?;
    Ok(())
}

/// Format an event as a log entry
fn format_event_for_log(event: &SessionEvent) -> String {
    use super::events::EventType;

    let timestamp = &event.timestamp;
    let event_type = event.event_type.name();

    match &event.event_type {
        EventType::UserPrompt { intent } => {
            let content = if intent.is_empty() {
                &event.content
            } else {
                intent
            };
            format_log_entry(
                timestamp,
                "User Prompt",
                &format!("**Request:** {}", content),
            )
        }

        EventType::FileEdit {
            path,
            operation,
            summary,
        } => {
            let op_str = match operation {
                super::events::FileOperation::Create => "Created",
                super::events::FileOperation::Modify => "Modified",
                super::events::FileOperation::Delete => "Deleted",
                super::events::FileOperation::Rename { from } => {
                    return format_log_entry(
                        timestamp,
                        "File Rename",
                        &format!("Renamed `{}` → `{}`", from.display(), path.display()),
                    );
                }
            };

            let desc = if let Some(sum) = summary {
                format!("{} `{}` — {}", op_str, path.display(), sum)
            } else {
                format!("{} `{}`", op_str, path.display())
            };

            if let Some(diff) = &event.diff {
                format_log_entry_with_diff(timestamp, event_type, &desc, diff)
            } else {
                format_log_entry(timestamp, event_type, &desc)
            }
        }

        EventType::ToolCall {
            tool_name,
            args_summary,
            success,
            ..
        } => {
            let status = if *success { "✓" } else { "✗" };
            format_log_entry(
                timestamp,
                &format!("Tool: {}", tool_name),
                &format!("{} {}", status, args_summary),
            )
        }

        EventType::AgentReasoning {
            content,
            decision_type,
        } => {
            let prefix = if decision_type.is_some() {
                "**Decision:** "
            } else {
                ""
            };
            format_log_entry(timestamp, "Reasoning", &format!("{}{}", prefix, content))
        }

        EventType::UserFeedback {
            feedback_type,
            target_tool,
            comment,
        } => {
            let fb = format!("{:?}", feedback_type);
            let target = target_tool.as_deref().unwrap_or("action");
            let cmt = comment
                .as_deref()
                .map(|c| format!(" — {}", c))
                .unwrap_or_default();
            format_log_entry(
                timestamp,
                "User Feedback",
                &format!("{} on {}{}", fb, target, cmt),
            )
        }

        EventType::ErrorRecovery {
            error_message,
            recovery_action,
            resolved,
        } => {
            let status = if *resolved { "Resolved" } else { "Encountered" };
            let recovery = recovery_action
                .as_deref()
                .map(|r| format!(" → {}", r))
                .unwrap_or_default();
            format_log_entry(
                timestamp,
                &format!("Error {}", status),
                &format!("{}{}", error_message, recovery),
            )
        }

        EventType::AiResponse {
            content,
            truncated,
            duration_ms,
        } => {
            let dur = duration_ms
                .map(|d| format!(" ({}ms)", d))
                .unwrap_or_default();
            let trunc = if *truncated { " [truncated]" } else { "" };
            format_log_entry(
                timestamp,
                "AI Response",
                &format!("{}{}{}", truncate_str(content, 200), trunc, dur),
            )
        }

        EventType::CommitBoundary {
            suggested_message,
            files_in_scope,
        } => {
            let files = files_in_scope
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let msg = suggested_message.as_deref().unwrap_or("Ready for commit");
            format_log_entry(
                timestamp,
                "Commit Boundary",
                &format!("**{}**\nFiles: {}", msg, files),
            )
        }

        EventType::SessionStart { initial_request } => format_log_entry(
            timestamp,
            "Session Start",
            &format!("**Goal:** {}", initial_request),
        ),

        EventType::SessionEnd { summary } => {
            let sum = summary.as_deref().unwrap_or("Session ended");
            format_log_entry(timestamp, "Session End", sum)
        }
    }
}

/// Update state.md using rule-based logic (fallback when LLM not available)
async fn update_state_rule_based(session: &Session, event: &SessionEvent) -> Result<()> {
    use super::events::EventType;

    let mut state = session.read_state().await?;
    let now = Utc::now().format("%Y-%m-%d %H:%M").to_string();

    // Update the "Updated:" timestamp
    if let Some(idx) = state.find("Updated:") {
        if let Some(end_idx) = state[idx..].find('\n') {
            let before = &state[..idx];
            let after = &state[idx + end_idx..];
            state = format!("{}Updated: {}{}", before, now, after);
        }
    }

    // Update based on event type
    match &event.event_type {
        EventType::FileEdit {
            path,
            operation: _,
            summary,
        } => {
            // Add file to "## Files" section if not present
            let path_str = path.display().to_string();
            if !state.contains(&path_str) {
                if let Some(files_idx) = state.find("## Files") {
                    if let Some(next_section) = state[files_idx + 8..].find("\n## ") {
                        let insert_pos = files_idx + 8 + next_section;
                        let summary_str = summary.as_deref().unwrap_or("In progress");
                        let entry = format!("\n- `{}` — {}", path_str, summary_str);
                        state.insert_str(insert_pos, &entry);
                    } else {
                        // No next section, append at end of Files section
                        let summary_str = summary.as_deref().unwrap_or("In progress");
                        let entry = format!("\n- `{}` — {}", path_str, summary_str);
                        if let Some(end_idx) = state[files_idx..].find("\n\n") {
                            state.insert_str(files_idx + end_idx, &entry);
                        } else {
                            state.push_str(&entry);
                        }
                    }
                }
            }
        }

        EventType::AgentReasoning {
            content,
            decision_type: Some(_),
        } => {
            // This is a decision - could update narrative
            // For now, just ensure we don't lose this info
            tracing::debug!("Decision recorded: {}", truncate_str(content, 100));
        }

        EventType::ErrorRecovery {
            error_message,
            resolved: false,
            ..
        } => {
            // Add to open questions if it's an unresolved error
            if state.contains("## Open Questions") {
                if let Some(idx) = state.find("## Open Questions") {
                    if let Some(next_section) = state[idx + 17..].find("\n## ") {
                        let insert_pos = idx + 17 + next_section;
                        let entry =
                            format!("\n- How to resolve: {}", truncate_str(error_message, 100));
                        state.insert_str(insert_pos, &entry);
                    }
                }
            }
        }

        _ => {
            // Other events don't modify state in rule-based mode
        }
    }

    // Write updated state
    session.update_state(&state).await?;
    Ok(())
}

/// Truncate a string to a maximum length
fn truncate_str(s: &str, max_len: usize) -> &str {
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

/// Context for LLM state update (future use)
#[allow(dead_code)]
pub struct StateUpdateContext {
    pub current_state: String,
    pub event_summary: String,
    pub system_prompt: &'static str,
}

impl StateUpdateContext {
    #[allow(dead_code)]
    pub fn new(current_state: String, event: &SessionEvent) -> Self {
        let event_summary = format!(
            "Event: {}\nContent: {}",
            event.event_type.name(),
            truncate_str(&event.content, 500)
        );

        Self {
            current_state,
            event_summary,
            system_prompt: STATE_UPDATE_PROMPT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::events::{EventType, FileOperation};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn make_test_event(event_type: EventType) -> SessionEvent {
        SessionEvent::new(
            "test-session".to_string(),
            event_type,
            "test content".to_string(),
        )
    }

    #[test]
    fn test_format_file_edit_log() {
        let event = SessionEvent::new(
            "test".to_string(),
            EventType::FileEdit {
                path: PathBuf::from("src/main.rs"),
                operation: FileOperation::Modify,
                summary: Some("Added main function".to_string()),
            },
            "content".to_string(),
        );

        let log = format_event_for_log(&event);
        assert!(log.contains("file_edit"));
        assert!(log.contains("src/main.rs"));
        assert!(log.contains("Added main function"));
    }

    #[test]
    fn test_format_tool_call_log() {
        let event = SessionEvent::new(
            "test".to_string(),
            EventType::ToolCall {
                tool_name: "read_file".to_string(),
                args_summary: "path=src/lib.rs".to_string(),
                reasoning: None,
                success: true,
            },
            "content".to_string(),
        );

        let log = format_event_for_log(&event);
        assert!(log.contains("Tool: read_file"));
        assert!(log.contains("✓"));
        assert!(log.contains("path=src/lib.rs"));
    }

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 5), "hello");
        // Test with multi-byte characters
        assert_eq!(truncate_str("héllo", 10), "héllo");
    }

    #[tokio::test]
    async fn test_processor_lifecycle() {
        let temp = TempDir::new().unwrap();
        let config = ProcessorConfig {
            sessions_dir: temp.path().to_path_buf(),
            use_llm: false,
            write_raw_events: true,
        };

        let processor = Processor::spawn(config);

        // Just test that it starts and shuts down cleanly
        processor.shutdown().await;
    }
}

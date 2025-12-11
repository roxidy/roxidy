//! File format templates for markdown-based session storage.
//!
//! This module defines the formats for:
//! - `meta.toml`: Machine-managed session metadata
//! - `state.md`: LLM-managed current session state
//! - `log.md`: Append-only event log

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Initial template for state.md when a session starts
pub fn initial_state_template(session_id: &str, initial_request: &str) -> String {
    let now = Utc::now().format("%Y-%m-%d %H:%M");
    format!(
        r#"# Session: {session_id}
Started: {now}
Updated: {now}

## Goal
{initial_request}

### Sub-goals
(none yet)

## Narrative
Session started. Working on the initial request.

## Files
(none yet)

## Open Questions
(none yet)
"#
    )
}

/// Initial template for log.md when a session starts
pub fn initial_log_template(session_id: &str, initial_request: &str) -> String {
    let time = Utc::now().format("%H:%M");
    format!(
        r#"# Session Log: {session_id}

## {time} — Session Start
**Goal:** {initial_request}
"#
    )
}

/// Format a log entry for appending to log.md
pub fn format_log_entry(timestamp: &DateTime<Utc>, event_type: &str, content: &str) -> String {
    let time = timestamp.format("%H:%M");
    format!(
        r#"
## {time} — {event_type}
{content}
"#
    )
}

/// Format a log entry with a diff
pub fn format_log_entry_with_diff(
    timestamp: &DateTime<Utc>,
    event_type: &str,
    description: &str,
    diff: &str,
) -> String {
    let time = timestamp.format("%H:%M");
    format!(
        r#"
## {time} — {event_type}
{description}
```diff
{diff}
```
"#
    )
}

/// Session metadata stored in meta.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetaToml {
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: String, // "active", "completed", "abandoned"
    pub context: SessionContextToml,
}

/// Context section of meta.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContextToml {
    pub cwd: PathBuf,
    pub git_root: Option<PathBuf>,
    pub git_branch: Option<String>,
    pub initial_request: String,
}

impl SessionMetaToml {
    /// Create a new session metadata
    pub fn new(session_id: String, cwd: PathBuf, initial_request: String) -> Self {
        let now = Utc::now();
        Self {
            session_id,
            created_at: now,
            updated_at: now,
            status: "active".to_string(),
            context: SessionContextToml {
                cwd,
                git_root: None,
                git_branch: None,
                initial_request,
            },
        }
    }

    /// Update the timestamp
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    /// Serialize to TOML string
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Deserialize from TOML string
    pub fn from_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }
}

/// LLM prompt for updating state.md
pub const STATE_UPDATE_PROMPT: &str = r#"You maintain a session state file in markdown format.

Given the current state file and a new event, return the COMPLETE updated state file.

## Rules
1. Keep the same structure (headers: Goal, Sub-goals, Narrative, Files, Open Questions)
2. Update the "Updated:" timestamp to the current time
3. Keep the state concise - this will be injected into agent context
4. Mark completed sub-goals with [x], incomplete with [ ]
5. The Narrative should be 2-3 sentences summarizing progress
6. Only list files that are actively relevant
7. Remove answered questions from Open Questions

## Important
- Return ONLY the updated markdown, no explanations
- Preserve information - don't lose context from previous state
- Keep total size under 2000 tokens
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state_template() {
        let state = initial_state_template("abc123", "Implement user authentication");
        assert!(state.contains("# Session: abc123"));
        assert!(state.contains("Implement user authentication"));
        assert!(state.contains("## Goal"));
        assert!(state.contains("## Narrative"));
    }

    #[test]
    fn test_initial_log_template() {
        let log = initial_log_template("abc123", "Add feature X");
        assert!(log.contains("# Session Log: abc123"));
        assert!(log.contains("Session Start"));
        assert!(log.contains("Add feature X"));
    }

    #[test]
    fn test_format_log_entry() {
        let now = Utc::now();
        let entry = format_log_entry(
            &now,
            "File Read",
            "Read `src/main.rs` to understand structure.",
        );
        assert!(entry.contains("File Read"));
        assert!(entry.contains("src/main.rs"));
    }

    #[test]
    fn test_format_log_entry_with_diff() {
        let now = Utc::now();
        let entry = format_log_entry_with_diff(
            &now,
            "File Edit",
            "Added authentication module",
            "+pub fn authenticate() {}\n+pub fn logout() {}",
        );
        assert!(entry.contains("File Edit"));
        assert!(entry.contains("```diff"));
        assert!(entry.contains("+pub fn authenticate"));
    }

    #[test]
    fn test_session_meta_toml() {
        let meta = SessionMetaToml::new(
            "test-123".to_string(),
            PathBuf::from("/home/user/project"),
            "Build something cool".to_string(),
        );

        assert_eq!(meta.session_id, "test-123");
        assert_eq!(meta.status, "active");

        let toml_str = meta.to_toml().unwrap();
        assert!(toml_str.contains("session_id = \"test-123\""));
        assert!(toml_str.contains("status = \"active\""));

        let parsed = SessionMetaToml::from_toml(&toml_str).unwrap();
        assert_eq!(parsed.session_id, meta.session_id);
    }
}

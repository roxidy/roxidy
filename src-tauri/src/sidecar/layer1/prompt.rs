//! Prompts for the sidecar LLM to interpret events and update session state.

use super::state::SessionState;
use crate::sidecar::events::SessionEvent;

/// System prompt for the session state interpreter
pub const STATE_INTERPRETER_SYSTEM: &str = r#"You are a session state interpreter for an AI coding agent.

Your job is to analyze incoming events and update the session state to reflect the current understanding of the session. You receive the current state and a new event, and return the updated state with a list of changes.

## State Components

1. **Goal Stack**: Track what the agent is trying to accomplish
   - Add goals from user prompts (source: initial_prompt or user_clarification)
   - Mark goals as completed when evidence shows they're done
   - Add sub-goals when the agent breaks down work
   - Goals can be inferred from context (source: inferred)

2. **Narrative**: 2-3 sentence summary of session progress
   - Update when significant progress occurs
   - Focus on what has been accomplished and what's in progress
   - Keep it concise and factual

3. **Decisions**: Record when the agent chooses between alternatives
   - Include the choice made and why
   - List alternatives that were rejected
   - Only record meaningful architectural/design decisions

4. **File Contexts**: Track understanding of files
   - Add when a file is read or modified
   - Include a brief summary of the file's purpose
   - Note why the file is relevant to current goals

5. **Errors**: Track problems encountered
   - Add when errors occur
   - Mark as resolved when fixed
   - Include context about what was being attempted

6. **Open Questions**: Track unresolved ambiguities
   - Add when the agent expresses uncertainty
   - Remove when questions are answered

## Response Format

Return a JSON object with:
- "updated_state": The full updated SessionState (or null if no changes)
- "changes": Array of human-readable change descriptions

Only update fields that have actually changed. If no updates are needed, return null for updated_state and an empty changes array."#;

/// Format the interpretation prompt for a specific event
pub fn format_interpretation_prompt(state: &SessionState, event: &SessionEvent) -> String {
    let state_json = serde_json::to_string_pretty(state).unwrap_or_else(|_| "{}".to_string());

    // Create a simplified event representation for the prompt
    let event_summary = format_event_summary(event);

    format!(
        r#"## Current State
```json
{state_json}
```

## New Event
{event_summary}

Analyze this event and return the updated state. Remember:
- Only update what has changed
- Keep the narrative concise (2-3 sentences)
- Only record meaningful decisions
- If no changes are needed, return {{"updated_state": null, "changes": []}}

Return valid JSON only."#
    )
}

/// Format an event into a summary for the prompt
fn format_event_summary(event: &SessionEvent) -> String {
    use crate::sidecar::events::EventType;

    let mut summary = format!(
        "Type: {}\nTimestamp: {}\n",
        event.event_type.name(),
        event.timestamp
    );

    match &event.event_type {
        EventType::UserPrompt { intent } => {
            summary.push_str(&format!("Intent: {}\n", truncate(intent, 500)));
        }
        EventType::FileEdit {
            path,
            operation,
            summary: file_summary,
        } => {
            summary.push_str(&format!(
                "File: {}\nOperation: {:?}\n",
                path.display(),
                operation
            ));
            if let Some(s) = file_summary {
                summary.push_str(&format!("Summary: {}\n", s));
            }
        }
        EventType::ToolCall {
            tool_name,
            args_summary,
            reasoning,
            success,
        } => {
            summary.push_str(&format!(
                "Tool: {}\nArgs: {}\nSuccess: {}\n",
                tool_name,
                truncate(args_summary, 200),
                success
            ));
            if let Some(r) = reasoning {
                summary.push_str(&format!("Reasoning: {}\n", truncate(r, 300)));
            }
        }
        EventType::AgentReasoning {
            content,
            decision_type,
        } => {
            summary.push_str(&format!("Reasoning: {}\n", truncate(content, 500)));
            if let Some(dt) = decision_type {
                summary.push_str(&format!("Decision Type: {:?}\n", dt));
            }
        }
        EventType::UserFeedback {
            feedback_type,
            target_tool,
            comment,
        } => {
            summary.push_str(&format!("Feedback: {:?}\n", feedback_type));
            if let Some(tool) = target_tool {
                summary.push_str(&format!("Target: {}\n", tool));
            }
            if let Some(c) = comment {
                summary.push_str(&format!("Comment: {}\n", c));
            }
        }
        EventType::ErrorRecovery {
            error_message,
            recovery_action,
            resolved,
        } => {
            summary.push_str(&format!(
                "Error: {}\nResolved: {}\n",
                truncate(error_message, 300),
                resolved
            ));
            if let Some(action) = recovery_action {
                summary.push_str(&format!("Recovery: {}\n", action));
            }
        }
        EventType::AiResponse {
            content,
            truncated,
            duration_ms,
        } => {
            summary.push_str(&format!("Response: {}\n", truncate(content, 500)));
            if *truncated {
                summary.push_str("(truncated)\n");
            }
            if let Some(ms) = duration_ms {
                summary.push_str(&format!("Duration: {}ms\n", ms));
            }
        }
        EventType::CommitBoundary {
            suggested_message,
            files_in_scope,
        } => {
            summary.push_str(&format!("Files: {:?}\n", files_in_scope));
            if let Some(msg) = suggested_message {
                summary.push_str(&format!("Suggested: {}\n", msg));
            }
        }
        EventType::SessionStart { initial_request } => {
            summary.push_str(&format!("Initial: {}\n", truncate(initial_request, 300)));
        }
        EventType::SessionEnd { summary: end_summary } => {
            if let Some(s) = end_summary {
                summary.push_str(&format!("Summary: {}\n", s));
            }
        }
    }

    // Add file context if present
    if !event.files_modified.is_empty() {
        summary.push_str(&format!("Files Modified: {:?}\n", event.files_modified));
    }
    if let Some(files) = &event.files_accessed {
        summary.push_str(&format!("Files Accessed: {:?}\n", files));
    }
    if let Some(output) = &event.tool_output {
        summary.push_str(&format!("Tool Output: {}\n", truncate(output, 500)));
    }
    if let Some(diff) = &event.diff {
        summary.push_str(&format!("Diff: {}\n", truncate(diff, 500)));
    }

    summary
}

/// Response from the state interpreter
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InterpreterResponse {
    /// Updated state (null if no changes)
    pub updated_state: Option<SessionState>,
    /// Human-readable change descriptions
    pub changes: Vec<String>,
}

impl InterpreterResponse {
    /// Check if there were any changes
    pub fn has_changes(&self) -> bool {
        self.updated_state.is_some()
    }
}

/// Parse the interpreter response from LLM output
pub fn parse_interpreter_response(response: &str) -> Result<InterpreterResponse, String> {
    // Try to find JSON in the response (handle markdown code blocks)
    let json_str = extract_json(response);

    serde_json::from_str(json_str).map_err(|e| format!("Failed to parse interpreter response: {}", e))
}

/// Extract JSON from a response that might contain markdown code blocks
fn extract_json(response: &str) -> &str {
    // Try to find ```json ... ``` block
    if let Some(start) = response.find("```json") {
        let content_start = start + 7;
        if let Some(end) = response[content_start..].find("```") {
            return response[content_start..content_start + end].trim();
        }
    }

    // Try to find ``` ... ``` block
    if let Some(start) = response.find("```") {
        let content_start = start + 3;
        // Skip language identifier if present
        let content_start = response[content_start..]
            .find('\n')
            .map(|i| content_start + i + 1)
            .unwrap_or(content_start);
        if let Some(end) = response[content_start..].find("```") {
            return response[content_start..content_start + end].trim();
        }
    }

    // Assume the whole response is JSON
    response.trim()
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
    use crate::sidecar::events::{DecisionType, EventType, FeedbackType, FileOperation};
    use std::path::PathBuf;
    use uuid::Uuid;

    #[test]
    fn test_format_user_prompt_event() {
        let session_id = Uuid::new_v4();
        let event = SessionEvent::user_prompt(session_id, "Add authentication to the API");

        let summary = format_event_summary(&event);

        assert!(summary.contains("user_prompt"));
        assert!(summary.contains("authentication"));
    }

    #[test]
    fn test_format_file_edit_event() {
        let session_id = Uuid::new_v4();
        let event = SessionEvent::file_edit(
            session_id,
            PathBuf::from("src/auth.rs"),
            FileOperation::Modify,
            Some("Added JWT validation".to_string()),
        );

        let summary = format_event_summary(&event);

        assert!(summary.contains("file_edit"));
        assert!(summary.contains("auth.rs"));
        assert!(summary.contains("Modify"));
    }

    #[test]
    fn test_format_reasoning_event() {
        let session_id = Uuid::new_v4();
        let event = SessionEvent::reasoning(
            session_id,
            "I'll use JWT for authentication because it's stateless",
            Some(DecisionType::ApproachChoice),
        );

        let summary = format_event_summary(&event);

        assert!(summary.contains("reasoning"));
        assert!(summary.contains("JWT"));
        assert!(summary.contains("ApproachChoice"));
    }

    #[test]
    fn test_format_interpretation_prompt() {
        let session_id = Uuid::new_v4();
        let state = SessionState::with_initial_goal(session_id, "Add auth");
        let event = SessionEvent::user_prompt(session_id, "Use JWT tokens");

        let prompt = format_interpretation_prompt(&state, &event);

        assert!(prompt.contains("Current State"));
        assert!(prompt.contains("New Event"));
        assert!(prompt.contains("Add auth"));
        assert!(prompt.contains("JWT"));
    }

    #[test]
    fn test_parse_interpreter_response() {
        let response = r#"{
            "updated_state": null,
            "changes": []
        }"#;

        let parsed = parse_interpreter_response(response).unwrap();
        assert!(!parsed.has_changes());
        assert!(parsed.changes.is_empty());
    }

    #[test]
    fn test_parse_interpreter_response_with_markdown() {
        let response = r#"Here's the updated state:

```json
{
    "updated_state": null,
    "changes": ["No significant changes"]
}
```"#;

        let parsed = parse_interpreter_response(response).unwrap();
        assert_eq!(parsed.changes.len(), 1);
    }

    #[test]
    fn test_extract_json() {
        // Plain JSON
        assert_eq!(extract_json(r#"{"a": 1}"#), r#"{"a": 1}"#);

        // Markdown code block
        let with_markdown = "```json\n{\"a\": 1}\n```";
        assert_eq!(extract_json(with_markdown), r#"{"a": 1}"#);

        // With explanation
        let with_explanation = "Here's the result:\n```\n{\"a\": 1}\n```\nDone!";
        assert_eq!(extract_json(with_explanation), r#"{"a": 1}"#);
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("a longer string here", 10), "a longer …");
    }
}

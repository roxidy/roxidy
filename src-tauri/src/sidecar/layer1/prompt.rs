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

   **New fields**:
   - `priority`: Set to high/medium/low based on user emphasis (e.g., "urgent", "critical", "when you have time")
     * high: User uses urgent language or this is blocking other work
     * medium: Normal priority (default)
     * low: Nice-to-have or future improvements
   - `blocked_by`: Note what's blocking progress (if anything), e.g., "waiting for user input on auth approach"
   - `progress_notes`: Add timestamped notes when significant progress occurs on a goal
     * Use for milestones like "completed data model design" or "all tests passing"
     * Don't add trivial notes for every small step

2. **Narrative**: 2-3 sentence summary of session progress
   - Update when significant progress occurs
   - Focus on what has been accomplished and what's in progress
   - Keep it concise and factual

3. **Decisions**: Record when the agent chooses between alternatives
   - Include the choice made and why
   - List alternatives that were rejected
   - Only record meaningful architectural/design decisions

   **New fields**:
   - `category`: Classify the decision type
     * architecture: Structural/design patterns (e.g., "use microservices vs monolith")
     * library: Dependency choices (e.g., "use tokio vs async-std")
     * approach: General methodology (e.g., "TDD vs implementation-first")
     * tradeoff: Competing concerns (e.g., "optimize for speed vs memory")
     * fallback: When primary approach fails (e.g., "use polling since websockets not available")
   - `confidence`: How certain is the agent about this decision?
     * high: Strong evidence, clear best choice
     * medium: Good reasoning, but some uncertainty (default)
     * low: Uncertain but proceeding with best guess
     * uncertain: Significant doubts, may need revision
   - `reversible`: Can this decision be easily changed later?
     * true: Easy to undo (e.g., config changes, feature flags) - default
     * false: Hard to reverse (e.g., database schema migrations, API contracts)
   - `alternatives`: Now includes `rejection_reason` for each alternative
     * Provide specific reasons why alternatives were rejected
     * Example: {"description": "Use sessions", "rejection_reason": "Requires server-side state management"}
   - `related_files`: List files affected by this decision
     * Include files that were created or modified as a result
     * Helps trace the impact of decisions

4. **File Contexts**: Track understanding of files
   - Add when a file is read or modified
   - Include a brief summary of the file's purpose
   - Note why the file is relevant to current goals

   **New fields**:
   - `understanding_level`: How well does the agent understand this file?
     * full: Read and understood the entire file (e.g., after Read tool with careful analysis)
     * partial: Skimmed or read portions (e.g., quick grep, read specific functions)
     * surface: Just aware of the file's existence/path (e.g., from file listings)
   - `key_exports`: Main functions/types/classes exported by this file
     * Example: ["authenticate_user", "TokenValidator", "AuthConfig"]
     * Helps quickly recall what this file provides
   - `dependencies`: Other files this file imports/depends on
     * Track relationships between files
     * Example: ["/src/config.rs", "/src/database.rs"]
   - `change_history`: Track modifications made to this file
     * Each entry includes: timestamp, summary, diff_preview (first ~200 chars)
     * Example: {"summary": "Added JWT validation function", "diff_preview": "+fn validate_token(...)"}
     * Helps understand the evolution of the file during the session
   - `notes`: Agent's observations about this file
     * Freeform notes like "This file needs refactoring" or "Contains legacy auth code"
     * Use for insights that don't fit other fields

5. **Errors**: Track problems encountered
   - Add when errors occur
   - Mark as resolved when fixed
   - Include context about what was being attempted

6. **Open Questions**: Track unresolved ambiguities
   - Now a structured object instead of just a string

   **Fields**:
   - `id`: Unique identifier (auto-generated)
   - `question`: The question text
   - `source`: Where did this question come from?
     * from_reasoning: Agent expressed uncertainty in its reasoning
     * from_user: User explicitly asked a question
     * inferred_from_error: Question arose from an error/failure
   - `context`: What situation prompted this question?
     * Example: "While implementing authentication, encountered multiple auth libraries"
   - `priority`: How important is answering this question?
     * blocking: Cannot proceed without an answer (agent should pause and ask user)
     * important: Should address soon but can continue (default)
     * informational: Nice to know but not urgent
   - `answer`: The answer text (if question has been answered)
     * Set this when the agent or user provides an answer
     * Also set `answered_at` timestamp

## Response Format

Return a JSON object with:
- "updated_state": The full updated SessionState (or null if no changes)
- "changes": Array of human-readable change descriptions

Only update fields that have actually changed. If no updates are needed, return null for updated_state and an empty changes array.

## Guidelines for New Fields

**Goal Priority**: Look for urgency indicators in user language:
- "urgent", "asap", "critical", "blocking" → high
- "when you get a chance", "nice to have", "eventually" → low
- Default to medium if no indicators

**Decision Confidence**: Base on the strength of reasoning and available information:
- Clear requirements + obvious best choice → high
- Good reasoning but some unknowns → medium
- Guessing or insufficient information → low/uncertain

**File Understanding Level**: Be honest about comprehension:
- Only set to "full" if the file was actually read completely
- Use "partial" for grep/search results or skimming
- Use "surface" for files mentioned but not examined

**Progress Notes**: Only add for significant milestones:
- Completion of major sub-tasks
- Important discoveries or insights
- Successful resolution of blockers
- Don't add notes for every small action

**Open Question Priority**:
- Set to "blocking" only if the agent genuinely cannot proceed
- Use "important" for questions that affect the approach but aren't blockers
- Use "informational" for curiosity or future improvements"#;

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
        EventType::SessionEnd {
            summary: end_summary,
        } => {
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

    serde_json::from_str(json_str)
        .map_err(|e| format!("Failed to parse interpreter response: {}", e))
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

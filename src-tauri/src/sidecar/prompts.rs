//! LLM prompt templates for the sidecar system.
//!
//! These prompts are designed to work with small models like Qwen 0.5B,
//! focusing on structured synthesis tasks rather than open-ended reasoning.

/// Context for generating a commit message
pub struct CommitContext {
    /// Summary of the session
    pub session_summary: String,
    /// Files that were changed
    pub files_changed: Vec<String>,
    /// Key decisions made during the session
    pub decisions: Vec<String>,
    /// Initial user request
    pub initial_request: String,
}

impl CommitContext {
    /// Format files for the prompt
    pub fn files_formatted(&self) -> String {
        if self.files_changed.is_empty() {
            "No files changed".to_string()
        } else {
            self.files_changed
                .iter()
                .map(|f| format!("- {}", f))
                .collect::<Vec<_>>()
                .join("\n")
        }
    }

    /// Format decisions for the prompt
    pub fn decisions_formatted(&self) -> String {
        if self.decisions.is_empty() {
            "No explicit decisions recorded".to_string()
        } else {
            self.decisions
                .iter()
                .map(|d| format!("- {}", d))
                .collect::<Vec<_>>()
                .join("\n")
        }
    }
}

/// Generate a checkpoint summary prompt
pub fn checkpoint_summary(events_context: &str) -> String {
    format!(
        r#"<task>
Summarize these agent actions into a brief checkpoint (2-3 sentences).
Focus on WHAT was accomplished and WHY, not implementation details.
</task>

<events>
{events_context}
</events>

<format>
Write a concise summary suitable for later retrieval. Include:
- Main goal/intent
- Key changes made
- Any important decisions or tradeoffs
</format>

Summary:"#
    )
}

/// Generate a commit message prompt
pub fn commit_message(context: &CommitContext) -> String {
    format!(
        r#"<task>
Generate a git commit message for these changes.
</task>

<user_request>
{initial_request}
</user_request>

<session_summary>
{summary}
</session_summary>

<files_changed>
{files}
</files_changed>

<key_decisions>
{decisions}
</key_decisions>

<format>
Output a commit message following conventional commits:
- Subject line: type(scope): description (max 50 chars)
- Blank line
- Body: explain WHY, not what (the diff shows what)
- Focus on intent and reasoning captured during the session

Types: feat, fix, refactor, docs, test, chore
</format>

<commit>"#,
        initial_request = context.initial_request,
        summary = context.session_summary,
        files = context.files_formatted(),
        decisions = context.decisions_formatted()
    )
}

/// Generate a history query prompt
pub fn history_query(question: &str, context: &str) -> String {
    format!(
        r#"<task>
Answer this question about past coding sessions using the provided context.
If the context doesn't contain enough information, say so.
</task>

<question>
{question}
</question>

<relevant_context>
{context}
</relevant_context>

<instructions>
- Answer directly and concisely
- Reference specific events/decisions when possible
- If uncertain, explain what information is missing
</instructions>

Answer:"#
    )
}

/// Generate a session summary prompt
pub fn session_summary(events_summary: &str, initial_request: &str) -> String {
    format!(
        r#"<task>
Summarize this coding session into 3-5 bullet points.
</task>

<initial_request>
{initial_request}
</initial_request>

<session_events>
{events_summary}
</session_events>

<format>
Write a summary that:
- Captures the main accomplishments
- Notes any significant decisions or tradeoffs
- Identifies incomplete work or follow-up needed
- Uses past tense ("Added", "Fixed", "Refactored")
</format>

Summary:
•"#
    )
}

/// Template-based commit message (no LLM required)
pub fn template_commit_message(
    files_changed: &[String],
    event_count: usize,
    initial_request: Option<&str>,
) -> String {
    let file_count = files_changed.len();

    // Determine commit type from file patterns
    let commit_type = if files_changed.iter().any(|f| f.contains("test")) {
        "test"
    } else if files_changed.iter().any(|f| f.ends_with(".md")) {
        "docs"
    } else {
        "feat"
    };

    // Determine scope from common directory
    let scope = find_common_scope(files_changed);

    // Build subject line
    let subject = if let Some(request) = initial_request {
        let truncated = if request.len() > 40 {
            format!("{}...", &request[..40])
        } else {
            request.to_string()
        };
        format!("{}({}): {}", commit_type, scope, truncated.to_lowercase())
    } else if file_count == 1 {
        format!("{}({}): update {}", commit_type, scope, files_changed[0])
    } else {
        format!("{}({}): update {} files", commit_type, scope, file_count)
    };

    // Build body
    let body = format!(
        "Changes across {} file(s) with {} recorded events.\n\nFiles:\n{}",
        file_count,
        event_count,
        files_changed
            .iter()
            .take(10)
            .map(|f| format!("  - {}", f))
            .collect::<Vec<_>>()
            .join("\n")
    );

    format!("{}\n\n{}", subject, body)
}

/// Find common scope from file paths
fn find_common_scope(files: &[String]) -> String {
    if files.is_empty() {
        return "root".to_string();
    }

    // Find common directory prefix
    let parts: Vec<Vec<&str>> = files
        .iter()
        .map(|f| f.split('/').collect::<Vec<_>>())
        .collect();

    if parts.is_empty() {
        return "root".to_string();
    }

    let mut common_depth = 0;
    let first = &parts[0];

    'outer: for i in 0..first.len() {
        let component = first[i];
        for other in &parts[1..] {
            if i >= other.len() || other[i] != component {
                break 'outer;
            }
        }
        common_depth = i + 1;
    }

    if common_depth > 0 && common_depth < first.len() {
        // Use the deepest common directory
        first[common_depth - 1].to_string()
    } else if !first.is_empty() {
        // Use the first component
        first[0].to_string()
    } else {
        "root".to_string()
    }
}

/// Template-based session summary (no LLM required)
pub fn template_session_summary(
    files_changed: &[String],
    event_count: usize,
    checkpoint_count: usize,
    initial_request: &str,
) -> String {
    let mut lines = Vec::new();

    lines.push(format!("• Goal: {}", truncate(initial_request, 100)));

    if !files_changed.is_empty() {
        lines.push(format!("• Modified {} file(s)", files_changed.len()));
    }

    lines.push(format!(
        "• {} event(s), {} checkpoint(s)",
        event_count, checkpoint_count
    ));

    if files_changed.len() <= 5 {
        let files_list = files_changed.join(", ");
        lines.push(format!("• Files: {}", files_list));
    }

    lines.join("\n")
}

/// Truncate a string to a maximum length
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let mut result: String = s.chars().take(max_len.saturating_sub(3)).collect();
        result.push_str("...");
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_summary_prompt() {
        let events = "- file_edit: /src/lib.rs\n- reasoning: Using approach A";
        let prompt = checkpoint_summary(events);

        assert!(prompt.contains("agent actions"));
        assert!(prompt.contains(events));
    }

    #[test]
    fn test_commit_message_prompt() {
        let context = CommitContext {
            session_summary: "Added authentication".to_string(),
            files_changed: vec!["src/auth.rs".to_string()],
            decisions: vec!["Using JWT tokens".to_string()],
            initial_request: "Add user login".to_string(),
        };

        let prompt = commit_message(&context);

        assert!(prompt.contains("Add user login"));
        assert!(prompt.contains("src/auth.rs"));
        assert!(prompt.contains("JWT tokens"));
    }

    #[test]
    fn test_history_query_prompt() {
        let question = "Why did I change the auth system?";
        let context = "- reasoning: Switched to JWT for statelessness";

        let prompt = history_query(question, context);

        assert!(prompt.contains(question));
        assert!(prompt.contains(context));
    }

    #[test]
    fn test_template_commit_message() {
        let files = vec!["src/lib.rs".to_string(), "src/auth.rs".to_string()];
        let message = template_commit_message(&files, 10, Some("Add authentication"));

        assert!(message.contains("feat"));
        assert!(message.contains("authentication"));
    }

    #[test]
    fn test_find_common_scope() {
        assert_eq!(
            find_common_scope(&["src/lib.rs".to_string(), "src/auth.rs".to_string()]),
            "src"
        );

        assert_eq!(
            find_common_scope(&[
                "src/auth/login.rs".to_string(),
                "src/auth/logout.rs".to_string()
            ]),
            "auth"
        );

        assert_eq!(find_common_scope(&[]), "root");
    }

    #[test]
    fn test_template_session_summary() {
        let files = vec!["src/lib.rs".to_string()];
        let summary = template_session_summary(&files, 15, 2, "Add feature X");

        assert!(summary.contains("Goal:"));
        assert!(summary.contains("15 event(s)"));
        assert!(summary.contains("2 checkpoint(s)"));
    }
}

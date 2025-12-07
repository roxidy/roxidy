//! Public API for Layer 1 session state.
//!
//! This module provides functions for querying session state and
//! generating injectable context for the agent.

use uuid::Uuid;

use super::processor::Layer1Processor;
use super::state::{Decision, FileContext, SessionState};
use super::storage::Layer1Storage;

/// Get the current session state from the in-memory cache
pub fn get_session_state(processor: &Layer1Processor, session_id: Uuid) -> Option<SessionState> {
    processor.get_current_state(session_id)
}

/// Get the session state, falling back to storage if not in memory
pub async fn get_session_state_or_load(
    processor: &Layer1Processor,
    storage: &Layer1Storage,
    session_id: Uuid,
) -> Option<SessionState> {
    // Try in-memory first
    if let Some(state) = processor.get_current_state(session_id) {
        return Some(state);
    }

    // Fall back to storage
    match storage.get_latest_state(session_id).await {
        Ok(state) => state,
        Err(e) => {
            tracing::warn!("[layer1] Failed to load state from storage: {}", e);
            None
        }
    }
}

/// Generate context suitable for injection into the agent's system prompt
pub fn get_injectable_context(state: &SessionState) -> String {
    let mut sections = Vec::new();

    // Current Goal
    if let Some(goal) = state.current_goal() {
        sections.push(format!("**Current Goal:** {}", goal.description));

        // Sub-goals with progress
        if !goal.sub_goals.is_empty() {
            let sub_goals: Vec<String> = goal
                .sub_goals
                .iter()
                .map(|sg| {
                    let marker = if sg.completed { "[x]" } else { "[ ]" };
                    format!("- {} {}", marker, sg.description)
                })
                .collect();

            sections.push(format!("**Sub-goals:**\n{}", sub_goals.join("\n")));
        }
    }

    // Progress narrative
    if !state.narrative.is_empty() {
        sections.push(format!("**Progress:** {}", state.narrative));
    }

    // Files in Focus (limit to most relevant)
    let files_in_focus: Vec<String> = state
        .file_contexts
        .values()
        .take(5)
        .map(|fc| format!("- {} — {}", fc.path.display(), fc.summary))
        .collect();

    if !files_in_focus.is_empty() {
        sections.push(format!(
            "**Files in Focus:**\n{}",
            files_in_focus.join("\n")
        ));
    }

    // Unresolved Errors
    let unresolved_errors: Vec<String> = state
        .unresolved_errors()
        .iter()
        .take(3)
        .map(|e| format!("- {}: {}", truncate(&e.error, 50), e.context))
        .collect();

    if !unresolved_errors.is_empty() {
        sections.push(format!(
            "**Unresolved Errors:**\n{}",
            unresolved_errors.join("\n")
        ));
    }

    // Open Questions
    if !state.open_questions.is_empty() {
        let questions: Vec<String> = state
            .open_questions
            .iter()
            .take(3)
            .map(|q| format!("- {}", q))
            .collect();

        sections.push(format!("**Open Questions:**\n{}", questions.join("\n")));
    }

    // Recent Decisions (last 2)
    let recent_decisions: Vec<String> = state
        .decisions
        .iter()
        .rev()
        .take(2)
        .map(|d| format!("- {} ({})", d.choice, truncate(&d.rationale, 50)))
        .collect();

    if !recent_decisions.is_empty() {
        sections.push(format!(
            "**Recent Decisions:**\n{}",
            recent_decisions.join("\n")
        ));
    }

    if sections.is_empty() {
        return String::new();
    }

    format!("## Session Context\n\n{}", sections.join("\n\n"))
}

/// Generate a compact summary of the session state
pub fn get_state_summary(state: &SessionState) -> String {
    let mut parts = Vec::new();

    // Goal summary
    let incomplete_goals = state.incomplete_goals();
    if !incomplete_goals.is_empty() {
        parts.push(format!("{} active goal(s)", incomplete_goals.len()));
    }

    // File count
    if !state.file_contexts.is_empty() {
        parts.push(format!("{} file(s) tracked", state.file_contexts.len()));
    }

    // Decision count
    if !state.decisions.is_empty() {
        parts.push(format!("{} decision(s)", state.decisions.len()));
    }

    // Error count
    let unresolved = state.unresolved_errors().len();
    if unresolved > 0 {
        parts.push(format!("{} unresolved error(s)", unresolved));
    }

    if parts.is_empty() {
        "No session data".to_string()
    } else {
        parts.join(", ")
    }
}

/// Get all decisions from the session
pub fn get_session_decisions(state: &SessionState) -> Vec<&Decision> {
    state.decisions.iter().collect()
}

/// Get file context for a specific path
pub fn get_file_context<'a>(
    state: &'a SessionState,
    path: &std::path::Path,
) -> Option<&'a FileContext> {
    state.file_contexts.get(path)
}

/// Check if the session has any active (incomplete) goals
pub fn has_active_goals(state: &SessionState) -> bool {
    !state.incomplete_goals().is_empty()
}

/// Get the progress of the current goal's sub-goals
pub fn get_goal_progress(state: &SessionState) -> Option<(usize, usize)> {
    state.current_goal().map(|g| g.sub_goal_progress())
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
    use crate::sidecar::layer1::state::{ErrorEntry, Goal, GoalSource};
    use std::path::PathBuf;

    fn create_test_state() -> SessionState {
        let session_id = Uuid::new_v4();
        let mut state = SessionState::with_initial_goal(session_id, "Implement user authentication");

        // Add sub-goals
        state.add_sub_goal("Create User model".to_string(), GoalSource::Inferred);
        state.add_sub_goal("Implement login endpoint".to_string(), GoalSource::Inferred);
        state.add_sub_goal("Add JWT middleware".to_string(), GoalSource::Inferred);

        // Complete one sub-goal
        let sub_goal_id = state.goal_stack[0].sub_goals[0].id;
        state.complete_goal(sub_goal_id);

        // Add narrative
        state.update_narrative(
            "Created User model with email/password fields. Now implementing login endpoint."
                .to_string(),
        );

        // Add file context
        let mut file_ctx = FileContext::new(
            PathBuf::from("src/models/user.rs"),
            "User struct with password hashing".to_string(),
            "Core user model".to_string(),
        );
        file_ctx.mark_modified();
        state.update_file_context(PathBuf::from("src/models/user.rs"), file_ctx);

        // Add open question
        state.add_open_question("Should refresh tokens be stored in DB or Redis?".to_string());

        // Add a decision
        let decision = Decision::new(
            "Use JWT over sessions".to_string(),
            "Stateless authentication is simpler to scale".to_string(),
            vec!["Session-based auth".to_string()],
            Uuid::new_v4(),
        );
        state.record_decision(decision);

        state
    }

    #[test]
    fn test_injectable_context_format() {
        let state = create_test_state();
        let context = get_injectable_context(&state);

        // Check structure
        assert!(context.contains("## Session Context"));
        assert!(context.contains("**Current Goal:**"));
        assert!(context.contains("**Sub-goals:**"));
        assert!(context.contains("**Progress:**"));
        assert!(context.contains("**Files in Focus:**"));
        assert!(context.contains("**Open Questions:**"));
        assert!(context.contains("**Recent Decisions:**"));

        // Check content
        assert!(context.contains("Implement user authentication"));
        assert!(context.contains("[x] Create User model"));
        assert!(context.contains("[ ] Implement login endpoint"));
        assert!(context.contains("src/models/user.rs"));
        assert!(context.contains("refresh tokens"));
        assert!(context.contains("JWT over sessions"));
    }

    #[test]
    fn test_injectable_context_empty_state() {
        let state = SessionState::new(Uuid::new_v4());
        let context = get_injectable_context(&state);

        // Empty state should return empty string
        assert!(context.is_empty());
    }

    #[test]
    fn test_state_summary() {
        let state = create_test_state();
        let summary = get_state_summary(&state);

        assert!(summary.contains("goal"));
        assert!(summary.contains("file"));
        assert!(summary.contains("decision"));
    }

    #[test]
    fn test_get_session_decisions() {
        let state = create_test_state();
        let decisions = get_session_decisions(&state);

        assert_eq!(decisions.len(), 1);
        assert!(decisions[0].choice.contains("JWT"));
    }

    #[test]
    fn test_get_file_context() {
        let state = create_test_state();
        let path = PathBuf::from("src/models/user.rs");

        let ctx = get_file_context(&state, &path);
        assert!(ctx.is_some());
        assert!(ctx.unwrap().summary.contains("User struct"));

        let missing = get_file_context(&state, &PathBuf::from("nonexistent.rs"));
        assert!(missing.is_none());
    }

    #[test]
    fn test_has_active_goals() {
        let state = create_test_state();
        assert!(has_active_goals(&state));

        let empty_state = SessionState::new(Uuid::new_v4());
        assert!(!has_active_goals(&empty_state));
    }

    #[test]
    fn test_goal_progress() {
        let state = create_test_state();
        let progress = get_goal_progress(&state);

        assert!(progress.is_some());
        let (completed, total) = progress.unwrap();
        assert_eq!(completed, 1);
        assert_eq!(total, 3);
    }

    #[test]
    fn test_injectable_context_with_errors() {
        let mut state = create_test_state();

        // Add an unresolved error
        let error = ErrorEntry::new(
            "Compilation failed".to_string(),
            "Building the project".to_string(),
        );
        state.record_error(error);

        let context = get_injectable_context(&state);
        assert!(context.contains("**Unresolved Errors:**"));
        assert!(context.contains("Compilation failed"));
    }

    #[test]
    fn test_truncate_long_values() {
        let mut state = SessionState::new(Uuid::new_v4());

        // Add a very long error message
        let error = ErrorEntry::new(
            "This is a very long error message that should be truncated when displayed in the injectable context because it's too verbose".to_string(),
            "Running tests".to_string(),
        );
        state.record_error(error);

        let context = get_injectable_context(&state);
        // Should contain truncated version
        assert!(context.contains("Unresolved Errors"));
        assert!(context.contains("…")); // Truncation marker
    }
}

//! Session state data structures for Layer 1.
//!
//! These types represent the interpreted, high-level understanding of a session
//! derived from L0 raw events.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

/// Root session state containing all interpreted information about a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    /// Session identifier
    pub session_id: Uuid,

    /// When this state was last updated
    pub updated_at: DateTime<Utc>,

    /// Stack of goals (active goals the agent is working on)
    pub goal_stack: Vec<Goal>,

    /// 2-3 sentence narrative summary of session progress
    pub narrative: String,

    /// When the narrative was last updated
    pub narrative_updated_at: DateTime<Utc>,

    /// Key decisions made during the session
    pub decisions: Vec<Decision>,

    /// Understanding of files accessed during the session
    pub file_contexts: HashMap<PathBuf, FileContext>,

    /// Errors encountered and their resolution status
    pub errors: Vec<ErrorEntry>,

    /// Unresolved questions or ambiguities
    pub open_questions: Vec<String>,
}

impl SessionState {
    /// Create a new empty session state
    pub fn new(session_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            session_id,
            updated_at: now,
            goal_stack: Vec::new(),
            narrative: String::new(),
            narrative_updated_at: now,
            decisions: Vec::new(),
            file_contexts: HashMap::new(),
            errors: Vec::new(),
            open_questions: Vec::new(),
        }
    }

    /// Create a session state with an initial goal from user prompt
    pub fn with_initial_goal(session_id: Uuid, initial_request: &str) -> Self {
        let mut state = Self::new(session_id);
        state.goal_stack.push(Goal::new(
            initial_request.to_string(),
            GoalSource::InitialPrompt,
        ));
        state.narrative = format!("Started: {}", truncate(initial_request, 100));
        state
    }

    /// Add a new goal to the stack
    pub fn push_goal(&mut self, description: String, source: GoalSource) {
        self.goal_stack.push(Goal::new(description, source));
        self.updated_at = Utc::now();
    }

    /// Mark the top goal as completed
    pub fn complete_current_goal(&mut self) {
        if let Some(goal) = self.goal_stack.last_mut() {
            if !goal.completed {
                goal.completed = true;
                goal.completed_at = Some(Utc::now());
                self.updated_at = Utc::now();
            }
        }
    }

    /// Mark a specific goal as completed by ID
    pub fn complete_goal(&mut self, goal_id: Uuid) {
        fn mark_complete(goals: &mut [Goal], goal_id: Uuid) -> bool {
            for goal in goals.iter_mut() {
                if goal.id == goal_id {
                    goal.completed = true;
                    goal.completed_at = Some(Utc::now());
                    return true;
                }
                if mark_complete(&mut goal.sub_goals, goal_id) {
                    return true;
                }
            }
            false
        }

        if mark_complete(&mut self.goal_stack, goal_id) {
            self.updated_at = Utc::now();
        }
    }

    /// Add a sub-goal to the current goal
    pub fn add_sub_goal(&mut self, description: String, source: GoalSource) {
        if let Some(parent) = self.goal_stack.last_mut() {
            parent.sub_goals.push(Goal::new(description, source));
            self.updated_at = Utc::now();
        }
    }

    /// Update the narrative
    pub fn update_narrative(&mut self, narrative: String) {
        self.narrative = narrative;
        self.narrative_updated_at = Utc::now();
        self.updated_at = Utc::now();
    }

    /// Record a decision
    pub fn record_decision(&mut self, decision: Decision) {
        self.decisions.push(decision);
        self.updated_at = Utc::now();
    }

    /// Update or add a file context
    pub fn update_file_context(&mut self, path: PathBuf, context: FileContext) {
        self.file_contexts.insert(path, context);
        self.updated_at = Utc::now();
    }

    /// Record an error
    pub fn record_error(&mut self, error: ErrorEntry) {
        self.errors.push(error);
        self.updated_at = Utc::now();
    }

    /// Mark an error as resolved
    pub fn resolve_error(&mut self, error_id: Uuid, resolution: String) {
        if let Some(error) = self.errors.iter_mut().find(|e| e.id == error_id) {
            error.resolution = Some(resolution);
            error.resolved = true;
            error.resolved_at = Some(Utc::now());
            self.updated_at = Utc::now();
        }
    }

    /// Add an open question
    pub fn add_open_question(&mut self, question: String) {
        if !self.open_questions.contains(&question) {
            self.open_questions.push(question);
            self.updated_at = Utc::now();
        }
    }

    /// Remove an open question (answered)
    pub fn answer_question(&mut self, question: &str) {
        self.open_questions.retain(|q| q != question);
        self.updated_at = Utc::now();
    }

    /// Get the current primary goal (top of stack that isn't completed)
    pub fn current_goal(&self) -> Option<&Goal> {
        self.goal_stack.iter().rev().find(|g| !g.completed)
    }

    /// Get all incomplete goals (flat list)
    pub fn incomplete_goals(&self) -> Vec<&Goal> {
        fn collect_incomplete<'a>(goals: &'a [Goal], result: &mut Vec<&'a Goal>) {
            for goal in goals {
                if !goal.completed {
                    result.push(goal);
                }
                collect_incomplete(&goal.sub_goals, result);
            }
        }

        let mut result = Vec::new();
        collect_incomplete(&self.goal_stack, &mut result);
        result
    }

    /// Get unresolved errors
    pub fn unresolved_errors(&self) -> Vec<&ErrorEntry> {
        self.errors.iter().filter(|e| !e.resolved).collect()
    }

    /// Check if there have been significant changes since a given time
    pub fn has_changes_since(&self, since: DateTime<Utc>) -> bool {
        self.updated_at > since
    }
}

/// A goal the agent is working towards
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    /// Unique identifier
    pub id: Uuid,

    /// Description of what needs to be accomplished
    pub description: String,

    /// Where this goal came from
    pub source: GoalSource,

    /// When this goal was created
    pub created_at: DateTime<Utc>,

    /// Whether the goal has been completed
    pub completed: bool,

    /// When the goal was completed (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    /// Sub-goals nested under this goal
    pub sub_goals: Vec<Goal>,
}

impl Goal {
    /// Create a new goal
    pub fn new(description: String, source: GoalSource) -> Self {
        Self {
            id: Uuid::new_v4(),
            description,
            source,
            created_at: Utc::now(),
            completed: false,
            completed_at: None,
            sub_goals: Vec::new(),
        }
    }

    /// Create a goal with a specific ID (for testing/deserialization)
    pub fn with_id(id: Uuid, description: String, source: GoalSource) -> Self {
        Self {
            id,
            description,
            source,
            created_at: Utc::now(),
            completed: false,
            completed_at: None,
            sub_goals: Vec::new(),
        }
    }

    /// Add a sub-goal
    pub fn add_sub_goal(&mut self, description: String, source: GoalSource) {
        self.sub_goals.push(Goal::new(description, source));
    }

    /// Mark as completed
    pub fn complete(&mut self) {
        self.completed = true;
        self.completed_at = Some(Utc::now());
    }

    /// Get progress as (completed, total) count of sub-goals
    pub fn sub_goal_progress(&self) -> (usize, usize) {
        let completed = self.sub_goals.iter().filter(|g| g.completed).count();
        (completed, self.sub_goals.len())
    }
}

/// Source of a goal
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalSource {
    /// From the initial user prompt
    InitialPrompt,
    /// User clarified or added this goal
    UserClarification,
    /// Agent inferred this goal from context
    Inferred,
}

/// A decision made by the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    /// Unique identifier
    pub id: Uuid,

    /// When the decision was made
    pub timestamp: DateTime<Utc>,

    /// What was chosen
    pub choice: String,

    /// Why this choice was made
    pub rationale: String,

    /// Alternatives that were considered but rejected
    pub alternatives_rejected: Vec<String>,

    /// ID of the event that triggered this decision
    pub triggering_event_id: Uuid,
}

impl Decision {
    /// Create a new decision
    pub fn new(
        choice: String,
        rationale: String,
        alternatives: Vec<String>,
        event_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            choice,
            rationale,
            alternatives_rejected: alternatives,
            triggering_event_id: event_id,
        }
    }
}

/// Understanding of a file's role in the session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContext {
    /// Path to the file
    pub path: PathBuf,

    /// What the agent understands about this file
    pub summary: String,

    /// When the file was last read
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_read_at: Option<DateTime<Utc>>,

    /// When the file was last modified by the agent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified_at: Option<DateTime<Utc>>,

    /// Why this file matters to the current goal
    pub relevance: String,
}

impl FileContext {
    /// Create a new file context
    pub fn new(path: PathBuf, summary: String, relevance: String) -> Self {
        Self {
            path,
            summary,
            last_read_at: None,
            last_modified_at: None,
            relevance,
        }
    }

    /// Record that the file was read
    pub fn mark_read(&mut self) {
        self.last_read_at = Some(Utc::now());
    }

    /// Record that the file was modified
    pub fn mark_modified(&mut self) {
        self.last_modified_at = Some(Utc::now());
    }

    /// Update the summary
    pub fn update_summary(&mut self, summary: String) {
        self.summary = summary;
    }
}

/// An error encountered during the session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEntry {
    /// Unique identifier
    pub id: Uuid,

    /// When the error occurred
    pub timestamp: DateTime<Utc>,

    /// The error message
    pub error: String,

    /// What was being attempted when the error occurred
    pub context: String,

    /// How the error was resolved (if resolved)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,

    /// Whether the error has been resolved
    pub resolved: bool,

    /// When the error was resolved
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
}

impl ErrorEntry {
    /// Create a new error entry
    pub fn new(error: String, context: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            error,
            context,
            resolution: None,
            resolved: false,
            resolved_at: None,
        }
    }

    /// Mark the error as resolved
    pub fn resolve(&mut self, resolution: String) {
        self.resolution = Some(resolution);
        self.resolved = true;
        self.resolved_at = Some(Utc::now());
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
    fn test_session_state_creation() {
        let session_id = Uuid::new_v4();
        let state = SessionState::new(session_id);

        assert_eq!(state.session_id, session_id);
        assert!(state.goal_stack.is_empty());
        assert!(state.narrative.is_empty());
        assert!(state.decisions.is_empty());
    }

    #[test]
    fn test_session_state_with_initial_goal() {
        let session_id = Uuid::new_v4();
        let state = SessionState::with_initial_goal(session_id, "Add authentication");

        assert_eq!(state.goal_stack.len(), 1);
        assert_eq!(state.goal_stack[0].description, "Add authentication");
        assert_eq!(state.goal_stack[0].source, GoalSource::InitialPrompt);
        assert!(state.narrative.contains("Add authentication"));
    }

    #[test]
    fn test_goal_lifecycle() {
        let session_id = Uuid::new_v4();
        let mut state = SessionState::with_initial_goal(session_id, "Main task");

        // Add sub-goals
        state.add_sub_goal("Step 1".to_string(), GoalSource::Inferred);
        state.add_sub_goal("Step 2".to_string(), GoalSource::Inferred);

        assert_eq!(state.goal_stack[0].sub_goals.len(), 2);

        // Complete a sub-goal
        let sub_goal_id = state.goal_stack[0].sub_goals[0].id;
        state.complete_goal(sub_goal_id);

        assert!(state.goal_stack[0].sub_goals[0].completed);
        assert!(!state.goal_stack[0].sub_goals[1].completed);
    }

    #[test]
    fn test_incomplete_goals() {
        let session_id = Uuid::new_v4();
        let mut state = SessionState::with_initial_goal(session_id, "Main task");
        state.add_sub_goal("Step 1".to_string(), GoalSource::Inferred);
        state.add_sub_goal("Step 2".to_string(), GoalSource::Inferred);

        // Complete one sub-goal
        let sub_goal_id = state.goal_stack[0].sub_goals[0].id;
        state.complete_goal(sub_goal_id);

        let incomplete = state.incomplete_goals();
        // Main task + Step 2 (Step 1 is completed)
        assert_eq!(incomplete.len(), 2);
    }

    #[test]
    fn test_decision_recording() {
        let session_id = Uuid::new_v4();
        let mut state = SessionState::new(session_id);

        let decision = Decision::new(
            "Use JWT".to_string(),
            "Stateless auth is simpler".to_string(),
            vec!["Sessions".to_string(), "OAuth only".to_string()],
            Uuid::new_v4(),
        );

        state.record_decision(decision);

        assert_eq!(state.decisions.len(), 1);
        assert_eq!(state.decisions[0].choice, "Use JWT");
    }

    #[test]
    fn test_file_context() {
        let session_id = Uuid::new_v4();
        let mut state = SessionState::new(session_id);

        let path = PathBuf::from("src/auth.rs");
        let mut context = FileContext::new(
            path.clone(),
            "Authentication middleware".to_string(),
            "Core auth logic".to_string(),
        );
        context.mark_read();

        state.update_file_context(path.clone(), context);

        assert!(state.file_contexts.contains_key(&path));
        assert!(state.file_contexts[&path].last_read_at.is_some());
    }

    #[test]
    fn test_error_lifecycle() {
        let session_id = Uuid::new_v4();
        let mut state = SessionState::new(session_id);

        let error = ErrorEntry::new(
            "Compilation error".to_string(),
            "Building the project".to_string(),
        );
        let error_id = error.id;

        state.record_error(error);
        assert_eq!(state.unresolved_errors().len(), 1);

        state.resolve_error(error_id, "Fixed syntax error".to_string());
        assert_eq!(state.unresolved_errors().len(), 0);
        assert!(state.errors[0].resolved);
    }

    #[test]
    fn test_open_questions() {
        let session_id = Uuid::new_v4();
        let mut state = SessionState::new(session_id);

        state.add_open_question("Should we use Redis?".to_string());
        state.add_open_question("Which auth provider?".to_string());

        assert_eq!(state.open_questions.len(), 2);

        // Don't add duplicates
        state.add_open_question("Should we use Redis?".to_string());
        assert_eq!(state.open_questions.len(), 2);

        // Answer a question
        state.answer_question("Should we use Redis?");
        assert_eq!(state.open_questions.len(), 1);
    }

    #[test]
    fn test_goal_sub_goal_progress() {
        let mut goal = Goal::new("Main task".to_string(), GoalSource::InitialPrompt);
        goal.add_sub_goal("Step 1".to_string(), GoalSource::Inferred);
        goal.add_sub_goal("Step 2".to_string(), GoalSource::Inferred);
        goal.add_sub_goal("Step 3".to_string(), GoalSource::Inferred);

        goal.sub_goals[0].complete();
        goal.sub_goals[1].complete();

        let (completed, total) = goal.sub_goal_progress();
        assert_eq!(completed, 2);
        assert_eq!(total, 3);
    }

    #[test]
    fn test_serialization() {
        let session_id = Uuid::new_v4();
        let mut state = SessionState::with_initial_goal(session_id, "Test task");
        state.record_decision(Decision::new(
            "Choice A".to_string(),
            "Reason".to_string(),
            vec![],
            Uuid::new_v4(),
        ));

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: SessionState = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.session_id, session_id);
        assert_eq!(deserialized.decisions.len(), 1);
    }
}

//! Session state data structures for Layer 1.
//!
//! These types represent the interpreted, high-level understanding of a session
//! derived from L0 raw events.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

// ============================================================================
// Enums
// ============================================================================

/// Priority level for goals
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GoalPriority {
    High,
    #[default]
    Medium,
    Low,
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

/// Category of a decision
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DecisionCategory {
    /// Architectural decisions (e.g., choosing patterns, structure)
    Architecture,
    /// Library or dependency choices
    Library,
    /// General approach or methodology
    #[default]
    Approach,
    /// Trade-off decisions between competing concerns
    Tradeoff,
    /// Fallback decisions when primary approach fails
    Fallback,
}

/// Confidence level for a decision
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DecisionConfidence {
    /// Very confident this is the right choice
    High,
    /// Reasonably confident
    #[default]
    Medium,
    /// Somewhat uncertain
    Low,
    /// Significant uncertainty
    Uncertain,
}

/// How well the agent understands a file
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UnderstandingLevel {
    /// Read and understood the entire file
    Full,
    /// Partially read or skimmed
    #[default]
    Partial,
    /// Only aware of the file's existence/path
    Surface,
}

/// Source of an open question
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum QuestionSource {
    /// Extracted from agent reasoning
    #[default]
    FromReasoning,
    /// Asked by the user
    FromUser,
    /// Inferred from an error condition
    InferredFromError,
}

/// Priority of an open question
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum QuestionPriority {
    /// Cannot proceed without an answer
    Blocking,
    /// Should address soon
    #[default]
    Important,
    /// Nice to know but not urgent
    Informational,
}

// ============================================================================
// Supporting Structs
// ============================================================================

/// A timestamped progress note for a goal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressNote {
    /// When this note was added
    pub timestamp: DateTime<Utc>,
    /// The progress note content
    pub note: String,
}

impl ProgressNote {
    /// Create a new progress note
    pub fn new(note: String) -> Self {
        Self {
            timestamp: Utc::now(),
            note,
        }
    }
}

/// An alternative that was considered but rejected
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alternative {
    /// Description of the alternative
    pub description: String,
    /// Why it was rejected
    pub rejection_reason: String,
}

impl Alternative {
    /// Create a new alternative
    pub fn new(description: String, rejection_reason: String) -> Self {
        Self {
            description,
            rejection_reason,
        }
    }

    /// Create from a simple description (for backward compatibility)
    pub fn from_description(description: String) -> Self {
        Self {
            description,
            rejection_reason: String::new(),
        }
    }
}

/// A change made to a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    /// When the change was made
    pub timestamp: DateTime<Utc>,
    /// Summary of what changed
    pub summary: String,
    /// Preview of the diff (first 200 chars)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff_preview: Option<String>,
}

impl FileChange {
    /// Create a new file change record
    pub fn new(summary: String, diff_preview: Option<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            summary,
            diff_preview,
        }
    }
}

/// An open question or ambiguity in the session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenQuestion {
    /// Unique identifier
    pub id: Uuid,
    /// The question itself
    pub question: String,
    /// When the question was identified
    pub created_at: DateTime<Utc>,
    /// Where this question came from
    #[serde(default)]
    pub source: QuestionSource,
    /// Context about what prompted this question
    #[serde(default)]
    pub context: String,
    /// How important is answering this question
    #[serde(default)]
    pub priority: QuestionPriority,
    /// When the question was answered (if answered)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answered_at: Option<DateTime<Utc>>,
    /// The answer (if answered)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

impl OpenQuestion {
    /// Create a new open question
    pub fn new(question: String, source: QuestionSource, context: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            question,
            created_at: Utc::now(),
            source,
            context,
            priority: QuestionPriority::Important,
            answered_at: None,
            answer: None,
        }
    }

    /// Create from just a question string (for backward compatibility)
    pub fn from_string(question: String) -> Self {
        Self::new(question, QuestionSource::FromReasoning, String::new())
    }

    /// Create a blocking question
    pub fn blocking(question: String, context: String) -> Self {
        let mut q = Self::new(question, QuestionSource::FromReasoning, context);
        q.priority = QuestionPriority::Blocking;
        q
    }

    /// Answer the question
    pub fn answer(&mut self, answer: String) {
        self.answer = Some(answer);
        self.answered_at = Some(Utc::now());
    }

    /// Check if the question has been answered
    pub fn is_answered(&self) -> bool {
        self.answer.is_some()
    }
}

// ============================================================================
// Main State Structs
// ============================================================================

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
    pub open_questions: Vec<OpenQuestion>,
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

    /// Add an open question (from string, for backward compatibility)
    pub fn add_open_question(&mut self, question: String) {
        // Check if this question already exists
        if !self.open_questions.iter().any(|q| q.question == question) {
            self.open_questions
                .push(OpenQuestion::from_string(question));
            self.updated_at = Utc::now();
        }
    }

    /// Add an open question with full details
    pub fn add_open_question_full(&mut self, question: OpenQuestion) {
        // Check if this question already exists
        if !self
            .open_questions
            .iter()
            .any(|q| q.question == question.question)
        {
            self.open_questions.push(question);
            self.updated_at = Utc::now();
        }
    }

    /// Remove an open question (answered) - by question text
    pub fn answer_question(&mut self, question: &str) {
        self.open_questions.retain(|q| q.question != question);
        self.updated_at = Utc::now();
    }

    /// Answer a question by ID with an answer string
    pub fn answer_question_by_id(&mut self, question_id: Uuid, answer: String) {
        if let Some(q) = self.open_questions.iter_mut().find(|q| q.id == question_id) {
            q.answer(answer);
            self.updated_at = Utc::now();
        }
    }

    /// Get unanswered questions
    pub fn unanswered_questions(&self) -> Vec<&OpenQuestion> {
        self.open_questions
            .iter()
            .filter(|q| !q.is_answered())
            .collect()
    }

    /// Get blocking questions
    pub fn blocking_questions(&self) -> Vec<&OpenQuestion> {
        self.open_questions
            .iter()
            .filter(|q| q.priority == QuestionPriority::Blocking && !q.is_answered())
            .collect()
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

    // === New fields (with defaults for backward compatibility) ===
    /// Priority of this goal
    #[serde(default)]
    pub priority: GoalPriority,

    /// What's blocking this goal (if anything)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_by: Option<String>,

    /// Timestamped progress notes
    #[serde(default)]
    pub progress_notes: Vec<ProgressNote>,
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
            priority: GoalPriority::Medium,
            blocked_by: None,
            progress_notes: Vec::new(),
        }
    }

    /// Create a new goal with priority
    pub fn with_priority(description: String, source: GoalSource, priority: GoalPriority) -> Self {
        let mut goal = Self::new(description, source);
        goal.priority = priority;
        goal
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
            priority: GoalPriority::Medium,
            blocked_by: None,
            progress_notes: Vec::new(),
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

    /// Add a progress note
    pub fn add_progress_note(&mut self, note: String) {
        self.progress_notes.push(ProgressNote::new(note));
    }

    /// Set what's blocking this goal
    pub fn set_blocked_by(&mut self, blocker: String) {
        self.blocked_by = Some(blocker);
    }

    /// Clear the blocker
    pub fn clear_blocked(&mut self) {
        self.blocked_by = None;
    }

    /// Check if the goal is blocked
    pub fn is_blocked(&self) -> bool {
        self.blocked_by.is_some()
    }
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

    /// Alternatives that were considered but rejected (legacy: Vec<String>)
    /// For new code, prefer using `alternatives` field with full Alternative structs
    #[serde(default)]
    pub alternatives_rejected: Vec<String>,

    /// ID of the event that triggered this decision
    pub triggering_event_id: Uuid,

    // === New fields (with defaults for backward compatibility) ===
    /// Alternatives with rejection reasons
    #[serde(default)]
    pub alternatives: Vec<Alternative>,

    /// Category of this decision
    #[serde(default)]
    pub category: DecisionCategory,

    /// How confident the agent is in this decision
    #[serde(default)]
    pub confidence: DecisionConfidence,

    /// Whether this decision can be easily reversed
    #[serde(default = "default_true")]
    pub reversible: bool,

    /// Files affected by this decision
    #[serde(default)]
    pub related_files: Vec<PathBuf>,
}

/// Default value for reversible field
fn default_true() -> bool {
    true
}

impl Decision {
    /// Create a new decision (backward compatible)
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
            alternatives: Vec::new(),
            category: DecisionCategory::Approach,
            confidence: DecisionConfidence::Medium,
            reversible: true,
            related_files: Vec::new(),
        }
    }

    /// Create a new decision with full details
    pub fn new_full(
        choice: String,
        rationale: String,
        alternatives: Vec<Alternative>,
        event_id: Uuid,
        category: DecisionCategory,
        confidence: DecisionConfidence,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            choice,
            rationale,
            alternatives_rejected: Vec::new(),
            triggering_event_id: event_id,
            alternatives,
            category,
            confidence,
            reversible: true,
            related_files: Vec::new(),
        }
    }

    /// Mark this decision as non-reversible
    pub fn set_irreversible(&mut self) {
        self.reversible = false;
    }

    /// Add a related file
    pub fn add_related_file(&mut self, path: PathBuf) {
        if !self.related_files.contains(&path) {
            self.related_files.push(path);
        }
    }

    /// Get all alternatives (combining legacy and new format)
    pub fn all_alternatives(&self) -> Vec<Alternative> {
        let mut alts = self.alternatives.clone();
        for desc in &self.alternatives_rejected {
            if !alts.iter().any(|a| a.description == *desc) {
                alts.push(Alternative::from_description(desc.clone()));
            }
        }
        alts
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

    // === New fields (with defaults for backward compatibility) ===
    /// How well the agent understands this file
    #[serde(default)]
    pub understanding_level: UnderstandingLevel,

    /// Key exports (functions, types, classes) from this file
    #[serde(default)]
    pub key_exports: Vec<String>,

    /// Other files this file depends on
    #[serde(default)]
    pub dependencies: Vec<PathBuf>,

    /// History of changes made to this file
    #[serde(default)]
    pub change_history: Vec<FileChange>,

    /// Agent's notes about this file
    #[serde(default)]
    pub notes: Vec<String>,
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
            understanding_level: UnderstandingLevel::Surface,
            key_exports: Vec::new(),
            dependencies: Vec::new(),
            change_history: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Create with full understanding (file was fully read)
    pub fn with_full_understanding(path: PathBuf, summary: String, relevance: String) -> Self {
        let mut ctx = Self::new(path, summary, relevance);
        ctx.understanding_level = UnderstandingLevel::Full;
        ctx.mark_read();
        ctx
    }

    /// Record that the file was read
    pub fn mark_read(&mut self) {
        self.last_read_at = Some(Utc::now());
    }

    /// Record that the file was read fully
    pub fn mark_fully_read(&mut self) {
        self.last_read_at = Some(Utc::now());
        self.understanding_level = UnderstandingLevel::Full;
    }

    /// Record that the file was modified
    pub fn mark_modified(&mut self) {
        self.last_modified_at = Some(Utc::now());
    }

    /// Record a modification with details
    pub fn record_modification(&mut self, summary: String, diff_preview: Option<String>) {
        self.mark_modified();
        self.change_history
            .push(FileChange::new(summary, diff_preview));
    }

    /// Update the summary
    pub fn update_summary(&mut self, summary: String) {
        self.summary = summary;
    }

    /// Add a key export
    pub fn add_export(&mut self, export: String) {
        if !self.key_exports.contains(&export) {
            self.key_exports.push(export);
        }
    }

    /// Add a dependency
    pub fn add_dependency(&mut self, dep: PathBuf) {
        if !self.dependencies.contains(&dep) {
            self.dependencies.push(dep);
        }
    }

    /// Add a note
    pub fn add_note(&mut self, note: String) {
        self.notes.push(note);
    }

    /// Get the number of changes made to this file
    pub fn change_count(&self) -> usize {
        self.change_history.len()
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

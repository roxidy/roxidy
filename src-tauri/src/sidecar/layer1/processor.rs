//! Event processor for Layer 1 session state.
//!
//! This module subscribes to L0 events and updates the session state
//! using the sidecar LLM for interpretation.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use parking_lot::RwLock;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::prompt::{
    format_interpretation_prompt, parse_interpreter_response, STATE_INTERPRETER_SYSTEM,
};
use super::state::{Decision, ErrorEntry, FileContext, GoalSource, SessionState};
use super::storage::{snapshot_reasons, Layer1Storage};
use crate::sidecar::events::{EventType, SessionEvent};
use crate::sidecar::synthesis_llm::SynthesisLlm;

/// Configuration for the Layer 1 processor
#[derive(Debug, Clone)]
pub struct Layer1Config {
    /// Whether to use LLM for state interpretation (vs rule-based only)
    pub use_llm: bool,
    /// Maximum events to process before taking a periodic snapshot
    pub snapshot_interval: usize,
    /// Maximum snapshots to keep per session
    pub max_snapshots_per_session: usize,
}

impl Default for Layer1Config {
    fn default() -> Self {
        Self {
            use_llm: true,
            snapshot_interval: 10,
            max_snapshots_per_session: 50,
        }
    }
}

/// Tasks that the Layer 1 processor can handle
pub enum Layer1Task {
    /// Process a new event
    ProcessEvent(SessionEvent),
    /// Take a snapshot of the current state
    TakeSnapshot { session_id: Uuid, reason: String },
    /// Initialize state for a new session
    InitSession {
        session_id: Uuid,
        initial_request: String,
    },
    /// End a session
    EndSession { session_id: Uuid },
    /// Shutdown the processor
    Shutdown,
}

/// Layer 1 processor for maintaining session state
pub struct Layer1Processor {
    /// Current session states (in-memory hot state)
    states: Arc<RwLock<HashMap<Uuid, SessionState>>>,
    /// Storage for state snapshots
    storage: Arc<Layer1Storage>,
    /// LLM for state interpretation
    llm: Arc<dyn SynthesisLlm>,
    /// Configuration
    config: Layer1Config,
    /// Events processed since last snapshot (per session)
    events_since_snapshot: Arc<RwLock<HashMap<Uuid, usize>>>,
}

impl Layer1Processor {
    /// Create a new Layer 1 processor
    pub fn new(
        storage: Arc<Layer1Storage>,
        llm: Arc<dyn SynthesisLlm>,
        config: Layer1Config,
    ) -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            storage,
            llm,
            config,
            events_since_snapshot: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Spawn the processor as a tokio task
    pub fn spawn(
        storage: Arc<Layer1Storage>,
        llm: Arc<dyn SynthesisLlm>,
        config: Layer1Config,
    ) -> mpsc::UnboundedSender<Layer1Task> {
        let (tx, rx) = mpsc::unbounded_channel();
        let processor = Self::new(storage, llm, config);

        tokio::spawn(async move {
            processor.run(rx).await;
        });

        tx
    }

    /// Main processing loop
    async fn run(self, mut rx: mpsc::UnboundedReceiver<Layer1Task>) {
        tracing::info!("[layer1] Processor started");

        while let Some(task) = rx.recv().await {
            match task {
                Layer1Task::ProcessEvent(event) => {
                    self.handle_event(event).await;
                }
                Layer1Task::TakeSnapshot { session_id, reason } => {
                    self.handle_snapshot(session_id, &reason).await;
                }
                Layer1Task::InitSession {
                    session_id,
                    initial_request,
                } => {
                    self.handle_init_session(session_id, &initial_request).await;
                }
                Layer1Task::EndSession { session_id } => {
                    self.handle_end_session(session_id).await;
                }
                Layer1Task::Shutdown => {
                    tracing::info!("[layer1] Processor shutting down");
                    break;
                }
            }
        }
    }

    /// Handle a new event
    pub async fn handle_event(&self, event: SessionEvent) {
        let session_id = event.session_id;

        // Get or create state for this session
        let state = {
            let states = self.states.read();
            states.get(&session_id).cloned()
        };

        let state = match state {
            Some(s) => s,
            None => {
                // Try to recover from storage
                match self.storage.get_latest_state(session_id).await {
                    Ok(Some(s)) => s,
                    Ok(None) => {
                        // Create new state based on event type
                        if let EventType::UserPrompt { intent } = &event.event_type {
                            SessionState::with_initial_goal(session_id, intent)
                        } else {
                            SessionState::new(session_id)
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "[layer1] Failed to recover state for session {}: {}",
                            session_id,
                            e
                        );
                        SessionState::new(session_id)
                    }
                }
            }
        };

        // Process the event
        let (updated_state, changes) = if self.config.use_llm && self.llm.is_available() {
            self.process_with_llm(&state, &event).await
        } else {
            self.process_with_rules(&state, &event)
        };

        if let Some(new_state) = updated_state {
            // Log changes
            if !changes.is_empty() {
                tracing::debug!(
                    "[layer1] Session {} updated: {}",
                    session_id,
                    changes.join(", ")
                );
            }

            // Update in-memory state
            self.states.write().insert(session_id, new_state.clone());

            // Check if we should take a snapshot
            let should_snapshot = self.check_snapshot_trigger(&event, &changes, session_id);
            if should_snapshot {
                let reason = self.determine_snapshot_reason(&event, &changes);
                if let Err(e) = self.storage.save_snapshot(&new_state, &reason).await {
                    tracing::warn!("[layer1] Failed to save snapshot: {}", e);
                }
                self.events_since_snapshot.write().insert(session_id, 0);
            } else {
                // Increment events since last snapshot
                let mut counts = self.events_since_snapshot.write();
                let count = counts.entry(session_id).or_insert(0);
                *count += 1;
            }
        }
    }

    /// Process an event using the LLM for interpretation
    async fn process_with_llm(
        &self,
        state: &SessionState,
        event: &SessionEvent,
    ) -> (Option<SessionState>, Vec<String>) {
        let prompt = format_interpretation_prompt(state, event);

        match self
            .llm
            .generate_chat(STATE_INTERPRETER_SYSTEM, &prompt, 1500)
            .await
        {
            Ok(response) => match parse_interpreter_response(&response) {
                Ok(parsed) => {
                    if parsed.has_changes() {
                        (parsed.updated_state, parsed.changes)
                    } else {
                        (None, vec![])
                    }
                }
                Err(e) => {
                    tracing::warn!("[layer1] Failed to parse LLM response: {}", e);
                    // Fall back to rule-based processing
                    self.process_with_rules(state, event)
                }
            },
            Err(e) => {
                tracing::warn!("[layer1] LLM interpretation failed: {}", e);
                // Fall back to rule-based processing
                self.process_with_rules(state, event)
            }
        }
    }

    /// Process an event using rule-based logic (no LLM)
    fn process_with_rules(
        &self,
        state: &SessionState,
        event: &SessionEvent,
    ) -> (Option<SessionState>, Vec<String>) {
        let mut new_state = state.clone();
        let mut changes = Vec::new();

        match &event.event_type {
            EventType::UserPrompt { intent } => {
                // Add as a new goal or update narrative
                if new_state.goal_stack.is_empty() {
                    new_state.push_goal(intent.clone(), GoalSource::InitialPrompt);
                    changes.push(format!("Added initial goal: {}", truncate(intent, 50)));
                } else {
                    // Could be a clarification or new direction
                    new_state.update_narrative(format!(
                        "{}. User asked: {}",
                        new_state.narrative,
                        truncate(intent, 100)
                    ));
                    changes.push("Updated narrative with user input".to_string());
                }
            }
            EventType::FileEdit { path, summary, .. } => {
                let file_summary = summary.clone().unwrap_or_else(|| "Modified".to_string());
                let mut context = FileContext::new(
                    path.clone(),
                    file_summary.clone(),
                    "Relevant to current task".to_string(),
                );
                context.mark_modified();
                new_state.update_file_context(path.clone(), context);
                changes.push(format!("Updated file context: {}", path.display()));
            }
            EventType::ToolCall {
                tool_name,
                success,
                reasoning,
                ..
            } => {
                // Track file reads
                if let Some(files) = &event.files_accessed {
                    for path in files {
                        let mut context = new_state
                            .file_contexts
                            .get(path)
                            .cloned()
                            .unwrap_or_else(|| {
                                FileContext::new(
                                    path.clone(),
                                    "Accessed by agent".to_string(),
                                    "Relevant to current task".to_string(),
                                )
                            });
                        context.mark_read();
                        if let Some(output) = &event.tool_output {
                            context.update_summary(truncate(output, 200));
                        }
                        new_state.update_file_context(path.clone(), context);
                    }
                    if !files.is_empty() {
                        changes.push(format!("Tracked {} file access(es)", files.len()));
                    }
                }

                // Track tool failures as errors
                if !success {
                    let error = ErrorEntry::new(
                        format!("Tool {} failed", tool_name),
                        reasoning.clone().unwrap_or_default(),
                    );
                    new_state.record_error(error);
                    changes.push(format!("Recorded error: {} failed", tool_name));
                }
            }
            EventType::AgentReasoning {
                content,
                decision_type,
            } => {
                // Extract decisions if present
                if decision_type.is_some() || content.to_lowercase().contains("because") {
                    let decision = Decision::new(
                        truncate(content, 200),
                        "Extracted from reasoning".to_string(),
                        vec![],
                        event.id,
                    );
                    new_state.record_decision(decision);
                    changes.push("Recorded decision from reasoning".to_string());
                }

                // Check for completion signals
                let lower = content.to_lowercase();
                if lower.contains("done")
                    || lower.contains("complete")
                    || lower.contains("finished")
                {
                    if new_state.current_goal().is_some() {
                        new_state.complete_current_goal();
                        changes.push("Marked current goal as complete".to_string());
                    }
                }

                // Check for questions/uncertainties
                if content.contains('?') || lower.contains("should we") || lower.contains("unclear")
                {
                    // Extract question-like phrases (simplified)
                    for sentence in content.split('.') {
                        if sentence.contains('?') {
                            new_state.add_open_question(sentence.trim().to_string());
                            changes.push("Added open question".to_string());
                            break;
                        }
                    }
                }
            }
            EventType::UserFeedback {
                feedback_type,
                comment,
                ..
            } => {
                use crate::sidecar::events::FeedbackType;

                match feedback_type {
                    FeedbackType::Deny => {
                        if let Some(c) = comment {
                            let decision = Decision::new(
                                format!("User denied: {}", truncate(c, 100)),
                                "User feedback".to_string(),
                                vec![],
                                event.id,
                            );
                            new_state.record_decision(decision);
                            changes.push("Recorded user denial".to_string());
                        }
                    }
                    FeedbackType::Approve => {
                        // Could indicate goal progress
                    }
                    FeedbackType::Modify => {
                        if let Some(c) = comment {
                            new_state.push_goal(c.clone(), GoalSource::UserClarification);
                            changes.push("Added goal from user modification".to_string());
                        }
                    }
                    FeedbackType::Annotate => {
                        if let Some(c) = comment {
                            new_state.update_narrative(format!(
                                "{}. User note: {}",
                                new_state.narrative,
                                truncate(c, 100)
                            ));
                            changes.push("Added user annotation to narrative".to_string());
                        }
                    }
                }
            }
            EventType::ErrorRecovery {
                error_message,
                recovery_action,
                resolved,
            } => {
                // Find or create error entry
                let existing_error = new_state
                    .errors
                    .iter()
                    .position(|e| e.error.contains(&truncate(error_message, 50)));

                if let Some(idx) = existing_error {
                    if *resolved {
                        if let Some(action) = recovery_action {
                            new_state.errors[idx].resolve(action.clone());
                            changes.push("Marked error as resolved".to_string());
                        }
                    }
                } else if !resolved {
                    let error = ErrorEntry::new(
                        error_message.clone(),
                        recovery_action.clone().unwrap_or_default(),
                    );
                    new_state.record_error(error);
                    changes.push("Recorded new error".to_string());
                }
            }
            EventType::AiResponse { content, .. } => {
                // Update narrative with response summary
                if content.len() > 50 {
                    // Only update for substantial responses
                    new_state.update_narrative(truncate(content, 200));
                    changes.push("Updated narrative from AI response".to_string());
                }
            }
            EventType::SessionEnd { summary } => {
                // Mark all goals as completed or abandoned
                if let Some(s) = summary {
                    new_state.update_narrative(s.clone());
                }
                changes.push("Session ended".to_string());
            }
            _ => {}
        }

        // Update timestamp
        if !changes.is_empty() {
            new_state.updated_at = Utc::now();
            (Some(new_state), changes)
        } else {
            (None, vec![])
        }
    }

    /// Check if we should take a snapshot based on the event and changes
    fn check_snapshot_trigger(
        &self,
        event: &SessionEvent,
        changes: &[String],
        session_id: Uuid,
    ) -> bool {
        // Always snapshot for significant events
        let is_significant = matches!(
            event.event_type,
            EventType::UserPrompt { .. }
                | EventType::SessionEnd { .. }
                | EventType::CommitBoundary { .. }
        );

        if is_significant {
            return true;
        }

        // Snapshot if we added/completed a goal
        let goal_change = changes
            .iter()
            .any(|c| c.contains("goal") || c.contains("complete"));

        if goal_change {
            return true;
        }

        // Snapshot if we recorded a decision
        let decision_change = changes.iter().any(|c| c.contains("decision"));
        if decision_change {
            return true;
        }

        // Periodic snapshot based on event count
        let events_count = self
            .events_since_snapshot
            .read()
            .get(&session_id)
            .copied()
            .unwrap_or(0);

        events_count >= self.config.snapshot_interval
    }

    /// Determine the reason for taking a snapshot
    fn determine_snapshot_reason(&self, event: &SessionEvent, changes: &[String]) -> String {
        if matches!(event.event_type, EventType::UserPrompt { .. }) {
            return snapshot_reasons::GOAL_ADDED.to_string();
        }
        if matches!(event.event_type, EventType::SessionEnd { .. }) {
            return snapshot_reasons::SESSION_END.to_string();
        }
        if changes.iter().any(|c| c.contains("complete")) {
            return snapshot_reasons::GOAL_COMPLETED.to_string();
        }
        if changes.iter().any(|c| c.contains("decision")) {
            return snapshot_reasons::DECISION_RECORDED.to_string();
        }
        if changes.iter().any(|c| c.contains("error") && c.contains("resolved")) {
            return snapshot_reasons::ERROR_RESOLVED.to_string();
        }
        if changes.iter().any(|c| c.contains("error")) {
            return snapshot_reasons::ERROR_ADDED.to_string();
        }
        snapshot_reasons::PERIODIC.to_string()
    }

    /// Handle taking a snapshot
    async fn handle_snapshot(&self, session_id: Uuid, reason: &str) {
        let state = self.states.read().get(&session_id).cloned();

        if let Some(state) = state {
            if let Err(e) = self.storage.save_snapshot(&state, reason).await {
                tracing::warn!("[layer1] Failed to save snapshot: {}", e);
            } else {
                tracing::debug!("[layer1] Saved snapshot for session {} ({})", session_id, reason);
            }

            // Cleanup old snapshots
            if let Err(e) = self
                .storage
                .cleanup_old_snapshots(session_id, self.config.max_snapshots_per_session)
                .await
            {
                tracing::warn!("[layer1] Failed to cleanup old snapshots: {}", e);
            }
        }
    }

    /// Handle initializing a new session
    async fn handle_init_session(&self, session_id: Uuid, initial_request: &str) {
        let state = SessionState::with_initial_goal(session_id, initial_request);

        self.states.write().insert(session_id, state.clone());
        self.events_since_snapshot.write().insert(session_id, 0);

        if let Err(e) = self
            .storage
            .save_snapshot(&state, snapshot_reasons::GOAL_ADDED)
            .await
        {
            tracing::warn!("[layer1] Failed to save initial snapshot: {}", e);
        }

        tracing::info!("[layer1] Initialized state for session {}", session_id);
    }

    /// Handle ending a session
    async fn handle_end_session(&self, session_id: Uuid) {
        // Get state first, dropping the guard before await
        let state = {
            let states = self.states.read();
            states.get(&session_id).cloned()
        };

        // Save final snapshot
        if let Some(state) = state {
            if let Err(e) = self
                .storage
                .save_snapshot(&state, snapshot_reasons::SESSION_END)
                .await
            {
                tracing::warn!("[layer1] Failed to save final snapshot: {}", e);
            }
        }

        // Remove from in-memory state
        self.states.write().remove(&session_id);
        self.events_since_snapshot.write().remove(&session_id);

        tracing::info!("[layer1] Ended session {}", session_id);
    }

    /// Get the current state for a session (from memory)
    pub fn get_current_state(&self, session_id: Uuid) -> Option<SessionState> {
        self.states.read().get(&session_id).cloned()
    }

    /// Get all active session IDs
    pub fn active_sessions(&self) -> Vec<Uuid> {
        self.states.read().keys().copied().collect()
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
    use crate::sidecar::events::FileOperation;
    use crate::sidecar::synthesis_llm::TemplateLlm;
    use std::path::PathBuf;
    use tempfile::TempDir;

    async fn setup_processor() -> (TempDir, Layer1Processor) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.lance");
        let connection = lancedb::connect(db_path.to_str().unwrap())
            .execute()
            .await
            .unwrap();

        let storage = Arc::new(Layer1Storage::new(connection).await.unwrap());
        let llm = Arc::new(TemplateLlm) as Arc<dyn SynthesisLlm>;
        let config = Layer1Config {
            use_llm: false, // Use rule-based for tests
            snapshot_interval: 5,
            max_snapshots_per_session: 10,
        };

        let processor = Layer1Processor::new(storage, llm, config);
        (temp_dir, processor)
    }

    #[tokio::test]
    async fn test_process_user_prompt() {
        let (_temp_dir, processor) = setup_processor().await;

        let session_id = Uuid::new_v4();
        let event = SessionEvent::user_prompt(session_id, "Add authentication to the API");

        processor.handle_event(event).await;

        let state = processor.get_current_state(session_id);
        assert!(state.is_some());
        let state = state.unwrap();
        assert_eq!(state.goal_stack.len(), 1);
        assert!(state.goal_stack[0].description.contains("authentication"));
    }

    #[tokio::test]
    async fn test_process_file_edit() {
        let (_temp_dir, processor) = setup_processor().await;

        let session_id = Uuid::new_v4();

        // First, initialize with a prompt
        let prompt = SessionEvent::user_prompt(session_id, "Fix the bug");
        processor.handle_event(prompt).await;

        // Then process a file edit
        let edit = SessionEvent::file_edit(
            session_id,
            PathBuf::from("src/lib.rs"),
            FileOperation::Modify,
            Some("Added fix".to_string()),
        );
        processor.handle_event(edit).await;

        let state = processor.get_current_state(session_id).unwrap();
        assert!(state.file_contexts.contains_key(&PathBuf::from("src/lib.rs")));
    }

    #[tokio::test]
    async fn test_process_reasoning_with_decision() {
        let (_temp_dir, processor) = setup_processor().await;

        let session_id = Uuid::new_v4();

        // Initialize
        let prompt = SessionEvent::user_prompt(session_id, "Add caching");
        processor.handle_event(prompt).await;

        // Add reasoning with decision
        let reasoning = SessionEvent::reasoning(
            session_id,
            "I'll use Redis because it's faster than in-memory caching",
            Some(crate::sidecar::events::DecisionType::ApproachChoice),
        );
        processor.handle_event(reasoning).await;

        let state = processor.get_current_state(session_id).unwrap();
        assert!(!state.decisions.is_empty());
    }

    #[tokio::test]
    async fn test_process_completion_signal() {
        let (_temp_dir, processor) = setup_processor().await;

        let session_id = Uuid::new_v4();

        // Initialize
        let prompt = SessionEvent::user_prompt(session_id, "Add feature");
        processor.handle_event(prompt).await;

        assert!(!processor.get_current_state(session_id).unwrap().goal_stack[0].completed);

        // Add completion signal
        let reasoning = SessionEvent::reasoning(
            session_id,
            "The feature is now complete and working",
            None,
        );
        processor.handle_event(reasoning).await;

        let state = processor.get_current_state(session_id).unwrap();
        assert!(state.goal_stack[0].completed);
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let (_temp_dir, processor) = setup_processor().await;

        let session_id = Uuid::new_v4();

        // Init
        processor
            .handle_init_session(session_id, "Test task")
            .await;
        assert!(processor.get_current_state(session_id).is_some());

        // End
        processor.handle_end_session(session_id).await;
        assert!(processor.get_current_state(session_id).is_none());
    }

    #[tokio::test]
    async fn test_snapshot_trigger() {
        let (_temp_dir, processor) = setup_processor().await;

        let session_id = Uuid::new_v4();

        // User prompt should trigger snapshot
        let event = SessionEvent::user_prompt(session_id, "Test");
        assert!(processor.check_snapshot_trigger(&event, &[], session_id));

        // Regular event shouldn't trigger immediately
        let event = SessionEvent::reasoning(session_id, "Thinking", None);
        assert!(!processor.check_snapshot_trigger(&event, &[], session_id));

        // Goal completion should trigger
        let changes = vec!["Marked goal as complete".to_string()];
        assert!(processor.check_snapshot_trigger(&event, &changes, session_id));
    }
}

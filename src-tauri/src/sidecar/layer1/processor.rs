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

use super::events::Layer1Event;
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

    // === Update frequency controls ===
    /// Events between narrative updates (0 = update on every significant event)
    pub narrative_update_interval: usize,
    /// Maximum file contexts to track per session
    pub max_file_contexts: usize,
    /// Maximum decisions to store per session
    pub max_decisions: usize,
    /// Maximum error entries to store per session
    pub max_errors: usize,
    /// Maximum open questions to track per session
    pub max_open_questions: usize,
}

impl Default for Layer1Config {
    fn default() -> Self {
        Self {
            use_llm: true,
            snapshot_interval: 10,
            max_snapshots_per_session: 50,
            narrative_update_interval: 5,
            max_file_contexts: 100,
            max_decisions: 50,
            max_errors: 30,
            max_open_questions: 20,
        }
    }
}

/// Per-session tracking for update frequency
#[derive(Debug, Default)]
struct SessionUpdateTracker {
    /// Events since last narrative update
    events_since_narrative: usize,
}

/// Tasks that the Layer 1 processor can handle
pub enum Layer1Task {
    /// Process a new event
    ProcessEvent(Box<SessionEvent>),
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
    /// Per-session update tracking
    update_trackers: Arc<RwLock<HashMap<Uuid, SessionUpdateTracker>>>,
    /// Channel to emit Layer1 events to subscribers (e.g., frontend)
    event_tx: Option<mpsc::UnboundedSender<Layer1Event>>,
}

impl Layer1Processor {
    /// Create a new Layer 1 processor
    pub fn new(
        storage: Arc<Layer1Storage>,
        llm: Arc<dyn SynthesisLlm>,
        config: Layer1Config,
        event_tx: Option<mpsc::UnboundedSender<Layer1Event>>,
    ) -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            storage,
            llm,
            config,
            events_since_snapshot: Arc::new(RwLock::new(HashMap::new())),
            update_trackers: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Spawn the processor as a tokio task.
    /// Returns a tuple of (task_sender, event_receiver).
    /// - task_sender: Send Layer1Task messages to control the processor
    /// - event_receiver: Receive Layer1Event updates when state changes
    pub fn spawn(
        storage: Arc<Layer1Storage>,
        llm: Arc<dyn SynthesisLlm>,
        config: Layer1Config,
    ) -> (
        mpsc::UnboundedSender<Layer1Task>,
        mpsc::UnboundedReceiver<Layer1Event>,
    ) {
        let (task_tx, task_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let processor = Self::new(storage, llm, config, Some(event_tx));

        tokio::spawn(async move {
            processor.run(task_rx).await;
        });

        (task_tx, event_rx)
    }

    /// Main processing loop
    async fn run(self, mut rx: mpsc::UnboundedReceiver<Layer1Task>) {
        tracing::info!("[layer1] Processor started (use_llm={}, snapshot_interval={})",
            self.config.use_llm, self.config.snapshot_interval);

        while let Some(task) = rx.recv().await {
            match task {
                Layer1Task::ProcessEvent(event) => {
                    tracing::debug!("[layer1] Received ProcessEvent task for session {}", event.session_id);
                    self.handle_event(*event).await;
                }
                Layer1Task::TakeSnapshot { session_id, reason } => {
                    tracing::debug!("[layer1] Received TakeSnapshot task for session {} (reason: {})", session_id, reason);
                    self.handle_snapshot(session_id, &reason).await;
                }
                Layer1Task::InitSession {
                    session_id,
                    initial_request,
                } => {
                    tracing::debug!("[layer1] Received InitSession task for session {}", session_id);
                    self.handle_init_session(session_id, &initial_request).await;
                }
                Layer1Task::EndSession { session_id } => {
                    tracing::debug!("[layer1] Received EndSession task for session {}", session_id);
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
        let event_type_name = event.event_type.name();

        tracing::debug!(
            "[layer1] Processing event: session={}, type={}, id={}",
            session_id,
            event_type_name,
            event.id
        );

        // Get or create state for this session
        let state = {
            let states = self.states.read();
            states.get(&session_id).cloned()
        };

        let state = match state {
            Some(s) => {
                tracing::debug!(
                    "[layer1] Using existing in-memory state for session {} (goals={}, files={}, decisions={})",
                    session_id,
                    s.goal_stack.len(),
                    s.file_contexts.len(),
                    s.decisions.len()
                );
                s
            }
            None => {
                tracing::debug!("[layer1] No in-memory state for session {}, checking storage...", session_id);
                // Try to recover from storage
                match self.storage.get_latest_state(session_id).await {
                    Ok(Some(s)) => {
                        tracing::debug!(
                            "[layer1] Recovered state from storage for session {} (goals={}, files={}, decisions={})",
                            session_id,
                            s.goal_stack.len(),
                            s.file_contexts.len(),
                            s.decisions.len()
                        );
                        s
                    }
                    Ok(None) => {
                        // Create new state based on event type
                        if let EventType::UserPrompt { intent } = &event.event_type {
                            tracing::debug!(
                                "[layer1] Creating new state with initial goal for session {}: {}",
                                session_id,
                                truncate(intent, 50)
                            );
                            SessionState::with_initial_goal(session_id, intent)
                        } else {
                            tracing::debug!(
                                "[layer1] Creating empty new state for session {}",
                                session_id
                            );
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
        let llm_available = self.llm.is_available();
        tracing::debug!(
            "[layer1] Processing strategy: use_llm={}, llm_available={}, will_use_llm={}",
            self.config.use_llm,
            llm_available,
            self.config.use_llm && llm_available
        );

        let (updated_state, changes) = if self.config.use_llm && llm_available {
            tracing::debug!("[layer1] Using LLM-based processing for event {}", event.id);
            self.process_with_llm(&state, &event).await
        } else {
            tracing::debug!("[layer1] Using rule-based processing for event {}", event.id);
            self.process_with_rules(&state, &event)
        };

        if let Some(mut new_state) = updated_state {
            // Enforce limits on collections
            self.enforce_limits(&mut new_state);

            // Log changes
            if !changes.is_empty() {
                tracing::debug!(
                    "[layer1] Session {} updated with {} change(s): {}",
                    session_id,
                    changes.len(),
                    changes.join(", ")
                );
            } else {
                tracing::debug!(
                    "[layer1] Session {} state updated but no tracked changes",
                    session_id
                );
            }

            // Update in-memory state
            self.states.write().insert(session_id, new_state.clone());
            tracing::debug!(
                "[layer1] Updated in-memory state for session {} (goals={}, files={}, decisions={}, errors={})",
                session_id,
                new_state.goal_stack.len(),
                new_state.file_contexts.len(),
                new_state.decisions.len(),
                new_state.errors.len()
            );

            // Emit events for changes
            let mut events_emitted = 0;
            for change in &changes {
                if let Some(l1_event) = self.change_to_event(session_id, &new_state, change) {
                    tracing::debug!("[layer1] Emitting event for change: {}", change);
                    self.emit_event(l1_event);
                    events_emitted += 1;
                }
            }
            if events_emitted > 0 {
                tracing::debug!("[layer1] Emitted {} Layer1Event(s) for session {}", events_emitted, session_id);
            }

            // Check if we should take a snapshot
            let should_snapshot = self.check_snapshot_trigger(&event, &changes, session_id);
            if should_snapshot {
                let reason = self.determine_snapshot_reason(&event, &changes);
                tracing::debug!("[layer1] Taking snapshot for session {} (reason: {})", session_id, reason);
                if let Err(e) = self.storage.save_snapshot(&new_state, &reason).await {
                    tracing::warn!("[layer1] Failed to save snapshot: {}", e);
                } else {
                    tracing::debug!("[layer1] Snapshot saved successfully for session {}", session_id);
                }
                self.events_since_snapshot.write().insert(session_id, 0);
            } else {
                // Increment events since last snapshot
                let mut counts = self.events_since_snapshot.write();
                let count = counts.entry(session_id).or_insert(0);
                *count += 1;
                tracing::debug!(
                    "[layer1] Events since last snapshot for session {}: {}",
                    session_id,
                    *count
                );
            }
        } else {
            tracing::debug!(
                "[layer1] Event {} did not result in state changes for session {}",
                event.id,
                session_id
            );
        }
    }

    /// Process an event using the LLM for interpretation
    async fn process_with_llm(
        &self,
        state: &SessionState,
        event: &SessionEvent,
    ) -> (Option<SessionState>, Vec<String>) {
        let prompt = format_interpretation_prompt(state, event);
        tracing::debug!(
            "[layer1] LLM prompt generated ({} chars) for event {}",
            prompt.len(),
            event.id
        );

        match self
            .llm
            .generate_chat(STATE_INTERPRETER_SYSTEM, &prompt, 1500)
            .await
        {
            Ok(response) => {
                tracing::debug!(
                    "[layer1] LLM response received ({} chars) for event {}",
                    response.len(),
                    event.id
                );
                match parse_interpreter_response(&response) {
                    Ok(parsed) => {
                        if parsed.has_changes() {
                            tracing::debug!(
                                "[layer1] LLM identified {} change(s) for event {}",
                                parsed.changes.len(),
                                event.id
                            );
                            (parsed.updated_state, parsed.changes)
                        } else {
                            tracing::debug!(
                                "[layer1] LLM determined no changes needed for event {}",
                                event.id
                            );
                            (None, vec![])
                        }
                    }
                    Err(e) => {
                        tracing::warn!("[layer1] Failed to parse LLM response: {}", e);
                        tracing::debug!("[layer1] Falling back to rule-based processing for event {}", event.id);
                        // Fall back to rule-based processing
                        self.process_with_rules(state, event)
                    }
                }
            }
            Err(e) => {
                tracing::warn!("[layer1] LLM interpretation failed: {}", e);
                tracing::debug!("[layer1] Falling back to rule-based processing for event {}", event.id);
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
        let event_type_name = event.event_type.name();

        tracing::debug!(
            "[layer1] Rule-based processing for event type: {}",
            event_type_name
        );

        match &event.event_type {
            EventType::UserPrompt { intent } => {
                tracing::debug!("[layer1] Processing UserPrompt: {}", truncate(intent, 80));
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
                tracing::debug!("[layer1] Processing FileEdit: {}", path.display());
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
                tracing::debug!(
                    "[layer1] Processing ToolCall: tool={}, success={}, files_accessed={}",
                    tool_name,
                    success,
                    event.files_accessed.as_ref().map(|f| f.len()).unwrap_or(0)
                );
                // Track file reads
                if let Some(files) = &event.files_accessed {
                    for path in files {
                        tracing::debug!("[layer1] Tracking file access: {}", path.display());
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
                    tracing::debug!("[layer1] Recording tool failure: {}", tool_name);
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
                tracing::debug!(
                    "[layer1] Processing AgentReasoning: decision_type={:?}, content_len={}",
                    decision_type,
                    content.len()
                );
                // Extract decisions if present
                if decision_type.is_some() || content.to_lowercase().contains("because") {
                    tracing::debug!("[layer1] Extracting decision from reasoning");
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
                if (lower.contains("done")
                    || lower.contains("complete")
                    || lower.contains("finished"))
                    && new_state.current_goal().is_some()
                {
                    tracing::debug!("[layer1] Detected completion signal in reasoning");
                    new_state.complete_current_goal();
                    changes.push("Marked current goal as complete".to_string());
                }

                // Check for questions/uncertainties
                if content.contains('?') || lower.contains("should we") || lower.contains("unclear")
                {
                    tracing::debug!("[layer1] Detected question/uncertainty in reasoning");
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
                tracing::debug!(
                    "[layer1] Processing UserFeedback: type={:?}, has_comment={}",
                    feedback_type,
                    comment.is_some()
                );

                match feedback_type {
                    FeedbackType::Deny => {
                        if let Some(c) = comment {
                            tracing::debug!("[layer1] Recording user denial");
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
                        tracing::debug!("[layer1] User approved (no state change)");
                        // Could indicate goal progress
                    }
                    FeedbackType::Modify => {
                        if let Some(c) = comment {
                            tracing::debug!("[layer1] Adding goal from user modification");
                            new_state.push_goal(c.clone(), GoalSource::UserClarification);
                            changes.push("Added goal from user modification".to_string());
                        }
                    }
                    FeedbackType::Annotate => {
                        if let Some(c) = comment {
                            tracing::debug!("[layer1] Adding user annotation to narrative");
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
                tracing::debug!(
                    "[layer1] Processing ErrorRecovery: resolved={}, error={}",
                    resolved,
                    truncate(error_message, 50)
                );
                // Find or create error entry
                let existing_error = new_state
                    .errors
                    .iter()
                    .position(|e| e.error.contains(&truncate(error_message, 50)));

                if let Some(idx) = existing_error {
                    if *resolved {
                        if let Some(action) = recovery_action {
                            tracing::debug!("[layer1] Marking existing error as resolved");
                            new_state.errors[idx].resolve(action.clone());
                            changes.push("Marked error as resolved".to_string());
                        }
                    }
                } else if !resolved {
                    tracing::debug!("[layer1] Recording new error");
                    let error = ErrorEntry::new(
                        error_message.clone(),
                        recovery_action.clone().unwrap_or_default(),
                    );
                    new_state.record_error(error);
                    changes.push("Recorded new error".to_string());
                }
            }
            EventType::AiResponse { content, .. } => {
                tracing::debug!(
                    "[layer1] Processing AiResponse: content_len={}",
                    content.len()
                );
                // Update narrative with response summary
                if content.len() > 50 {
                    // Only update for substantial responses
                    tracing::debug!("[layer1] Updating narrative from AI response");
                    new_state.update_narrative(truncate(content, 200));
                    changes.push("Updated narrative from AI response".to_string());
                } else {
                    tracing::debug!("[layer1] AI response too short to update narrative ({} chars)", content.len());
                }
            }
            EventType::SessionEnd { summary } => {
                tracing::debug!(
                    "[layer1] Processing SessionEnd: has_summary={}",
                    summary.is_some()
                );
                // Mark all goals as completed or abandoned
                if let Some(s) = summary {
                    new_state.update_narrative(s.clone());
                }
                changes.push("Session ended".to_string());
            }
            _ => {
                tracing::debug!(
                    "[layer1] No specific handler for event type: {}",
                    event_type_name
                );
            }
        }

        // Update timestamp
        if !changes.is_empty() {
            new_state.updated_at = Utc::now();
            tracing::debug!(
                "[layer1] Rule-based processing completed: {} change(s) detected",
                changes.len()
            );
            (Some(new_state), changes)
        } else {
            tracing::debug!("[layer1] Rule-based processing completed: no changes detected");
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
        if changes
            .iter()
            .any(|c| c.contains("error") && c.contains("resolved"))
        {
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
                tracing::debug!(
                    "[layer1] Saved snapshot for session {} ({})",
                    session_id,
                    reason
                );
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
        self.update_trackers
            .write()
            .insert(session_id, SessionUpdateTracker::default());

        // Emit event for the initial goal
        if let Some(goal) = state.goal_stack.first() {
            self.emit_event(Layer1Event::GoalAdded {
                session_id,
                goal: goal.clone(),
            });
        }

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
        self.update_trackers.write().remove(&session_id);

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

    /// Enforce configured limits on state collections
    /// This removes the oldest items when limits are exceeded
    fn enforce_limits(&self, state: &mut SessionState) {
        // Limit file contexts (remove oldest by last_read_at or last_modified_at)
        if state.file_contexts.len() > self.config.max_file_contexts {
            let mut contexts: Vec<_> = state.file_contexts.drain().collect();
            contexts.sort_by(|(_, a), (_, b)| {
                let a_time = a.last_modified_at.or(a.last_read_at);
                let b_time = b.last_modified_at.or(b.last_read_at);
                b_time.cmp(&a_time) // Most recent first
            });
            contexts.truncate(self.config.max_file_contexts);
            state.file_contexts = contexts.into_iter().collect();
        }

        // Limit decisions (keep most recent)
        if state.decisions.len() > self.config.max_decisions {
            let excess = state.decisions.len() - self.config.max_decisions;
            state.decisions.drain(0..excess);
        }

        // Limit errors (keep most recent)
        if state.errors.len() > self.config.max_errors {
            let excess = state.errors.len() - self.config.max_errors;
            state.errors.drain(0..excess);
        }

        // Limit open questions (keep unanswered ones, oldest first)
        if state.open_questions.len() > self.config.max_open_questions {
            // Partition into answered and unanswered
            let (answered, mut unanswered): (Vec<_>, Vec<_>) = state
                .open_questions
                .drain(..)
                .partition(|q| q.is_answered());

            // Keep most recent unanswered questions
            if unanswered.len() > self.config.max_open_questions {
                unanswered.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                unanswered.truncate(self.config.max_open_questions);
            }

            // Add back answered questions only if we have room
            let remaining = self
                .config
                .max_open_questions
                .saturating_sub(unanswered.len());
            let mut answered_to_keep: Vec<_> = answered.into_iter().take(remaining).collect();

            state.open_questions = unanswered;
            state.open_questions.append(&mut answered_to_keep);
        }
    }

    /// Check if narrative should be updated based on event count
    #[allow(dead_code)]
    fn should_update_narrative(&self, session_id: Uuid) -> bool {
        if self.config.narrative_update_interval == 0 {
            return true;
        }

        let mut trackers = self.update_trackers.write();
        let tracker = trackers.entry(session_id).or_default();
        tracker.events_since_narrative += 1;

        if tracker.events_since_narrative >= self.config.narrative_update_interval {
            tracker.events_since_narrative = 0;
            true
        } else {
            false
        }
    }

    /// Reset narrative update counter (call when narrative is updated)
    #[allow(dead_code)]
    fn reset_narrative_counter(&self, session_id: Uuid) {
        let mut trackers = self.update_trackers.write();
        if let Some(tracker) = trackers.get_mut(&session_id) {
            tracker.events_since_narrative = 0;
        }
    }

    /// Emit a Layer1Event to subscribers
    fn emit_event(&self, event: Layer1Event) {
        if let Some(ref tx) = self.event_tx {
            if tx.send(event).is_err() {
                tracing::trace!("[layer1] Event receiver dropped, event not sent");
            }
        }
    }

    /// Convert a change description to a Layer1Event
    fn change_to_event(
        &self,
        session_id: Uuid,
        state: &SessionState,
        change: &str,
    ) -> Option<Layer1Event> {
        let lower = change.to_lowercase();

        if lower.contains("goal") && (lower.contains("added") || lower.contains("initial")) {
            state.goal_stack.last().map(|g| Layer1Event::GoalAdded {
                session_id,
                goal: g.clone(),
            })
        } else if lower.contains("goal") && lower.contains("complete") {
            state
                .goal_stack
                .iter()
                .find(|g| g.completed)
                .map(|g| Layer1Event::GoalCompleted {
                    session_id,
                    goal_id: g.id,
                })
        } else if lower.contains("narrative") {
            Some(Layer1Event::NarrativeUpdated {
                session_id,
                narrative: state.narrative.clone(),
            })
        } else if lower.contains("decision") {
            state
                .decisions
                .last()
                .map(|d| Layer1Event::DecisionRecorded {
                    session_id,
                    decision: d.clone(),
                })
        } else if lower.contains("error") && !lower.contains("resolved") {
            state.errors.last().map(|e| Layer1Event::ErrorUpdated {
                session_id,
                error: e.clone(),
            })
        } else if lower.contains("question") && !lower.contains("answered") {
            state
                .open_questions
                .last()
                .map(|q| Layer1Event::QuestionAdded {
                    session_id,
                    question: q.clone(),
                })
        } else if lower.contains("question") && lower.contains("answered") {
            // Find most recently answered question
            state
                .open_questions
                .iter()
                .filter(|q| q.is_answered())
                .max_by_key(|q| q.answered_at)
                .map(|q| Layer1Event::QuestionAnswered {
                    session_id,
                    question_id: q.id,
                    answer: q.answer.clone().unwrap_or_default(),
                })
        } else if lower.contains("file context") || lower.contains("file access") {
            // Extract path from change message patterns:
            // "Updated file context: {path}"
            // "Tracked N file access(es)"
            if let Some(path_str) = change.strip_prefix("Updated file context: ") {
                let path = std::path::PathBuf::from(path_str);
                state
                    .file_contexts
                    .get(&path)
                    .map(|ctx| Layer1Event::FileContextUpdated {
                        session_id,
                        path,
                        context: ctx.clone(),
                    })
            } else {
                // For batch file access, emit the most recent file context
                state.file_contexts.iter().next().map(|(path, ctx)| {
                    Layer1Event::FileContextUpdated {
                        session_id,
                        path: path.clone(),
                        context: ctx.clone(),
                    }
                })
            }
        } else {
            None
        }
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
            narrative_update_interval: 5,
            max_file_contexts: 100,
            max_decisions: 50,
            max_errors: 30,
            max_open_questions: 20,
        };

        let processor = Layer1Processor::new(storage, llm, config, None);
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
        assert!(state
            .file_contexts
            .contains_key(&PathBuf::from("src/lib.rs")));
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
        let reasoning =
            SessionEvent::reasoning(session_id, "The feature is now complete and working", None);
        processor.handle_event(reasoning).await;

        let state = processor.get_current_state(session_id).unwrap();
        assert!(state.goal_stack[0].completed);
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let (_temp_dir, processor) = setup_processor().await;

        let session_id = Uuid::new_v4();

        // Init
        processor.handle_init_session(session_id, "Test task").await;
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

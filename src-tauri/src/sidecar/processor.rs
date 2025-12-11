//! Event processor for simplified sidecar.
//!
//! Processes events asynchronously, updating:
//! - `state.md` with session context
//! - `patches/staged/` with commit patches (L2)

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

use tokio::sync::mpsc;

use super::commits::{BoundaryReason, PatchManager};
use super::events::{CommitBoundaryDetector, EventType, SessionEvent};
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
    /// Whether to generate staged patches (L2)
    pub generate_patches: bool,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            sessions_dir: super::session::default_sessions_dir(),
            generate_patches: true,
        }
    }
}

/// Tracks file changes for patch generation
#[derive(Debug, Default)]
struct FileChangeTracker {
    /// Files changed since last commit boundary
    files: Vec<PathBuf>,
}

impl FileChangeTracker {
    fn new() -> Self {
        Self { files: Vec::new() }
    }

    fn record_change(&mut self, path: PathBuf) {
        if !self.files.contains(&path) {
            self.files.push(path);
        }
    }

    fn get_files(&self) -> Vec<PathBuf> {
        self.files.clone()
    }

    fn clear(&mut self) {
        self.files.clear();
    }

    fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// State for a single session's processing
struct SessionProcessorState {
    /// Commit boundary detector
    boundary_detector: CommitBoundaryDetector,
    /// File change tracker for patch generation
    file_tracker: FileChangeTracker,
}

impl SessionProcessorState {
    fn new() -> Self {
        Self {
            boundary_detector: CommitBoundaryDetector::new(),
            file_tracker: FileChangeTracker::new(),
        }
    }
}

/// Event processor
pub struct Processor {
    task_tx: mpsc::Sender<ProcessorTask>,
}

impl Processor {
    /// Create a new processor and spawn its background task
    pub fn spawn(config: ProcessorConfig) -> Self {
        let (task_tx, task_rx) = mpsc::channel(256);

        tokio::spawn(async move {
            run_processor(config, task_rx).await;
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

    let mut session_states: HashMap<String, SessionProcessorState> = HashMap::new();

    while let Some(task) = task_rx.recv().await {
        match task {
            ProcessorTask::ProcessEvent { session_id, event } => {
                let session_state = session_states
                    .entry(session_id.clone())
                    .or_insert_with(SessionProcessorState::new);

                if let Err(e) = handle_event(&config, &session_id, &event, session_state).await {
                    tracing::error!("Failed to process event for {}: {}", session_id, e);
                }
            }
            ProcessorTask::EndSession { session_id } => {
                // Generate final patch if there are pending changes
                if let Some(session_state) = session_states.get_mut(&session_id) {
                    if config.generate_patches && !session_state.file_tracker.is_empty() {
                        if let Err(e) = generate_patch(
                            &config,
                            &session_id,
                            session_state,
                            BoundaryReason::SessionEnd,
                        )
                        .await
                        {
                            tracing::error!(
                                "Failed to generate final patch for {}: {}",
                                session_id,
                                e
                            );
                        }
                    }
                }

                if let Err(e) = handle_end_session(&config, &session_id).await {
                    tracing::error!("Failed to end session {}: {}", session_id, e);
                }

                session_states.remove(&session_id);
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
    session_state: &mut SessionProcessorState,
) -> Result<()> {
    // Track file changes for L2 patch generation
    if config.generate_patches {
        track_file_changes(event, session_state);

        // Check for commit boundary
        if let Some(boundary_info) = session_state.boundary_detector.check_boundary(event) {
            let reason = parse_boundary_reason(&boundary_info.reason);
            if !session_state.file_tracker.is_empty() {
                generate_patch(config, session_id, session_state, reason).await?;
            }
        }
    }

    tracing::debug!(
        "Processed event for session {}: {:?}",
        session_id,
        event.event_type.name()
    );
    Ok(())
}

/// Track file changes from an event
fn track_file_changes(event: &SessionEvent, session_state: &mut SessionProcessorState) {
    match &event.event_type {
        EventType::FileEdit { path, .. } => {
            session_state.file_tracker.record_change(path.clone());
        }
        EventType::ToolCall { tool_name, .. } => {
            if is_write_tool(tool_name) {
                for path in &event.files_modified {
                    session_state.file_tracker.record_change(path.clone());
                }
            }
        }
        _ => {}
    }
}

/// Check if a tool is a write tool
fn is_write_tool(tool_name: &str) -> bool {
    matches!(
        tool_name.to_lowercase().as_str(),
        "write" | "write_file" | "edit" | "edit_file" | "create_file" | "delete_file"
    )
}

/// Parse boundary reason from string
fn parse_boundary_reason(reason: &str) -> BoundaryReason {
    let lower = reason.to_lowercase();
    if lower.contains("completion") {
        BoundaryReason::CompletionSignal
    } else if lower.contains("approv") {
        BoundaryReason::UserApproval
    } else if lower.contains("session") || lower.contains("end") {
        BoundaryReason::SessionEnd
    } else if lower.contains("pause") {
        BoundaryReason::ActivityPause
    } else {
        BoundaryReason::CompletionSignal
    }
}

/// Generate a staged patch from tracked file changes
async fn generate_patch(
    config: &ProcessorConfig,
    session_id: &str,
    session_state: &mut SessionProcessorState,
    reason: BoundaryReason,
) -> Result<()> {
    let session = Session::load(&config.sessions_dir, session_id)
        .await
        .context("Failed to load session")?;

    let git_root = session
        .meta()
        .git_root
        .clone()
        .unwrap_or_else(|| session.meta().cwd.clone());

    let manager = PatchManager::new(session.dir().to_path_buf());

    let files = session_state.file_tracker.get_files();
    if files.is_empty() {
        return Ok(());
    }

    // Generate a simple commit message based on files
    let message = generate_simple_message(&files);

    // Create patch
    manager
        .create_patch_from_changes(&git_root, &files, &message, reason)
        .await?;

    // Clear tracked changes
    session_state.file_tracker.clear();
    session_state.boundary_detector.clear();

    Ok(())
}

/// Generate a simple commit message from files
fn generate_simple_message(files: &[PathBuf]) -> String {
    if files.len() == 1 {
        let file = &files[0];
        let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        format!("chore: update {}", name)
    } else {
        format!("chore: update {} files", files.len())
    }
}

/// Handle session end
async fn handle_end_session(config: &ProcessorConfig, session_id: &str) -> Result<()> {
    let mut session = Session::load(&config.sessions_dir, session_id).await?;
    session.complete().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_processor_lifecycle() {
        let temp = TempDir::new().unwrap();
        let config = ProcessorConfig {
            sessions_dir: temp.path().to_path_buf(),
            generate_patches: true,
        };

        let processor = Processor::spawn(config);
        processor.shutdown().await;
    }
}

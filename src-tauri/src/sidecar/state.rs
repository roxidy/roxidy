//! Sidecar state management.
//!
//! This module provides the main `SidecarState` struct that manages:
//! - Active session tracking
//! - Event buffering
//! - Coordination with the async processor

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use tokio::sync::{mpsc, oneshot, Mutex as TokioMutex};
use uuid::Uuid;

use super::config::SidecarConfig;
use super::events::{
    Checkpoint, CommitBoundaryDetector, CommitBoundaryInfo, EventType, SessionEvent, SidecarSession,
};
use super::processor::{ProcessorHandle, ProcessorTask, SidecarProcessor};
use super::storage::SidecarStorage;

/// Status of the sidecar system
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SidecarStatus {
    /// Whether a session is currently active
    pub active_session: bool,
    /// Current session ID if active
    pub session_id: Option<Uuid>,
    /// Number of events in current session
    pub event_count: usize,
    /// Number of events in buffer (not yet persisted)
    pub buffer_size: usize,
    /// Whether embedding models are ready
    pub embeddings_ready: bool,
    /// Whether LLM models are ready
    pub llm_ready: bool,
    /// Whether storage is initialized
    pub storage_ready: bool,
    /// Current workspace path
    pub workspace_path: Option<PathBuf>,
}

/// Main state manager for the sidecar system
pub struct SidecarState {
    /// Current active session (if any)
    session: RwLock<Option<SidecarSession>>,

    /// Event buffer (not yet persisted)
    event_buffer: RwLock<Vec<SessionEvent>>,

    /// Events since last checkpoint
    events_since_checkpoint: RwLock<Vec<SessionEvent>>,

    /// Last checkpoint time
    last_checkpoint_time: RwLock<DateTime<Utc>>,

    /// Vector storage (initialized lazily)
    storage: RwLock<Option<Arc<SidecarStorage>>>,

    /// Workspace path
    workspace_root: RwLock<Option<PathBuf>>,

    /// Channel for async processing tasks
    processor_tx: RwLock<Option<mpsc::UnboundedSender<ProcessorTask>>>,

    /// Shutdown completion signal (for graceful async shutdown)
    shutdown_rx: TokioMutex<Option<oneshot::Receiver<()>>>,

    /// Configuration
    config: RwLock<SidecarConfig>,

    /// Whether embeddings are available
    embeddings_ready: RwLock<bool>,

    /// Whether LLM is available
    llm_ready: RwLock<bool>,

    /// Commit boundary detector
    commit_boundary_detector: RwLock<CommitBoundaryDetector>,
}

impl SidecarState {
    /// Create a new sidecar state with default configuration
    pub fn new() -> Self {
        Self {
            session: RwLock::new(None),
            event_buffer: RwLock::new(Vec::new()),
            events_since_checkpoint: RwLock::new(Vec::new()),
            last_checkpoint_time: RwLock::new(Utc::now()),
            storage: RwLock::new(None),
            workspace_root: RwLock::new(None),
            processor_tx: RwLock::new(None),
            shutdown_rx: TokioMutex::new(None),
            config: RwLock::new(SidecarConfig::default()),
            embeddings_ready: RwLock::new(false),
            llm_ready: RwLock::new(false),
            commit_boundary_detector: RwLock::new(CommitBoundaryDetector::new()),
        }
    }

    /// Create a new sidecar state with custom configuration
    #[allow(dead_code)]
    pub fn with_config(config: SidecarConfig) -> Self {
        Self {
            session: RwLock::new(None),
            event_buffer: RwLock::new(Vec::new()),
            events_since_checkpoint: RwLock::new(Vec::new()),
            last_checkpoint_time: RwLock::new(Utc::now()),
            storage: RwLock::new(None),
            workspace_root: RwLock::new(None),
            processor_tx: RwLock::new(None),
            shutdown_rx: TokioMutex::new(None),
            config: RwLock::new(config),
            embeddings_ready: RwLock::new(false),
            llm_ready: RwLock::new(false),
            commit_boundary_detector: RwLock::new(CommitBoundaryDetector::new()),
        }
    }

    /// Initialize the sidecar for a workspace
    pub async fn initialize(&self, workspace_path: PathBuf) -> anyhow::Result<()> {
        tracing::info!("[sidecar] Initializing for workspace: {:?}", workspace_path);

        // Ensure directories exist
        let config = self.config.read().clone();
        tracing::debug!("[sidecar] Data directory: {:?}", config.data_dir);
        config.ensure_directories()?;
        tracing::debug!("[sidecar] Directories created");

        // Initialize storage
        tracing::debug!("[sidecar] Initializing LanceDB storage...");
        let storage = SidecarStorage::new(&config.data_dir).await?;
        let storage = Arc::new(storage);
        *self.storage.write() = Some(storage.clone());
        tracing::info!("[sidecar] LanceDB storage initialized");

        // Store workspace path
        *self.workspace_root.write() = Some(workspace_path.clone());

        // Check model availability
        let embeddings_available = config.embedding_model_available();
        let llm_available = config.llm_model_available();
        *self.embeddings_ready.write() = embeddings_available;
        *self.llm_ready.write() = llm_available;
        tracing::info!(
            "[sidecar] Model status: embeddings={}, llm={}",
            embeddings_available,
            llm_available
        );

        // Spawn the background processor
        tracing::debug!("[sidecar] Spawning background processor...");
        let handle = SidecarProcessor::spawn(storage, config);
        *self.processor_tx.write() = Some(handle.task_tx);
        *self.shutdown_rx.lock().await = Some(handle.shutdown_complete);

        tracing::info!(
            "[sidecar] Initialized successfully for workspace: {:?}",
            workspace_path
        );
        Ok(())
    }

    /// Get the current status
    pub fn status(&self) -> SidecarStatus {
        let session = self.session.read();
        let buffer = self.event_buffer.read();

        SidecarStatus {
            active_session: session.is_some(),
            session_id: session.as_ref().map(|s| s.id),
            event_count: session.as_ref().map(|s| s.event_count).unwrap_or(0),
            buffer_size: buffer.len(),
            embeddings_ready: *self.embeddings_ready.read(),
            llm_ready: *self.llm_ready.read(),
            storage_ready: self.storage.read().is_some(),
            workspace_path: self.workspace_root.read().clone(),
        }
    }

    /// Start a new capture session
    pub fn start_session(&self, initial_request: &str) -> anyhow::Result<Uuid> {
        let workspace_path = self
            .workspace_root
            .read()
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));

        let session = SidecarSession::new(workspace_path.clone(), initial_request.to_string());
        let session_id = session.id;

        tracing::info!(
            "[sidecar] Starting session {} for workspace {:?}",
            session_id,
            workspace_path
        );
        tracing::debug!(
            "[sidecar] Initial request: {}",
            truncate(initial_request, 100)
        );

        // NOTE: session_start events are no longer emitted.
        // Sessions begin with the first user_prompt event.

        *self.session.write() = Some(session);
        *self.last_checkpoint_time.write() = Utc::now();

        tracing::info!("[sidecar] Session {} started successfully", session_id);
        Ok(session_id)
    }

    /// End the current session
    pub fn end_session(&self) -> anyhow::Result<Option<SidecarSession>> {
        let mut session_guard = self.session.write();

        if let Some(mut session) = session_guard.take() {
            tracing::info!(
                "[sidecar] Ending session {} ({} events, {} files, {} checkpoints)",
                session.id,
                session.event_count,
                session.files_touched.len(),
                session.checkpoint_count
            );

            // Create session end event
            let event = SessionEvent::new(
                session.id,
                EventType::SessionEnd { summary: None },
                "Session ended".to_string(),
            );

            session.end(None);

            // Capture the end event and flush
            drop(session_guard);
            self.capture_internal(event);
            self.request_flush();

            tracing::info!("[sidecar] Session {} ended, buffer flushed", session.id);
            return Ok(Some(session));
        }

        tracing::debug!("[sidecar] No active session to end");
        Ok(None)
    }

    /// Get the current session ID
    pub fn current_session_id(&self) -> Option<Uuid> {
        self.session.read().as_ref().map(|s| s.id)
    }

    /// Check if there's an active session
    #[allow(dead_code)]
    pub fn has_active_session(&self) -> bool {
        self.session.read().is_some()
    }

    /// Capture an event (synchronous, returns immediately)
    pub fn capture(&self, event: SessionEvent) {
        // Validate event has a session
        if self.session.read().is_none() {
            tracing::debug!("Ignoring event - no active session");
            return;
        }

        self.capture_internal(event);
    }

    /// Internal capture logic
    fn capture_internal(&self, event: SessionEvent) {
        let config = self.config.read();

        // Check minimum content length
        if event.content.len() < config.min_content_length {
            tracing::trace!(
                "[sidecar] Skipping event (content too short: {} < {})",
                event.content.len(),
                config.min_content_length
            );
            return;
        }

        tracing::debug!(
            "[sidecar] Capturing event: type={}, content_len={}, session={}",
            event.event_type.name(),
            event.content.len(),
            event.session_id
        );

        // Update session stats
        if let Some(ref mut session) = *self.session.write() {
            session.increment_events();
            for file in &event.files_modified {
                session.touch_file(file.clone());
            }
            tracing::trace!(
                "[sidecar] Session {} now has {} events, {} files",
                session.id,
                session.event_count,
                session.files_touched.len()
            );
        }

        // Add to buffer
        {
            let mut buffer = self.event_buffer.write();
            buffer.push(event.clone());
            let buffer_len = buffer.len();
            tracing::trace!("[sidecar] Buffer size: {}", buffer_len);

            // Check if we should flush
            if buffer_len >= config.buffer_flush_threshold {
                tracing::debug!(
                    "[sidecar] Buffer threshold reached ({} >= {}), requesting flush",
                    buffer_len,
                    config.buffer_flush_threshold
                );
                drop(buffer);
                self.request_flush();
            }
        }

        // Track for checkpoint generation
        {
            let mut checkpoint_events = self.events_since_checkpoint.write();
            checkpoint_events.push(event.clone());
            let checkpoint_count = checkpoint_events.len();

            // Check if we should generate a checkpoint
            if checkpoint_count >= config.checkpoint_event_threshold {
                tracing::debug!(
                    "[sidecar] Checkpoint threshold reached ({} >= {}), requesting checkpoint",
                    checkpoint_count,
                    config.checkpoint_event_threshold
                );
                drop(checkpoint_events);
                self.request_checkpoint();
            }
        }

        // Also check time-based checkpoint
        self.maybe_time_checkpoint();

        // Check for commit boundary
        if let Some(boundary_info) = self.commit_boundary_detector.write().check_boundary(&event) {
            if let Some(session_id) = self.current_session_id() {
                // Create and capture the commit boundary event
                let boundary_event = SessionEvent::commit_boundary(
                    session_id,
                    boundary_info.files_in_scope.clone(),
                    Some(boundary_info.reason.clone()),
                );

                // Add to buffer (don't recurse into full capture_internal)
                let mut buffer = self.event_buffer.write();
                buffer.push(boundary_event);

                tracing::info!(
                    "[sidecar] Commit boundary detected: {} files, reason: {}",
                    boundary_info.files_in_scope.len(),
                    boundary_info.reason
                );
            }
        }
    }

    /// Check if we should generate a time-based checkpoint
    fn maybe_time_checkpoint(&self) {
        let config = self.config.read();
        let last_time = *self.last_checkpoint_time.read();
        let elapsed = Utc::now().signed_duration_since(last_time).num_seconds() as u64;

        if elapsed >= config.checkpoint_time_threshold_secs {
            let events = self.events_since_checkpoint.read();
            if !events.is_empty() {
                drop(events);
                self.request_checkpoint();
            }
        }
    }

    /// Request a buffer flush to storage
    fn request_flush(&self) {
        let events: Vec<SessionEvent> = {
            let mut buffer = self.event_buffer.write();
            std::mem::take(&mut *buffer)
        };

        if events.is_empty() {
            tracing::trace!("[sidecar] Flush requested but buffer empty");
            return;
        }

        tracing::info!("[sidecar] Flushing {} events to storage", events.len());

        if let Some(ref tx) = *self.processor_tx.read() {
            if tx.send(ProcessorTask::FlushEvents(events)).is_err() {
                tracing::warn!("[sidecar] Failed to send flush task to processor");
            }
        } else {
            tracing::warn!("[sidecar] No processor available for flush");
        }
    }

    /// Request checkpoint generation
    fn request_checkpoint(&self) {
        let events: Vec<SessionEvent> = {
            let mut checkpoint_events = self.events_since_checkpoint.write();
            std::mem::take(&mut *checkpoint_events)
        };

        if events.is_empty() {
            tracing::trace!("[sidecar] Checkpoint requested but no events");
            return;
        }

        tracing::info!(
            "[sidecar] Generating checkpoint from {} events",
            events.len()
        );

        // Update last checkpoint time
        *self.last_checkpoint_time.write() = Utc::now();

        // Update session checkpoint count
        if let Some(ref mut session) = *self.session.write() {
            session.increment_checkpoints();
            tracing::debug!(
                "[sidecar] Session {} now has {} checkpoints",
                session.id,
                session.checkpoint_count
            );
        }

        if let Some(ref tx) = *self.processor_tx.read() {
            if tx.send(ProcessorTask::GenerateCheckpoint(events)).is_err() {
                tracing::warn!("[sidecar] Failed to send checkpoint task to processor");
            }
        } else {
            tracing::warn!("[sidecar] No processor available for checkpoint");
        }
    }

    /// Get events for a session from storage
    pub async fn get_session_events(&self, session_id: Uuid) -> anyhow::Result<Vec<SessionEvent>> {
        let storage = self
            .storage
            .read()
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Storage not initialized"))?;

        storage.get_session_events(session_id).await
    }

    /// Get checkpoints for a session from storage
    pub async fn get_session_checkpoints(
        &self,
        session_id: Uuid,
    ) -> anyhow::Result<Vec<Checkpoint>> {
        let storage = self
            .storage
            .read()
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Storage not initialized"))?;

        storage.get_session_checkpoints(session_id).await
    }

    /// Search events semantically
    pub async fn search_events(
        &self,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<SessionEvent>> {
        let storage = self
            .storage
            .read()
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Storage not initialized"))?;

        // For now, do keyword search until we have embeddings
        storage.search_events_keyword(query, limit).await
    }

    /// Get the configuration
    pub fn config(&self) -> SidecarConfig {
        self.config.read().clone()
    }

    /// Update the configuration
    pub fn set_config(&self, config: SidecarConfig) {
        *self.config.write() = config;
    }

    /// Check if embeddings are ready
    #[allow(dead_code)]
    pub fn embeddings_ready(&self) -> bool {
        *self.embeddings_ready.read()
    }

    /// Check if LLM is ready
    pub fn llm_ready(&self) -> bool {
        *self.llm_ready.read()
    }

    /// Set embeddings ready status
    pub fn set_embeddings_ready(&self, ready: bool) {
        *self.embeddings_ready.write() = ready;
    }

    /// Set LLM ready status
    pub fn set_llm_ready(&self, ready: bool) {
        *self.llm_ready.write() = ready;
    }

    /// Get the storage instance
    pub fn storage(&self) -> Option<Arc<SidecarStorage>> {
        self.storage.read().clone()
    }

    /// Get pending files for commit boundary detection
    pub fn pending_commit_files(&self) -> Vec<PathBuf> {
        self.commit_boundary_detector
            .read()
            .pending_files()
            .to_vec()
    }

    /// Clear commit boundary tracking (after manual commit)
    pub fn clear_commit_boundary(&self) {
        self.commit_boundary_detector.write().clear();
    }

    /// Check for commit boundary and return boundary info if detected
    #[allow(dead_code)]
    pub fn check_commit_boundary(&self, event: &SessionEvent) -> Option<CommitBoundaryInfo> {
        self.commit_boundary_detector.write().check_boundary(event)
    }

    /// Shutdown the sidecar synchronously (deprecated, use shutdown_async for graceful shutdown)
    pub fn shutdown(&self) {
        tracing::info!("Shutting down sidecar (sync)");

        // End any active session
        let _ = self.end_session();

        // Send shutdown signal to processor (but don't wait)
        if let Some(tx) = self.processor_tx.write().take() {
            let _ = tx.send(ProcessorTask::Shutdown);
        }

        // Clear state
        *self.storage.write() = None;
        *self.workspace_root.write() = None;
    }

    /// Gracefully shutdown the sidecar, waiting for the processor to complete.
    ///
    /// This ensures all pending events are flushed to storage before returning.
    pub async fn shutdown_async(&self) {
        tracing::info!("Shutting down sidecar (async, graceful)");

        // End any active session (flushes remaining events)
        let _ = self.end_session();

        // Send shutdown signal to processor
        if let Some(tx) = self.processor_tx.write().take() {
            let _ = tx.send(ProcessorTask::Shutdown);
        }

        // Wait for processor to complete (with timeout)
        if let Some(rx) = self.shutdown_rx.lock().await.take() {
            match tokio::time::timeout(std::time::Duration::from_secs(5), rx).await {
                Ok(Ok(())) => {
                    tracing::info!("Sidecar processor shutdown complete");
                }
                Ok(Err(_)) => {
                    tracing::warn!("Sidecar processor shutdown signal dropped");
                }
                Err(_) => {
                    tracing::warn!("Sidecar processor shutdown timed out after 5s");
                }
            }
        }

        // Clear state
        *self.storage.write() = None;
        *self.workspace_root.write() = None;

        tracing::info!("Sidecar shutdown complete");
    }
}

impl Default for SidecarState {
    fn default() -> Self {
        Self::new()
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
    fn test_sidecar_state_creation() {
        let state = SidecarState::new();
        assert!(!state.has_active_session());
        assert!(state.current_session_id().is_none());
    }

    #[test]
    fn test_session_lifecycle() {
        let state = SidecarState::new();

        // Start session
        let session_id = state.start_session("Test request").unwrap();
        assert!(state.has_active_session());
        assert_eq!(state.current_session_id(), Some(session_id));

        // End session
        let session = state.end_session().unwrap();
        assert!(session.is_some());
        assert!(!state.has_active_session());
    }

    #[test]
    fn test_event_capture() {
        let state = SidecarState::new();

        // Start session
        let session_id = state.start_session("Test request").unwrap();

        // Capture some events
        let event = SessionEvent::user_prompt(session_id, "Do something");
        state.capture(event);

        let event = SessionEvent::reasoning(session_id, "I'll try approach A", None);
        state.capture(event);

        // Check buffer
        assert!(state.event_buffer.read().len() >= 2);
    }

    #[test]
    fn test_status() {
        let state = SidecarState::new();

        let status = state.status();
        assert!(!status.active_session);
        assert!(status.session_id.is_none());
        assert_eq!(status.event_count, 0);
    }

    #[test]
    fn test_config() {
        let config = SidecarConfig::default().without_synthesis();
        let state = SidecarState::with_config(config.clone());

        assert!(!state.config().synthesis_enabled);
    }
}

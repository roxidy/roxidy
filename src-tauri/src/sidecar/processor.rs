//! Async background processor for the sidecar system.
//!
//! This module handles:
//! - Batch event flushing to storage
//! - Embedding generation
//! - Checkpoint generation

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use super::config::SidecarConfig;
use super::events::{Checkpoint, SessionEvent};
use super::models::ModelManager;
use super::prompts;
use super::storage::SidecarStorage;

/// Tasks that the processor can handle
#[allow(dead_code)]
pub enum ProcessorTask {
    /// Flush events to storage
    FlushEvents(Vec<SessionEvent>),
    /// Generate embeddings for events
    EmbedEvents(Vec<SessionEvent>),
    /// Generate a checkpoint from events
    GenerateCheckpoint(Vec<SessionEvent>),
    /// Shutdown the processor
    Shutdown,
}

/// Handle returned from spawn() for managing the processor lifecycle
pub struct ProcessorHandle {
    /// Sender for submitting tasks
    pub task_tx: mpsc::UnboundedSender<ProcessorTask>,
    /// Receiver that completes when the processor shuts down
    pub shutdown_complete: oneshot::Receiver<()>,
}

/// Background processor for async sidecar operations
pub struct SidecarProcessor {
    /// Storage instance
    storage: Arc<SidecarStorage>,
    /// Model manager for embeddings (has internal synchronization)
    model_manager: Arc<ModelManager>,
    /// Configuration
    config: SidecarConfig,
    /// Task receiver
    task_rx: mpsc::UnboundedReceiver<ProcessorTask>,
    /// Shutdown completion signal
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl SidecarProcessor {
    /// Spawn the processor as a tokio task.
    ///
    /// Returns a `ProcessorHandle` containing:
    /// - `task_tx`: Sender for submitting tasks
    /// - `shutdown_complete`: Receiver that signals when shutdown is done
    pub fn spawn(storage: Arc<SidecarStorage>, config: SidecarConfig) -> ProcessorHandle {
        let (task_tx, task_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let model_manager = Arc::new(ModelManager::new(config.models_dir.clone()));

        tokio::spawn(async move {
            let mut processor = SidecarProcessor {
                storage,
                model_manager,
                config,
                task_rx,
                shutdown_tx: Some(shutdown_tx),
            };
            processor.run().await;
        });

        ProcessorHandle {
            task_tx,
            shutdown_complete: shutdown_rx,
        }
    }

    /// Main processing loop
    async fn run(&mut self) {
        tracing::info!("Sidecar processor started");

        // Periodic cleanup interval (1 hour)
        let mut cleanup_interval = tokio::time::interval(Duration::from_secs(3600));

        loop {
            tokio::select! {
                Some(task) = self.task_rx.recv() => {
                    match task {
                        ProcessorTask::FlushEvents(events) => {
                            self.handle_flush(events).await;
                        }
                        ProcessorTask::EmbedEvents(events) => {
                            self.handle_embed(events).await;
                        }
                        ProcessorTask::GenerateCheckpoint(events) => {
                            self.handle_checkpoint(events).await;
                        }
                        ProcessorTask::Shutdown => {
                            tracing::info!("Sidecar processor shutting down");
                            break;
                        }
                    }
                }
                _ = cleanup_interval.tick() => {
                    self.handle_cleanup().await;
                }
            }
        }

        // Signal shutdown complete
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        tracing::info!("Sidecar processor shutdown complete");
    }

    /// Handle flushing events to storage
    async fn handle_flush(&self, events: Vec<SessionEvent>) {
        if events.is_empty() {
            return;
        }

        tracing::debug!("Flushing {} events to storage", events.len());

        // Generate embeddings if enabled
        let events_to_save = if self.config.embeddings_enabled {
            self.embed_events(events).await
        } else {
            events
        };

        if let Err(e) = self.storage.save_events(&events_to_save).await {
            tracing::error!("Failed to flush events: {}", e);
        }
    }

    /// Generate embeddings for events that should be embedded (called during flush)
    async fn embed_events(&self, mut events: Vec<SessionEvent>) -> Vec<SessionEvent> {
        // Find indices of events that should be embedded
        let indices: Vec<usize> = events
            .iter()
            .enumerate()
            .filter(|(_, e)| e.should_embed())
            .map(|(i, _)| i)
            .collect();

        if indices.is_empty() {
            tracing::debug!(
                "No embeddable events in batch of {} (skipping tool calls, file edits, etc.)",
                events.len()
            );
            return events;
        }

        // Collect texts to embed (clone to avoid borrow issues)
        let texts: Vec<String> = indices.iter().map(|&i| events[i].content.clone()).collect();
        let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

        tracing::debug!(
            "Generating embeddings for {}/{} events (user_prompt, reasoning, read outputs)",
            indices.len(),
            events.len()
        );

        // Generate embeddings
        match self.model_manager.embed(&text_refs) {
            Ok(embeddings) => {
                // Update only the embeddable events
                for (idx, embedding) in indices.into_iter().zip(embeddings.into_iter()) {
                    events[idx].embedding = Some(embedding);
                }
                tracing::debug!("Successfully generated {} embeddings", texts.len());
            }
            Err(e) => {
                // Log error but continue - events will be saved without embeddings
                tracing::warn!(
                    "Failed to generate embeddings: {}. Events will be saved without embeddings.",
                    e
                );
            }
        }

        events
    }

    /// Handle embedding generation for events
    async fn handle_embed(&self, mut events: Vec<SessionEvent>) {
        if !self.config.embeddings_enabled {
            return;
        }

        if events.is_empty() {
            return;
        }

        tracing::debug!("Generating embeddings for {} events", events.len());

        // Get texts to embed
        let texts: Vec<&str> = events.iter().map(|e| e.content.as_str()).collect();

        // Generate embeddings
        let embeddings = match self.model_manager.embed(&texts) {
            Ok(emb) => emb,
            Err(e) => {
                tracing::error!("Failed to generate embeddings: {}", e);
                return;
            }
        };

        // Update events with embeddings
        for (event, embedding) in events.iter_mut().zip(embeddings.into_iter()) {
            event.embedding = Some(embedding);
        }

        // Save updated events to storage
        if let Err(e) = self.storage.save_events(&events).await {
            tracing::error!("Failed to save embedded events: {}", e);
        }
    }

    /// Handle checkpoint generation
    async fn handle_checkpoint(&self, events: Vec<SessionEvent>) {
        if events.is_empty() {
            return;
        }

        tracing::debug!("Generating checkpoint for {} events", events.len());

        // First, save all events to storage
        if let Err(e) = self.storage.save_events(&events).await {
            tracing::error!("Failed to save events before checkpoint: {}", e);
            return;
        }

        // Generate checkpoint summary
        let summary = if self.config.synthesis_enabled && self.model_manager.llm_available()
        {
            // Try LLM-based summary
            match self.generate_llm_summary(&events).await {
                Ok(summary) => {
                    tracing::debug!("Generated LLM checkpoint summary");
                    summary
                }
                Err(e) => {
                    tracing::warn!("LLM summary failed, using template: {}", e);
                    self.generate_template_summary(&events)
                }
            }
        } else {
            self.generate_template_summary(&events)
        };

        // Extract metadata
        let session_id = events
            .first()
            .map(|e| e.session_id)
            .unwrap_or_else(uuid::Uuid::new_v4);

        let event_ids: Vec<_> = events.iter().map(|e| e.id).collect();

        let files_touched: Vec<_> = events
            .iter()
            .flat_map(|e| e.files_modified.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let checkpoint = Checkpoint::new(session_id, summary, event_ids, files_touched);

        if let Err(e) = self.storage.save_checkpoint(&checkpoint).await {
            tracing::error!("Failed to save checkpoint: {}", e);
        } else {
            tracing::info!("Generated checkpoint: {}", checkpoint.id);
        }
    }

    /// Generate an LLM-based summary
    async fn generate_llm_summary(&self, events: &[SessionEvent]) -> anyhow::Result<String> {
        // Format events for the LLM prompt
        let events_context = events
            .iter()
            .filter(|e| e.event_type.is_high_signal())
            .take(15) // Limit to avoid token overflow
            .map(|e| format!("- {}: {}", e.event_type.name(), truncate(&e.content, 100)))
            .collect::<Vec<_>>()
            .join("\n");

        if events_context.is_empty() {
            return Ok(self.generate_template_summary(events));
        }

        let prompt = prompts::checkpoint_summary(&events_context);

        // Generate with LLM
        let summary = self.model_manager.generate(&prompt, 200).await?;

        // Clean up the response
        let summary = summary.trim();

        // Validate the summary is reasonable
        if summary.len() < 10 || summary.len() > 500 {
            anyhow::bail!("LLM summary too short or too long");
        }

        Ok(summary.to_string())
    }

    /// Generate a template-based summary (no LLM required)
    fn generate_template_summary(&self, events: &[SessionEvent]) -> String {
        let mut summary_parts = Vec::new();

        // Count event types
        let mut file_edits = 0;
        let mut tool_calls = 0;
        let mut reasoning_events = 0;
        let mut files = std::collections::HashSet::new();

        for event in events {
            match &event.event_type {
                super::events::EventType::FileEdit { path, .. } => {
                    file_edits += 1;
                    files.insert(path.display().to_string());
                }
                super::events::EventType::ToolCall { .. } => {
                    tool_calls += 1;
                }
                super::events::EventType::AgentReasoning { .. } => {
                    reasoning_events += 1;
                }
                _ => {}
            }
        }

        // Build summary
        if file_edits > 0 {
            summary_parts.push(format!("Modified {} file(s)", file_edits));
        }
        if tool_calls > 0 {
            summary_parts.push(format!("{} tool call(s)", tool_calls));
        }
        if reasoning_events > 0 {
            summary_parts.push(format!("{} decision(s)", reasoning_events));
        }

        let mut summary = if summary_parts.is_empty() {
            format!("{} event(s)", events.len())
        } else {
            summary_parts.join(", ")
        };

        // Add file list if not too long
        if !files.is_empty() && files.len() <= 5 {
            let file_list: Vec<_> = files.into_iter().collect();
            summary.push_str(&format!(". Files: {}", file_list.join(", ")));
        }

        summary
    }

    /// Handle periodic cleanup
    async fn handle_cleanup(&self) {
        if self.config.retention_days == 0 {
            return;
        }

        tracing::debug!("Running periodic cleanup");

        match self
            .storage
            .cleanup_old_events(self.config.retention_days)
            .await
        {
            Ok(count) => {
                if count > 0 {
                    tracing::info!("Cleaned up {} old events", count);
                }
            }
            Err(e) => {
                tracing::error!("Cleanup failed: {}", e);
            }
        }
    }
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
    use crate::sidecar::events::FileOperation;
    use lancedb::query::ExecutableQuery;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use uuid::Uuid;

    async fn setup_processor() -> (
        TempDir,
        Arc<SidecarStorage>,
        mpsc::UnboundedSender<ProcessorTask>,
    ) {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(SidecarStorage::new(temp_dir.path()).await.unwrap());
        let config = SidecarConfig::test_config(temp_dir.path());

        let handle = SidecarProcessor::spawn(storage.clone(), config);

        (temp_dir, storage, handle.task_tx)
    }

    #[tokio::test]
    async fn test_flush_events() {
        let (_temp_dir, storage, tx) = setup_processor().await;

        let session_id = Uuid::new_v4();
        let event = SessionEvent::user_prompt(session_id, "Test prompt");

        tx.send(ProcessorTask::FlushEvents(vec![event.clone()]))
            .unwrap();

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        let events = storage.get_session_events(session_id).await.unwrap();
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn test_generate_checkpoint() {
        let (_temp_dir, storage, tx) = setup_processor().await;

        let session_id = Uuid::new_v4();
        let events = vec![
            SessionEvent::user_prompt(session_id, "Add feature"),
            SessionEvent::file_edit(
                session_id,
                PathBuf::from("/src/lib.rs"),
                FileOperation::Modify,
                None,
            ),
            SessionEvent::reasoning(session_id, "Using approach A", None),
        ];

        tx.send(ProcessorTask::GenerateCheckpoint(events)).unwrap();

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        let checkpoints = storage.get_session_checkpoints(session_id).await.unwrap();
        assert_eq!(checkpoints.len(), 1);
        assert!(checkpoints[0].summary.contains("Modified"));
    }

    #[tokio::test]
    async fn test_shutdown() {
        let (_temp_dir, _storage, tx) = setup_processor().await;

        tx.send(ProcessorTask::Shutdown).unwrap();

        // Should complete without hanging
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    /// TDD: This test exposes the event loss issue
    /// Multiple rapid flushes should all persist
    #[tokio::test]
    async fn test_multiple_rapid_flushes_all_persist() {
        let (_temp_dir, storage, tx) = setup_processor().await;

        let session_id = Uuid::new_v4();

        // Send 5 rapid flush requests with 10 events each = 50 events total
        for batch in 0..5 {
            let events: Vec<SessionEvent> = (0..10)
                .map(|i| {
                    SessionEvent::user_prompt(session_id, &format!("Batch {} Event {}", batch, i))
                })
                .collect();
            tx.send(ProcessorTask::FlushEvents(events)).unwrap();
        }

        // Wait for processing - the current sleep-based approach is racy
        tokio::time::sleep(Duration::from_millis(500)).await;

        // ALL 50 events should be persisted
        let events = storage.get_session_events(session_id).await.unwrap();
        assert_eq!(
            events.len(),
            50,
            "All 50 events should be persisted, got {}",
            events.len()
        );
    }

    /// Test that SidecarStorage correctly persists multiple batches of events
    #[tokio::test]
    async fn test_storage_multiple_batches() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session_id = Uuid::new_v4();

        // Write 5 batches of 10 events each
        for batch in 0..5 {
            let events: Vec<SessionEvent> = (0..10)
                .map(|i| {
                    SessionEvent::user_prompt(session_id, &format!("Batch {} Event {}", batch, i))
                })
                .collect();
            storage.save_events(&events).await.unwrap();

            // Verify we can read back all events
            let retrieved = storage.get_session_events(session_id).await.unwrap();
            let expected = (batch + 1) * 10;
            assert_eq!(
                retrieved.len(),
                expected,
                "After batch {}: expected {} events, got {}",
                batch,
                expected,
                retrieved.len()
            );
        }
    }
}

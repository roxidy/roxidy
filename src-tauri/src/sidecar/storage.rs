//! Storage layer for the sidecar system using LanceDB.
//!
//! This module provides persistent vector storage for events, checkpoints, and sessions
//! using LanceDB for efficient similarity search and retrieval.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use arrow_array::{
    types::Float32Type, Array, ArrayRef, FixedSizeListArray, Float32Array, Int64Array, RecordBatch,
    RecordBatchIterator, StringArray, UInt64Array,
};
use arrow_schema::{DataType, Field, Schema};
use chrono::{TimeZone, Utc};
use futures::TryStreamExt;
use lancedb::index::Index;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, Table};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::events::{Checkpoint, SessionEvent, SidecarSession};

/// Embedding dimension for AllMiniLM-L6-v2
pub const EMBEDDING_DIM: i32 = 384;

/// Default high limit for queries (LanceDB defaults to 10 which is too low)
const QUERY_LIMIT: usize = 1_000_000;

/// Storage for sidecar data using LanceDB
pub struct SidecarStorage {
    /// LanceDB connection
    connection: Connection,
    /// Base directory for storage
    data_dir: PathBuf,
    /// Events table
    events_table: Option<Table>,
    /// Checkpoints table
    checkpoints_table: Option<Table>,
    /// Sessions table
    sessions_table: Option<Table>,
}

impl SidecarStorage {
    /// Create a new storage instance
    pub async fn new(data_dir: &Path) -> Result<Self> {
        // Ensure data directory exists
        tokio::fs::create_dir_all(data_dir).await?;

        let db_path = data_dir.join("sidecar.lance");
        let connection = lancedb::connect(db_path.to_str().unwrap())
            .execute()
            .await
            .context("Failed to connect to LanceDB")?;

        let mut storage = Self {
            connection,
            data_dir: data_dir.to_path_buf(),
            events_table: None,
            checkpoints_table: None,
            sessions_table: None,
        };

        // Initialize tables
        storage.ensure_tables().await?;

        Ok(storage)
    }

    /// Ensure all tables exist
    async fn ensure_tables(&mut self) -> Result<()> {
        self.events_table = Some(self.ensure_events_table().await?);
        self.checkpoints_table = Some(self.ensure_checkpoints_table().await?);
        self.sessions_table = Some(self.ensure_sessions_table().await?);
        Ok(())
    }

    /// Get or create the events table
    async fn ensure_events_table(&self) -> Result<Table> {
        let table_name = "events";

        // Try to open existing table
        if let Ok(table) = self.connection.open_table(table_name).execute().await {
            return Ok(table);
        }

        // Create new table with schema
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("timestamp_ms", DataType::Int64, false),
            Field::new("event_type", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            Field::new("cwd", DataType::Utf8, true),
            Field::new("tool_output", DataType::Utf8, true),
            Field::new("files_accessed", DataType::Utf8, true),
            Field::new("files_modified", DataType::Utf8, true),
            Field::new("diff", DataType::Utf8, true),
            Field::new("event_data_json", DataType::Utf8, false),
            Field::new(
                "embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                true,
            ),
        ]));

        // Create empty initial batch
        let batches = RecordBatchIterator::new(vec![].into_iter().map(Ok), schema.clone());

        self.connection
            .create_table(table_name, Box::new(batches))
            .execute()
            .await
            .context("Failed to create events table")
    }

    /// Get or create the checkpoints table
    async fn ensure_checkpoints_table(&self) -> Result<Table> {
        let table_name = "checkpoints";

        if let Ok(table) = self.connection.open_table(table_name).execute().await {
            return Ok(table);
        }

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("timestamp_ms", DataType::Int64, false),
            Field::new("summary", DataType::Utf8, false),
            Field::new("event_ids_json", DataType::Utf8, false),
            Field::new("files_touched_json", DataType::Utf8, true),
            Field::new(
                "embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                true,
            ),
        ]));

        let batches = RecordBatchIterator::new(vec![].into_iter().map(Ok), schema.clone());

        self.connection
            .create_table(table_name, Box::new(batches))
            .execute()
            .await
            .context("Failed to create checkpoints table")
    }

    /// Get or create the sessions table
    async fn ensure_sessions_table(&self) -> Result<Table> {
        let table_name = "sessions";

        if let Ok(table) = self.connection.open_table(table_name).execute().await {
            return Ok(table);
        }

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("started_at_ms", DataType::Int64, false),
            Field::new("ended_at_ms", DataType::Int64, true),
            Field::new("initial_request", DataType::Utf8, false),
            Field::new("workspace_path", DataType::Utf8, false),
            Field::new("event_count", DataType::UInt64, false),
            Field::new("checkpoint_count", DataType::UInt64, false),
            Field::new("files_touched_json", DataType::Utf8, true),
            Field::new("final_summary", DataType::Utf8, true),
        ]));

        let batches = RecordBatchIterator::new(vec![].into_iter().map(Ok), schema.clone());

        self.connection
            .create_table(table_name, Box::new(batches))
            .execute()
            .await
            .context("Failed to create sessions table")
    }

    /// Save events to storage
    pub async fn save_events(&self, events: &[SessionEvent]) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        let table = self
            .events_table
            .as_ref()
            .context("Events table not initialized")?;

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("timestamp_ms", DataType::Int64, false),
            Field::new("event_type", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            Field::new("cwd", DataType::Utf8, true),
            Field::new("tool_output", DataType::Utf8, true),
            Field::new("files_accessed", DataType::Utf8, true),
            Field::new("files_modified", DataType::Utf8, true),
            Field::new("diff", DataType::Utf8, true),
            Field::new("event_data_json", DataType::Utf8, false),
            Field::new(
                "embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                true,
            ),
        ]));

        let ids: Vec<String> = events.iter().map(|e| e.id.to_string()).collect();
        let session_ids: Vec<String> = events.iter().map(|e| e.session_id.to_string()).collect();
        let timestamps: Vec<i64> = events
            .iter()
            .map(|e| e.timestamp.timestamp_millis())
            .collect();
        let event_types: Vec<String> = events
            .iter()
            .map(|e| e.event_type.name().to_string())
            .collect();
        let contents: Vec<String> = events.iter().map(|e| e.content.clone()).collect();
        let cwds: Vec<Option<String>> = events.iter().map(|e| e.cwd.clone()).collect();
        let tool_outputs: Vec<Option<String>> =
            events.iter().map(|e| e.tool_output.clone()).collect();
        let files_accessed: Vec<Option<String>> = events
            .iter()
            .map(|e| {
                e.files_accessed
                    .as_ref()
                    .map(|f| serde_json::to_string(f).unwrap_or_default())
            })
            .collect();
        let files_modified: Vec<Option<String>> = events
            .iter()
            .map(|e| Some(serde_json::to_string(&e.files_modified).unwrap_or_default()))
            .collect();
        let diffs: Vec<Option<String>> = events.iter().map(|e| e.diff.clone()).collect();
        let event_data_json: Vec<String> = events
            .iter()
            .map(|e| serde_json::to_string(&e.event_type).unwrap_or_default())
            .collect();

        // Build embeddings array
        let embeddings =
            self.build_embeddings_array(events.iter().map(|e| e.embedding.as_ref()).collect());

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(ids)) as ArrayRef,
                Arc::new(StringArray::from(session_ids)) as ArrayRef,
                Arc::new(Int64Array::from(timestamps)) as ArrayRef,
                Arc::new(StringArray::from(event_types)) as ArrayRef,
                Arc::new(StringArray::from(contents)) as ArrayRef,
                Arc::new(StringArray::from(cwds)) as ArrayRef,
                Arc::new(StringArray::from(tool_outputs)) as ArrayRef,
                Arc::new(StringArray::from(files_accessed)) as ArrayRef,
                Arc::new(StringArray::from(files_modified)) as ArrayRef,
                Arc::new(StringArray::from(diffs)) as ArrayRef,
                Arc::new(StringArray::from(event_data_json)) as ArrayRef,
                embeddings,
            ],
        )?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
        table.add(Box::new(batches)).execute().await?;

        tracing::debug!("Saved {} events to LanceDB", events.len());
        Ok(())
    }

    /// Build a FixedSizeListArray from optional embeddings
    fn build_embeddings_array(&self, embeddings: Vec<Option<&Vec<f32>>>) -> ArrayRef {
        // For events without embeddings, we use zeros as placeholder
        let iter = embeddings
            .iter()
            .map(|opt_emb| opt_emb.map(|emb| emb.iter().copied().map(Some).collect::<Vec<_>>()));

        let list_array =
            FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(iter, EMBEDDING_DIM);

        Arc::new(list_array)
    }

    /// Save a checkpoint to storage
    pub async fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
        let table = self
            .checkpoints_table
            .as_ref()
            .context("Checkpoints table not initialized")?;

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("timestamp_ms", DataType::Int64, false),
            Field::new("summary", DataType::Utf8, false),
            Field::new("event_ids_json", DataType::Utf8, false),
            Field::new("files_touched_json", DataType::Utf8, true),
            Field::new(
                "embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                true,
            ),
        ]));

        let embeddings = self.build_embeddings_array(vec![checkpoint.embedding.as_ref()]);

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![checkpoint.id.to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![checkpoint.session_id.to_string()])) as ArrayRef,
                Arc::new(Int64Array::from(vec![checkpoint
                    .timestamp
                    .timestamp_millis()])) as ArrayRef,
                Arc::new(StringArray::from(vec![checkpoint.summary.clone()])) as ArrayRef,
                Arc::new(StringArray::from(vec![serde_json::to_string(
                    &checkpoint.event_ids,
                )?])) as ArrayRef,
                Arc::new(StringArray::from(vec![Some(serde_json::to_string(
                    &checkpoint.files_touched,
                )?)])) as ArrayRef,
                embeddings,
            ],
        )?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
        table.add(Box::new(batches)).execute().await?;

        tracing::debug!("Saved checkpoint {} to LanceDB", checkpoint.id);
        Ok(())
    }

    /// Save a session to storage
    pub async fn save_session(&self, session: &SidecarSession) -> Result<()> {
        let table = self
            .sessions_table
            .as_ref()
            .context("Sessions table not initialized")?;

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("started_at_ms", DataType::Int64, false),
            Field::new("ended_at_ms", DataType::Int64, true),
            Field::new("initial_request", DataType::Utf8, false),
            Field::new("workspace_path", DataType::Utf8, false),
            Field::new("event_count", DataType::UInt64, false),
            Field::new("checkpoint_count", DataType::UInt64, false),
            Field::new("files_touched_json", DataType::Utf8, true),
            Field::new("final_summary", DataType::Utf8, true),
        ]));

        let ended_at: Vec<Option<i64>> = vec![session.ended_at.map(|dt| dt.timestamp_millis())];

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![session.id.to_string()])) as ArrayRef,
                Arc::new(Int64Array::from(vec![session
                    .started_at
                    .timestamp_millis()])) as ArrayRef,
                Arc::new(Int64Array::from(ended_at)) as ArrayRef,
                Arc::new(StringArray::from(vec![session.initial_request.clone()])) as ArrayRef,
                Arc::new(StringArray::from(vec![session
                    .workspace_path
                    .to_string_lossy()
                    .to_string()])) as ArrayRef,
                Arc::new(UInt64Array::from(vec![session.event_count as u64])) as ArrayRef,
                Arc::new(UInt64Array::from(vec![session.checkpoint_count as u64])) as ArrayRef,
                Arc::new(StringArray::from(vec![Some(serde_json::to_string(
                    &session.files_touched,
                )?)])) as ArrayRef,
                Arc::new(StringArray::from(vec![session.final_summary.clone()])) as ArrayRef,
            ],
        )?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
        table.add(Box::new(batches)).execute().await?;

        tracing::debug!("Saved session {} to LanceDB", session.id);
        Ok(())
    }

    /// Get all events for a session
    pub async fn get_session_events(&self, session_id: Uuid) -> Result<Vec<SessionEvent>> {
        // Use the cached table handle which is updated by save_events
        let table = self
            .events_table
            .as_ref()
            .context("Events table not initialized")?;

        // Ensure we see the latest version
        table.checkout_latest().await?;

        let results = table
            .query()
            .only_if(format!("session_id = '{}'", session_id))
            .limit(QUERY_LIMIT)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut events = Vec::new();
        for batch in results {
            events.extend(self.record_batch_to_events(&batch)?);
        }

        // Sort by timestamp
        events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        Ok(events)
    }

    /// Get all checkpoints for a session
    pub async fn get_session_checkpoints(&self, session_id: Uuid) -> Result<Vec<Checkpoint>> {
        let table = self
            .checkpoints_table
            .as_ref()
            .context("Checkpoints table not initialized")?;

        // Ensure we see the latest data
        table.checkout_latest().await?;

        let results = table
            .query()
            .only_if(format!("session_id = '{}'", session_id))
            .limit(QUERY_LIMIT)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut checkpoints = Vec::new();
        for batch in results {
            checkpoints.extend(self.record_batch_to_checkpoints(&batch)?);
        }

        checkpoints.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        Ok(checkpoints)
    }

    /// Get a session by ID
    pub async fn get_session(&self, session_id: Uuid) -> Result<Option<SidecarSession>> {
        let table = self
            .sessions_table
            .as_ref()
            .context("Sessions table not initialized")?;

        // Ensure we see the latest data
        table.checkout_latest().await?;

        let results = table
            .query()
            .only_if(format!("id = '{}'", session_id))
            .limit(1)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        if results.is_empty() {
            return Ok(None);
        }

        let sessions = self.record_batch_to_sessions(&results[0])?;
        Ok(sessions.into_iter().next())
    }

    /// List all sessions
    pub async fn list_sessions(&self) -> Result<Vec<SidecarSession>> {
        let table = self
            .sessions_table
            .as_ref()
            .context("Sessions table not initialized")?;

        // Ensure we see the latest data
        table.checkout_latest().await?;

        let results = table
            .query()
            .limit(QUERY_LIMIT)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut sessions = Vec::new();
        for batch in results {
            sessions.extend(self.record_batch_to_sessions(&batch)?);
        }

        // Sort by start time (newest first)
        sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        Ok(sessions)
    }

    /// Vector similarity search across events
    #[allow(dead_code)]
    pub async fn search_events_vector(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<SessionEvent>> {
        let table = self
            .events_table
            .as_ref()
            .context("Events table not initialized")?;

        // Ensure we see the latest data
        table.checkout_latest().await?;

        let results = table
            .query()
            .nearest_to(query_embedding)?
            .limit(limit)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut events = Vec::new();
        for batch in results {
            events.extend(self.record_batch_to_events(&batch)?);
        }

        Ok(events)
    }

    /// Hybrid search: vector similarity + keyword filter
    pub async fn search_events_hybrid(
        &self,
        query_embedding: &[f32],
        keyword: &str,
        limit: usize,
    ) -> Result<Vec<SessionEvent>> {
        let table = self
            .events_table
            .as_ref()
            .context("Events table not initialized")?;

        // Ensure we see the latest data
        table.checkout_latest().await?;

        // Escape single quotes in keyword for SQL
        let escaped_keyword = keyword.replace('\'', "''");

        let results = table
            .query()
            .nearest_to(query_embedding)?
            .only_if(format!("content LIKE '%{}%'", escaped_keyword))
            .limit(limit)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut events = Vec::new();
        for batch in results {
            events.extend(self.record_batch_to_events(&batch)?);
        }

        Ok(events)
    }

    /// Keyword search across events (no embeddings required)
    pub async fn search_events_keyword(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SessionEvent>> {
        let table = self
            .events_table
            .as_ref()
            .context("Events table not initialized")?;

        // Ensure we see the latest data
        table.checkout_latest().await?;

        let escaped_query = query.replace('\'', "''").to_lowercase();

        let results = table
            .query()
            .only_if(format!("LOWER(content) LIKE '%{}%'", escaped_query))
            .limit(limit)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut events = Vec::new();
        for batch in results {
            events.extend(self.record_batch_to_events(&batch)?);
        }

        // Sort by timestamp (newest first)
        events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(events)
    }

    /// Search events by file path
    #[allow(dead_code)]
    pub async fn search_events_by_file(
        &self,
        file_path: &Path,
        limit: usize,
    ) -> Result<Vec<SessionEvent>> {
        let table = self
            .events_table
            .as_ref()
            .context("Events table not initialized")?;

        // Ensure we see the latest data
        table.checkout_latest().await?;

        let path_str = file_path.to_string_lossy().replace('\'', "''");

        let results = table
            .query()
            .only_if(format!("files_json LIKE '%{}%'", path_str))
            .limit(limit)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut events = Vec::new();
        for batch in results {
            events.extend(self.record_batch_to_events(&batch)?);
        }

        events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(events)
    }

    /// Get recent events across all sessions
    #[allow(dead_code)]
    pub async fn get_recent_events(&self, limit: usize) -> Result<Vec<SessionEvent>> {
        let table = self
            .events_table
            .as_ref()
            .context("Events table not initialized")?;

        // Ensure we see the latest data
        table.checkout_latest().await?;

        // Get all events and sort client-side (LanceDB doesn't have ORDER BY in basic query)
        let results = table
            .query()
            .limit(QUERY_LIMIT)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut events = Vec::new();
        for batch in results {
            events.extend(self.record_batch_to_events(&batch)?);
        }

        // Sort by timestamp (newest first) and take limit
        events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        events.truncate(limit);

        Ok(events)
    }

    /// Delete old events based on retention policy
    pub async fn cleanup_old_events(&self, max_age_days: u32) -> Result<usize> {
        if max_age_days == 0 {
            return Ok(0);
        }

        let cutoff = Utc::now() - chrono::Duration::days(max_age_days as i64);
        let cutoff_ms = cutoff.timestamp_millis();

        let table = self
            .events_table
            .as_ref()
            .context("Events table not initialized")?;

        // Count events to delete first
        let old_events = table
            .query()
            .only_if(format!("timestamp_ms < {}", cutoff_ms))
            .limit(QUERY_LIMIT)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let count: usize = old_events.iter().map(|b| b.num_rows()).sum();

        if count > 0 {
            // Delete old events
            table
                .delete(&format!("timestamp_ms < {}", cutoff_ms))
                .await?;

            tracing::info!("Cleaned up {} old events", count);
        }

        Ok(count)
    }

    /// Create a vector index on the events table for faster similarity search.
    /// This should be called after enough events have been accumulated (e.g., 256+).
    /// Returns true if index was created, false if not enough data.
    pub async fn create_events_index(&self) -> Result<bool> {
        let table = self
            .events_table
            .as_ref()
            .context("Events table not initialized")?;

        // Check if we have enough data for indexing (IVF_PQ needs at least num_partitions * 256)
        let count = table.count_rows(None).await?;
        if count < 256 {
            tracing::debug!(
                "Skipping index creation: {} events, need at least 256",
                count
            );
            return Ok(false);
        }

        // Calculate optimal number of partitions (sqrt(n) is a good heuristic)
        let num_partitions = ((count as f64).sqrt() as u32).clamp(1, 256);

        tracing::info!(
            "Creating IVF_PQ index on events table ({} events, {} partitions)",
            count,
            num_partitions
        );

        // Create IVF_PQ index on embedding column
        table
            .create_index(
                &["embedding"],
                Index::IvfPq(
                    lancedb::index::vector::IvfPqIndexBuilder::default()
                        .num_partitions(num_partitions)
                        .num_sub_vectors(16), // 384 dims / 16 = 24 dims per sub-vector
                ),
            )
            .execute()
            .await
            .context("Failed to create events index")?;

        tracing::info!("Events index created successfully");
        Ok(true)
    }

    /// Create a vector index on the checkpoints table
    pub async fn create_checkpoints_index(&self) -> Result<bool> {
        let table = self
            .checkpoints_table
            .as_ref()
            .context("Checkpoints table not initialized")?;

        let count = table.count_rows(None).await?;
        if count < 256 {
            tracing::debug!(
                "Skipping checkpoints index: {} checkpoints, need at least 256",
                count
            );
            return Ok(false);
        }

        let num_partitions = ((count as f64).sqrt() as u32).clamp(1, 256);

        tracing::info!(
            "Creating IVF_PQ index on checkpoints table ({} checkpoints)",
            count
        );

        table
            .create_index(
                &["embedding"],
                Index::IvfPq(
                    lancedb::index::vector::IvfPqIndexBuilder::default()
                        .num_partitions(num_partitions)
                        .num_sub_vectors(16),
                ),
            )
            .execute()
            .await
            .context("Failed to create checkpoints index")?;

        tracing::info!("Checkpoints index created successfully");
        Ok(true)
    }

    /// Check if vector indexes exist
    pub async fn indexes_exist(&self) -> Result<IndexStatus> {
        let events_table = self
            .events_table
            .as_ref()
            .context("Events table not initialized")?;
        let checkpoints_table = self
            .checkpoints_table
            .as_ref()
            .context("Checkpoints table not initialized")?;

        let events_indexes = events_table.list_indices().await?;
        let checkpoints_indexes = checkpoints_table.list_indices().await?;

        Ok(IndexStatus {
            events_indexed: !events_indexes.is_empty(),
            checkpoints_indexed: !checkpoints_indexes.is_empty(),
            events_count: events_table.count_rows(None).await?,
            checkpoints_count: checkpoints_table.count_rows(None).await?,
        })
    }

    /// Get the data directory path
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Get storage statistics
    pub async fn stats(&self) -> Result<StorageStats> {
        let events_table = self
            .events_table
            .as_ref()
            .context("Events table not initialized")?;
        let checkpoints_table = self
            .checkpoints_table
            .as_ref()
            .context("Checkpoints table not initialized")?;
        let sessions_table = self
            .sessions_table
            .as_ref()
            .context("Sessions table not initialized")?;

        let event_count = events_table.count_rows(None).await?;
        let checkpoint_count = checkpoints_table.count_rows(None).await?;
        let session_count = sessions_table.count_rows(None).await?;

        // Estimate size from data directory
        let total_size = Self::dir_size(&self.data_dir).await.unwrap_or(0);

        Ok(StorageStats {
            event_count,
            checkpoint_count,
            session_count,
            total_size_bytes: total_size,
            data_dir: self.data_dir.clone(),
        })
    }

    /// Calculate directory size recursively
    async fn dir_size(path: &Path) -> Result<u64> {
        let mut total = 0u64;
        let mut entries = tokio::fs::read_dir(path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_dir() {
                total += Box::pin(Self::dir_size(&entry.path())).await?;
            } else {
                total += metadata.len();
            }
        }

        Ok(total)
    }

    /// Convert a RecordBatch to SessionEvents
    fn record_batch_to_events(&self, batch: &RecordBatch) -> Result<Vec<SessionEvent>> {
        let ids = batch
            .column_by_name("id")
            .context("Missing id column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("id column is not StringArray")?;

        let session_ids = batch
            .column_by_name("session_id")
            .context("Missing session_id column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("session_id column is not StringArray")?;

        let timestamps = batch
            .column_by_name("timestamp_ms")
            .context("Missing timestamp_ms column")?
            .as_any()
            .downcast_ref::<Int64Array>()
            .context("timestamp_ms column is not Int64Array")?;

        let contents = batch
            .column_by_name("content")
            .context("Missing content column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("content column is not StringArray")?;

        // New columns (nullable for backwards compat with old data)
        let cwds = batch
            .column_by_name("cwd")
            .and_then(|col| col.as_any().downcast_ref::<StringArray>());

        let tool_outputs = batch
            .column_by_name("tool_output")
            .and_then(|col| col.as_any().downcast_ref::<StringArray>());

        let files_accessed_col = batch
            .column_by_name("files_accessed")
            .and_then(|col| col.as_any().downcast_ref::<StringArray>());

        // Support both old "files_json" and new "files_modified" column names
        let files_modified_col = batch
            .column_by_name("files_modified")
            .or_else(|| batch.column_by_name("files_json"))
            .and_then(|col| col.as_any().downcast_ref::<StringArray>());

        let diffs = batch
            .column_by_name("diff")
            .and_then(|col| col.as_any().downcast_ref::<StringArray>());

        let event_data_json = batch
            .column_by_name("event_data_json")
            .context("Missing event_data_json column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("event_data_json column is not StringArray")?;

        let embeddings = batch
            .column_by_name("embedding")
            .and_then(|col| col.as_any().downcast_ref::<FixedSizeListArray>());

        let mut events = Vec::with_capacity(batch.num_rows());

        for i in 0..batch.num_rows() {
            let id = Uuid::parse_str(ids.value(i))?;
            let session_id = Uuid::parse_str(session_ids.value(i))?;
            let timestamp = Utc.timestamp_millis_opt(timestamps.value(i)).unwrap();
            let content = contents.value(i).to_string();

            // Read new nullable columns
            let cwd = cwds.and_then(|col| {
                if col.is_null(i) {
                    None
                } else {
                    Some(col.value(i).to_string())
                }
            });

            let tool_output = tool_outputs.and_then(|col| {
                if col.is_null(i) {
                    None
                } else {
                    Some(col.value(i).to_string())
                }
            });

            let files_accessed: Option<Vec<PathBuf>> = files_accessed_col.and_then(|col| {
                if col.is_null(i) {
                    None
                } else {
                    serde_json::from_str(col.value(i)).ok()
                }
            });

            let files_modified: Vec<PathBuf> = files_modified_col
                .and_then(|col| {
                    if col.is_null(i) {
                        None
                    } else {
                        serde_json::from_str(col.value(i)).ok()
                    }
                })
                .unwrap_or_default();

            let diff = diffs.and_then(|col| {
                if col.is_null(i) {
                    None
                } else {
                    Some(col.value(i).to_string())
                }
            });

            let event_type = serde_json::from_str(event_data_json.value(i))?;

            let embedding = embeddings.and_then(|emb| {
                if emb.is_null(i) {
                    None
                } else {
                    let values = emb.value(i);
                    let float_array = values.as_any().downcast_ref::<Float32Array>()?;
                    Some(float_array.values().to_vec())
                }
            });

            events.push(SessionEvent {
                id,
                session_id,
                timestamp,
                event_type,
                content,
                cwd,
                tool_output,
                files_accessed,
                files_modified,
                diff,
                embedding,
            });
        }

        Ok(events)
    }

    /// Convert a RecordBatch to Checkpoints
    fn record_batch_to_checkpoints(&self, batch: &RecordBatch) -> Result<Vec<Checkpoint>> {
        let ids = batch
            .column_by_name("id")
            .context("Missing id column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("id column is not StringArray")?;

        let session_ids = batch
            .column_by_name("session_id")
            .context("Missing session_id column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("session_id column is not StringArray")?;

        let timestamps = batch
            .column_by_name("timestamp_ms")
            .context("Missing timestamp_ms column")?
            .as_any()
            .downcast_ref::<Int64Array>()
            .context("timestamp_ms column is not Int64Array")?;

        let summaries = batch
            .column_by_name("summary")
            .context("Missing summary column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("summary column is not StringArray")?;

        let event_ids_json = batch
            .column_by_name("event_ids_json")
            .context("Missing event_ids_json column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("event_ids_json column is not StringArray")?;

        let files_touched_json = batch
            .column_by_name("files_touched_json")
            .context("Missing files_touched_json column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("files_touched_json column is not StringArray")?;

        let embeddings = batch
            .column_by_name("embedding")
            .and_then(|col| col.as_any().downcast_ref::<FixedSizeListArray>());

        let mut checkpoints = Vec::with_capacity(batch.num_rows());

        for i in 0..batch.num_rows() {
            let id = Uuid::parse_str(ids.value(i))?;
            let session_id = Uuid::parse_str(session_ids.value(i))?;
            let timestamp = Utc.timestamp_millis_opt(timestamps.value(i)).unwrap();
            let summary = summaries.value(i).to_string();
            let event_ids: Vec<Uuid> = serde_json::from_str(event_ids_json.value(i))?;
            let files_touched: Vec<PathBuf> = files_touched_json
                .value(i)
                .parse::<serde_json::Value>()
                .ok()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();

            let embedding = embeddings.and_then(|emb| {
                if emb.is_null(i) {
                    None
                } else {
                    let values = emb.value(i);
                    let float_array = values.as_any().downcast_ref::<Float32Array>()?;
                    Some(float_array.values().to_vec())
                }
            });

            checkpoints.push(Checkpoint {
                id,
                session_id,
                timestamp,
                summary,
                event_ids,
                files_touched,
                embedding,
            });
        }

        Ok(checkpoints)
    }

    /// Convert a RecordBatch to SidecarSessions
    fn record_batch_to_sessions(&self, batch: &RecordBatch) -> Result<Vec<SidecarSession>> {
        let ids = batch
            .column_by_name("id")
            .context("Missing id column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("id column is not StringArray")?;

        let started_at = batch
            .column_by_name("started_at_ms")
            .context("Missing started_at_ms column")?
            .as_any()
            .downcast_ref::<Int64Array>()
            .context("started_at_ms column is not Int64Array")?;

        let ended_at = batch
            .column_by_name("ended_at_ms")
            .context("Missing ended_at_ms column")?
            .as_any()
            .downcast_ref::<Int64Array>()
            .context("ended_at_ms column is not Int64Array")?;

        let initial_requests = batch
            .column_by_name("initial_request")
            .context("Missing initial_request column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("initial_request column is not StringArray")?;

        let workspace_paths = batch
            .column_by_name("workspace_path")
            .context("Missing workspace_path column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("workspace_path column is not StringArray")?;

        let event_counts = batch
            .column_by_name("event_count")
            .context("Missing event_count column")?
            .as_any()
            .downcast_ref::<UInt64Array>()
            .context("event_count column is not UInt64Array")?;

        let checkpoint_counts = batch
            .column_by_name("checkpoint_count")
            .context("Missing checkpoint_count column")?
            .as_any()
            .downcast_ref::<UInt64Array>()
            .context("checkpoint_count column is not UInt64Array")?;

        let files_touched_json = batch
            .column_by_name("files_touched_json")
            .context("Missing files_touched_json column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("files_touched_json column is not StringArray")?;

        let final_summaries = batch
            .column_by_name("final_summary")
            .context("Missing final_summary column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("final_summary column is not StringArray")?;

        let mut sessions = Vec::with_capacity(batch.num_rows());

        for i in 0..batch.num_rows() {
            let id = Uuid::parse_str(ids.value(i))?;
            let started = Utc.timestamp_millis_opt(started_at.value(i)).unwrap();
            let ended = if ended_at.is_null(i) {
                None
            } else {
                Some(Utc.timestamp_millis_opt(ended_at.value(i)).unwrap())
            };
            let initial_request = initial_requests.value(i).to_string();
            let workspace_path = PathBuf::from(workspace_paths.value(i));
            let event_count = event_counts.value(i) as usize;
            let checkpoint_count = checkpoint_counts.value(i) as usize;
            let files_touched: Vec<PathBuf> = if files_touched_json.is_null(i) {
                vec![]
            } else {
                serde_json::from_str(files_touched_json.value(i)).unwrap_or_default()
            };
            let final_summary = if final_summaries.is_null(i) {
                None
            } else {
                Some(final_summaries.value(i).to_string())
            };

            sessions.push(SidecarSession {
                id,
                started_at: started,
                ended_at: ended,
                initial_request,
                workspace_path,
                event_count,
                checkpoint_count,
                files_touched,
                final_summary,
            });
        }

        Ok(sessions)
    }
}

/// Storage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    /// Number of events stored
    pub event_count: usize,
    /// Number of checkpoints stored
    pub checkpoint_count: usize,
    /// Number of sessions stored
    pub session_count: usize,
    /// Total size of stored data in bytes
    pub total_size_bytes: u64,
    /// Data directory path
    pub data_dir: PathBuf,
}

impl StorageStats {
    /// Get human-readable size
    #[allow(dead_code)]
    pub fn human_size(&self) -> String {
        let bytes = self.total_size_bytes;
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    }
}

/// Vector index status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStatus {
    /// Whether events table has a vector index
    pub events_indexed: bool,
    /// Whether checkpoints table has a vector index
    pub checkpoints_indexed: bool,
    /// Number of events (for determining if indexing is worthwhile)
    pub events_count: usize,
    /// Number of checkpoints
    pub checkpoints_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_storage_creation() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        assert!(storage.events_table.is_some());
        assert!(storage.checkpoints_table.is_some());
        assert!(storage.sessions_table.is_some());
    }

    #[tokio::test]
    async fn test_event_storage() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session_id = Uuid::new_v4();
        let event = SessionEvent::user_prompt(session_id, "Test prompt");

        storage.save_events(&[event.clone()]).await.unwrap();

        let events = storage.get_session_events(session_id).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, event.id);
    }

    #[tokio::test]
    async fn test_checkpoint_storage() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session_id = Uuid::new_v4();
        let checkpoint = Checkpoint::new(
            session_id,
            "Test summary".into(),
            vec![Uuid::new_v4()],
            vec![],
        );

        storage.save_checkpoint(&checkpoint).await.unwrap();

        let checkpoints = storage.get_session_checkpoints(session_id).await.unwrap();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints[0].id, checkpoint.id);
    }

    #[tokio::test]
    async fn test_session_storage() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session = SidecarSession::new(PathBuf::from("/test"), "Initial request".into());

        storage.save_session(&session).await.unwrap();

        let loaded = storage.get_session(session.id).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, session.id);
    }

    #[tokio::test]
    async fn test_keyword_search() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session_id = Uuid::new_v4();

        let event1 = SessionEvent::user_prompt(session_id, "Add authentication to the API");
        let event2 = SessionEvent::user_prompt(session_id, "Fix the database connection");

        storage.save_events(&[event1, event2]).await.unwrap();

        let results = storage
            .search_events_keyword("authentication", 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.to_lowercase().contains("authentication"));
    }

    #[tokio::test]
    async fn test_storage_stats() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session_id = Uuid::new_v4();
        let event = SessionEvent::user_prompt(session_id, "Test prompt");
        storage.save_events(&[event]).await.unwrap();

        let stats = storage.stats().await.unwrap();
        assert_eq!(stats.event_count, 1);
    }

    #[test]
    fn test_human_size() {
        let stats = StorageStats {
            event_count: 0,
            checkpoint_count: 0,
            session_count: 0,
            total_size_bytes: 500,
            data_dir: PathBuf::from("/test"),
        };
        assert_eq!(stats.human_size(), "500 B");

        let stats = StorageStats {
            total_size_bytes: 1500,
            ..stats
        };
        assert_eq!(stats.human_size(), "1.5 KB");

        let stats = StorageStats {
            total_size_bytes: 1500000,
            ..stats
        };
        assert_eq!(stats.human_size(), "1.4 MB");
    }

    #[tokio::test]
    async fn test_event_with_cwd_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session_id = Uuid::new_v4();
        // Create event with context XML that sets cwd
        let input = r#"<context>
<cwd>/Users/test/project</cwd>
</context>

list files in current dir"#;
        let event = SessionEvent::user_prompt(session_id, input);

        assert_eq!(event.cwd, Some("/Users/test/project".to_string()));

        storage.save_events(&[event.clone()]).await.unwrap();
        let retrieved = storage.get_session_events(session_id).await.unwrap();

        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].cwd, Some("/Users/test/project".to_string()));
        assert_eq!(retrieved[0].content, "list files in current dir");
    }

    #[tokio::test]
    async fn test_event_with_tool_output_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session_id = Uuid::new_v4();
        let event = SessionEvent::tool_call_with_output(
            session_id,
            "read_file",
            "path=src/main.rs",
            true,
            Some("fn main() {\n    println!(\"Hello, world!\");\n}".to_string()),
            Some(vec![PathBuf::from("src/main.rs")]),
            vec![],
            None,
        );

        storage.save_events(&[event.clone()]).await.unwrap();
        let retrieved = storage.get_session_events(session_id).await.unwrap();

        assert_eq!(retrieved.len(), 1);
        assert_eq!(
            retrieved[0].tool_output,
            Some("fn main() {\n    println!(\"Hello, world!\");\n}".to_string())
        );
        assert_eq!(
            retrieved[0].files_accessed,
            Some(vec![PathBuf::from("src/main.rs")])
        );
    }

    #[tokio::test]
    async fn test_event_with_diff_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session_id = Uuid::new_v4();
        let diff = "--- src/lib.rs\n+++ src/lib.rs\n@@ -1,3 +1,4 @@\n fn main() {\n-    old_code();\n+    new_code();\n+    extra_line();\n }";
        let event = SessionEvent::tool_call_with_output(
            session_id,
            "edit_file",
            "path=src/lib.rs",
            true,
            Some("Edit applied".to_string()),
            None,
            vec![PathBuf::from("src/lib.rs")],
            Some(diff.to_string()),
        );

        storage.save_events(&[event.clone()]).await.unwrap();
        let retrieved = storage.get_session_events(session_id).await.unwrap();

        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].diff, Some(diff.to_string()));
        assert_eq!(
            retrieved[0].files_modified,
            vec![PathBuf::from("src/lib.rs")]
        );
    }

    #[tokio::test]
    async fn test_event_with_files_accessed_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session_id = Uuid::new_v4();
        let files = vec![
            PathBuf::from("src/main.rs"),
            PathBuf::from("src/lib.rs"),
            PathBuf::from("Cargo.toml"),
        ];
        let event = SessionEvent::tool_call_with_output(
            session_id,
            "list_files",
            "path=.",
            true,
            Some("src/\n  main.rs\n  lib.rs\nCargo.toml".to_string()),
            Some(files.clone()),
            vec![],
            None,
        );

        storage.save_events(&[event.clone()]).await.unwrap();
        let retrieved = storage.get_session_events(session_id).await.unwrap();

        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].files_accessed, Some(files));
    }

    #[tokio::test]
    async fn test_event_all_new_fields_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session_id = Uuid::new_v4();

        // Create an event with ALL new fields populated
        let mut event = SessionEvent::tool_call_with_output(
            session_id,
            "edit_file",
            "path=src/api.rs",
            true,
            Some("Applied 3 changes".to_string()),
            Some(vec![PathBuf::from("src/api.rs")]), // Read before edit
            vec![PathBuf::from("src/api.rs")],       // Modified after edit
            Some("--- src/api.rs\n+++ src/api.rs\n@@ @@\n-old\n+new".to_string()),
        );
        event.cwd = Some("/workspace/project".to_string());

        storage.save_events(&[event.clone()]).await.unwrap();
        let retrieved = storage.get_session_events(session_id).await.unwrap();

        assert_eq!(retrieved.len(), 1);
        let r = &retrieved[0];
        assert_eq!(r.cwd, Some("/workspace/project".to_string()));
        assert_eq!(r.tool_output, Some("Applied 3 changes".to_string()));
        assert_eq!(r.files_accessed, Some(vec![PathBuf::from("src/api.rs")]));
        assert_eq!(r.files_modified, vec![PathBuf::from("src/api.rs")]);
        assert!(r.diff.is_some());
        assert!(r.diff.as_ref().unwrap().contains("-old"));
        assert!(r.diff.as_ref().unwrap().contains("+new"));
    }

    #[tokio::test]
    async fn test_event_null_fields_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session_id = Uuid::new_v4();
        // Create event with no optional fields
        let event = SessionEvent::tool_call_with_output(
            session_id,
            "bash",
            "cmd=echo hello",
            true,
            None,   // No output
            None,   // No files accessed
            vec![], // No files modified
            None,   // No diff
        );

        storage.save_events(&[event.clone()]).await.unwrap();
        let retrieved = storage.get_session_events(session_id).await.unwrap();

        assert_eq!(retrieved.len(), 1);
        assert!(retrieved[0].cwd.is_none());
        assert!(retrieved[0].tool_output.is_none());
        assert!(retrieved[0].files_accessed.is_none());
        assert!(retrieved[0].files_modified.is_empty());
        assert!(retrieved[0].diff.is_none());
    }

    #[tokio::test]
    async fn test_mixed_events_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session_id = Uuid::new_v4();

        // User prompt with context
        let prompt_input = r#"<context>
<cwd>/project</cwd>
</context>

edit the config file"#;
        let event1 = SessionEvent::user_prompt(session_id, prompt_input);

        // Read operation
        let event2 = SessionEvent::tool_call_with_output(
            session_id,
            "read_file",
            "path=config.toml",
            true,
            Some("[package]\nname = \"test\"".to_string()),
            Some(vec![PathBuf::from("config.toml")]),
            vec![],
            None,
        );

        // Edit operation
        let event3 = SessionEvent::tool_call_with_output(
            session_id,
            "edit_file",
            "path=config.toml",
            true,
            Some("OK".to_string()),
            None,
            vec![PathBuf::from("config.toml")],
            Some("--- config.toml\n+++ config.toml".to_string()),
        );

        storage
            .save_events(&[event1.clone(), event2.clone(), event3.clone()])
            .await
            .unwrap();
        let retrieved = storage.get_session_events(session_id).await.unwrap();

        assert_eq!(retrieved.len(), 3);

        // Check event1 (user prompt)
        assert_eq!(retrieved[0].cwd, Some("/project".to_string()));
        assert_eq!(retrieved[0].content, "edit the config file");

        // Check event2 (read)
        assert!(retrieved[1].tool_output.is_some());
        assert_eq!(
            retrieved[1].files_accessed,
            Some(vec![PathBuf::from("config.toml")])
        );

        // Check event3 (edit)
        assert!(retrieved[2].diff.is_some());
        assert_eq!(
            retrieved[2].files_modified,
            vec![PathBuf::from("config.toml")]
        );
    }
}

/// Property-based tests for edge cases
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use tempfile::TempDir;

    /// Truncate a string to a maximum length (mirrors the private truncate in events.rs)
    fn truncate(s: &str, max_len: usize) -> String {
        if s.chars().count() <= max_len {
            s.to_string()
        } else {
            let mut result: String = s.chars().take(max_len.saturating_sub(1)).collect();
            result.push('…');
            result
        }
    }

    // Strategy for generating special/edge case content
    fn special_content_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            // Empty string
            Just(String::new()),
            // Unicode content
            Just("Hello 世界 🚀 émojis".to_string()),
            // SQL injection attempts
            Just("'; DROP TABLE events; --".to_string()),
            Just("' OR '1'='1".to_string()),
            // NULL bytes and control characters
            Just("test\0null\x01control".to_string()),
            // Very long content
            Just("x".repeat(10000)),
            // Newlines and tabs
            Just("line1\nline2\n\ttabbed".to_string()),
            // Path-like content
            Just("/path/to/file with spaces/and'quotes\"".to_string()),
            // JSON-like content
            Just(r#"{"key": "value", "nested": {"a": 1}}"#.to_string()),
            // Random ASCII
            "[a-zA-Z0-9 ]{0,500}",
        ]
    }

    /// Property: Any event saved can be retrieved with content intact
    #[test]
    fn prop_event_roundtrip() {
        let rt = tokio::runtime::Runtime::new().unwrap();

        // Test each special content case
        let test_cases = vec![
            String::new(),
            "Hello 世界 🚀 émojis".to_string(),
            "'; DROP TABLE events; --".to_string(),
            "' OR '1'='1".to_string(),
            "test\0null\x01control".to_string(),
            "x".repeat(10000),
            "line1\nline2\n\ttabbed".to_string(),
            "/path/to/file with spaces/and'quotes\"".to_string(),
            r#"{"key": "value", "nested": {"a": 1}}"#.to_string(),
        ];

        for content in test_cases {
            rt.block_on(async {
                let temp_dir = TempDir::new().unwrap();
                let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

                let session_id = Uuid::new_v4();
                let event = SessionEvent::user_prompt(session_id, &content);
                let event_id = event.id;
                // user_prompt now stores clean content directly (without prefix)
                let expected_content = truncate(&content, 500);

                storage.save_events(&[event]).await.unwrap();

                let retrieved = storage.get_session_events(session_id).await.unwrap();
                assert_eq!(retrieved.len(), 1);
                assert_eq!(retrieved[0].id, event_id);
                assert_eq!(
                    retrieved[0].content, expected_content,
                    "Content mismatch for: {:?}",
                    content
                );
            });
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20))]

        /// Property: count_rows matches query results
        #[test]
        fn prop_count_matches_query(num_events in 1usize..20) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                let temp_dir = TempDir::new().unwrap();
                let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

                let session_id = Uuid::new_v4();
                let events: Vec<SessionEvent> = (0..num_events)
                    .map(|i| SessionEvent::user_prompt(session_id, &format!("Event {}", i)))
                    .collect();

                storage.save_events(&events).await.unwrap();

                let stats = storage.stats().await.unwrap();
                let retrieved = storage.get_session_events(session_id).await.unwrap();

                (stats.event_count as usize, retrieved.len(), num_events)
            });
            prop_assert_eq!(result.0, result.2, "stats.event_count mismatch");
            prop_assert_eq!(result.1, result.2, "retrieved.len mismatch");
        }

        /// Property: Events from different sessions are isolated
        #[test]
        fn prop_session_isolation(
            events_a in 1usize..10,
            events_b in 1usize..10
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                let temp_dir = TempDir::new().unwrap();
                let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

                let session_a = Uuid::new_v4();
                let session_b = Uuid::new_v4();

                let events_for_a: Vec<SessionEvent> = (0..events_a)
                    .map(|i| SessionEvent::user_prompt(session_a, &format!("A-{}", i)))
                    .collect();
                let events_for_b: Vec<SessionEvent> = (0..events_b)
                    .map(|i| SessionEvent::user_prompt(session_b, &format!("B-{}", i)))
                    .collect();

                storage.save_events(&events_for_a).await.unwrap();
                storage.save_events(&events_for_b).await.unwrap();

                let retrieved_a = storage.get_session_events(session_a).await.unwrap();
                let retrieved_b = storage.get_session_events(session_b).await.unwrap();

                // Verify no cross-contamination (content is wrapped as "User asked: A-0")
                let a_valid = retrieved_a.iter().all(|e| e.session_id == session_a && e.content.contains("A-"));
                let b_valid = retrieved_b.iter().all(|e| e.session_id == session_b && e.content.contains("B-"));

                (retrieved_a.len(), retrieved_b.len(), events_a, events_b, a_valid, b_valid)
            });
            prop_assert_eq!(result.0, result.2, "Session A count mismatch");
            prop_assert_eq!(result.1, result.3, "Session B count mismatch");
            prop_assert!(result.4, "Session A has invalid events");
            prop_assert!(result.5, "Session B has invalid events");
        }

        /// Property: Multiple batches all persist correctly
        #[test]
        fn prop_batch_persistence(
            batch_sizes in prop::collection::vec(1usize..20, 1..5)
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                let temp_dir = TempDir::new().unwrap();
                let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

                let session_id = Uuid::new_v4();
                let mut total_events = 0usize;
                let mut all_correct = true;

                for (batch_idx, &batch_size) in batch_sizes.iter().enumerate() {
                    let events: Vec<SessionEvent> = (0..batch_size)
                        .map(|i| SessionEvent::user_prompt(
                            session_id,
                            &format!("Batch {} Event {}", batch_idx, i)
                        ))
                        .collect();

                    storage.save_events(&events).await.unwrap();
                    total_events += batch_size;

                    // Verify after each batch
                    let retrieved = storage.get_session_events(session_id).await.unwrap();
                    if retrieved.len() != total_events {
                        all_correct = false;
                        break;
                    }
                }

                (total_events, all_correct)
            });
            prop_assert!(result.1, "Batch persistence failed for {} total events", result.0);
        }

        /// Property: Events are returned sorted by timestamp
        #[test]
        fn prop_events_sorted_by_timestamp(num_events in 2usize..10) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let is_sorted = rt.block_on(async {
                let temp_dir = TempDir::new().unwrap();
                let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

                let session_id = Uuid::new_v4();

                // Create events with small delays to ensure different timestamps
                let mut events = Vec::new();
                for i in 0..num_events {
                    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                    events.push(SessionEvent::user_prompt(session_id, &format!("Event {}", i)));
                }

                // Reverse to save in wrong order
                events.reverse();

                storage.save_events(&events).await.unwrap();

                let retrieved = storage.get_session_events(session_id).await.unwrap();

                // Verify sorted by timestamp
                retrieved.windows(2).all(|w| w[0].timestamp <= w[1].timestamp)
            });
            prop_assert!(is_sorted, "Events not sorted by timestamp");
        }
    }
}

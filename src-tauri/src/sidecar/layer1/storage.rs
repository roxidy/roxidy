//! Storage layer for Layer 1 session state snapshots.
//!
//! Persists session state snapshots to LanceDB for recovery and history.

use std::sync::Arc;

use anyhow::{Context, Result};
use arrow_array::{ArrayRef, Int64Array, RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use chrono::{TimeZone, Utc};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, Table};
use uuid::Uuid;

use super::state::SessionState;

/// Default high limit for queries
const QUERY_LIMIT: usize = 1_000_000;

/// Storage for Layer 1 session state snapshots
pub struct Layer1Storage {
    /// LanceDB connection (shared with L0)
    connection: Connection,
    /// Session states table
    states_table: Option<Table>,
}

impl Layer1Storage {
    /// Create a new Layer1Storage using an existing LanceDB connection
    pub async fn new(connection: Connection) -> Result<Self> {
        let mut storage = Self {
            connection,
            states_table: None,
        };
        storage.ensure_tables().await?;
        Ok(storage)
    }

    /// Create from the parent sidecar storage connection
    pub async fn from_sidecar_storage(
        sidecar_storage: &crate::sidecar::storage::SidecarStorage,
    ) -> Result<Self> {
        // For now, create a new connection to the same database
        // In the future, we could share the connection
        let data_dir = sidecar_storage.data_dir();
        let db_path = data_dir.join("sidecar.lance");
        let connection = lancedb::connect(db_path.to_str().unwrap())
            .execute()
            .await
            .context("Failed to connect to LanceDB")?;

        Self::new(connection).await
    }

    /// Ensure all tables exist
    async fn ensure_tables(&mut self) -> Result<()> {
        self.states_table = Some(self.ensure_states_table().await?);
        Ok(())
    }

    /// Get or create the session_states table
    async fn ensure_states_table(&self) -> Result<Table> {
        let table_name = "session_states";

        // Try to open existing table
        if let Ok(table) = self.connection.open_table(table_name).execute().await {
            return Ok(table);
        }

        // Create new table with schema
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("timestamp_ms", DataType::Int64, false),
            Field::new("state_json", DataType::Utf8, false),
            Field::new("snapshot_reason", DataType::Utf8, false),
        ]));

        // Create empty initial batch
        let batches = RecordBatchIterator::new(vec![].into_iter().map(Ok), schema.clone());

        self.connection
            .create_table(table_name, Box::new(batches))
            .execute()
            .await
            .context("Failed to create session_states table")
    }

    /// Save a session state snapshot
    pub async fn save_snapshot(&self, state: &SessionState, reason: &str) -> Result<()> {
        let table = self
            .states_table
            .as_ref()
            .context("States table not initialized")?;

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("timestamp_ms", DataType::Int64, false),
            Field::new("state_json", DataType::Utf8, false),
            Field::new("snapshot_reason", DataType::Utf8, false),
        ]));

        let state_json =
            serde_json::to_string(state).context("Failed to serialize session state")?;

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![Uuid::new_v4().to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![state.session_id.to_string()])) as ArrayRef,
                Arc::new(Int64Array::from(vec![state.updated_at.timestamp_millis()])) as ArrayRef,
                Arc::new(StringArray::from(vec![state_json])) as ArrayRef,
                Arc::new(StringArray::from(vec![reason.to_string()])) as ArrayRef,
            ],
        )?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
        table.add(Box::new(batches)).execute().await?;

        tracing::debug!(
            "Saved session state snapshot for session {} (reason: {})",
            state.session_id,
            reason
        );

        Ok(())
    }

    /// Get the latest state snapshot for a session
    pub async fn get_latest_state(&self, session_id: Uuid) -> Result<Option<SessionState>> {
        let table = self
            .states_table
            .as_ref()
            .context("States table not initialized")?;

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

        if results.is_empty() {
            return Ok(None);
        }

        // Find the most recent snapshot
        let mut latest_state: Option<(i64, SessionState)> = None;

        for batch in results {
            let timestamps = batch
                .column_by_name("timestamp_ms")
                .context("Missing timestamp_ms column")?
                .as_any()
                .downcast_ref::<Int64Array>()
                .context("timestamp_ms column is not Int64Array")?;

            let state_jsons = batch
                .column_by_name("state_json")
                .context("Missing state_json column")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("state_json column is not StringArray")?;

            for i in 0..batch.num_rows() {
                let timestamp = timestamps.value(i);
                let state_json = state_jsons.value(i);

                if latest_state
                    .as_ref()
                    .map(|(t, _)| timestamp > *t)
                    .unwrap_or(true)
                {
                    let state: SessionState = serde_json::from_str(state_json)
                        .context("Failed to deserialize session state")?;
                    latest_state = Some((timestamp, state));
                }
            }
        }

        Ok(latest_state.map(|(_, state)| state))
    }

    /// Get all state snapshots for a session (for debugging/history)
    pub async fn get_state_history(&self, session_id: Uuid) -> Result<Vec<StateSnapshot>> {
        let table = self
            .states_table
            .as_ref()
            .context("States table not initialized")?;

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

        let mut snapshots = Vec::new();

        for batch in results {
            let ids = batch
                .column_by_name("id")
                .context("Missing id column")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("id column is not StringArray")?;

            let timestamps = batch
                .column_by_name("timestamp_ms")
                .context("Missing timestamp_ms column")?
                .as_any()
                .downcast_ref::<Int64Array>()
                .context("timestamp_ms column is not Int64Array")?;

            let state_jsons = batch
                .column_by_name("state_json")
                .context("Missing state_json column")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("state_json column is not StringArray")?;

            let reasons = batch
                .column_by_name("snapshot_reason")
                .context("Missing snapshot_reason column")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("snapshot_reason column is not StringArray")?;

            for i in 0..batch.num_rows() {
                let id = Uuid::parse_str(ids.value(i))?;
                let timestamp = Utc.timestamp_millis_opt(timestamps.value(i)).unwrap();
                let state: SessionState = serde_json::from_str(state_jsons.value(i))
                    .context("Failed to deserialize session state")?;
                let reason = reasons.value(i).to_string();

                snapshots.push(StateSnapshot {
                    id,
                    timestamp,
                    state,
                    reason,
                });
            }
        }

        // Sort by timestamp (oldest first)
        snapshots.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        Ok(snapshots)
    }

    /// Delete old snapshots for a session (keep only the last N)
    pub async fn cleanup_old_snapshots(
        &self,
        session_id: Uuid,
        keep_count: usize,
    ) -> Result<usize> {
        let snapshots = self.get_state_history(session_id).await?;

        if snapshots.len() <= keep_count {
            return Ok(0);
        }

        let to_delete = snapshots.len() - keep_count;
        let delete_ids: Vec<_> = snapshots
            .iter()
            .take(to_delete)
            .map(|s| s.id.to_string())
            .collect();

        let table = self
            .states_table
            .as_ref()
            .context("States table not initialized")?;

        for id in &delete_ids {
            table.delete(&format!("id = '{}'", id)).await?;
        }

        tracing::debug!(
            "Cleaned up {} old snapshots for session {}",
            to_delete,
            session_id
        );

        Ok(to_delete)
    }

    /// Get snapshot count for a session
    pub async fn snapshot_count(&self, session_id: Uuid) -> Result<usize> {
        let table = self
            .states_table
            .as_ref()
            .context("States table not initialized")?;

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

        Ok(results.iter().map(|b| b.num_rows()).sum())
    }
}

/// A snapshot of session state with metadata
#[derive(Debug, Clone)]
pub struct StateSnapshot {
    /// Unique identifier
    pub id: Uuid,
    /// When this snapshot was taken
    pub timestamp: chrono::DateTime<Utc>,
    /// The session state at this point
    pub state: SessionState,
    /// Why this snapshot was taken
    pub reason: String,
}

/// Reasons for taking a snapshot
pub mod snapshot_reasons {
    pub const GOAL_ADDED: &str = "goal_added";
    pub const GOAL_COMPLETED: &str = "goal_completed";
    pub const DECISION_RECORDED: &str = "decision_recorded";
    pub const ERROR_ADDED: &str = "error_added";
    pub const ERROR_RESOLVED: &str = "error_resolved";
    pub const PERIODIC: &str = "periodic";
    pub const SESSION_END: &str = "session_end";
    pub const MANUAL: &str = "manual";
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn setup_storage() -> (TempDir, Layer1Storage) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.lance");
        let connection = lancedb::connect(db_path.to_str().unwrap())
            .execute()
            .await
            .unwrap();

        let storage = Layer1Storage::new(connection).await.unwrap();
        (temp_dir, storage)
    }

    #[tokio::test]
    async fn test_save_and_retrieve_snapshot() {
        let (_temp_dir, storage) = setup_storage().await;

        let session_id = Uuid::new_v4();
        let state = SessionState::with_initial_goal(session_id, "Test task");

        storage
            .save_snapshot(&state, snapshot_reasons::GOAL_ADDED)
            .await
            .unwrap();

        let retrieved = storage.get_latest_state(session_id).await.unwrap();

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.session_id, session_id);
        assert_eq!(retrieved.goal_stack.len(), 1);
    }

    #[tokio::test]
    async fn test_get_latest_state_multiple_snapshots() {
        let (_temp_dir, storage) = setup_storage().await;

        let session_id = Uuid::new_v4();
        let mut state = SessionState::with_initial_goal(session_id, "Test task");

        // Save first snapshot
        storage
            .save_snapshot(&state, snapshot_reasons::GOAL_ADDED)
            .await
            .unwrap();

        // Update and save second snapshot
        state.update_narrative("Making progress".to_string());
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        storage
            .save_snapshot(&state, snapshot_reasons::PERIODIC)
            .await
            .unwrap();

        // Should get the latest one
        let retrieved = storage.get_latest_state(session_id).await.unwrap().unwrap();
        assert_eq!(retrieved.narrative, "Making progress");
    }

    #[tokio::test]
    async fn test_get_state_history() {
        let (_temp_dir, storage) = setup_storage().await;

        let session_id = Uuid::new_v4();
        let mut state = SessionState::with_initial_goal(session_id, "Test task");

        // Save multiple snapshots
        storage
            .save_snapshot(&state, snapshot_reasons::GOAL_ADDED)
            .await
            .unwrap();

        state.update_narrative("Step 1".to_string());
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        storage
            .save_snapshot(&state, snapshot_reasons::PERIODIC)
            .await
            .unwrap();

        state.update_narrative("Step 2".to_string());
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        storage
            .save_snapshot(&state, snapshot_reasons::PERIODIC)
            .await
            .unwrap();

        let history = storage.get_state_history(session_id).await.unwrap();
        assert_eq!(history.len(), 3);

        // Should be sorted by timestamp
        assert!(history[0].timestamp <= history[1].timestamp);
        assert!(history[1].timestamp <= history[2].timestamp);
    }

    #[tokio::test]
    async fn test_cleanup_old_snapshots() {
        let (_temp_dir, storage) = setup_storage().await;

        let session_id = Uuid::new_v4();
        let state = SessionState::with_initial_goal(session_id, "Test task");

        // Save 5 snapshots
        for _ in 0..5 {
            storage
                .save_snapshot(&state, snapshot_reasons::PERIODIC)
                .await
                .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        assert_eq!(storage.snapshot_count(session_id).await.unwrap(), 5);

        // Keep only 2
        let deleted = storage.cleanup_old_snapshots(session_id, 2).await.unwrap();
        assert_eq!(deleted, 3);
        assert_eq!(storage.snapshot_count(session_id).await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_nonexistent_session() {
        let (_temp_dir, storage) = setup_storage().await;

        let session_id = Uuid::new_v4();
        let retrieved = storage.get_latest_state(session_id).await.unwrap();

        assert!(retrieved.is_none());
    }
}

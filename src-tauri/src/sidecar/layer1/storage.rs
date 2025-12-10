//! Storage layer for Layer 1 session state.
//!
//! Provides normalized storage for session state entities (goals, decisions, errors, etc.)
//! with embedding support for semantic search and cross-session analytics.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use arrow_array::{
    types::Float32Type, Array, ArrayRef, BooleanArray, FixedSizeListArray, Int32Array, Int64Array,
    RecordBatch, RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use chrono::{DateTime, TimeZone, Utc};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, Table};
use parking_lot::RwLock;
use uuid::Uuid;

use super::state::{
    Decision, DecisionCategory, DecisionConfidence, ErrorEntry, FileContext, Goal, GoalPriority,
    GoalSource, OpenQuestion, QuestionPriority, QuestionSource, SessionState, UnderstandingLevel,
};

/// Embedding dimension for AllMiniLM-L6-v2
pub const EMBEDDING_DIM: i32 = 384;

/// Default high limit for queries
const QUERY_LIMIT: usize = 1_000_000;

/// Storage for Layer 1 session state with normalized tables
pub struct Layer1Storage {
    /// LanceDB connection (shared with L0)
    connection: Connection,
    /// Sessions metadata table
    sessions_table: Option<Table>,
    /// Goals table
    goals_table: Option<Table>,
    /// Decisions table
    decisions_table: Option<Table>,
    /// Errors table
    errors_table: Option<Table>,
    /// File contexts table
    file_contexts_table: Option<Table>,
    /// Questions table
    questions_table: Option<Table>,
    /// Goal progress notes table
    goal_progress_table: Option<Table>,
    /// File changes table
    file_changes_table: Option<Table>,
    /// Legacy session states table (for backward compat during migration)
    #[allow(dead_code)]
    states_table: Option<Table>,
    /// Embedding model (optional - if initialization fails, embeddings will be None)
    embedding_model: Arc<RwLock<Option<TextEmbedding>>>,
    /// Models directory for fastembed cache
    models_dir: PathBuf,
}

impl Layer1Storage {
    /// Create a new Layer1Storage using an existing LanceDB connection
    pub async fn new(connection: Connection) -> Result<Self> {
        Self::new_with_models_dir(connection, PathBuf::from(".qbit/models")).await
    }

    /// Create with a custom models directory for embedding cache
    pub async fn new_with_models_dir(connection: Connection, models_dir: PathBuf) -> Result<Self> {
        let mut storage = Self {
            connection,
            sessions_table: None,
            goals_table: None,
            decisions_table: None,
            errors_table: None,
            file_contexts_table: None,
            questions_table: None,
            goal_progress_table: None,
            file_changes_table: None,
            states_table: None,
            embedding_model: Arc::new(RwLock::new(None)),
            models_dir,
        };
        storage.ensure_tables().await?;
        storage.init_embedding_model();
        Ok(storage)
    }

    /// Create from the parent sidecar storage connection
    pub async fn from_sidecar_storage(
        sidecar_storage: &crate::sidecar::storage::SidecarStorage,
    ) -> Result<Self> {
        let data_dir = sidecar_storage.data_dir();
        let db_path = data_dir.join("sidecar.lance");
        let connection = lancedb::connect(db_path.to_str().unwrap())
            .execute()
            .await
            .context("Failed to connect to LanceDB")?;

        let models_dir = data_dir
            .parent()
            .map(|p| p.join("models"))
            .unwrap_or_else(|| PathBuf::from(".qbit/models"));

        Self::new_with_models_dir(connection, models_dir).await
    }

    /// Initialize the embedding model (lazy loading, optional)
    /// If this fails, embeddings will simply be None - not a fatal error
    fn init_embedding_model(&self) {
        let mut model = self.embedding_model.write();
        if model.is_some() {
            return;
        }

        // Ensure models directory exists
        if let Err(e) = std::fs::create_dir_all(&self.models_dir) {
            tracing::warn!(
                "Failed to create models directory: {}. Embeddings will be disabled.",
                e
            );
            return;
        }

        tracing::info!("Initializing Layer1 embedding model (AllMiniLM-L6-V2)...");

        let options = InitOptions::new(EmbeddingModel::AllMiniLML6V2)
            .with_cache_dir(self.models_dir.clone())
            .with_show_download_progress(true);

        match TextEmbedding::try_new(options) {
            Ok(embedding) => {
                *model = Some(embedding);
                tracing::info!("Layer1 embedding model initialized successfully");
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize Layer1 embedding model: {}. Embeddings will be disabled.",
                    e
                );
            }
        }
    }

    /// Generate embedding for a single text
    /// Returns None if embedding model is not available
    fn embed_text(&self, text: &str) -> Option<Vec<f32>> {
        let model = self.embedding_model.read();
        let model = model.as_ref()?;

        match model.embed(vec![text], None) {
            Ok(mut embeddings) => {
                if embeddings.is_empty() {
                    tracing::warn!("Embedding generation returned empty result");
                    return None;
                }

                let embedding = embeddings.remove(0);

                // Verify dimension
                if embedding.len() != EMBEDDING_DIM as usize {
                    tracing::warn!(
                        "Unexpected embedding dimension: {} (expected {})",
                        embedding.len(),
                        EMBEDDING_DIM
                    );
                    return None;
                }

                Some(embedding)
            }
            Err(e) => {
                tracing::warn!("Failed to generate embedding: {}", e);
                None
            }
        }
    }

    /// Ensure all tables exist
    async fn ensure_tables(&mut self) -> Result<()> {
        // New normalized tables
        self.sessions_table = Some(self.ensure_sessions_table().await?);
        self.goals_table = Some(self.ensure_goals_table().await?);
        self.decisions_table = Some(self.ensure_decisions_table().await?);
        self.errors_table = Some(self.ensure_errors_table().await?);
        self.file_contexts_table = Some(self.ensure_file_contexts_table().await?);
        self.questions_table = Some(self.ensure_questions_table().await?);
        self.goal_progress_table = Some(self.ensure_goal_progress_table().await?);
        self.file_changes_table = Some(self.ensure_file_changes_table().await?);

        // Legacy table (for backward compat)
        self.states_table = Some(self.ensure_legacy_states_table().await?);

        Ok(())
    }

    // ========================================================================
    // Table Schema Definitions
    // ========================================================================

    /// l1_sessions - Session metadata
    async fn ensure_sessions_table(&self) -> Result<Table> {
        let table_name = "l1_sessions";

        if let Ok(table) = self.connection.open_table(table_name).execute().await {
            return Ok(table);
        }

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("created_at_ms", DataType::Int64, false),
            Field::new("updated_at_ms", DataType::Int64, false),
            Field::new("initial_request", DataType::Utf8, false),
            Field::new("narrative", DataType::Utf8, false),
            Field::new("narrative_updated_at_ms", DataType::Int64, false),
            Field::new(
                "narrative_embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                true,
            ),
            Field::new("is_active", DataType::Boolean, false),
        ]));

        let batches = RecordBatchIterator::new(vec![].into_iter().map(Ok), schema.clone());

        self.connection
            .create_table(table_name, Box::new(batches))
            .execute()
            .await
            .context("Failed to create l1_sessions table")
    }

    /// l1_goals - Goal stack with hierarchy
    async fn ensure_goals_table(&self) -> Result<Table> {
        let table_name = "l1_goals";

        if let Ok(table) = self.connection.open_table(table_name).execute().await {
            return Ok(table);
        }

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("parent_goal_id", DataType::Utf8, true),
            Field::new("description", DataType::Utf8, false),
            Field::new(
                "description_embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                true,
            ),
            Field::new("source", DataType::Utf8, false),
            Field::new("priority", DataType::Utf8, false),
            Field::new("blocked_by", DataType::Utf8, true),
            Field::new("completed", DataType::Boolean, false),
            Field::new("created_at_ms", DataType::Int64, false),
            Field::new("completed_at_ms", DataType::Int64, true),
            Field::new("stack_position", DataType::Int32, false),
        ]));

        let batches = RecordBatchIterator::new(vec![].into_iter().map(Ok), schema.clone());

        self.connection
            .create_table(table_name, Box::new(batches))
            .execute()
            .await
            .context("Failed to create l1_goals table")
    }

    /// l1_decisions - Decision log
    async fn ensure_decisions_table(&self) -> Result<Table> {
        let table_name = "l1_decisions";

        if let Ok(table) = self.connection.open_table(table_name).execute().await {
            return Ok(table);
        }

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("timestamp_ms", DataType::Int64, false),
            Field::new("choice", DataType::Utf8, false),
            Field::new("rationale", DataType::Utf8, false),
            Field::new(
                "rationale_embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                true,
            ),
            Field::new("category", DataType::Utf8, false),
            Field::new("confidence", DataType::Utf8, false),
            Field::new("reversible", DataType::Boolean, false),
            Field::new("triggering_event_id", DataType::Utf8, false),
            Field::new("related_files_json", DataType::Utf8, false),
            Field::new("alternatives_json", DataType::Utf8, false),
        ]));

        let batches = RecordBatchIterator::new(vec![].into_iter().map(Ok), schema.clone());

        self.connection
            .create_table(table_name, Box::new(batches))
            .execute()
            .await
            .context("Failed to create l1_decisions table")
    }

    /// l1_errors - Error journal
    async fn ensure_errors_table(&self) -> Result<Table> {
        let table_name = "l1_errors";

        if let Ok(table) = self.connection.open_table(table_name).execute().await {
            return Ok(table);
        }

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("timestamp_ms", DataType::Int64, false),
            Field::new("error", DataType::Utf8, false),
            Field::new(
                "error_embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                true,
            ),
            Field::new("context", DataType::Utf8, false),
            Field::new("resolved", DataType::Boolean, false),
            Field::new("resolution", DataType::Utf8, true),
            Field::new("resolved_at_ms", DataType::Int64, true),
        ]));

        let batches = RecordBatchIterator::new(vec![].into_iter().map(Ok), schema.clone());

        self.connection
            .create_table(table_name, Box::new(batches))
            .execute()
            .await
            .context("Failed to create l1_errors table")
    }

    /// l1_file_contexts - File understanding
    async fn ensure_file_contexts_table(&self) -> Result<Table> {
        let table_name = "l1_file_contexts";

        if let Ok(table) = self.connection.open_table(table_name).execute().await {
            return Ok(table);
        }

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("path", DataType::Utf8, false),
            Field::new("summary", DataType::Utf8, false),
            Field::new(
                "summary_embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                true,
            ),
            Field::new("relevance", DataType::Utf8, false),
            Field::new("understanding_level", DataType::Utf8, false),
            Field::new("key_exports_json", DataType::Utf8, false),
            Field::new("dependencies_json", DataType::Utf8, false),
            Field::new("notes_json", DataType::Utf8, false),
            Field::new("last_read_at_ms", DataType::Int64, true),
            Field::new("last_modified_at_ms", DataType::Int64, true),
            Field::new("created_at_ms", DataType::Int64, false),
            Field::new("updated_at_ms", DataType::Int64, false),
        ]));

        let batches = RecordBatchIterator::new(vec![].into_iter().map(Ok), schema.clone());

        self.connection
            .create_table(table_name, Box::new(batches))
            .execute()
            .await
            .context("Failed to create l1_file_contexts table")
    }

    /// l1_questions - Open questions
    async fn ensure_questions_table(&self) -> Result<Table> {
        let table_name = "l1_questions";

        if let Ok(table) = self.connection.open_table(table_name).execute().await {
            return Ok(table);
        }

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("question", DataType::Utf8, false),
            Field::new(
                "question_embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                true,
            ),
            Field::new("source", DataType::Utf8, false),
            Field::new("context", DataType::Utf8, false),
            Field::new("priority", DataType::Utf8, false),
            Field::new("created_at_ms", DataType::Int64, false),
            Field::new("answered_at_ms", DataType::Int64, true),
            Field::new("answer", DataType::Utf8, true),
        ]));

        let batches = RecordBatchIterator::new(vec![].into_iter().map(Ok), schema.clone());

        self.connection
            .create_table(table_name, Box::new(batches))
            .execute()
            .await
            .context("Failed to create l1_questions table")
    }

    /// l1_goal_progress - Progress notes (append-only)
    async fn ensure_goal_progress_table(&self) -> Result<Table> {
        let table_name = "l1_goal_progress";

        if let Ok(table) = self.connection.open_table(table_name).execute().await {
            return Ok(table);
        }

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("goal_id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("timestamp_ms", DataType::Int64, false),
            Field::new("note", DataType::Utf8, false),
        ]));

        let batches = RecordBatchIterator::new(vec![].into_iter().map(Ok), schema.clone());

        self.connection
            .create_table(table_name, Box::new(batches))
            .execute()
            .await
            .context("Failed to create l1_goal_progress table")
    }

    /// l1_file_changes - File change history (append-only)
    async fn ensure_file_changes_table(&self) -> Result<Table> {
        let table_name = "l1_file_changes";

        if let Ok(table) = self.connection.open_table(table_name).execute().await {
            return Ok(table);
        }

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("file_context_id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("timestamp_ms", DataType::Int64, false),
            Field::new("summary", DataType::Utf8, false),
            Field::new("diff_preview", DataType::Utf8, true),
        ]));

        let batches = RecordBatchIterator::new(vec![].into_iter().map(Ok), schema.clone());

        self.connection
            .create_table(table_name, Box::new(batches))
            .execute()
            .await
            .context("Failed to create l1_file_changes table")
    }

    /// Legacy session_states table (for backward compat)
    async fn ensure_legacy_states_table(&self) -> Result<Table> {
        let table_name = "session_states";

        if let Ok(table) = self.connection.open_table(table_name).execute().await {
            return Ok(table);
        }

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("timestamp_ms", DataType::Int64, false),
            Field::new("state_json", DataType::Utf8, false),
            Field::new("snapshot_reason", DataType::Utf8, false),
        ]));

        let batches = RecordBatchIterator::new(vec![].into_iter().map(Ok), schema.clone());

        self.connection
            .create_table(table_name, Box::new(batches))
            .execute()
            .await
            .context("Failed to create session_states table")
    }

    // ========================================================================
    // Helper: Embedding Array Builder
    // ========================================================================

    /// Build a FixedSizeListArray from optional embeddings
    fn build_embedding_array(&self, embedding: Option<&Vec<f32>>) -> ArrayRef {
        let iter =
            std::iter::once(embedding.map(|emb| emb.iter().copied().map(Some).collect::<Vec<_>>()));
        let list_array =
            FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(iter, EMBEDDING_DIM);
        Arc::new(list_array)
    }

    /// Build embeddings array for multiple items
    fn build_embeddings_array(&self, embeddings: Vec<Option<&Vec<f32>>>) -> ArrayRef {
        let iter = embeddings
            .iter()
            .map(|opt_emb| opt_emb.map(|emb| emb.iter().copied().map(Some).collect::<Vec<_>>()));
        let list_array =
            FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(iter, EMBEDDING_DIM);
        Arc::new(list_array)
    }

    // ========================================================================
    // Session CRUD Operations
    // ========================================================================

    /// Create or update session metadata
    /// If narrative_embedding is not provided, it will be generated automatically
    pub async fn save_session(
        &self,
        session_id: Uuid,
        initial_request: &str,
        narrative: &str,
        narrative_embedding: Option<&Vec<f32>>,
        is_active: bool,
    ) -> Result<()> {
        // Generate embedding if not provided
        let generated_embedding;
        let embedding = match narrative_embedding {
            Some(e) => Some(e),
            None => {
                generated_embedding = self.embed_text(narrative);
                generated_embedding.as_ref()
            }
        };
        let table = self
            .sessions_table
            .as_ref()
            .context("Sessions table not initialized")?;

        let now = Utc::now();
        let now_ms = now.timestamp_millis();

        // Check if session exists
        table.checkout_latest().await?;
        let existing = table
            .query()
            .only_if(format!("id = '{}'", session_id))
            .limit(1)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        if !existing.is_empty() {
            // Update existing session
            table
                .update()
                .only_if(format!("id = '{}'", session_id))
                .column("updated_at_ms", format!("{}", now_ms))
                .column("narrative", format!("'{}'", narrative.replace('\'', "''")))
                .column("narrative_updated_at_ms", format!("{}", now_ms))
                .column("is_active", format!("{}", is_active))
                .execute()
                .await?;
            return Ok(());
        }

        // Insert new session
        let schema = self.sessions_schema();
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![session_id.to_string()])) as ArrayRef,
                Arc::new(Int64Array::from(vec![now_ms])) as ArrayRef,
                Arc::new(Int64Array::from(vec![now_ms])) as ArrayRef,
                Arc::new(StringArray::from(vec![initial_request.to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![narrative.to_string()])) as ArrayRef,
                Arc::new(Int64Array::from(vec![now_ms])) as ArrayRef,
                self.build_embedding_array(embedding),
                Arc::new(BooleanArray::from(vec![is_active])) as ArrayRef,
            ],
        )?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
        table.add(Box::new(batches)).execute().await?;

        tracing::debug!("Created session {} in l1_sessions", session_id);
        Ok(())
    }

    fn sessions_schema(&self) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("created_at_ms", DataType::Int64, false),
            Field::new("updated_at_ms", DataType::Int64, false),
            Field::new("initial_request", DataType::Utf8, false),
            Field::new("narrative", DataType::Utf8, false),
            Field::new("narrative_updated_at_ms", DataType::Int64, false),
            Field::new(
                "narrative_embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                true,
            ),
            Field::new("is_active", DataType::Boolean, false),
        ]))
    }

    /// Get session metadata
    pub async fn get_session(&self, session_id: Uuid) -> Result<Option<SessionMetadata>> {
        let table = self
            .sessions_table
            .as_ref()
            .context("Sessions table not initialized")?;

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

        let batch = &results[0];
        self.batch_to_session_metadata(batch, 0)
    }

    fn batch_to_session_metadata(
        &self,
        batch: &RecordBatch,
        idx: usize,
    ) -> Result<Option<SessionMetadata>> {
        if idx >= batch.num_rows() {
            return Ok(None);
        }

        let ids = batch
            .column_by_name("id")
            .context("Missing id")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("id not StringArray")?;
        let created_at = batch
            .column_by_name("created_at_ms")
            .context("Missing created_at_ms")?
            .as_any()
            .downcast_ref::<Int64Array>()
            .context("created_at_ms not Int64Array")?;
        let updated_at = batch
            .column_by_name("updated_at_ms")
            .context("Missing updated_at_ms")?
            .as_any()
            .downcast_ref::<Int64Array>()
            .context("updated_at_ms not Int64Array")?;
        let initial_requests = batch
            .column_by_name("initial_request")
            .context("Missing initial_request")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("initial_request not StringArray")?;
        let narratives = batch
            .column_by_name("narrative")
            .context("Missing narrative")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("narrative not StringArray")?;
        let narrative_updated_at = batch
            .column_by_name("narrative_updated_at_ms")
            .context("Missing narrative_updated_at_ms")?
            .as_any()
            .downcast_ref::<Int64Array>()
            .context("narrative_updated_at_ms not Int64Array")?;
        let is_actives = batch
            .column_by_name("is_active")
            .context("Missing is_active")?
            .as_any()
            .downcast_ref::<BooleanArray>()
            .context("is_active not BooleanArray")?;

        Ok(Some(SessionMetadata {
            id: Uuid::parse_str(ids.value(idx))?,
            created_at: Utc.timestamp_millis_opt(created_at.value(idx)).unwrap(),
            updated_at: Utc.timestamp_millis_opt(updated_at.value(idx)).unwrap(),
            initial_request: initial_requests.value(idx).to_string(),
            narrative: narratives.value(idx).to_string(),
            narrative_updated_at: Utc
                .timestamp_millis_opt(narrative_updated_at.value(idx))
                .unwrap(),
            is_active: is_actives.value(idx),
        }))
    }

    /// List all sessions (optionally including inactive sessions)
    pub async fn list_sessions(&self, include_inactive: bool) -> Result<Vec<SessionMetadata>> {
        let table = self
            .sessions_table
            .as_ref()
            .context("Sessions table not initialized")?;

        table.checkout_latest().await?;
        let query = if include_inactive {
            table.query().limit(QUERY_LIMIT)
        } else {
            table.query().only_if("is_active = true").limit(QUERY_LIMIT)
        };

        let results = query.execute().await?.try_collect::<Vec<_>>().await?;

        let mut sessions = Vec::new();
        for batch in &results {
            for i in 0..batch.num_rows() {
                if let Some(session) = self.batch_to_session_metadata(batch, i)? {
                    sessions.push(session);
                }
            }
        }

        // Sort by created_at descending (most recent first)
        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(sessions)
    }

    // ========================================================================
    // Goal CRUD Operations
    // ========================================================================

    /// Save a goal to the normalized table
    /// If embedding is not provided, it will be generated automatically from the description
    pub async fn save_goal(
        &self,
        goal: &Goal,
        session_id: Uuid,
        parent_goal_id: Option<Uuid>,
        stack_position: i32,
        embedding: Option<&Vec<f32>>,
    ) -> Result<()> {
        // Generate embedding if not provided
        let generated_embedding;
        let embedding = match embedding {
            Some(e) => Some(e),
            None => {
                generated_embedding = self.embed_text(&goal.description);
                generated_embedding.as_ref()
            }
        };

        let table = self
            .goals_table
            .as_ref()
            .context("Goals table not initialized")?;

        let schema = self.goals_schema();
        let completed_at: Vec<Option<i64>> =
            vec![goal.completed.then(|| Utc::now().timestamp_millis())];
        let parent_id: Vec<Option<String>> = vec![parent_goal_id.map(|id| id.to_string())];
        let blocked_by: Vec<Option<String>> = vec![goal.blocked_by.clone()];

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![goal.id.to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![session_id.to_string()])) as ArrayRef,
                Arc::new(StringArray::from(parent_id)) as ArrayRef,
                Arc::new(StringArray::from(vec![goal.description.clone()])) as ArrayRef,
                self.build_embedding_array(embedding),
                Arc::new(StringArray::from(vec![goal.source.as_str().to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![goal.priority.as_str().to_string()])) as ArrayRef,
                Arc::new(StringArray::from(blocked_by)) as ArrayRef,
                Arc::new(BooleanArray::from(vec![goal.completed])) as ArrayRef,
                Arc::new(Int64Array::from(vec![goal.created_at.timestamp_millis()])) as ArrayRef,
                Arc::new(Int64Array::from(completed_at)) as ArrayRef,
                Arc::new(Int32Array::from(vec![stack_position])) as ArrayRef,
            ],
        )?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
        table.add(Box::new(batches)).execute().await?;

        // Save sub-goals recursively
        for (i, sub_goal) in goal.sub_goals.iter().enumerate() {
            Box::pin(self.save_goal(sub_goal, session_id, Some(goal.id), i as i32, None)).await?;
        }

        // Save progress notes
        for note in &goal.progress_notes {
            self.save_goal_progress(goal.id, session_id, &note.note)
                .await?;
        }

        tracing::debug!("Saved goal {} for session {}", goal.id, session_id);
        Ok(())
    }

    fn goals_schema(&self) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("parent_goal_id", DataType::Utf8, true),
            Field::new("description", DataType::Utf8, false),
            Field::new(
                "description_embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                true,
            ),
            Field::new("source", DataType::Utf8, false),
            Field::new("priority", DataType::Utf8, false),
            Field::new("blocked_by", DataType::Utf8, true),
            Field::new("completed", DataType::Boolean, false),
            Field::new("created_at_ms", DataType::Int64, false),
            Field::new("completed_at_ms", DataType::Int64, true),
            Field::new("stack_position", DataType::Int32, false),
        ]))
    }

    /// Update goal completion status
    pub async fn update_goal_completed(&self, goal_id: Uuid, completed: bool) -> Result<()> {
        let table = self
            .goals_table
            .as_ref()
            .context("Goals table not initialized")?;

        let completed_at_ms = if completed {
            Utc::now().timestamp_millis().to_string()
        } else {
            "NULL".to_string()
        };

        table
            .update()
            .only_if(format!("id = '{}'", goal_id))
            .column("completed", format!("{}", completed))
            .column("completed_at_ms", completed_at_ms)
            .execute()
            .await?;

        tracing::debug!("Updated goal {} completed={}", goal_id, completed);
        Ok(())
    }

    /// Get all goals for a session
    pub async fn get_goals_for_session(&self, session_id: Uuid) -> Result<Vec<Goal>> {
        let table = self
            .goals_table
            .as_ref()
            .context("Goals table not initialized")?;

        table.checkout_latest().await?;
        let results = table
            .query()
            .only_if(format!("session_id = '{}'", session_id))
            .limit(QUERY_LIMIT)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut flat_goals: Vec<(Option<Uuid>, i32, Goal)> = Vec::new();

        for batch in &results {
            let ids = batch
                .column_by_name("id")
                .context("Missing id")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("id not StringArray")?;
            let parent_ids = batch
                .column_by_name("parent_goal_id")
                .context("Missing parent_goal_id")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("parent_goal_id not StringArray")?;
            let descriptions = batch
                .column_by_name("description")
                .context("Missing description")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("description not StringArray")?;
            let sources = batch
                .column_by_name("source")
                .context("Missing source")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("source not StringArray")?;
            let priorities = batch
                .column_by_name("priority")
                .context("Missing priority")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("priority not StringArray")?;
            let blocked_bys = batch
                .column_by_name("blocked_by")
                .context("Missing blocked_by")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("blocked_by not StringArray")?;
            let completeds = batch
                .column_by_name("completed")
                .context("Missing completed")?
                .as_any()
                .downcast_ref::<BooleanArray>()
                .context("completed not BooleanArray")?;
            let created_ats = batch
                .column_by_name("created_at_ms")
                .context("Missing created_at_ms")?
                .as_any()
                .downcast_ref::<Int64Array>()
                .context("created_at_ms not Int64Array")?;
            let stack_positions = batch
                .column_by_name("stack_position")
                .context("Missing stack_position")?
                .as_any()
                .downcast_ref::<Int32Array>()
                .context("stack_position not Int32Array")?;

            for i in 0..batch.num_rows() {
                let id = Uuid::parse_str(ids.value(i))?;
                let parent_id = if parent_ids.is_null(i) {
                    None
                } else {
                    Some(Uuid::parse_str(parent_ids.value(i))?)
                };
                let blocked_by = if blocked_bys.is_null(i) {
                    None
                } else {
                    Some(blocked_bys.value(i).to_string())
                };

                let goal = Goal {
                    id,
                    description: descriptions.value(i).to_string(),
                    source: GoalSource::from_str(sources.value(i)),
                    created_at: Utc.timestamp_millis_opt(created_ats.value(i)).unwrap(),
                    completed: completeds.value(i),
                    completed_at: None,    // Will be set if completed
                    sub_goals: Vec::new(), // Will be populated by hierarchy builder
                    priority: GoalPriority::from_str(priorities.value(i)),
                    blocked_by,
                    progress_notes: Vec::new(), // Will be populated separately
                };

                flat_goals.push((parent_id, stack_positions.value(i), goal));
            }
        }

        // Build hierarchy from flat list
        self.build_goal_hierarchy(flat_goals)
    }

    fn build_goal_hierarchy(
        &self,
        flat_goals: Vec<(Option<Uuid>, i32, Goal)>,
    ) -> Result<Vec<Goal>> {
        use std::collections::HashMap;

        // Separate root goals from sub-goals
        let mut root_goals: Vec<(i32, Goal)> = Vec::new();
        let mut children: HashMap<Uuid, Vec<(i32, Goal)>> = HashMap::new();

        for (parent_id, position, goal) in flat_goals {
            if let Some(pid) = parent_id {
                children.entry(pid).or_default().push((position, goal));
            } else {
                root_goals.push((position, goal));
            }
        }

        // Recursively attach children
        fn attach_children(goal: &mut Goal, children: &mut HashMap<Uuid, Vec<(i32, Goal)>>) {
            if let Some(mut subs) = children.remove(&goal.id) {
                subs.sort_by_key(|(pos, _)| *pos);
                goal.sub_goals = subs
                    .into_iter()
                    .map(|(_, mut g)| {
                        attach_children(&mut g, children);
                        g
                    })
                    .collect();
            }
        }

        // Sort root goals by position
        root_goals.sort_by_key(|(pos, _)| *pos);

        let mut result: Vec<Goal> = root_goals.into_iter().map(|(_, g)| g).collect();
        for goal in &mut result {
            attach_children(goal, &mut children);
        }

        Ok(result)
    }

    /// Search goals by text query (generates embedding internally)
    pub async fn search_goals_by_query(
        &self,
        query: &str,
        completed_only: Option<bool>,
        limit: usize,
    ) -> Result<Vec<(Uuid, Goal)>> {
        let embedding = self
            .embed_text(query)
            .context("Embedding model not available for semantic search")?;

        let table = self
            .goals_table
            .as_ref()
            .context("Goals table not initialized")?;

        table.checkout_latest().await?;

        let query_builder = table.query().nearest_to(embedding)?;
        let query_builder = if let Some(completed) = completed_only {
            query_builder.only_if(format!("completed = {}", completed))
        } else {
            query_builder
        };

        let results = query_builder
            .limit(limit)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut goals = Vec::new();
        for batch in &results {
            let session_ids = batch
                .column_by_name("session_id")
                .context("Missing session_id")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("session_id not StringArray")?;

            let ids = batch
                .column_by_name("id")
                .context("Missing id")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("id not StringArray")?;
            let descriptions = batch
                .column_by_name("description")
                .context("Missing description")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("description not StringArray")?;
            let sources = batch
                .column_by_name("source")
                .context("Missing source")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("source not StringArray")?;
            let priorities = batch
                .column_by_name("priority")
                .context("Missing priority")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("priority not StringArray")?;
            let blocked_bys = batch
                .column_by_name("blocked_by")
                .context("Missing blocked_by")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("blocked_by not StringArray")?;
            let completeds = batch
                .column_by_name("completed")
                .context("Missing completed")?
                .as_any()
                .downcast_ref::<BooleanArray>()
                .context("completed not BooleanArray")?;
            let created_ats = batch
                .column_by_name("created_at_ms")
                .context("Missing created_at_ms")?
                .as_any()
                .downcast_ref::<Int64Array>()
                .context("created_at_ms not Int64Array")?;

            for i in 0..batch.num_rows() {
                let session_id = Uuid::parse_str(session_ids.value(i))?;
                let blocked_by = if blocked_bys.is_null(i) {
                    None
                } else {
                    Some(blocked_bys.value(i).to_string())
                };

                let goal = Goal {
                    id: Uuid::parse_str(ids.value(i))?,
                    description: descriptions.value(i).to_string(),
                    source: GoalSource::from_str(sources.value(i)),
                    created_at: Utc.timestamp_millis_opt(created_ats.value(i)).unwrap(),
                    completed: completeds.value(i),
                    completed_at: None,
                    sub_goals: Vec::new(),
                    priority: GoalPriority::from_str(priorities.value(i)),
                    blocked_by,
                    progress_notes: Vec::new(),
                };

                goals.push((session_id, goal));
            }
        }

        Ok(goals)
    }

    /// Save a goal progress note
    async fn save_goal_progress(&self, goal_id: Uuid, session_id: Uuid, note: &str) -> Result<()> {
        let table = self
            .goal_progress_table
            .as_ref()
            .context("Goal progress table not initialized")?;

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("goal_id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("timestamp_ms", DataType::Int64, false),
            Field::new("note", DataType::Utf8, false),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![Uuid::new_v4().to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![goal_id.to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![session_id.to_string()])) as ArrayRef,
                Arc::new(Int64Array::from(vec![Utc::now().timestamp_millis()])) as ArrayRef,
                Arc::new(StringArray::from(vec![note.to_string()])) as ArrayRef,
            ],
        )?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
        table.add(Box::new(batches)).execute().await?;
        Ok(())
    }

    // ========================================================================
    // Decision CRUD Operations
    // ========================================================================

    /// Save a decision
    /// If embedding is not provided, it will be generated automatically from the rationale
    pub async fn save_decision(
        &self,
        decision: &Decision,
        session_id: Uuid,
        embedding: Option<&Vec<f32>>,
    ) -> Result<()> {
        // Generate embedding if not provided
        let generated_embedding;
        let embedding = match embedding {
            Some(e) => Some(e),
            None => {
                generated_embedding = self.embed_text(&decision.rationale);
                generated_embedding.as_ref()
            }
        };

        let table = self
            .decisions_table
            .as_ref()
            .context("Decisions table not initialized")?;

        let schema = self.decisions_schema();
        let related_files_json = serde_json::to_string(&decision.related_files)?;
        let alternatives_json = serde_json::to_string(&decision.alternatives)?;

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![decision.id.to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![session_id.to_string()])) as ArrayRef,
                Arc::new(Int64Array::from(vec![decision
                    .timestamp
                    .timestamp_millis()])) as ArrayRef,
                Arc::new(StringArray::from(vec![decision.choice.clone()])) as ArrayRef,
                Arc::new(StringArray::from(vec![decision.rationale.clone()])) as ArrayRef,
                self.build_embedding_array(embedding),
                Arc::new(StringArray::from(vec![decision
                    .category
                    .as_str()
                    .to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![decision
                    .confidence
                    .as_str()
                    .to_string()])) as ArrayRef,
                Arc::new(BooleanArray::from(vec![decision.reversible])) as ArrayRef,
                Arc::new(StringArray::from(vec![decision
                    .triggering_event_id
                    .to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![related_files_json])) as ArrayRef,
                Arc::new(StringArray::from(vec![alternatives_json])) as ArrayRef,
            ],
        )?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
        table.add(Box::new(batches)).execute().await?;

        tracing::debug!("Saved decision {} for session {}", decision.id, session_id);
        Ok(())
    }

    fn decisions_schema(&self) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("timestamp_ms", DataType::Int64, false),
            Field::new("choice", DataType::Utf8, false),
            Field::new("rationale", DataType::Utf8, false),
            Field::new(
                "rationale_embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                true,
            ),
            Field::new("category", DataType::Utf8, false),
            Field::new("confidence", DataType::Utf8, false),
            Field::new("reversible", DataType::Boolean, false),
            Field::new("triggering_event_id", DataType::Utf8, false),
            Field::new("related_files_json", DataType::Utf8, false),
            Field::new("alternatives_json", DataType::Utf8, false),
        ]))
    }

    /// Get all decisions for a session
    pub async fn get_decisions_for_session(&self, session_id: Uuid) -> Result<Vec<Decision>> {
        let table = self
            .decisions_table
            .as_ref()
            .context("Decisions table not initialized")?;

        table.checkout_latest().await?;
        let results = table
            .query()
            .only_if(format!("session_id = '{}'", session_id))
            .limit(QUERY_LIMIT)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut decisions = Vec::new();
        for batch in &results {
            decisions.extend(self.batch_to_decisions(batch)?);
        }

        decisions.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(decisions)
    }

    /// Search decisions by category
    pub async fn get_decisions_by_category(&self, category: &str) -> Result<Vec<(Uuid, Decision)>> {
        let table = self
            .decisions_table
            .as_ref()
            .context("Decisions table not initialized")?;

        table.checkout_latest().await?;
        let results = table
            .query()
            .only_if(format!("category = '{}'", category))
            .limit(QUERY_LIMIT)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut decisions = Vec::new();
        for batch in &results {
            let session_ids = batch
                .column_by_name("session_id")
                .context("Missing session_id")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("session_id not StringArray")?;

            for (i, decision) in self.batch_to_decisions(batch)?.into_iter().enumerate() {
                let session_id = Uuid::parse_str(session_ids.value(i))?;
                decisions.push((session_id, decision));
            }
        }

        Ok(decisions)
    }

    /// Search decisions by semantic similarity
    pub async fn search_decisions_by_embedding(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(Uuid, Decision)>> {
        let table = self
            .decisions_table
            .as_ref()
            .context("Decisions table not initialized")?;

        table.checkout_latest().await?;
        let results = table
            .query()
            .nearest_to(query_embedding)?
            .limit(limit)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut decisions = Vec::new();
        for batch in &results {
            let session_ids = batch
                .column_by_name("session_id")
                .context("Missing session_id")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("session_id not StringArray")?;

            for (i, decision) in self.batch_to_decisions(batch)?.into_iter().enumerate() {
                let session_id = Uuid::parse_str(session_ids.value(i))?;
                decisions.push((session_id, decision));
            }
        }

        Ok(decisions)
    }

    /// Search decisions by text query (generates embedding internally)
    pub async fn search_decisions_by_query(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(Uuid, Decision)>> {
        let embedding = self
            .embed_text(query)
            .context("Embedding model not available for semantic search")?;

        self.search_decisions_by_embedding(&embedding, limit).await
    }

    fn batch_to_decisions(&self, batch: &RecordBatch) -> Result<Vec<Decision>> {
        let ids = batch
            .column_by_name("id")
            .context("Missing id")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("id not StringArray")?;
        let timestamps = batch
            .column_by_name("timestamp_ms")
            .context("Missing timestamp_ms")?
            .as_any()
            .downcast_ref::<Int64Array>()
            .context("timestamp_ms not Int64Array")?;
        let choices = batch
            .column_by_name("choice")
            .context("Missing choice")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("choice not StringArray")?;
        let rationales = batch
            .column_by_name("rationale")
            .context("Missing rationale")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("rationale not StringArray")?;
        let categories = batch
            .column_by_name("category")
            .context("Missing category")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("category not StringArray")?;
        let confidences = batch
            .column_by_name("confidence")
            .context("Missing confidence")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("confidence not StringArray")?;
        let reversibles = batch
            .column_by_name("reversible")
            .context("Missing reversible")?
            .as_any()
            .downcast_ref::<BooleanArray>()
            .context("reversible not BooleanArray")?;
        let triggering_events = batch
            .column_by_name("triggering_event_id")
            .context("Missing triggering_event_id")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("triggering_event_id not StringArray")?;
        let related_files = batch
            .column_by_name("related_files_json")
            .context("Missing related_files_json")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("related_files_json not StringArray")?;
        let alternatives = batch
            .column_by_name("alternatives_json")
            .context("Missing alternatives_json")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("alternatives_json not StringArray")?;

        let mut decisions = Vec::new();
        for i in 0..batch.num_rows() {
            decisions.push(Decision {
                id: Uuid::parse_str(ids.value(i))?,
                timestamp: Utc.timestamp_millis_opt(timestamps.value(i)).unwrap(),
                choice: choices.value(i).to_string(),
                rationale: rationales.value(i).to_string(),
                alternatives_rejected: Vec::new(), // Legacy field, not used in normalized storage
                triggering_event_id: Uuid::parse_str(triggering_events.value(i))?,
                alternatives: serde_json::from_str(alternatives.value(i)).unwrap_or_default(),
                category: DecisionCategory::from_str(categories.value(i)),
                confidence: DecisionConfidence::from_str(confidences.value(i)),
                reversible: reversibles.value(i),
                related_files: serde_json::from_str(related_files.value(i)).unwrap_or_default(),
            });
        }

        Ok(decisions)
    }

    // ========================================================================
    // Error CRUD Operations
    // ========================================================================

    /// Save an error entry
    /// If embedding is not provided, it will be generated automatically from the error message
    pub async fn save_error(
        &self,
        error: &ErrorEntry,
        session_id: Uuid,
        embedding: Option<&Vec<f32>>,
    ) -> Result<()> {
        // Generate embedding if not provided
        let generated_embedding;
        let embedding = match embedding {
            Some(e) => Some(e),
            None => {
                generated_embedding = self.embed_text(&error.error);
                generated_embedding.as_ref()
            }
        };

        let table = self
            .errors_table
            .as_ref()
            .context("Errors table not initialized")?;

        let schema = self.errors_schema();
        let resolution: Vec<Option<String>> = vec![error.resolution.clone()];
        let resolved_at: Vec<Option<i64>> =
            vec![error.resolved.then(|| Utc::now().timestamp_millis())];

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![error.id.to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![session_id.to_string()])) as ArrayRef,
                Arc::new(Int64Array::from(vec![error.timestamp.timestamp_millis()])) as ArrayRef,
                Arc::new(StringArray::from(vec![error.error.clone()])) as ArrayRef,
                self.build_embedding_array(embedding),
                Arc::new(StringArray::from(vec![error.context.clone()])) as ArrayRef,
                Arc::new(BooleanArray::from(vec![error.resolved])) as ArrayRef,
                Arc::new(StringArray::from(resolution)) as ArrayRef,
                Arc::new(Int64Array::from(resolved_at)) as ArrayRef,
            ],
        )?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
        table.add(Box::new(batches)).execute().await?;

        tracing::debug!("Saved error {} for session {}", error.id, session_id);
        Ok(())
    }

    fn errors_schema(&self) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("timestamp_ms", DataType::Int64, false),
            Field::new("error", DataType::Utf8, false),
            Field::new(
                "error_embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                true,
            ),
            Field::new("context", DataType::Utf8, false),
            Field::new("resolved", DataType::Boolean, false),
            Field::new("resolution", DataType::Utf8, true),
            Field::new("resolved_at_ms", DataType::Int64, true),
        ]))
    }

    /// Mark an error as resolved
    pub async fn mark_error_resolved(&self, error_id: Uuid, resolution: &str) -> Result<()> {
        let table = self
            .errors_table
            .as_ref()
            .context("Errors table not initialized")?;

        table
            .update()
            .only_if(format!("id = '{}'", error_id))
            .column("resolved", "true".to_string())
            .column(
                "resolution",
                format!("'{}'", resolution.replace('\'', "''")),
            )
            .column("resolved_at_ms", Utc::now().timestamp_millis().to_string())
            .execute()
            .await?;

        tracing::debug!("Marked error {} as resolved", error_id);
        Ok(())
    }

    /// Get all errors for a session
    pub async fn get_errors_for_session(&self, session_id: Uuid) -> Result<Vec<ErrorEntry>> {
        let table = self
            .errors_table
            .as_ref()
            .context("Errors table not initialized")?;

        table.checkout_latest().await?;
        let results = table
            .query()
            .only_if(format!("session_id = '{}'", session_id))
            .limit(QUERY_LIMIT)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut errors = Vec::new();
        for batch in &results {
            errors.extend(self.batch_to_errors(batch)?);
        }

        errors.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(errors)
    }

    /// Get all unresolved errors across all sessions
    pub async fn get_unresolved_errors(&self) -> Result<Vec<(Uuid, ErrorEntry)>> {
        let table = self
            .errors_table
            .as_ref()
            .context("Errors table not initialized")?;

        table.checkout_latest().await?;
        let results = table
            .query()
            .only_if("resolved = false")
            .limit(QUERY_LIMIT)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut errors = Vec::new();
        for batch in &results {
            let session_ids = batch
                .column_by_name("session_id")
                .context("Missing session_id")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("session_id not StringArray")?;

            for (i, error) in self.batch_to_errors(batch)?.into_iter().enumerate() {
                let session_id = Uuid::parse_str(session_ids.value(i))?;
                errors.push((session_id, error));
            }
        }

        Ok(errors)
    }

    /// Search errors by text query (generates embedding internally)
    pub async fn search_errors_by_query(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(Uuid, ErrorEntry)>> {
        let embedding = self
            .embed_text(query)
            .context("Embedding model not available for semantic search")?;

        let table = self
            .errors_table
            .as_ref()
            .context("Errors table not initialized")?;

        table.checkout_latest().await?;
        let results = table
            .query()
            .nearest_to(embedding)?
            .limit(limit)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut errors = Vec::new();
        for batch in &results {
            let session_ids = batch
                .column_by_name("session_id")
                .context("Missing session_id")?
                .as_any()
                .downcast_ref::<StringArray>()
                .context("session_id not StringArray")?;

            for (i, error) in self.batch_to_errors(batch)?.into_iter().enumerate() {
                let session_id = Uuid::parse_str(session_ids.value(i))?;
                errors.push((session_id, error));
            }
        }

        Ok(errors)
    }

    fn batch_to_errors(&self, batch: &RecordBatch) -> Result<Vec<ErrorEntry>> {
        let ids = batch
            .column_by_name("id")
            .context("Missing id")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("id not StringArray")?;
        let timestamps = batch
            .column_by_name("timestamp_ms")
            .context("Missing timestamp_ms")?
            .as_any()
            .downcast_ref::<Int64Array>()
            .context("timestamp_ms not Int64Array")?;
        let error_msgs = batch
            .column_by_name("error")
            .context("Missing error")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("error not StringArray")?;
        let contexts = batch
            .column_by_name("context")
            .context("Missing context")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("context not StringArray")?;
        let resolveds = batch
            .column_by_name("resolved")
            .context("Missing resolved")?
            .as_any()
            .downcast_ref::<BooleanArray>()
            .context("resolved not BooleanArray")?;
        let resolutions = batch
            .column_by_name("resolution")
            .context("Missing resolution")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("resolution not StringArray")?;

        let mut errors = Vec::new();
        for i in 0..batch.num_rows() {
            let resolution = if resolutions.is_null(i) {
                None
            } else {
                Some(resolutions.value(i).to_string())
            };

            errors.push(ErrorEntry {
                id: Uuid::parse_str(ids.value(i))?,
                timestamp: Utc.timestamp_millis_opt(timestamps.value(i)).unwrap(),
                error: error_msgs.value(i).to_string(),
                context: contexts.value(i).to_string(),
                resolved: resolveds.value(i),
                resolution,
                resolved_at: None, // TODO: Read from resolved_at_ms column
            });
        }

        Ok(errors)
    }

    // ========================================================================
    // File Context CRUD Operations
    // ========================================================================

    /// Upsert a file context (insert or update)
    /// If embedding is not provided, it will be generated automatically from the summary
    pub async fn upsert_file_context(
        &self,
        context: &FileContext,
        session_id: Uuid,
        embedding: Option<&Vec<f32>>,
    ) -> Result<()> {
        // Generate embedding if not provided
        let generated_embedding;
        let embedding = match embedding {
            Some(e) => Some(e),
            None => {
                generated_embedding = self.embed_text(&context.summary);
                generated_embedding.as_ref()
            }
        };

        let table = self
            .file_contexts_table
            .as_ref()
            .context("File contexts table not initialized")?;

        let path_str = context.path.to_string_lossy().to_string();

        // Check if exists
        table.checkout_latest().await?;
        let existing = table
            .query()
            .only_if(format!(
                "session_id = '{}' AND path = '{}'",
                session_id,
                path_str.replace('\'', "''")
            ))
            .limit(1)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let now_ms = Utc::now().timestamp_millis();

        if !existing.is_empty() {
            // Update existing
            table
                .update()
                .only_if(format!(
                    "session_id = '{}' AND path = '{}'",
                    session_id,
                    path_str.replace('\'', "''")
                ))
                .column(
                    "summary",
                    format!("'{}'", context.summary.replace('\'', "''")),
                )
                .column(
                    "relevance",
                    format!("'{}'", context.relevance.replace('\'', "''")),
                )
                .column(
                    "understanding_level",
                    format!("'{}'", context.understanding_level.as_str()),
                )
                .column("updated_at_ms", now_ms.to_string())
                .execute()
                .await?;
            return Ok(());
        }

        // Insert new
        let schema = self.file_contexts_schema();
        let key_exports_json = serde_json::to_string(&context.key_exports)?;
        let dependencies_json = serde_json::to_string(&context.dependencies)?;
        let notes_json = serde_json::to_string(&context.notes)?;
        let last_read: Vec<Option<i64>> =
            vec![context.last_read_at.map(|dt| dt.timestamp_millis())];
        let last_modified: Vec<Option<i64>> =
            vec![context.last_modified_at.map(|dt| dt.timestamp_millis())];

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![Uuid::new_v4().to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![session_id.to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![path_str])) as ArrayRef,
                Arc::new(StringArray::from(vec![context.summary.clone()])) as ArrayRef,
                self.build_embedding_array(embedding),
                Arc::new(StringArray::from(vec![context.relevance.clone()])) as ArrayRef,
                Arc::new(StringArray::from(vec![context
                    .understanding_level
                    .as_str()
                    .to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![key_exports_json])) as ArrayRef,
                Arc::new(StringArray::from(vec![dependencies_json])) as ArrayRef,
                Arc::new(StringArray::from(vec![notes_json])) as ArrayRef,
                Arc::new(Int64Array::from(last_read)) as ArrayRef,
                Arc::new(Int64Array::from(last_modified)) as ArrayRef,
                Arc::new(Int64Array::from(vec![now_ms])) as ArrayRef,
                Arc::new(Int64Array::from(vec![now_ms])) as ArrayRef,
            ],
        )?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
        table.add(Box::new(batches)).execute().await?;

        tracing::debug!(
            "Saved file context {} for session {}",
            context.path.display(),
            session_id
        );
        Ok(())
    }

    fn file_contexts_schema(&self) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("path", DataType::Utf8, false),
            Field::new("summary", DataType::Utf8, false),
            Field::new(
                "summary_embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                true,
            ),
            Field::new("relevance", DataType::Utf8, false),
            Field::new("understanding_level", DataType::Utf8, false),
            Field::new("key_exports_json", DataType::Utf8, false),
            Field::new("dependencies_json", DataType::Utf8, false),
            Field::new("notes_json", DataType::Utf8, false),
            Field::new("last_read_at_ms", DataType::Int64, true),
            Field::new("last_modified_at_ms", DataType::Int64, true),
            Field::new("created_at_ms", DataType::Int64, false),
            Field::new("updated_at_ms", DataType::Int64, false),
        ]))
    }

    /// Get all file contexts for a session
    pub async fn get_file_contexts_for_session(
        &self,
        session_id: Uuid,
    ) -> Result<HashMap<PathBuf, FileContext>> {
        let table = self
            .file_contexts_table
            .as_ref()
            .context("File contexts table not initialized")?;

        table.checkout_latest().await?;
        let results = table
            .query()
            .only_if(format!("session_id = '{}'", session_id))
            .limit(QUERY_LIMIT)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut contexts = HashMap::new();
        for batch in &results {
            for ctx in self.batch_to_file_contexts(batch)? {
                contexts.insert(ctx.path.clone(), ctx);
            }
        }

        Ok(contexts)
    }

    fn batch_to_file_contexts(&self, batch: &RecordBatch) -> Result<Vec<FileContext>> {
        let paths = batch
            .column_by_name("path")
            .context("Missing path")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("path not StringArray")?;
        let summaries = batch
            .column_by_name("summary")
            .context("Missing summary")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("summary not StringArray")?;
        let relevances = batch
            .column_by_name("relevance")
            .context("Missing relevance")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("relevance not StringArray")?;
        let understanding_levels = batch
            .column_by_name("understanding_level")
            .context("Missing understanding_level")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("understanding_level not StringArray")?;
        let key_exports = batch
            .column_by_name("key_exports_json")
            .context("Missing key_exports_json")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("key_exports_json not StringArray")?;
        let dependencies = batch
            .column_by_name("dependencies_json")
            .context("Missing dependencies_json")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("dependencies_json not StringArray")?;
        let notes = batch
            .column_by_name("notes_json")
            .context("Missing notes_json")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("notes_json not StringArray")?;
        let last_reads = batch
            .column_by_name("last_read_at_ms")
            .context("Missing last_read_at_ms")?
            .as_any()
            .downcast_ref::<Int64Array>()
            .context("last_read_at_ms not Int64Array")?;
        let last_modifieds = batch
            .column_by_name("last_modified_at_ms")
            .context("Missing last_modified_at_ms")?
            .as_any()
            .downcast_ref::<Int64Array>()
            .context("last_modified_at_ms not Int64Array")?;

        let mut contexts = Vec::new();
        for i in 0..batch.num_rows() {
            let last_read_at = if last_reads.is_null(i) {
                None
            } else {
                Some(Utc.timestamp_millis_opt(last_reads.value(i)).unwrap())
            };
            let last_modified_at = if last_modifieds.is_null(i) {
                None
            } else {
                Some(Utc.timestamp_millis_opt(last_modifieds.value(i)).unwrap())
            };

            contexts.push(FileContext {
                path: PathBuf::from(paths.value(i)),
                summary: summaries.value(i).to_string(),
                relevance: relevances.value(i).to_string(),
                understanding_level: UnderstandingLevel::from_str(understanding_levels.value(i)),
                key_exports: serde_json::from_str(key_exports.value(i)).unwrap_or_default(),
                dependencies: serde_json::from_str(dependencies.value(i)).unwrap_or_default(),
                notes: serde_json::from_str(notes.value(i)).unwrap_or_default(),
                last_read_at,
                last_modified_at,
                change_history: Vec::new(), // TODO: Load from file_changes table
            });
        }

        Ok(contexts)
    }

    // ========================================================================
    // Question CRUD Operations
    // ========================================================================

    /// Save a question
    /// If embedding is not provided, it will be generated automatically from the question text
    pub async fn save_question(
        &self,
        question: &OpenQuestion,
        session_id: Uuid,
        embedding: Option<&Vec<f32>>,
    ) -> Result<()> {
        // Generate embedding if not provided
        let generated_embedding;
        let embedding = match embedding {
            Some(e) => Some(e),
            None => {
                generated_embedding = self.embed_text(&question.question);
                generated_embedding.as_ref()
            }
        };

        let table = self
            .questions_table
            .as_ref()
            .context("Questions table not initialized")?;

        let schema = self.questions_schema();
        let answered_at: Vec<Option<i64>> =
            vec![question.answered_at.map(|dt| dt.timestamp_millis())];
        let answer: Vec<Option<String>> = vec![question.answer.clone()];

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![question.id.to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![session_id.to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![question.question.clone()])) as ArrayRef,
                self.build_embedding_array(embedding),
                Arc::new(StringArray::from(vec![question
                    .source
                    .as_str()
                    .to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![question.context.clone()])) as ArrayRef,
                Arc::new(StringArray::from(vec![question
                    .priority
                    .as_str()
                    .to_string()])) as ArrayRef,
                Arc::new(Int64Array::from(vec![question
                    .created_at
                    .timestamp_millis()])) as ArrayRef,
                Arc::new(Int64Array::from(answered_at)) as ArrayRef,
                Arc::new(StringArray::from(answer)) as ArrayRef,
            ],
        )?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
        table.add(Box::new(batches)).execute().await?;

        tracing::debug!("Saved question {} for session {}", question.id, session_id);
        Ok(())
    }

    fn questions_schema(&self) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("question", DataType::Utf8, false),
            Field::new(
                "question_embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                true,
            ),
            Field::new("source", DataType::Utf8, false),
            Field::new("context", DataType::Utf8, false),
            Field::new("priority", DataType::Utf8, false),
            Field::new("created_at_ms", DataType::Int64, false),
            Field::new("answered_at_ms", DataType::Int64, true),
            Field::new("answer", DataType::Utf8, true),
        ]))
    }

    /// Mark a question as answered
    pub async fn mark_question_answered(&self, question_id: Uuid, answer: &str) -> Result<()> {
        let table = self
            .questions_table
            .as_ref()
            .context("Questions table not initialized")?;

        table
            .update()
            .only_if(format!("id = '{}'", question_id))
            .column("answered_at_ms", Utc::now().timestamp_millis().to_string())
            .column("answer", format!("'{}'", answer.replace('\'', "''")))
            .execute()
            .await?;

        tracing::debug!("Marked question {} as answered", question_id);
        Ok(())
    }

    /// Get all questions for a session
    pub async fn get_questions_for_session(&self, session_id: Uuid) -> Result<Vec<OpenQuestion>> {
        let table = self
            .questions_table
            .as_ref()
            .context("Questions table not initialized")?;

        table.checkout_latest().await?;
        let results = table
            .query()
            .only_if(format!("session_id = '{}'", session_id))
            .limit(QUERY_LIMIT)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut questions = Vec::new();
        for batch in &results {
            questions.extend(self.batch_to_questions(batch)?);
        }

        questions.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(questions)
    }

    fn batch_to_questions(&self, batch: &RecordBatch) -> Result<Vec<OpenQuestion>> {
        let ids = batch
            .column_by_name("id")
            .context("Missing id")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("id not StringArray")?;
        let question_texts = batch
            .column_by_name("question")
            .context("Missing question")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("question not StringArray")?;
        let sources = batch
            .column_by_name("source")
            .context("Missing source")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("source not StringArray")?;
        let contexts = batch
            .column_by_name("context")
            .context("Missing context")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("context not StringArray")?;
        let priorities = batch
            .column_by_name("priority")
            .context("Missing priority")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("priority not StringArray")?;
        let created_ats = batch
            .column_by_name("created_at_ms")
            .context("Missing created_at_ms")?
            .as_any()
            .downcast_ref::<Int64Array>()
            .context("created_at_ms not Int64Array")?;
        let answered_ats = batch
            .column_by_name("answered_at_ms")
            .context("Missing answered_at_ms")?
            .as_any()
            .downcast_ref::<Int64Array>()
            .context("answered_at_ms not Int64Array")?;
        let answers = batch
            .column_by_name("answer")
            .context("Missing answer")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("answer not StringArray")?;

        let mut questions = Vec::new();
        for i in 0..batch.num_rows() {
            let answered_at = if answered_ats.is_null(i) {
                None
            } else {
                Some(Utc.timestamp_millis_opt(answered_ats.value(i)).unwrap())
            };
            let answer = if answers.is_null(i) {
                None
            } else {
                Some(answers.value(i).to_string())
            };

            questions.push(OpenQuestion {
                id: Uuid::parse_str(ids.value(i))?,
                question: question_texts.value(i).to_string(),
                source: QuestionSource::from_str(sources.value(i)),
                context: contexts.value(i).to_string(),
                priority: QuestionPriority::from_str(priorities.value(i)),
                created_at: Utc.timestamp_millis_opt(created_ats.value(i)).unwrap(),
                answered_at,
                answer,
            });
        }

        Ok(questions)
    }

    // ========================================================================
    // Session State Reconstruction (backward compatibility)
    // ========================================================================

    /// Reconstruct a full SessionState from normalized tables
    pub async fn reconstruct_session_state(
        &self,
        session_id: Uuid,
    ) -> Result<Option<SessionState>> {
        // Get session metadata
        let session = match self.get_session(session_id).await? {
            Some(s) => s,
            None => return Ok(None),
        };

        // Get all entities
        let goals = self.get_goals_for_session(session_id).await?;
        let decisions = self.get_decisions_for_session(session_id).await?;
        let errors = self.get_errors_for_session(session_id).await?;
        let file_contexts = self.get_file_contexts_for_session(session_id).await?;
        let questions = self.get_questions_for_session(session_id).await?;

        Ok(Some(SessionState {
            session_id,
            updated_at: session.updated_at,
            goal_stack: goals,
            narrative: session.narrative,
            narrative_updated_at: session.narrative_updated_at,
            decisions,
            file_contexts,
            errors,
            open_questions: questions,
        }))
    }

    /// Get the latest state - first tries normalized tables, falls back to legacy
    pub async fn get_latest_state(&self, session_id: Uuid) -> Result<Option<SessionState>> {
        tracing::debug!(
            "[layer1-storage] Getting latest state for session {}",
            session_id
        );

        // First try to reconstruct from normalized tables
        if let Some(state) = self.reconstruct_session_state(session_id).await? {
            tracing::debug!("[layer1-storage] Reconstructed state from normalized tables");
            return Ok(Some(state));
        }

        // Fall back to legacy JSON snapshot
        self.get_latest_state_legacy(session_id).await
    }

    /// Legacy: Get state from JSON snapshots (for backward compat)
    async fn get_latest_state_legacy(&self, session_id: Uuid) -> Result<Option<SessionState>> {
        let table = self
            .states_table
            .as_ref()
            .context("States table not initialized")?;

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
            tracing::debug!(
                "[layer1-storage] No snapshots found for session {}",
                session_id
            );
            return Ok(None);
        }

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

    // ========================================================================
    // Legacy Methods (for backward compat during migration)
    // ========================================================================

    /// Save a session state snapshot (legacy - writes to both old and new tables)
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

    /// Get all state snapshots for a session (for debugging/history)
    pub async fn get_state_history(&self, session_id: Uuid) -> Result<Vec<StateSnapshot>> {
        let table = self
            .states_table
            .as_ref()
            .context("States table not initialized")?;

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

        snapshots.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(snapshots)
    }

    /// Get snapshot count for a session
    pub async fn snapshot_count(&self, session_id: Uuid) -> Result<usize> {
        let table = self
            .states_table
            .as_ref()
            .context("States table not initialized")?;

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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StateSnapshot {
    /// Unique identifier
    pub id: Uuid,
    /// When this snapshot was taken
    pub timestamp: DateTime<Utc>,
    /// The session state at this point
    pub state: SessionState,
    /// Why this snapshot was taken
    pub reason: String,
}

/// Metadata for a session (from l1_sessions table)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionMetadata {
    /// Session ID
    pub id: Uuid,
    /// When the session was created
    pub created_at: DateTime<Utc>,
    /// When the session was last updated
    pub updated_at: DateTime<Utc>,
    /// The initial user request that started this session
    pub initial_request: String,
    /// Current narrative summary
    pub narrative: String,
    /// When the narrative was last updated
    pub narrative_updated_at: DateTime<Utc>,
    /// Whether the session is currently active
    pub is_active: bool,
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

        let models_dir = temp_dir.path().join("models");
        let storage = Layer1Storage::new_with_models_dir(connection, models_dir)
            .await
            .unwrap();
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

//! Session management for markdown-based sidecar storage.
//!
//! Each session is stored as a directory containing:
//! - `meta.toml`: Machine-managed metadata
//! - `state.md`: LLM-managed current state
//! - `state.md.bak`: Previous state backup
//! - `log.md`: Append-only event log
//! - `events.jsonl`: Raw events (optional)

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use super::formats::{
    format_log_entry, initial_log_template, initial_state_template, SessionMetaToml,
};

/// Session status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Completed,
    Abandoned,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Active => write!(f, "active"),
            SessionStatus::Completed => write!(f, "completed"),
            SessionStatus::Abandoned => write!(f, "abandoned"),
        }
    }
}

impl std::str::FromStr for SessionStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(SessionStatus::Active),
            "completed" => Ok(SessionStatus::Completed),
            "abandoned" => Ok(SessionStatus::Abandoned),
            _ => anyhow::bail!("Invalid session status: {}", s),
        }
    }
}

/// Session metadata (mirrors meta.toml but with typed status)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: SessionStatus,
    pub cwd: PathBuf,
    pub git_root: Option<PathBuf>,
    pub git_branch: Option<String>,
    pub initial_request: String,
}

impl SessionMeta {
    /// Create new session metadata
    pub fn new(session_id: String, cwd: PathBuf, initial_request: String) -> Self {
        let now = Utc::now();
        Self {
            session_id,
            created_at: now,
            updated_at: now,
            status: SessionStatus::Active,
            cwd,
            git_root: None,
            git_branch: None,
            initial_request,
        }
    }
}

/// Manages a single session's files
pub struct Session {
    /// Session directory path
    dir: PathBuf,
    /// Session metadata
    meta: SessionMeta,
}

impl Session {
    /// File names
    const META_FILE: &'static str = "meta.toml";
    const STATE_FILE: &'static str = "state.md";
    const STATE_BACKUP: &'static str = "state.md.bak";
    const LOG_FILE: &'static str = "log.md";
    const EVENTS_FILE: &'static str = "events.jsonl";

    /// Create a new session
    pub async fn create(
        sessions_dir: &Path,
        session_id: String,
        cwd: PathBuf,
        initial_request: String,
    ) -> Result<Self> {
        let dir = sessions_dir.join(&session_id);

        // Create session directory
        fs::create_dir_all(&dir)
            .await
            .context("Failed to create session directory")?;

        // Create metadata
        let meta = SessionMeta::new(session_id.clone(), cwd, initial_request.clone());

        // Write meta.toml
        let meta_toml = SessionMetaToml::new(
            meta.session_id.clone(),
            meta.cwd.clone(),
            meta.initial_request.clone(),
        );
        let meta_content = meta_toml.to_toml()?;
        fs::write(dir.join(Self::META_FILE), &meta_content)
            .await
            .context("Failed to write meta.toml")?;

        // Write initial state.md
        let state_content = initial_state_template(&session_id, &initial_request);
        fs::write(dir.join(Self::STATE_FILE), &state_content)
            .await
            .context("Failed to write state.md")?;

        // Write initial log.md
        let log_content = initial_log_template(&session_id, &initial_request);
        fs::write(dir.join(Self::LOG_FILE), &log_content)
            .await
            .context("Failed to write log.md")?;

        // Create empty events.jsonl
        fs::write(dir.join(Self::EVENTS_FILE), "")
            .await
            .context("Failed to create events.jsonl")?;

        tracing::info!("Created new session: {}", session_id);

        Ok(Self { dir, meta })
    }

    /// Load an existing session
    pub async fn load(sessions_dir: &Path, session_id: &str) -> Result<Self> {
        let dir = sessions_dir.join(session_id);

        if !dir.exists() {
            anyhow::bail!("Session directory does not exist: {}", session_id);
        }

        // Read meta.toml
        let meta_path = dir.join(Self::META_FILE);
        let meta_content = fs::read_to_string(&meta_path)
            .await
            .context("Failed to read meta.toml")?;
        let meta_toml = SessionMetaToml::from_toml(&meta_content)?;

        let meta = SessionMeta {
            session_id: meta_toml.session_id,
            created_at: meta_toml.created_at,
            updated_at: meta_toml.updated_at,
            status: meta_toml.status.parse()?,
            cwd: meta_toml.context.cwd,
            git_root: meta_toml.context.git_root,
            git_branch: meta_toml.context.git_branch,
            initial_request: meta_toml.context.initial_request,
        };

        Ok(Self { dir, meta })
    }

    /// Get session metadata
    pub fn meta(&self) -> &SessionMeta {
        &self.meta
    }

    /// Read the current state.md content
    pub async fn read_state(&self) -> Result<String> {
        let path = self.dir.join(Self::STATE_FILE);
        fs::read_to_string(&path)
            .await
            .context("Failed to read state.md")
    }

    /// Read the log.md content
    pub async fn read_log(&self) -> Result<String> {
        let path = self.dir.join(Self::LOG_FILE);
        fs::read_to_string(&path)
            .await
            .context("Failed to read log.md")
    }

    /// Update state.md with backup
    pub async fn update_state(&self, new_state: &str) -> Result<()> {
        let state_path = self.dir.join(Self::STATE_FILE);
        let backup_path = self.dir.join(Self::STATE_BACKUP);

        // Backup current state
        if state_path.exists() {
            fs::copy(&state_path, &backup_path)
                .await
                .context("Failed to backup state.md")?;
        }

        // Write new state
        fs::write(&state_path, new_state)
            .await
            .context("Failed to write state.md")?;

        // Update meta timestamp
        self.touch_meta().await?;

        Ok(())
    }

    /// Append to log.md
    pub async fn append_log(&self, entry: &str) -> Result<()> {
        let path = self.dir.join(Self::LOG_FILE);
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .context("Failed to open log.md")?;

        file.write_all(entry.as_bytes())
            .await
            .context("Failed to append to log.md")?;

        Ok(())
    }

    /// Append a simple log entry
    pub async fn log_event(&self, event_type: &str, content: &str) -> Result<()> {
        let entry = format_log_entry(&Utc::now(), event_type, content);
        self.append_log(&entry).await
    }

    /// Append a raw event to events.jsonl
    pub async fn append_event(&self, event_json: &str) -> Result<()> {
        let path = self.dir.join(Self::EVENTS_FILE);
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .context("Failed to open events.jsonl")?;

        file.write_all(event_json.as_bytes())
            .await
            .context("Failed to write event")?;
        file.write_all(b"\n")
            .await
            .context("Failed to write newline")?;

        Ok(())
    }

    /// Mark session as completed
    pub async fn complete(&mut self) -> Result<()> {
        self.meta.status = SessionStatus::Completed;
        self.meta.updated_at = Utc::now();
        self.save_meta().await?;

        // Add completion entry to log
        self.log_event("Session End", "Session completed.").await?;

        tracing::info!("Session completed: {}", self.meta.session_id);
        Ok(())
    }

    /// Update the meta.toml timestamp
    async fn touch_meta(&self) -> Result<()> {
        let path = self.dir.join(Self::META_FILE);
        let content = fs::read_to_string(&path).await?;
        let mut meta_toml = SessionMetaToml::from_toml(&content)?;
        meta_toml.touch();
        fs::write(&path, meta_toml.to_toml()?).await?;
        Ok(())
    }

    /// Save meta.toml
    async fn save_meta(&self) -> Result<()> {
        let meta_toml = SessionMetaToml {
            session_id: self.meta.session_id.clone(),
            created_at: self.meta.created_at,
            updated_at: self.meta.updated_at,
            status: self.meta.status.to_string(),
            context: super::formats::SessionContextToml {
                cwd: self.meta.cwd.clone(),
                git_root: self.meta.git_root.clone(),
                git_branch: self.meta.git_branch.clone(),
                initial_request: self.meta.initial_request.clone(),
            },
        };
        let content = meta_toml.to_toml()?;
        fs::write(self.dir.join(Self::META_FILE), content).await?;
        Ok(())
    }
}

/// List all sessions in the sessions directory
pub async fn list_sessions(sessions_dir: &Path) -> Result<Vec<SessionMeta>> {
    let mut sessions = Vec::new();

    if !sessions_dir.exists() {
        return Ok(sessions);
    }

    let mut entries = fs::read_dir(sessions_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_dir() {
            let session_id = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();

            match Session::load(sessions_dir, session_id).await {
                Ok(session) => sessions.push(session.meta().clone()),
                Err(e) => {
                    tracing::warn!("Failed to load session {}: {}", session_id, e);
                }
            }
        }
    }

    // Sort by updated_at descending (most recent first)
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    Ok(sessions)
}

/// Get the default sessions directory
pub fn default_sessions_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".qbit")
        .join("sessions")
}

/// Ensure sessions directory exists
pub async fn ensure_sessions_dir(sessions_dir: &Path) -> Result<()> {
    fs::create_dir_all(sessions_dir)
        .await
        .context("Failed to create sessions directory")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn setup_test_dir() -> TempDir {
        TempDir::new().unwrap()
    }

    #[tokio::test]
    async fn test_session_create_and_load() {
        let temp = setup_test_dir().await;
        let sessions_dir = temp.path();

        // Create session
        let session = Session::create(
            sessions_dir,
            "test-session".to_string(),
            PathBuf::from("/home/user/project"),
            "Build something amazing".to_string(),
        )
        .await
        .unwrap();

        assert_eq!(session.meta().session_id, "test-session");
        assert_eq!(session.meta().status, SessionStatus::Active);

        // Verify files exist
        assert!(sessions_dir.join("test-session/meta.toml").exists());
        assert!(sessions_dir.join("test-session/state.md").exists());
        assert!(sessions_dir.join("test-session/log.md").exists());
        assert!(sessions_dir.join("test-session/events.jsonl").exists());

        // Load session
        let loaded = Session::load(sessions_dir, "test-session").await.unwrap();
        assert_eq!(loaded.meta().session_id, "test-session");
        assert_eq!(loaded.meta().initial_request, "Build something amazing");
    }

    #[tokio::test]
    async fn test_session_state_operations() {
        let temp = setup_test_dir().await;
        let sessions_dir = temp.path();

        let session = Session::create(
            sessions_dir,
            "state-test".to_string(),
            PathBuf::from("/tmp"),
            "Test state ops".to_string(),
        )
        .await
        .unwrap();

        // Read initial state
        let state = session.read_state().await.unwrap();
        assert!(state.contains("Test state ops"));

        // Update state
        let new_state = "# Updated State\n\nNew content here.";
        session.update_state(new_state).await.unwrap();

        // Verify update
        let updated = session.read_state().await.unwrap();
        assert_eq!(updated, new_state);

        // Verify backup exists
        assert!(sessions_dir.join("state-test/state.md.bak").exists());
    }

    #[tokio::test]
    async fn test_session_log_operations() {
        let temp = setup_test_dir().await;
        let sessions_dir = temp.path();

        let session = Session::create(
            sessions_dir,
            "log-test".to_string(),
            PathBuf::from("/tmp"),
            "Test log ops".to_string(),
        )
        .await
        .unwrap();

        // Append log entry
        session
            .log_event("File Read", "Read main.rs")
            .await
            .unwrap();
        session
            .log_event("Decision", "Chose approach A")
            .await
            .unwrap();

        // Read log
        let log = session.read_log().await.unwrap();
        assert!(log.contains("Session Start"));
        assert!(log.contains("File Read"));
        assert!(log.contains("Read main.rs"));
        assert!(log.contains("Decision"));
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let temp = setup_test_dir().await;
        let sessions_dir = temp.path();

        let mut session = Session::create(
            sessions_dir,
            "lifecycle-test".to_string(),
            PathBuf::from("/tmp"),
            "Test lifecycle".to_string(),
        )
        .await
        .unwrap();

        assert_eq!(session.meta().status, SessionStatus::Active);

        // Complete session
        session.complete().await.unwrap();
        assert_eq!(session.meta().status, SessionStatus::Completed);

        // Reload and verify
        let loaded = Session::load(sessions_dir, "lifecycle-test").await.unwrap();
        assert_eq!(loaded.meta().status, SessionStatus::Completed);
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let temp = setup_test_dir().await;
        let sessions_dir = temp.path();

        // Create multiple sessions
        Session::create(
            sessions_dir,
            "session-1".to_string(),
            PathBuf::from("/tmp"),
            "First".to_string(),
        )
        .await
        .unwrap();

        Session::create(
            sessions_dir,
            "session-2".to_string(),
            PathBuf::from("/tmp"),
            "Second".to_string(),
        )
        .await
        .unwrap();

        let sessions = list_sessions(sessions_dir).await.unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[tokio::test]
    async fn test_append_event() {
        let temp = setup_test_dir().await;
        let sessions_dir = temp.path();

        let session = Session::create(
            sessions_dir,
            "events-test".to_string(),
            PathBuf::from("/tmp"),
            "Test events".to_string(),
        )
        .await
        .unwrap();

        // Append events
        session
            .append_event(r#"{"type":"user_prompt","content":"hello"}"#)
            .await
            .unwrap();
        session
            .append_event(r#"{"type":"file_read","path":"main.rs"}"#)
            .await
            .unwrap();

        // Read events file
        let events_content = fs::read_to_string(sessions_dir.join("events-test/events.jsonl"))
            .await
            .unwrap();

        let lines: Vec<&str> = events_content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("user_prompt"));
        assert!(lines[1].contains("file_read"));
    }
}

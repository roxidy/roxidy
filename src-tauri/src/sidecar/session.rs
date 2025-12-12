//! Session management for simplified sidecar storage.
//!
//! Each session is stored as a directory containing:
//! - `state.md`: YAML frontmatter (metadata) + markdown body (context)
//! - `patches/staged/`: Pending patches in git format-patch style
//! - `patches/applied/`: Applied patches (moved after git am)

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

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

/// Session metadata (stored in YAML frontmatter of state.md)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: SessionStatus,
    pub cwd: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_root: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
    /// File/directory names
    const STATE_FILE: &'static str = "state.md";
    const LOG_FILE: &'static str = "log.md";
    const PATCHES_DIR: &'static str = "patches";
    const ARTIFACTS_DIR: &'static str = "artifacts";
    const STAGED_DIR: &'static str = "staged";
    const PENDING_DIR: &'static str = "pending";
    const APPLIED_DIR: &'static str = "applied";

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

        // Create patches directories
        fs::create_dir_all(dir.join(Self::PATCHES_DIR).join(Self::STAGED_DIR))
            .await
            .context("Failed to create staged patches directory")?;
        fs::create_dir_all(dir.join(Self::PATCHES_DIR).join(Self::APPLIED_DIR))
            .await
            .context("Failed to create applied patches directory")?;

        // Create artifacts directories (L3)
        fs::create_dir_all(dir.join(Self::ARTIFACTS_DIR).join(Self::PENDING_DIR))
            .await
            .context("Failed to create pending artifacts directory")?;
        fs::create_dir_all(dir.join(Self::ARTIFACTS_DIR).join(Self::APPLIED_DIR))
            .await
            .context("Failed to create applied artifacts directory")?;

        // Create metadata
        let meta = SessionMeta::new(session_id.clone(), cwd, initial_request.clone());

        // Write initial state.md with frontmatter
        let state_content = Self::format_state_file(&meta, &initial_state_body(&initial_request));
        fs::write(dir.join(Self::STATE_FILE), &state_content)
            .await
            .context("Failed to write state.md")?;

        // Write initial log.md (append-only event log)
        let log_content = format!(
            "# Session Log\n\n> Session started: {}\n\n",
            meta.created_at.format("%Y-%m-%d %H:%M:%S UTC")
        );
        fs::write(dir.join(Self::LOG_FILE), &log_content)
            .await
            .context("Failed to write log.md")?;

        tracing::info!("Created new session: {}", session_id);

        Ok(Self { dir, meta })
    }

    /// Load an existing session
    pub async fn load(sessions_dir: &Path, session_id: &str) -> Result<Self> {
        let dir = sessions_dir.join(session_id);

        if !dir.exists() {
            anyhow::bail!("Session directory does not exist: {}", session_id);
        }

        // Read state.md and parse frontmatter
        let state_path = dir.join(Self::STATE_FILE);
        let content = fs::read_to_string(&state_path)
            .await
            .context("Failed to read state.md")?;

        let (meta, _body) = Self::parse_state_file(&content)?;

        Ok(Self { dir, meta })
    }

    /// Get session metadata
    pub fn meta(&self) -> &SessionMeta {
        &self.meta
    }

    /// Get the session directory path
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Get the staged patches directory path
    #[allow(dead_code)]
    pub fn staged_patches_dir(&self) -> PathBuf {
        self.dir.join(Self::PATCHES_DIR).join(Self::STAGED_DIR)
    }

    /// Get the applied patches directory path
    #[allow(dead_code)]
    pub fn applied_patches_dir(&self) -> PathBuf {
        self.dir.join(Self::PATCHES_DIR).join(Self::APPLIED_DIR)
    }

    /// Get the pending artifacts directory path (L3)
    #[allow(dead_code)]
    pub fn pending_artifacts_dir(&self) -> PathBuf {
        self.dir.join(Self::ARTIFACTS_DIR).join(Self::PENDING_DIR)
    }

    /// Get the applied artifacts directory path (L3)
    #[allow(dead_code)]
    pub fn applied_artifacts_dir(&self) -> PathBuf {
        self.dir.join(Self::ARTIFACTS_DIR).join(Self::APPLIED_DIR)
    }

    /// Read the current state.md content (body only, without frontmatter)
    pub async fn read_state(&self) -> Result<String> {
        let path = self.dir.join(Self::STATE_FILE);
        let content = fs::read_to_string(&path)
            .await
            .context("Failed to read state.md")?;

        let (_meta, body) = Self::parse_state_file(&content)?;
        Ok(body)
    }

    /// Read the full state.md content (frontmatter + body)
    #[allow(dead_code)]
    pub async fn read_state_full(&self) -> Result<String> {
        let path = self.dir.join(Self::STATE_FILE);
        fs::read_to_string(&path)
            .await
            .context("Failed to read state.md")
    }

    /// Read the log.md content (append-only event log)
    pub async fn read_log(&self) -> Result<String> {
        let path = self.dir.join(Self::LOG_FILE);
        if path.exists() {
            fs::read_to_string(&path)
                .await
                .context("Failed to read log.md")
        } else {
            // Return empty string if log doesn't exist (backwards compatibility)
            Ok(String::new())
        }
    }

    /// Append an entry to log.md
    pub async fn append_log(&self, entry: &str) -> Result<()> {
        use tokio::io::AsyncWriteExt;

        let path = self.dir.join(Self::LOG_FILE);
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let formatted = format!("\n---\n\n**{}**\n\n{}\n", timestamp, entry);

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .context("Failed to open log.md")?;

        file.write_all(formatted.as_bytes())
            .await
            .context("Failed to append to log.md")?;

        Ok(())
    }

    /// Update state.md body (preserves and updates frontmatter)
    pub async fn update_state(&mut self, new_body: &str) -> Result<()> {
        // Update timestamp
        self.meta.updated_at = Utc::now();

        // Write new state file
        let content = Self::format_state_file(&self.meta, new_body);
        fs::write(self.dir.join(Self::STATE_FILE), &content)
            .await
            .context("Failed to write state.md")?;

        Ok(())
    }

    /// Mark session as completed
    pub async fn complete(&mut self) -> Result<()> {
        self.meta.status = SessionStatus::Completed;
        self.meta.updated_at = Utc::now();

        // Re-read current body and save with updated metadata
        let body = self.read_state().await.unwrap_or_default();
        let content = Self::format_state_file(&self.meta, &body);
        fs::write(self.dir.join(Self::STATE_FILE), &content)
            .await
            .context("Failed to write state.md")?;

        tracing::info!("Session completed: {}", self.meta.session_id);
        Ok(())
    }

    /// Format state.md with YAML frontmatter
    fn format_state_file(meta: &SessionMeta, body: &str) -> String {
        let yaml = serde_yaml::to_string(meta).unwrap_or_default();
        format!("---\n{}---\n\n{}", yaml, body)
    }

    /// Parse state.md into metadata and body
    fn parse_state_file(content: &str) -> Result<(SessionMeta, String)> {
        // Check for frontmatter
        if !content.starts_with("---\n") {
            anyhow::bail!("state.md missing YAML frontmatter");
        }

        // Find end of frontmatter
        let rest = &content[4..]; // Skip opening "---\n"
        let end_idx = rest
            .find("\n---")
            .context("state.md missing frontmatter closing delimiter")?;

        let yaml_content = &rest[..end_idx];
        let body_start = end_idx + 4; // Skip "\n---"

        // Skip any leading newlines in body
        let body = rest[body_start..].trim_start_matches('\n').to_string();

        // Parse YAML
        let meta: SessionMeta =
            serde_yaml::from_str(yaml_content).context("Failed to parse state.md frontmatter")?;

        Ok((meta, body))
    }
}

/// Generate initial state body
fn initial_state_body(initial_request: &str) -> String {
    format!(
        r#"# Goal
{}

## Progress
Session started.

## Files
(none yet)

## Open Questions
(none yet)
"#,
        initial_request
    )
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
        assert!(sessions_dir.join("test-session/state.md").exists());
        assert!(sessions_dir.join("test-session/patches/staged").exists());
        assert!(sessions_dir.join("test-session/patches/applied").exists());
        // L3 artifact directories
        assert!(sessions_dir.join("test-session/artifacts/pending").exists());
        assert!(sessions_dir.join("test-session/artifacts/applied").exists());

        // Load session
        let loaded = Session::load(sessions_dir, "test-session").await.unwrap();
        assert_eq!(loaded.meta().session_id, "test-session");
        assert_eq!(loaded.meta().initial_request, "Build something amazing");
    }

    #[tokio::test]
    async fn test_session_state_operations() {
        let temp = setup_test_dir().await;
        let sessions_dir = temp.path();

        let mut session = Session::create(
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
        assert!(state.contains("# Goal"));

        // Update state
        let new_body = "# Goal\nUpdated goal\n\n## Progress\nMade progress!";
        session.update_state(new_body).await.unwrap();

        // Verify update
        let updated = session.read_state().await.unwrap();
        assert!(updated.contains("Updated goal"));
        assert!(updated.contains("Made progress!"));
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
    async fn test_frontmatter_parsing() {
        let content = r#"---
session_id: test-123
created_at: 2025-12-10T14:30:00Z
updated_at: 2025-12-10T15:00:00Z
status: active
cwd: /home/user/project
initial_request: Build something
---

# Goal
Build something

## Progress
Working on it.
"#;

        let (meta, body) = Session::parse_state_file(content).unwrap();
        assert_eq!(meta.session_id, "test-123");
        assert_eq!(meta.status, SessionStatus::Active);
        assert!(body.contains("# Goal"));
        assert!(body.contains("Working on it."));
    }
}

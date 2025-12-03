//! Session persistence module for Qbit AI conversations.
//!
//! This module provides session archiving, conversation logs, and transcript export
//! capabilities by integrating with vtcode-core's session_archive system.

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rig::completion::{AssistantContent, Message};
use rig::message::UserContent;
use rig::one_or_many::OneOrMany;
use serde::{Deserialize, Serialize};

use vtcode_core::utils::session_archive::{
    self, SessionArchive, SessionArchiveMetadata, SessionMessage,
};

/// Role of a message in the conversation (simplified for Qbit).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum QbitMessageRole {
    User,
    Assistant,
    System,
    Tool,
}

/// A simplified message format for Qbit sessions.
/// This provides a bridge between rig's Message type and vtcode's SessionMessage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QbitSessionMessage {
    pub role: QbitMessageRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

impl QbitSessionMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: QbitMessageRole::User,
            content: content.into(),
            tool_call_id: None,
            tool_name: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: QbitMessageRole::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_name: None,
        }
    }

    #[allow(dead_code)]
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: QbitMessageRole::System,
            content: content.into(),
            tool_call_id: None,
            tool_name: None,
        }
    }

    pub fn tool_use(tool_name: impl Into<String>, result: impl Into<String>) -> Self {
        let tool_name = tool_name.into();
        Self {
            role: QbitMessageRole::Tool,
            content: result.into(),
            tool_call_id: None,
            tool_name: Some(tool_name),
        }
    }

    #[allow(dead_code)]
    pub fn tool_result(content: impl Into<String>, tool_call_id: impl Into<String>) -> Self {
        Self {
            role: QbitMessageRole::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_name: None,
        }
    }
}

/// Convert rig Message to QbitSessionMessage for persistence.
impl From<&Message> for QbitSessionMessage {
    fn from(message: &Message) -> Self {
        match message {
            Message::User { content } => {
                let text = content
                    .iter()
                    .filter_map(|c| match c {
                        rig::message::UserContent::Text(t) => Some(t.text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Self::user(text)
            }
            Message::Assistant { content, .. } => {
                let text = content
                    .iter()
                    .filter_map(|c| match c {
                        rig::completion::AssistantContent::Text(t) => Some(t.text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Self::assistant(text)
            }
        }
    }
}

impl QbitSessionMessage {
    /// Convert QbitSessionMessage back to rig Message for restoring sessions.
    /// Note: Tool messages are converted to assistant messages since rig's Message
    /// enum only supports User and Assistant variants for chat history.
    pub fn to_rig_message(&self) -> Option<Message> {
        match self.role {
            QbitMessageRole::User => Some(Message::User {
                content: OneOrMany::one(UserContent::Text(rig::message::Text {
                    text: self.content.clone(),
                })),
            }),
            QbitMessageRole::Assistant => Some(Message::Assistant {
                id: None,
                content: OneOrMany::one(AssistantContent::Text(rig::message::Text {
                    text: self.content.clone(),
                })),
            }),
            // System and Tool messages cannot be directly represented in rig's Message enum
            // for chat history, so we skip them (they were already processed)
            QbitMessageRole::System | QbitMessageRole::Tool => None,
        }
    }
}

/// Qbit session snapshot containing conversation data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QbitSessionSnapshot {
    /// Session metadata
    pub workspace_label: String,
    pub workspace_path: String,
    pub model: String,
    pub provider: String,

    /// Timestamps
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,

    /// Session statistics
    pub total_messages: usize,
    pub distinct_tools: Vec<String>,

    /// Human-readable transcript lines
    pub transcript: Vec<String>,

    /// Full message history
    pub messages: Vec<QbitSessionMessage>,
}

/// Active session manager for creating and finalizing session archives.
pub struct QbitSessionManager {
    archive: Option<SessionArchive>,
    #[allow(dead_code)]
    workspace_label: String,
    #[allow(dead_code)]
    workspace_path: PathBuf,
    #[allow(dead_code)]
    model: String,
    #[allow(dead_code)]
    provider: String,
    messages: Vec<QbitSessionMessage>,
    tools_used: std::collections::HashSet<String>,
    transcript: Vec<String>,
}

impl QbitSessionManager {
    /// Create a new session manager.
    pub async fn new(
        workspace_path: PathBuf,
        model: impl Into<String>,
        provider: impl Into<String>,
    ) -> Result<Self> {
        let workspace_label = workspace_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace")
            .to_string();

        let model = model.into();
        let provider = provider.into();

        let metadata = SessionArchiveMetadata::new(
            &workspace_label,
            workspace_path.display().to_string(),
            &model,
            &provider,
            "default",  // theme
            "standard", // reasoning_effort
        );

        let archive = SessionArchive::new(metadata)
            .await
            .context("Failed to create session archive")?;

        Ok(Self {
            archive: Some(archive),
            workspace_label,
            workspace_path,
            model,
            provider,
            messages: Vec::new(),
            tools_used: std::collections::HashSet::new(),
            transcript: Vec::new(),
        })
    }

    /// Record a user message.
    pub fn add_user_message(&mut self, content: &str) {
        self.messages.push(QbitSessionMessage::user(content));
        self.transcript
            .push(format!("User: {}", truncate(content, 200)));
    }

    /// Record an assistant message.
    pub fn add_assistant_message(&mut self, content: &str) {
        self.messages.push(QbitSessionMessage::assistant(content));
        self.transcript
            .push(format!("Assistant: {}", truncate(content, 200)));
    }

    /// Record a tool use.
    pub fn add_tool_use(&mut self, tool_name: &str, result: &str) {
        self.tools_used.insert(tool_name.to_string());
        self.messages
            .push(QbitSessionMessage::tool_use(tool_name, result));
        self.transcript
            .push(format!("Tool[{}]: {}", tool_name, truncate(result, 100)));
    }

    /// Convert rig Messages to session messages.
    #[allow(dead_code)]
    pub fn add_rig_messages(&mut self, messages: &[Message]) {
        for msg in messages {
            let qbit_msg = QbitSessionMessage::from(msg);
            match &qbit_msg.role {
                QbitMessageRole::User => {
                    self.transcript
                        .push(format!("User: {}", truncate(&qbit_msg.content, 200)));
                }
                QbitMessageRole::Assistant => {
                    self.transcript
                        .push(format!("Assistant: {}", truncate(&qbit_msg.content, 200)));
                }
                _ => {}
            }
            self.messages.push(qbit_msg);
        }
    }

    /// Save the current session state to disk without finalizing.
    /// This allows incremental saves after each message.
    ///
    /// Returns the path to the saved session file.
    pub fn save(&self) -> Result<PathBuf> {
        let archive = self.archive.as_ref().context("Session already finalized")?;

        // Convert QbitSessionMessages to vtcode SessionMessages
        let vtcode_messages: Vec<SessionMessage> = self
            .messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    QbitMessageRole::User => vtcode_core::llm::provider::MessageRole::User,
                    QbitMessageRole::Assistant => {
                        vtcode_core::llm::provider::MessageRole::Assistant
                    }
                    QbitMessageRole::System => vtcode_core::llm::provider::MessageRole::System,
                    QbitMessageRole::Tool => vtcode_core::llm::provider::MessageRole::Tool,
                };
                SessionMessage::with_tool_call_id(role, &m.content, m.tool_call_id.clone())
            })
            .collect();

        let distinct_tools: Vec<String> = self.tools_used.iter().cloned().collect();

        archive
            .finalize(
                self.transcript.clone(),
                self.messages.len(),
                distinct_tools,
                vtcode_messages,
            )
            .context("Failed to save session archive")
    }

    /// Finalize the session and save to disk.
    /// After this, the session cannot be updated further.
    ///
    /// Returns the path to the saved session file.
    pub fn finalize(&mut self) -> Result<PathBuf> {
        let archive = self.archive.take().context("Session already finalized")?;

        // Convert QbitSessionMessages to vtcode SessionMessages
        let vtcode_messages: Vec<SessionMessage> = self
            .messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    QbitMessageRole::User => vtcode_core::llm::provider::MessageRole::User,
                    QbitMessageRole::Assistant => {
                        vtcode_core::llm::provider::MessageRole::Assistant
                    }
                    QbitMessageRole::System => vtcode_core::llm::provider::MessageRole::System,
                    QbitMessageRole::Tool => vtcode_core::llm::provider::MessageRole::Tool,
                };
                SessionMessage::with_tool_call_id(role, &m.content, m.tool_call_id.clone())
            })
            .collect();

        let distinct_tools: Vec<String> = self.tools_used.iter().cloned().collect();

        archive
            .finalize(
                self.transcript.clone(),
                self.messages.len(),
                distinct_tools,
                vtcode_messages,
            )
            .context("Failed to finalize session archive")
    }

    /// Get the current message count.
    #[allow(dead_code)]
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Get the tools used in this session.
    #[allow(dead_code)]
    pub fn tools_used(&self) -> Vec<String> {
        self.tools_used.iter().cloned().collect()
    }

    /// Get the workspace path.
    #[allow(dead_code)]
    pub fn workspace_path(&self) -> &PathBuf {
        &self.workspace_path
    }
}

/// List recent sessions.
///
/// # Arguments
/// * `limit` - Maximum number of sessions to return (0 for all)
pub async fn list_recent_sessions(limit: usize) -> Result<Vec<SessionListingInfo>> {
    let listings = session_archive::list_recent_sessions(limit).await?;

    Ok(listings
        .into_iter()
        .map(|listing| SessionListingInfo {
            identifier: listing.identifier(),
            path: listing.path.clone(),
            workspace_label: listing.snapshot.metadata.workspace_label.clone(),
            workspace_path: listing.snapshot.metadata.workspace_path.clone(),
            model: listing.snapshot.metadata.model.clone(),
            provider: listing.snapshot.metadata.provider.clone(),
            started_at: listing.snapshot.started_at,
            ended_at: listing.snapshot.ended_at,
            total_messages: listing.snapshot.total_messages,
            distinct_tools: listing.snapshot.distinct_tools.clone(),
            first_prompt_preview: listing.first_prompt_preview(),
            first_reply_preview: listing.first_reply_preview(),
        })
        .collect())
}

/// Find a session by its identifier.
pub async fn find_session(identifier: &str) -> Result<Option<SessionListingInfo>> {
    let listing = session_archive::find_session_by_identifier(identifier).await?;

    Ok(listing.map(|l| SessionListingInfo {
        identifier: l.identifier(),
        path: l.path.clone(),
        workspace_label: l.snapshot.metadata.workspace_label.clone(),
        workspace_path: l.snapshot.metadata.workspace_path.clone(),
        model: l.snapshot.metadata.model.clone(),
        provider: l.snapshot.metadata.provider.clone(),
        started_at: l.snapshot.started_at,
        ended_at: l.snapshot.ended_at,
        total_messages: l.snapshot.total_messages,
        distinct_tools: l.snapshot.distinct_tools.clone(),
        first_prompt_preview: l.first_prompt_preview(),
        first_reply_preview: l.first_reply_preview(),
    }))
}

/// Load a full session by identifier.
pub async fn load_session(identifier: &str) -> Result<Option<QbitSessionSnapshot>> {
    let listing = session_archive::find_session_by_identifier(identifier).await?;

    Ok(listing.map(|l| {
        let messages = l
            .snapshot
            .messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    vtcode_core::llm::provider::MessageRole::User => QbitMessageRole::User,
                    vtcode_core::llm::provider::MessageRole::Assistant => {
                        QbitMessageRole::Assistant
                    }
                    vtcode_core::llm::provider::MessageRole::System => QbitMessageRole::System,
                    vtcode_core::llm::provider::MessageRole::Tool => QbitMessageRole::Tool,
                };
                QbitSessionMessage {
                    role,
                    content: m.content.as_text().to_string(),
                    tool_call_id: m.tool_call_id.clone(),
                    tool_name: None,
                }
            })
            .collect();

        QbitSessionSnapshot {
            workspace_label: l.snapshot.metadata.workspace_label,
            workspace_path: l.snapshot.metadata.workspace_path,
            model: l.snapshot.metadata.model,
            provider: l.snapshot.metadata.provider,
            started_at: l.snapshot.started_at,
            ended_at: l.snapshot.ended_at,
            total_messages: l.snapshot.total_messages,
            distinct_tools: l.snapshot.distinct_tools,
            transcript: l.snapshot.transcript,
            messages,
        }
    }))
}

/// Session listing information for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListingInfo {
    pub identifier: String,
    pub path: PathBuf,
    pub workspace_label: String,
    pub workspace_path: String,
    pub model: String,
    pub provider: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub total_messages: usize,
    pub distinct_tools: Vec<String>,
    pub first_prompt_preview: Option<String>,
    pub first_reply_preview: Option<String>,
}

/// Truncate a string to a maximum length.
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
    use rig::message::Text;
    use tempfile::TempDir;

    #[test]
    fn test_qbit_session_message_creation() {
        let user_msg = QbitSessionMessage::user("Hello");
        assert_eq!(user_msg.role, QbitMessageRole::User);
        assert_eq!(user_msg.content, "Hello");

        let assistant_msg = QbitSessionMessage::assistant("Hi there");
        assert_eq!(assistant_msg.role, QbitMessageRole::Assistant);
        assert_eq!(assistant_msg.content, "Hi there");
    }

    #[test]
    fn test_qbit_session_message_system() {
        let system_msg = QbitSessionMessage::system("You are a helpful assistant");
        assert_eq!(system_msg.role, QbitMessageRole::System);
        assert_eq!(system_msg.content, "You are a helpful assistant");
        assert!(system_msg.tool_call_id.is_none());
        assert!(system_msg.tool_name.is_none());
    }

    #[test]
    fn test_qbit_session_message_tool_result() {
        let tool_msg = QbitSessionMessage::tool_result("File contents here", "call_123");
        assert_eq!(tool_msg.role, QbitMessageRole::Tool);
        assert_eq!(tool_msg.content, "File contents here");
        assert_eq!(tool_msg.tool_call_id, Some("call_123".to_string()));
        assert!(tool_msg.tool_name.is_none());
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("a longer string", 5), "a lo…");
        assert_eq!(truncate("", 10), "");
    }

    #[test]
    fn test_truncate_exact_length() {
        assert_eq!(truncate("12345", 5), "12345");
        assert_eq!(truncate("123456", 5), "1234…");
    }

    #[test]
    fn test_truncate_unicode() {
        // Unicode characters should be counted as single chars
        assert_eq!(truncate("héllo", 5), "héllo");
        assert_eq!(truncate("héllo world", 5), "héll…");
    }

    #[test]
    fn test_rig_message_conversion_user() {
        let rig_msg = Message::User {
            content: OneOrMany::one(UserContent::Text(Text {
                text: "Hello from user".to_string(),
            })),
        };

        let qbit_msg = QbitSessionMessage::from(&rig_msg);
        assert_eq!(qbit_msg.role, QbitMessageRole::User);
        assert_eq!(qbit_msg.content, "Hello from user");
    }

    #[test]
    fn test_rig_message_conversion_assistant() {
        let rig_msg = Message::Assistant {
            id: None,
            content: OneOrMany::one(AssistantContent::Text(Text {
                text: "Hello from assistant".to_string(),
            })),
        };

        let qbit_msg = QbitSessionMessage::from(&rig_msg);
        assert_eq!(qbit_msg.role, QbitMessageRole::Assistant);
        assert_eq!(qbit_msg.content, "Hello from assistant");
    }

    #[test]
    fn test_qbit_message_to_rig_user() {
        let qbit_msg = QbitSessionMessage::user("Test user message");
        let rig_msg = qbit_msg.to_rig_message();

        assert!(rig_msg.is_some());
        let rig_msg = rig_msg.unwrap();
        match rig_msg {
            Message::User { content } => {
                let text = content
                    .iter()
                    .filter_map(|c| match c {
                        UserContent::Text(t) => Some(t.text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");
                assert_eq!(text, "Test user message");
            }
            _ => panic!("Expected User message"),
        }
    }

    #[test]
    fn test_qbit_message_to_rig_assistant() {
        let qbit_msg = QbitSessionMessage::assistant("Test assistant message");
        let rig_msg = qbit_msg.to_rig_message();

        assert!(rig_msg.is_some());
        let rig_msg = rig_msg.unwrap();
        match rig_msg {
            Message::Assistant { content, .. } => {
                let text = content
                    .iter()
                    .filter_map(|c| match c {
                        AssistantContent::Text(t) => Some(t.text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");
                assert_eq!(text, "Test assistant message");
            }
            _ => panic!("Expected Assistant message"),
        }
    }

    #[test]
    fn test_qbit_message_to_rig_system_returns_none() {
        let qbit_msg = QbitSessionMessage::system("System prompt");
        assert!(qbit_msg.to_rig_message().is_none());
    }

    #[test]
    fn test_qbit_message_to_rig_tool_returns_none() {
        let qbit_msg = QbitSessionMessage::tool_result("Result", "call_id");
        assert!(qbit_msg.to_rig_message().is_none());
    }

    #[test]
    fn test_qbit_session_snapshot_serialization() {
        let snapshot = QbitSessionSnapshot {
            workspace_label: "test-workspace".to_string(),
            workspace_path: "/tmp/test".to_string(),
            model: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            started_at: Utc::now(),
            ended_at: Utc::now(),
            total_messages: 2,
            distinct_tools: vec!["read_file".to_string(), "write_file".to_string()],
            transcript: vec!["User: Hello".to_string(), "Assistant: Hi".to_string()],
            messages: vec![
                QbitSessionMessage::user("Hello"),
                QbitSessionMessage::assistant("Hi"),
            ],
        };

        let json = serde_json::to_string(&snapshot).expect("Failed to serialize");
        let deserialized: QbitSessionSnapshot =
            serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(deserialized.workspace_label, "test-workspace");
        assert_eq!(deserialized.total_messages, 2);
        assert_eq!(deserialized.messages.len(), 2);
        assert_eq!(deserialized.distinct_tools.len(), 2);
    }

    #[test]
    fn test_session_listing_info_serialization() {
        let info = SessionListingInfo {
            identifier: "session-test-123".to_string(),
            path: PathBuf::from("/tmp/sessions/session-test-123.json"),
            workspace_label: "my-project".to_string(),
            workspace_path: "/home/user/my-project".to_string(),
            model: "claude-3-opus".to_string(),
            provider: "anthropic".to_string(),
            started_at: Utc::now(),
            ended_at: Utc::now(),
            total_messages: 10,
            distinct_tools: vec!["bash".to_string()],
            first_prompt_preview: Some("Help me debug...".to_string()),
            first_reply_preview: Some("I'd be happy to help...".to_string()),
        };

        let json = serde_json::to_string(&info).expect("Failed to serialize");
        let deserialized: SessionListingInfo =
            serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(deserialized.identifier, "session-test-123");
        assert_eq!(deserialized.workspace_label, "my-project");
        assert_eq!(
            deserialized.first_prompt_preview,
            Some("Help me debug...".to_string())
        );
    }

    #[test]
    fn test_qbit_message_role_serialization() {
        // Test that roles serialize to lowercase as expected
        let user_msg = QbitSessionMessage::user("test");
        let json = serde_json::to_string(&user_msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));

        let assistant_msg = QbitSessionMessage::assistant("test");
        let json = serde_json::to_string(&assistant_msg).unwrap();
        assert!(json.contains("\"role\":\"assistant\""));

        let system_msg = QbitSessionMessage::system("test");
        let json = serde_json::to_string(&system_msg).unwrap();
        assert!(json.contains("\"role\":\"system\""));

        let tool_msg = QbitSessionMessage::tool_result("test", "id");
        let json = serde_json::to_string(&tool_msg).unwrap();
        assert!(json.contains("\"role\":\"tool\""));
    }

    #[test]
    fn test_qbit_message_optional_fields_skip_when_none() {
        let msg = QbitSessionMessage::user("Hello");
        let json = serde_json::to_string(&msg).unwrap();

        // tool_call_id and tool_name should not appear when None
        assert!(!json.contains("tool_call_id"));
        assert!(!json.contains("tool_name"));
    }

    #[test]
    fn test_qbit_message_includes_tool_call_id_when_present() {
        let msg = QbitSessionMessage::tool_result("result", "call_abc");
        let json = serde_json::to_string(&msg).unwrap();

        assert!(json.contains("\"tool_call_id\":\"call_abc\""));
    }

    // Note: The async tests that interact with the filesystem via vtcode-core's
    // session_archive are integration tests that depend on the VT_SESSION_DIR
    // environment variable. These tests are difficult to run in parallel because
    // they share global state. For comprehensive session persistence testing,
    // see the integration tests or run these with --test-threads=1.
    //
    // The tests below focus on unit-level functionality that doesn't require
    // filesystem isolation.

    #[tokio::test]
    async fn test_session_manager_creation() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Set VT_SESSION_DIR for this test
        std::env::set_var("VT_SESSION_DIR", temp_dir.path());

        let manager =
            QbitSessionManager::new(temp_dir.path().to_path_buf(), "test-model", "test-provider")
                .await;

        assert!(manager.is_ok());
        let manager = manager.unwrap();
        assert_eq!(manager.message_count(), 0);
        assert!(manager.tools_used().is_empty());

        // Clean up
        std::env::remove_var("VT_SESSION_DIR");
    }

    #[tokio::test]
    async fn test_session_manager_add_messages() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        std::env::set_var("VT_SESSION_DIR", temp_dir.path());

        let mut manager =
            QbitSessionManager::new(temp_dir.path().to_path_buf(), "test-model", "test-provider")
                .await
                .expect("Failed to create manager");

        manager.add_user_message("Hello, how are you?");
        assert_eq!(manager.message_count(), 1);

        manager.add_assistant_message("I'm doing well, thank you!");
        assert_eq!(manager.message_count(), 2);

        manager.add_tool_use("read_file", "File contents: hello world");
        assert_eq!(manager.message_count(), 3);
        assert!(manager.tools_used().contains(&"read_file".to_string()));

        std::env::remove_var("VT_SESSION_DIR");
    }

    #[tokio::test]
    async fn test_session_manager_tools_tracking() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        std::env::set_var("VT_SESSION_DIR", temp_dir.path());

        let mut manager =
            QbitSessionManager::new(temp_dir.path().to_path_buf(), "test-model", "test-provider")
                .await
                .expect("Failed to create manager");

        manager.add_tool_use("read_file", "contents");
        manager.add_tool_use("write_file", "success");
        manager.add_tool_use("read_file", "more contents"); // Duplicate tool

        let tools = manager.tools_used();
        assert_eq!(tools.len(), 2); // Should dedupe
        assert!(tools.contains(&"read_file".to_string()));
        assert!(tools.contains(&"write_file".to_string()));

        std::env::remove_var("VT_SESSION_DIR");
    }

    #[tokio::test]
    async fn test_list_empty_sessions_dir() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        std::env::set_var("VT_SESSION_DIR", temp_dir.path());

        let sessions = list_recent_sessions(10).await.expect("Failed to list");
        assert!(sessions.is_empty());

        std::env::remove_var("VT_SESSION_DIR");
    }

    #[tokio::test]
    async fn test_list_recent_sessions_with_limit() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        std::env::set_var("VT_SESSION_DIR", temp_dir.path());

        // Create 5 sessions
        for i in 0..5 {
            let mut manager = QbitSessionManager::new(
                temp_dir.path().to_path_buf(),
                format!("model-{}", i),
                "provider",
            )
            .await
            .expect("Failed to create manager");

            manager.add_user_message(&format!("Message {}", i));
            manager.finalize().expect("Failed to finalize");
        }

        let sessions = list_recent_sessions(2).await.expect("Failed to list");
        assert_eq!(sessions.len(), 2);

        std::env::remove_var("VT_SESSION_DIR");
    }

    #[test]
    fn test_session_message_roundtrip() {
        // Test that messages survive serialization roundtrip
        let original = QbitSessionMessage {
            role: QbitMessageRole::Tool,
            content: "Tool result with special chars: <>&\"'".to_string(),
            tool_call_id: Some("call_123".to_string()),
            tool_name: Some("read_file".to_string()),
        };

        let json = serde_json::to_string(&original).unwrap();
        let restored: QbitSessionMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.role, original.role);
        assert_eq!(restored.content, original.content);
        assert_eq!(restored.tool_call_id, original.tool_call_id);
        assert_eq!(restored.tool_name, original.tool_name);
    }
}

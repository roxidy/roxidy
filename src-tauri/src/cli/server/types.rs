//! Request/Response types for the HTTP server.
//!
//! These types define the wire format for all HTTP API endpoints.
//! They are designed to be clear, minimal, and easy to use from Python clients.

use serde::{Deserialize, Serialize};

/// Health check response (API-1, API-2)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthResponse {
    /// Server status (always "ok" when healthy)
    pub status: String,
    /// Server version from Cargo.toml
    pub version: String,
}

impl HealthResponse {
    /// Create a healthy response with the current package version
    pub fn healthy() -> Self {
        Self {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Request to create a new session (API-3)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateSessionRequest {
    /// Workspace path (defaults to server's default workspace)
    #[serde(default)]
    pub workspace: Option<String>,

    /// Auto-approve tool calls (defaults to true for evals)
    #[serde(default)]
    pub auto_approve: Option<bool>,
}

/// Response after creating a session (API-4)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateSessionResponse {
    /// Server-generated session ID (UUID v4)
    pub session_id: String,
    /// Creation timestamp in RFC 3339 format
    pub created_at: String,
}

/// Information about a session (API-5)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionInfo {
    /// Session ID
    pub id: String,
    /// Milliseconds since session creation
    pub created_at_ms: u64,
    /// Whether the session is active (not cancelled)
    pub is_active: bool,
}

/// Response listing all sessions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListSessionsResponse {
    /// All active sessions
    pub sessions: Vec<SessionInfo>,
    /// Current session count
    pub count: usize,
    /// Maximum allowed sessions
    pub max_sessions: usize,
}

/// Request to execute a prompt (API-7)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteRequest {
    /// The prompt to execute
    pub prompt: String,

    /// Execution timeout in seconds (defaults to 300)
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

impl ExecuteRequest {
    /// Default execution timeout in seconds (5 minutes)
    pub const DEFAULT_TIMEOUT_SECS: u64 = 300;

    /// Get the timeout, using default if not specified
    pub fn timeout_secs(&self) -> u64 {
        self.timeout_secs.unwrap_or(Self::DEFAULT_TIMEOUT_SECS)
    }
}

/// Error response body
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorResponse {
    /// Error message
    pub error: String,
    /// Error code for programmatic handling
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl ErrorResponse {
    /// Create an error response with just a message
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            error: message.into(),
            code: None,
        }
    }

    /// Create an error response with a message and code
    pub fn with_code(message: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            error: message.into(),
            code: Some(code.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod health_response_tests {
        use super::*;

        #[test]
        fn healthy_returns_ok_status() {
            let resp = HealthResponse::healthy();
            assert_eq!(resp.status, "ok");
        }

        #[test]
        fn healthy_includes_version() {
            let resp = HealthResponse::healthy();
            assert!(!resp.version.is_empty());
            // Should match Cargo.toml version
            assert_eq!(resp.version, env!("CARGO_PKG_VERSION"));
        }

        #[test]
        fn serializes_to_json() {
            let resp = HealthResponse::healthy();
            let json = serde_json::to_string(&resp).unwrap();
            assert!(json.contains("\"status\":\"ok\""));
            assert!(json.contains("\"version\""));
        }

        #[test]
        fn deserializes_from_json() {
            let json = r#"{"status":"ok","version":"1.0.0"}"#;
            let resp: HealthResponse = serde_json::from_str(json).unwrap();
            assert_eq!(resp.status, "ok");
            assert_eq!(resp.version, "1.0.0");
        }
    }

    mod create_session_request_tests {
        use super::*;

        #[test]
        fn default_has_no_workspace() {
            let req = CreateSessionRequest::default();
            assert!(req.workspace.is_none());
            assert!(req.auto_approve.is_none());
        }

        #[test]
        fn deserializes_empty_json() {
            let json = "{}";
            let req: CreateSessionRequest = serde_json::from_str(json).unwrap();
            assert!(req.workspace.is_none());
            assert!(req.auto_approve.is_none());
        }

        #[test]
        fn deserializes_with_workspace() {
            let json = r#"{"workspace":"/tmp/test"}"#;
            let req: CreateSessionRequest = serde_json::from_str(json).unwrap();
            assert_eq!(req.workspace, Some("/tmp/test".to_string()));
        }

        #[test]
        fn deserializes_with_auto_approve() {
            let json = r#"{"auto_approve":true}"#;
            let req: CreateSessionRequest = serde_json::from_str(json).unwrap();
            assert_eq!(req.auto_approve, Some(true));
        }

        #[test]
        fn deserializes_full_request() {
            let json = r#"{"workspace":"/home/user/project","auto_approve":false}"#;
            let req: CreateSessionRequest = serde_json::from_str(json).unwrap();
            assert_eq!(req.workspace, Some("/home/user/project".to_string()));
            assert_eq!(req.auto_approve, Some(false));
        }
    }

    mod create_session_response_tests {
        use super::*;

        #[test]
        fn serializes_to_json() {
            let resp = CreateSessionResponse {
                session_id: "abc-123".to_string(),
                created_at: "2024-01-01T00:00:00Z".to_string(),
            };
            let json = serde_json::to_string(&resp).unwrap();
            assert!(json.contains("\"session_id\":\"abc-123\""));
            assert!(json.contains("\"created_at\":\"2024-01-01T00:00:00Z\""));
        }
    }

    mod session_info_tests {
        use super::*;

        #[test]
        fn serializes_correctly() {
            let info = SessionInfo {
                id: "session-456".to_string(),
                created_at_ms: 5000,
                is_active: true,
            };
            let json = serde_json::to_string(&info).unwrap();
            assert!(json.contains("\"id\":\"session-456\""));
            assert!(json.contains("\"created_at_ms\":5000"));
            assert!(json.contains("\"is_active\":true"));
        }

        #[test]
        fn deserializes_correctly() {
            let json = r#"{"id":"sess-1","created_at_ms":1000,"is_active":false}"#;
            let info: SessionInfo = serde_json::from_str(json).unwrap();
            assert_eq!(info.id, "sess-1");
            assert_eq!(info.created_at_ms, 1000);
            assert!(!info.is_active);
        }
    }

    mod list_sessions_response_tests {
        use super::*;

        #[test]
        fn serializes_empty_list() {
            let resp = ListSessionsResponse {
                sessions: vec![],
                count: 0,
                max_sessions: 10,
            };
            let json = serde_json::to_string(&resp).unwrap();
            assert!(json.contains("\"sessions\":[]"));
            assert!(json.contains("\"count\":0"));
            assert!(json.contains("\"max_sessions\":10"));
        }

        #[test]
        fn serializes_with_sessions() {
            let resp = ListSessionsResponse {
                sessions: vec![
                    SessionInfo {
                        id: "s1".to_string(),
                        created_at_ms: 100,
                        is_active: true,
                    },
                    SessionInfo {
                        id: "s2".to_string(),
                        created_at_ms: 200,
                        is_active: false,
                    },
                ],
                count: 2,
                max_sessions: 5,
            };
            let json = serde_json::to_string(&resp).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed["sessions"].as_array().unwrap().len(), 2);
            assert_eq!(parsed["count"], 2);
            assert_eq!(parsed["max_sessions"], 5);
        }
    }

    mod execute_request_tests {
        use super::*;

        #[test]
        fn deserializes_minimal() {
            let json = r#"{"prompt":"Hello world"}"#;
            let req: ExecuteRequest = serde_json::from_str(json).unwrap();
            assert_eq!(req.prompt, "Hello world");
            assert!(req.timeout_secs.is_none());
        }

        #[test]
        fn deserializes_with_timeout() {
            let json = r#"{"prompt":"Test","timeout_secs":60}"#;
            let req: ExecuteRequest = serde_json::from_str(json).unwrap();
            assert_eq!(req.prompt, "Test");
            assert_eq!(req.timeout_secs, Some(60));
        }

        #[test]
        fn timeout_secs_uses_default() {
            let req = ExecuteRequest {
                prompt: "test".to_string(),
                timeout_secs: None,
            };
            assert_eq!(req.timeout_secs(), ExecuteRequest::DEFAULT_TIMEOUT_SECS);
        }

        #[test]
        fn timeout_secs_uses_specified_value() {
            let req = ExecuteRequest {
                prompt: "test".to_string(),
                timeout_secs: Some(120),
            };
            assert_eq!(req.timeout_secs(), 120);
        }

        #[test]
        fn default_timeout_is_five_minutes() {
            assert_eq!(ExecuteRequest::DEFAULT_TIMEOUT_SECS, 300);
        }
    }

    mod error_response_tests {
        use super::*;

        #[test]
        fn new_creates_message_only() {
            let err = ErrorResponse::new("Something went wrong");
            assert_eq!(err.error, "Something went wrong");
            assert!(err.code.is_none());
        }

        #[test]
        fn with_code_creates_full_response() {
            let err = ErrorResponse::with_code("Not found", "NOT_FOUND");
            assert_eq!(err.error, "Not found");
            assert_eq!(err.code, Some("NOT_FOUND".to_string()));
        }

        #[test]
        fn serializes_without_code() {
            let err = ErrorResponse::new("Error");
            let json = serde_json::to_string(&err).unwrap();
            // Code should not appear when None
            assert!(!json.contains("code"));
            assert!(json.contains("\"error\":\"Error\""));
        }

        #[test]
        fn serializes_with_code() {
            let err = ErrorResponse::with_code("Error", "ERR_CODE");
            let json = serde_json::to_string(&err).unwrap();
            assert!(json.contains("\"code\":\"ERR_CODE\""));
        }
    }
}

//! HTTP request handlers for the eval server.
//!
//! This module implements all HTTP endpoint handlers for the server mode.
//! Each handler is designed to be testable in isolation and follows RESTful conventions.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures::stream::{Stream, StreamExt};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::session::SessionManager;
use super::types::*;
use crate::cli::args::Args;
use crate::cli::bootstrap;
use crate::cli::output::{event_stream, is_terminal_event, runtime_event_to_sse};
use crate::runtime::{CliRuntime, RuntimeEvent};

/// Application state shared across all handlers
pub struct AppState {
    /// Session manager for creating/retrieving sessions
    pub session_manager: SessionManager,
    /// Shutdown token for graceful server shutdown
    pub shutdown_token: CancellationToken,
    /// Default workspace path for new sessions
    pub default_workspace: std::path::PathBuf,
}

impl AppState {
    /// Create new application state with the given configuration
    pub fn new(
        max_sessions: usize,
        default_workspace: std::path::PathBuf,
    ) -> (Arc<Self>, CancellationToken) {
        let shutdown_token = CancellationToken::new();
        let state = Arc::new(Self {
            session_manager: SessionManager::new(max_sessions),
            shutdown_token: shutdown_token.clone(),
            default_workspace,
        });
        (state, shutdown_token)
    }
}

// =============================================================================
// API-1, API-2: Health Check
// =============================================================================

/// Health check endpoint.
///
/// Returns 200 OK with version information when the server is healthy.
///
/// # Response
///
/// - `200 OK`: Server is healthy
///
/// # Example Response
///
/// ```json
/// {
///   "status": "ok",
///   "version": "0.1.0"
/// }
/// ```
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse::healthy())
}

// =============================================================================
// API-3, API-4: Session Creation
// =============================================================================

/// Create a new session.
///
/// Creates a new isolated session with its own context and cancellation token.
/// The session is assigned a server-generated UUID.
///
/// # Request Body
///
/// ```json
/// {
///   "workspace": "/path/to/workspace",  // optional
///   "auto_approve": true                 // optional, defaults to true
/// }
/// ```
///
/// # Response
///
/// - `201 Created`: Session created successfully
/// - `503 Service Unavailable`: Maximum session limit reached
/// - `500 Internal Server Error`: Failed to initialize agent context
///
/// # Example Response
///
/// ```json
/// {
///   "session_id": "550e8400-e29b-41d4-a716-446655440000",
///   "created_at": "2024-01-01T00:00:00Z"
/// }
/// ```
pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<CreateSessionResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Use provided workspace or default
    let workspace = req
        .workspace
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| state.default_workspace.clone());

    // Note: auto_approve defaults to true for eval sessions
    let auto_approve = req.auto_approve.unwrap_or(true);

    // Try to create the session
    let session = match state.session_manager.create() {
        Ok(session) => session,
        Err(e) => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse::with_code(
                    e.to_string(),
                    "SESSION_LIMIT_REACHED",
                )),
            ));
        }
    };

    // Initialize CLI context for this session
    let args = Args::for_server_session(workspace, auto_approve);
    let ctx = match bootstrap::initialize(&args).await {
        Ok(ctx) => ctx,
        Err(e) => {
            // Remove the session since we failed to initialize context
            state.session_manager.remove(&session.id);
            tracing::error!("Failed to initialize session context: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::with_code(
                    format!("Failed to initialize agent: {}", e),
                    "AGENT_INIT_FAILED",
                )),
            ));
        }
    };

    // Set the context on the session
    session.set_context(ctx).await;

    let response = CreateSessionResponse {
        session_id: session.id.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    tracing::info!("Created session {} with agent context", session.id);
    Ok((StatusCode::CREATED, Json(response)))
}

// =============================================================================
// API-5: List Sessions
// =============================================================================

/// List all active sessions.
///
/// Returns information about all currently active sessions.
///
/// # Response
///
/// - `200 OK`: Always (even if no sessions exist)
///
/// # Example Response
///
/// ```json
/// {
///   "sessions": [
///     {
///       "id": "550e8400-e29b-41d4-a716-446655440000",
///       "created_at_ms": 5000,
///       "is_active": true
///     }
///   ],
///   "count": 1,
///   "max_sessions": 10
/// }
/// ```
pub async fn list_sessions(State(state): State<Arc<AppState>>) -> Json<ListSessionsResponse> {
    let sessions = state.session_manager.list_sessions();
    Json(ListSessionsResponse {
        count: sessions.len(),
        max_sessions: state.session_manager.max_sessions,
        sessions,
    })
}

// =============================================================================
// API-5: Get Session
// =============================================================================

/// Get a specific session by ID.
///
/// # Path Parameters
///
/// - `session_id`: The session ID to retrieve
///
/// # Response
///
/// - `200 OK`: Session found
/// - `404 Not Found`: Session does not exist
pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionInfo>, (StatusCode, Json<ErrorResponse>)> {
    state
        .session_manager
        .get(&session_id)
        .map(|session| {
            Json(SessionInfo {
                id: session.id.clone(),
                created_at_ms: session.created_at.elapsed().as_millis() as u64,
                is_active: !session.cancel_token.is_cancelled(),
            })
        })
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::with_code(
                    format!("Session '{}' not found", session_id),
                    "SESSION_NOT_FOUND",
                )),
            )
        })
}

// =============================================================================
// API-6: Delete Session
// =============================================================================

/// Delete a session.
///
/// Removes the session and cancels any running execution.
///
/// # Path Parameters
///
/// - `session_id`: The session ID to delete
///
/// # Response
///
/// - `204 No Content`: Session deleted successfully
/// - `404 Not Found`: Session does not exist
pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    if state.session_manager.remove(&session_id).is_some() {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::with_code(
                format!("Session '{}' not found", session_id),
                "SESSION_NOT_FOUND",
            )),
        ))
    }
}

// =============================================================================
// API-7: Execute Prompt (SSE Streaming)
// =============================================================================

/// Execute a prompt and stream events via SSE.
///
/// This endpoint executes an AI agent prompt and streams events back to the client
/// using Server-Sent Events (SSE). The stream terminates when the agent completes
/// or encounters an error.
///
/// # Path Parameters
///
/// - `session_id`: The session ID to execute in
///
/// # Request Body
///
/// ```json
/// {
///   "prompt": "What is 2+2?",
///   "timeout_secs": 300  // optional
/// }
/// ```
///
/// # Response
///
/// - `200 OK`: SSE stream of events
/// - `404 Not Found`: Session does not exist
/// - `400 Bad Request`: Session context not initialized
///
/// # SSE Events
///
/// Events are streamed with the following format:
///
/// ```text
/// event: ai_event
/// data: {"event":"started","timestamp":1234567890,"turn_id":"..."}
///
/// event: ai_event
/// data: {"event":"text_delta","timestamp":1234567890,"delta":"Hello",...}
///
/// event: ai_event
/// data: {"event":"completed","timestamp":1234567890,"response":"...",...}
/// ```
pub async fn execute(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    // Get session
    let session = state.session_manager.get(&session_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::with_code(
                format!("Session '{}' not found", session_id),
                "SESSION_NOT_FOUND",
            )),
        )
    })?;

    // Update last activity
    session.touch().await;

    // Verify session has context initialized
    if !session.has_context().await {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::with_code(
                "Session context not initialized".to_string(),
                "CONTEXT_NOT_INITIALIZED",
            )),
        ));
    }

    // Create a child cancellation token for this specific execution
    let exec_cancel = session.cancel_token.child_token();

    // Set up timeout
    let timeout_secs = req.timeout_secs();
    let timeout_cancel = exec_cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(timeout_secs)).await;
        if !timeout_cancel.is_cancelled() {
            tracing::warn!("Execution timeout after {}s", timeout_secs);
            timeout_cancel.cancel();
        }
    });

    // Create event channel for this execution
    let (event_tx, event_rx) = mpsc::unbounded_channel::<RuntimeEvent>();

    // Get the context and replace the runtime's event sender
    // so events from the AgentBridge flow to our SSE stream
    {
        let ctx_guard = session.context.read().await;
        if let Some(ref ctx) = *ctx_guard {
            // Downcast runtime to CliRuntime to access replace_event_tx
            if let Some(cli_runtime) = ctx.runtime.as_any().downcast_ref::<CliRuntime>() {
                cli_runtime.replace_event_tx(event_tx);
            } else {
                tracing::warn!("Runtime is not CliRuntime, events may not be received");
            }
        }
    }

    // Spawn the agent execution task
    let prompt = req.prompt.clone();
    let cancel_token = exec_cancel.clone();
    let session_clone = session.clone();

    tokio::spawn(async move {
        // Get the AgentBridge and execute
        let ctx_guard = session_clone.context.read().await;
        if let Some(ref ctx) = *ctx_guard {
            let bridge_guard = ctx.bridge().await;
            if let Some(ref bridge) = *bridge_guard {
                // Execute with cancellation support
                let result = bridge
                    .execute_with_cancellation(&prompt, cancel_token)
                    .await;

                if let Err(e) = result {
                    tracing::warn!("Agent execution failed: {}", e);
                    // Error event is already emitted by the agent
                }
            } else {
                tracing::error!("AgentBridge not initialized in session context");
            }
        } else {
            tracing::error!("Session context disappeared during execution");
        }
    });

    // Create SSE stream from event receiver
    // We use a shared state to track the last event for the terminal check
    let stream = event_stream(event_rx)
        .map(|event| {
            let is_term = is_terminal_event(&event);
            (event, is_term)
        })
        // Take events until we see a terminal event, but include it
        .scan(false, |seen_terminal, (event, is_term)| {
            if *seen_terminal {
                // Already saw terminal, stop stream
                std::future::ready(None)
            } else {
                if is_term {
                    *seen_terminal = true;
                }
                std::future::ready(Some(event))
            }
        })
        .filter_map(|event| async move { runtime_event_to_sse(&event) })
        .map(Ok);

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        Router,
    };
    use tower::ServiceExt;

    /// Create a test app with routes
    fn create_test_app() -> Router {
        let (state, _shutdown) = AppState::new(10, std::path::PathBuf::from("/tmp"));

        Router::new()
            .route("/health", axum::routing::get(health))
            .route("/sessions", axum::routing::post(create_session))
            .route("/sessions", axum::routing::get(list_sessions))
            .route("/sessions/{session_id}", axum::routing::get(get_session))
            .route(
                "/sessions/{session_id}",
                axum::routing::delete(delete_session),
            )
            .route(
                "/sessions/{session_id}/execute",
                axum::routing::post(execute),
            )
            .with_state(state)
    }

    // =========================================================================
    // Health endpoint tests
    // =========================================================================

    mod health_tests {
        use super::*;

        #[tokio::test]
        async fn health_returns_200_ok() {
            let app = create_test_app();

            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/health")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
        }

        #[tokio::test]
        async fn health_returns_correct_body() {
            let app = create_test_app();

            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/health")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let health: HealthResponse = serde_json::from_slice(&body).unwrap();

            assert_eq!(health.status, "ok");
            assert!(!health.version.is_empty());
        }
    }

    // =========================================================================
    // Session CRUD tests
    // =========================================================================

    mod session_crud_tests {
        use super::*;

        #[tokio::test]
        async fn create_session_returns_201() {
            let app = create_test_app();

            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/sessions")
                        .header("content-type", "application/json")
                        .body(Body::from("{}"))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::CREATED);
        }

        #[tokio::test]
        async fn create_session_returns_session_id() {
            let app = create_test_app();

            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/sessions")
                        .header("content-type", "application/json")
                        .body(Body::from("{}"))
                        .unwrap(),
                )
                .await
                .unwrap();

            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let resp: CreateSessionResponse = serde_json::from_slice(&body).unwrap();

            assert!(!resp.session_id.is_empty());
            assert!(!resp.created_at.is_empty());

            // Verify session ID is a valid UUID
            assert!(uuid::Uuid::parse_str(&resp.session_id).is_ok());
        }

        #[tokio::test]
        async fn list_sessions_returns_empty_initially() {
            let app = create_test_app();

            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/sessions")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);

            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let resp: ListSessionsResponse = serde_json::from_slice(&body).unwrap();

            assert_eq!(resp.count, 0);
            assert!(resp.sessions.is_empty());
            assert_eq!(resp.max_sessions, 10);
        }

        #[tokio::test]
        async fn get_nonexistent_session_returns_404() {
            let app = create_test_app();

            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/sessions/nonexistent-id")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }

        #[tokio::test]
        async fn delete_nonexistent_session_returns_404() {
            let app = create_test_app();

            let response = app
                .oneshot(
                    Request::builder()
                        .method("DELETE")
                        .uri("/sessions/nonexistent-id")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    // =========================================================================
    // Session capacity tests
    // =========================================================================

    mod session_capacity_tests {
        use super::*;

        #[tokio::test]
        async fn create_fails_at_capacity() {
            // Create app with max 2 sessions
            let (state, _shutdown) = AppState::new(2, std::path::PathBuf::from("/tmp"));
            let app = Router::new()
                .route("/sessions", axum::routing::post(create_session))
                .with_state(state.clone());

            // Create 2 sessions - should succeed
            for _ in 0..2 {
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/sessions")
                            .header("content-type", "application/json")
                            .body(Body::from("{}"))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::CREATED);
            }

            // Third session should fail
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/sessions")
                        .header("content-type", "application/json")
                        .body(Body::from("{}"))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        }
    }

    // =========================================================================
    // Execute endpoint tests
    // =========================================================================

    mod execute_tests {
        use super::*;

        #[tokio::test]
        async fn execute_nonexistent_session_returns_404() {
            let app = create_test_app();

            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/sessions/nonexistent-id/execute")
                        .header("content-type", "application/json")
                        .body(Body::from(r#"{"prompt":"Hello"}"#))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }

        #[tokio::test]
        async fn execute_without_context_returns_400() {
            // Create a session WITHOUT initializing the CLI context
            // This simulates the case where session creation succeeded but
            // context initialization failed or was skipped
            let (state, _shutdown) = AppState::new(10, std::path::PathBuf::from("/tmp"));
            let session = state.session_manager.create().unwrap();
            let session_id = session.id.clone();

            let app = Router::new()
                .route(
                    "/sessions/{session_id}/execute",
                    axum::routing::post(execute),
                )
                .with_state(state);

            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/sessions/{}/execute", session_id))
                        .header("content-type", "application/json")
                        .body(Body::from(r#"{"prompt":"What is 2+2?"}"#))
                        .unwrap(),
                )
                .await
                .unwrap();

            // Without context initialized, execute returns 400 Bad Request
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);

            // Verify error body contains the right error code
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let error: ErrorResponse = serde_json::from_slice(&body).unwrap();
            assert_eq!(error.code, Some("CONTEXT_NOT_INITIALIZED".to_string()));
        }

        // Note: A full integration test for SSE streaming with agent execution
        // would require initializing the full CLI context (settings, PTY, indexer,
        // AI agent, etc.) which is done in the actual create_session handler.
        // Such tests are better suited for the evals/ framework which tests
        // the full server end-to-end.
    }

    // =========================================================================
    // Integration tests for session lifecycle
    // =========================================================================

    mod lifecycle_tests {
        use super::*;

        #[tokio::test]
        async fn full_session_lifecycle() {
            let (state, _shutdown) = AppState::new(10, std::path::PathBuf::from("/tmp"));

            // 1. Create session
            let session = state.session_manager.create().unwrap();
            let session_id = session.id.clone();

            // 2. Verify session exists
            assert!(state.session_manager.get(&session_id).is_some());
            assert_eq!(state.session_manager.count(), 1);

            // 3. List sessions
            let sessions = state.session_manager.list_sessions();
            assert_eq!(sessions.len(), 1);
            assert_eq!(sessions[0].id, session_id);

            // 4. Delete session
            let removed = state.session_manager.remove(&session_id);
            assert!(removed.is_some());

            // 5. Verify session is gone
            assert!(state.session_manager.get(&session_id).is_none());
            assert_eq!(state.session_manager.count(), 0);
        }
    }
}

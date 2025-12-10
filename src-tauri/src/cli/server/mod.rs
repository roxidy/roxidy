//! HTTP/SSE server module for the Qbit eval framework.
//!
//! This module provides an HTTP server with SSE (Server-Sent Events) streaming
//! for real-time eval testing. It extends the CLI functionality to expose
//! agent execution over HTTP.
//!
//! # Architecture
//!
//! The server manages isolated sessions, each owning its own `CliContext`:
//!
//! ```text
//! +------------------------------------------+
//! |  Axum HTTP Server                        |
//! |  /health (GET)        -> health check    |
//! |  /sessions (POST)     -> create session  |
//! |  /sessions (GET)      -> list sessions   |
//! |  /sessions/{id} (GET) -> get session     |
//! |  /sessions/{id} (DELETE) -> delete       |
//! |  /sessions/{id}/execute (POST) -> SSE    |
//! +------------------------------------------+
//!          |
//!          v
//! +------------------------------------------+
//! |  SessionManager (DashMap)                |
//! |    +-- Session 1: CliContext (owned)     |
//! |    +-- Session 2: CliContext (owned)     |
//! |    +-- ... (max configurable sessions)   |
//! +------------------------------------------+
//! ```
//!
//! # Usage
//!
//! Start the server with:
//! ```bash
//! qbit-cli --server --port 8080
//! ```
//!
//! # Endpoints
//!
//! | Method | Path | Description |
//! |--------|------|-------------|
//! | GET | /health | Health check |
//! | POST | /sessions | Create new session |
//! | GET | /sessions | List all sessions |
//! | GET | /sessions/:id | Get session info |
//! | DELETE | /sessions/:id | Delete session |
//! | POST | /sessions/:id/execute | Execute prompt (SSE) |
//!
//! # Feature Flag
//!
//! This module is only available when the `server` feature is enabled.

mod handlers;
mod session;
pub mod types;

pub use handlers::AppState;
pub use session::{Session, SessionManager, DEFAULT_MAX_SESSIONS, DEFAULT_SESSION_TTL_SECS};
pub use types::{
    CreateSessionRequest, CreateSessionResponse, ErrorResponse, ExecuteRequest, HealthResponse,
    ListSessionsResponse, SessionInfo,
};

use axum::{
    routing::{delete, get, post},
    Router,
};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

/// Start the HTTP server.
///
/// This function creates and starts the Axum HTTP server with all routes configured.
/// The server supports graceful shutdown via the returned cancellation token.
///
/// # Arguments
///
/// * `port` - Port to listen on. Use 0 for a random available port.
/// * `workspace` - Default workspace path for new sessions
/// * `max_sessions` - Maximum concurrent sessions allowed
///
/// # Returns
///
/// A tuple containing:
/// - The actual bound address (useful when port=0)
/// - A cancellation token to trigger graceful shutdown
///
/// # Example
///
/// ```ignore
/// let (addr, shutdown) = start_server(8080, PathBuf::from("."), 10).await?;
/// println!("Server listening on {}", addr);
///
/// // Later, to shut down:
/// shutdown.cancel();
/// ```
pub async fn start_server(
    port: u16,
    workspace: PathBuf,
    max_sessions: usize,
) -> anyhow::Result<(SocketAddr, CancellationToken)> {
    let (state, shutdown_token) = AppState::new(max_sessions, workspace);

    let app = create_router(state.clone());

    // Bind with port 0 support for random port
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    let actual_addr = listener.local_addr()?;

    tracing::info!("HTTP server listening on {}", actual_addr);

    // Spawn cleanup task for idle sessions
    let cleanup_state = state.clone();
    let cleanup_shutdown = shutdown_token.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let cleaned = cleanup_state.session_manager.cleanup_idle(DEFAULT_SESSION_TTL_SECS).await;
                    if cleaned > 0 {
                        tracing::info!("Cleaned up {} idle sessions", cleaned);
                    }
                }
                _ = cleanup_shutdown.cancelled() => {
                    tracing::debug!("Session cleanup task shutting down");
                    break;
                }
            }
        }
    });

    // Run server with graceful shutdown
    let server_shutdown = shutdown_token.clone();
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app)
            .with_graceful_shutdown(server_shutdown.cancelled_owned())
            .await
        {
            tracing::error!("Server error: {}", e);
        }
    });

    Ok((actual_addr, shutdown_token))
}

/// Create the router with all routes configured.
///
/// This is separated from `start_server` to enable easier testing.
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/sessions", post(handlers::create_session))
        .route("/sessions", get(handlers::list_sessions))
        .route("/sessions/{session_id}", get(handlers::get_session))
        .route("/sessions/{session_id}", delete(handlers::delete_session))
        .route("/sessions/{session_id}/execute", post(handlers::execute))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_manager_is_exported() {
        // Verify SessionManager is properly exported
        let manager = SessionManager::new(5);
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn session_is_exported() {
        // Verify Session is properly exported
        let session = Session::new();
        assert!(!session.id.is_empty());
    }

    #[test]
    fn constants_are_exported() {
        // Verify constants are accessible
        assert_eq!(DEFAULT_MAX_SESSIONS, 10);
        assert_eq!(DEFAULT_SESSION_TTL_SECS, 30 * 60);
    }

    #[test]
    fn types_are_exported() {
        // Verify type exports
        let _health = HealthResponse::healthy();
        let _req = CreateSessionRequest::default();
        let _err = ErrorResponse::new("test");
    }

    mod server_tests {
        use super::*;

        #[tokio::test]
        async fn start_server_binds_to_port() {
            let (addr, shutdown) = start_server(0, PathBuf::from("/tmp"), 10)
                .await
                .expect("Server should start");

            // Should have bound to a port
            assert!(addr.port() > 0);

            // Cleanup
            shutdown.cancel();
        }

        #[tokio::test]
        async fn start_server_returns_shutdown_token() {
            let (_, shutdown) = start_server(0, PathBuf::from("/tmp"), 10)
                .await
                .expect("Server should start");

            // Token should not be cancelled initially
            assert!(!shutdown.is_cancelled());

            // Cancel and verify
            shutdown.cancel();
            assert!(shutdown.is_cancelled());
        }

        #[tokio::test]
        async fn create_router_returns_router() {
            let (state, _) = AppState::new(10, PathBuf::from("/tmp"));
            let _router = create_router(state);
            // If we get here without panic, the router was created successfully
        }
    }

    mod integration_tests {
        use super::*;
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        #[tokio::test]
        async fn health_endpoint_works() {
            let (state, _) = AppState::new(10, PathBuf::from("/tmp"));
            let app = create_router(state);

            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/health")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), axum::http::StatusCode::OK);
        }

        #[tokio::test]
        async fn sessions_endpoint_works() {
            let (state, _) = AppState::new(10, PathBuf::from("/tmp"));
            let app = create_router(state);

            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/sessions")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), axum::http::StatusCode::OK);
        }

        #[tokio::test]
        async fn create_session_endpoint_works() {
            let (state, _) = AppState::new(10, PathBuf::from("/tmp"));
            let app = create_router(state);

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

            assert_eq!(response.status(), axum::http::StatusCode::CREATED);
        }
    }
}

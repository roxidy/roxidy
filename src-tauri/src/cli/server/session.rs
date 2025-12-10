//! Session management for the HTTP/SSE eval server.
//!
//! This module provides thread-safe session management using DashMap for O(1) lookup.
//! Each session owns its own isolated `CliContext` to prevent state interference.
//!
//! # Architecture
//!
//! ```text
//! +------------------------------------------+
//! |  SessionManager (DashMap)                |
//! |    +-- Session 1: CliContext (owned)     |
//! |    +-- Session 2: CliContext (owned)     |
//! |    +-- ... (max configurable sessions)   |
//! +------------------------------------------+
//! ```
//!
//! # Thread Safety
//!
//! - `SessionManager` uses `DashMap` for lock-free concurrent access
//! - `Session.last_activity` uses `RwLock` for atomic updates
//! - `CancellationToken` enables graceful shutdown propagation

use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::cli::bootstrap::CliContext;

/// Maximum concurrent sessions to prevent resource exhaustion (default)
pub const DEFAULT_MAX_SESSIONS: usize = 10;

/// Session TTL for idle cleanup - 30 minutes (default)
pub const DEFAULT_SESSION_TTL_SECS: u64 = 30 * 60;

/// A single eval session with its own isolated context.
///
/// Each session has:
/// - A unique UUID identifier (server-generated)
/// - A cancellation token for graceful shutdown
/// - Activity tracking for idle cleanup
/// - An optional CliContext for agent execution (initialized lazily)
///
/// Note: Manual Debug impl due to fields that don't implement Debug (RwLock, CancellationToken)
pub struct Session {
    /// Unique session identifier (UUID v4)
    pub id: String,

    /// Cancellation token for graceful shutdown.
    /// When cancelled, any running execution should terminate.
    pub cancel_token: CancellationToken,

    /// When the session was created
    pub created_at: Instant,

    /// Last activity timestamp (updated on each request)
    pub last_activity: RwLock<Instant>,

    /// CLI context for agent execution (initialized when session is created with context)
    pub context: RwLock<Option<CliContext>>,
}

impl Session {
    /// Create a new session with a server-generated UUID.
    ///
    /// The session is created in an active state with current timestamps
    /// for both `created_at` and `last_activity`. The context is None
    /// and must be set separately via `set_context()`.
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            id: Uuid::new_v4().to_string(),
            cancel_token: CancellationToken::new(),
            created_at: now,
            last_activity: RwLock::new(now),
            context: RwLock::new(None),
        }
    }

    /// Create a session with a specific ID (for testing).
    #[cfg(test)]
    pub fn with_id(id: impl Into<String>) -> Self {
        let now = Instant::now();
        Self {
            id: id.into(),
            cancel_token: CancellationToken::new(),
            created_at: now,
            last_activity: RwLock::new(now),
            context: RwLock::new(None),
        }
    }

    /// Set the CLI context for this session.
    ///
    /// This should be called after session creation to initialize the
    /// agent execution context.
    pub async fn set_context(&self, ctx: CliContext) {
        let mut guard = self.context.write().await;
        *guard = Some(ctx);
    }

    /// Check if the session has a context initialized.
    pub async fn has_context(&self) -> bool {
        self.context.read().await.is_some()
    }

    /// Update the last activity timestamp to now.
    ///
    /// This should be called on each request to the session
    /// to prevent idle cleanup.
    pub async fn touch(&self) {
        *self.last_activity.write().await = Instant::now();
    }

    /// Check if this session is idle (no activity for given duration).
    pub async fn is_idle(&self, max_idle_secs: u64) -> bool {
        let last = *self.last_activity.read().await;
        last.elapsed().as_secs() > max_idle_secs
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("id", &self.id)
            .field("created_at", &self.created_at)
            .field("is_cancelled", &self.cancel_token.is_cancelled())
            .finish_non_exhaustive()
    }
}

/// Thread-safe session manager using DashMap for O(1) lookup.
///
/// The manager enforces a maximum session limit to prevent resource exhaustion.
/// Idle sessions can be cleaned up periodically via `cleanup_idle()`.
pub struct SessionManager {
    /// Concurrent hash map for session storage
    sessions: DashMap<String, Arc<Session>>,

    /// Maximum allowed concurrent sessions
    pub max_sessions: usize,
}

impl SessionManager {
    /// Create a new session manager with the specified capacity limit.
    ///
    /// # Arguments
    ///
    /// * `max_sessions` - Maximum number of concurrent sessions allowed
    pub fn new(max_sessions: usize) -> Self {
        Self {
            sessions: DashMap::new(),
            max_sessions,
        }
    }

    /// Create a new session, returning error if at capacity.
    ///
    /// The session is assigned a server-generated UUID and added to the manager.
    ///
    /// # Returns
    ///
    /// - `Ok(Arc<Session>)` - The newly created session
    /// - `Err` - If the maximum session limit has been reached
    pub fn create(&self) -> anyhow::Result<Arc<Session>> {
        // Check capacity BEFORE creating the session
        if self.sessions.len() >= self.max_sessions {
            anyhow::bail!("Maximum session limit ({}) reached", self.max_sessions);
        }

        let session = Arc::new(Session::new());
        self.sessions.insert(session.id.clone(), session.clone());
        Ok(session)
    }

    /// Get an existing session by ID.
    ///
    /// This is an O(1) operation using DashMap's concurrent hash map.
    ///
    /// # Arguments
    ///
    /// * `id` - The session ID to look up
    ///
    /// # Returns
    ///
    /// - `Some(Arc<Session>)` - If the session exists
    /// - `None` - If no session with that ID exists
    pub fn get(&self, id: &str) -> Option<Arc<Session>> {
        self.sessions.get(id).map(|r| r.clone())
    }

    /// Remove a session by ID.
    ///
    /// The session's cancellation token is triggered to signal any running
    /// executions to terminate gracefully.
    ///
    /// # Arguments
    ///
    /// * `id` - The session ID to remove
    ///
    /// # Returns
    ///
    /// - `Some(Arc<Session>)` - The removed session
    /// - `None` - If no session with that ID existed
    pub fn remove(&self, id: &str) -> Option<Arc<Session>> {
        if let Some((_, session)) = self.sessions.remove(id) {
            // Cancel any running execution
            session.cancel_token.cancel();
            Some(session)
        } else {
            None
        }
    }

    /// Get the current number of active sessions.
    pub fn count(&self) -> usize {
        self.sessions.len()
    }

    /// Check if a session exists.
    pub fn contains(&self, id: &str) -> bool {
        self.sessions.contains_key(id)
    }

    /// List all sessions with their info.
    ///
    /// Returns a vector of `SessionInfo` for all active sessions.
    /// This is used by the HTTP list sessions endpoint.
    pub fn list_sessions(&self) -> Vec<crate::cli::server::types::SessionInfo> {
        self.sessions
            .iter()
            .map(|entry| crate::cli::server::types::SessionInfo {
                id: entry.key().clone(),
                created_at_ms: entry.value().created_at.elapsed().as_millis() as u64,
                is_active: !entry.value().cancel_token.is_cancelled(),
            })
            .collect()
    }

    /// Cleanup sessions that have been idle for longer than the specified duration.
    ///
    /// This should be called periodically (e.g., every minute) to prevent
    /// resource leaks from abandoned sessions.
    ///
    /// # Arguments
    ///
    /// * `max_idle_secs` - Maximum idle time in seconds before cleanup
    ///
    /// # Returns
    ///
    /// The number of sessions that were cleaned up.
    pub async fn cleanup_idle(&self, max_idle_secs: u64) -> usize {
        let mut to_remove = Vec::new();

        // Identify idle sessions
        for entry in self.sessions.iter() {
            if entry.value().is_idle(max_idle_secs).await {
                to_remove.push(entry.key().clone());
            }
        }

        // Remove idle sessions
        let removed_count = to_remove.len();
        for id in to_remove {
            if let Some((_, session)) = self.sessions.remove(&id) {
                // Cancel any running execution
                session.cancel_token.cancel();
                tracing::info!("Cleaned up idle session: {}", id);
            }
        }

        removed_count
    }

    /// Get an iterator over all session IDs.
    #[cfg(test)]
    pub fn session_ids(&self) -> Vec<String> {
        self.sessions.iter().map(|r| r.key().clone()).collect()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_SESSIONS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // =========================================================================
    // SM-1: Session IDs are valid UUIDs
    // =========================================================================

    mod session_id_tests {
        use super::*;

        #[test]
        fn session_id_is_valid_uuid() {
            let session = Session::new();

            // Parse should succeed if it's a valid UUID
            let parsed = Uuid::parse_str(&session.id);
            assert!(
                parsed.is_ok(),
                "Session ID '{}' is not a valid UUID: {:?}",
                session.id,
                parsed.err()
            );

            // Should be UUID v4 (random)
            let uuid = parsed.unwrap();
            assert_eq!(
                uuid.get_version(),
                Some(uuid::Version::Random),
                "Expected UUID v4 (random), got {:?}",
                uuid.get_version()
            );
        }

        #[test]
        fn session_ids_are_unique() {
            let session1 = Session::new();
            let session2 = Session::new();
            let session3 = Session::new();

            assert_ne!(session1.id, session2.id);
            assert_ne!(session2.id, session3.id);
            assert_ne!(session1.id, session3.id);
        }

        #[test]
        fn created_session_has_uuid_id() {
            let manager = SessionManager::new(10);
            let session = manager.create().expect("Should create session");

            let parsed = Uuid::parse_str(&session.id);
            assert!(parsed.is_ok(), "Created session should have valid UUID");
        }
    }

    // =========================================================================
    // SM-2: Session creation fails at capacity
    // =========================================================================

    mod capacity_tests {
        use super::*;

        #[test]
        fn create_fails_at_capacity() {
            let manager = SessionManager::new(2);

            // Create first two sessions - should succeed
            let s1 = manager.create();
            assert!(s1.is_ok(), "First session should succeed");

            let s2 = manager.create();
            assert!(s2.is_ok(), "Second session should succeed");

            // Third session should fail
            let s3 = manager.create();
            assert!(s3.is_err(), "Third session should fail at capacity");

            let err_msg = s3.unwrap_err().to_string();
            assert!(
                err_msg.contains("Maximum session limit"),
                "Error should mention limit: {}",
                err_msg
            );
            assert!(
                err_msg.contains("2"),
                "Error should mention max sessions (2): {}",
                err_msg
            );
        }

        #[test]
        fn create_succeeds_after_removal() {
            let manager = SessionManager::new(1);

            // Create first session
            let session = manager.create().expect("First create should succeed");
            let id = session.id.clone();

            // Second create should fail
            assert!(manager.create().is_err(), "At capacity, should fail");

            // Remove the session
            manager.remove(&id);

            // Now create should succeed again
            assert!(manager.create().is_ok(), "After removal, should succeed");
        }

        #[test]
        fn zero_capacity_always_fails() {
            let manager = SessionManager::new(0);

            let result = manager.create();
            assert!(result.is_err(), "Zero capacity should always fail");
        }

        #[test]
        fn count_reflects_actual_sessions() {
            let manager = SessionManager::new(5);
            assert_eq!(manager.count(), 0);

            let s1 = manager.create().unwrap();
            assert_eq!(manager.count(), 1);

            let _s2 = manager.create().unwrap();
            assert_eq!(manager.count(), 2);

            manager.remove(&s1.id);
            assert_eq!(manager.count(), 1);
        }
    }

    // =========================================================================
    // SM-4: Concurrent session creation is thread-safe
    // =========================================================================

    mod concurrency_tests {
        use super::*;
        use std::sync::atomic::{AtomicUsize, Ordering};

        #[tokio::test]
        async fn concurrent_creates_respect_capacity() {
            let manager = Arc::new(SessionManager::new(5));
            let success_count = Arc::new(AtomicUsize::new(0));
            let failure_count = Arc::new(AtomicUsize::new(0));

            // Spawn 20 concurrent tasks trying to create sessions
            let mut handles = Vec::new();
            for _ in 0..20 {
                let mgr = manager.clone();
                let success = success_count.clone();
                let failure = failure_count.clone();

                handles.push(tokio::spawn(async move {
                    match mgr.create() {
                        Ok(_) => success.fetch_add(1, Ordering::SeqCst),
                        Err(_) => failure.fetch_add(1, Ordering::SeqCst),
                    };
                }));
            }

            // Wait for all tasks
            for handle in handles {
                handle.await.expect("Task should complete");
            }

            // Exactly 5 should succeed, 15 should fail
            let successes = success_count.load(Ordering::SeqCst);
            let failures = failure_count.load(Ordering::SeqCst);

            assert_eq!(successes, 5, "Exactly max_sessions should succeed");
            assert_eq!(failures, 15, "Rest should fail");
            assert_eq!(manager.count(), 5, "Manager should have exactly 5 sessions");
        }

        #[tokio::test]
        async fn concurrent_gets_are_safe() {
            let manager = Arc::new(SessionManager::new(10));

            // Create some sessions
            let sessions: Vec<_> = (0..5).map(|_| manager.create().unwrap()).collect();

            // Spawn many concurrent gets
            let mut handles = Vec::new();
            for session in &sessions {
                let id = session.id.clone();
                let mgr = manager.clone();

                // Multiple concurrent gets for the same session
                for _ in 0..10 {
                    let id_clone = id.clone();
                    let mgr_clone = mgr.clone();
                    handles.push(tokio::spawn(async move {
                        let result = mgr_clone.get(&id_clone);
                        assert!(result.is_some(), "Get should succeed");
                        result.unwrap().id == id_clone
                    }));
                }
            }

            // All should succeed
            for handle in handles {
                assert!(handle.await.expect("Task should complete"));
            }
        }

        #[tokio::test]
        async fn concurrent_removes_are_safe() {
            let manager = Arc::new(SessionManager::new(10));

            // Create a session
            let session = manager.create().unwrap();
            let id = session.id.clone();

            // Spawn multiple concurrent removes for the same session
            let mut handles = Vec::new();
            for _ in 0..10 {
                let id_clone = id.clone();
                let mgr = manager.clone();
                handles.push(tokio::spawn(async move { mgr.remove(&id_clone).is_some() }));
            }

            // Collect results
            let mut results = Vec::new();
            for handle in handles {
                results.push(handle.await.expect("Task should complete"));
            }

            // Exactly one should succeed (return Some)
            let success_count: usize = results.iter().filter(|&&r| r).count();
            assert_eq!(
                success_count, 1,
                "Exactly one remove should succeed, got {}",
                success_count
            );
            assert_eq!(manager.count(), 0, "Session should be removed");
        }

        #[tokio::test]
        async fn session_ids_are_unique_under_concurrency() {
            let manager = Arc::new(SessionManager::new(100));
            let ids = Arc::new(tokio::sync::Mutex::new(Vec::new()));

            // Create many sessions concurrently
            let mut handles = Vec::new();
            for _ in 0..50 {
                let mgr = manager.clone();
                let ids_clone = ids.clone();
                handles.push(tokio::spawn(async move {
                    if let Ok(session) = mgr.create() {
                        ids_clone.lock().await.push(session.id.clone());
                    }
                }));
            }

            for handle in handles {
                handle.await.expect("Task should complete");
            }

            // Check all IDs are unique
            let collected_ids = ids.lock().await;
            let mut sorted_ids = collected_ids.clone();
            sorted_ids.sort();
            sorted_ids.dedup();

            assert_eq!(
                collected_ids.len(),
                sorted_ids.len(),
                "All session IDs should be unique"
            );
        }
    }

    // =========================================================================
    // SM-5: Idle sessions are cleaned up after TTL
    // =========================================================================

    mod cleanup_tests {
        use super::*;

        #[tokio::test]
        async fn cleanup_removes_idle_sessions() {
            let manager = SessionManager::new(10);

            // Create a session
            let session = manager.create().unwrap();
            let id = session.id.clone();

            // Manipulate last_activity to be in the past
            {
                let mut last_activity = session.last_activity.write().await;
                *last_activity = Instant::now() - Duration::from_secs(120);
            }

            // Session should be idle
            assert!(session.is_idle(60).await, "Session should be idle");

            // Cleanup with 60 second threshold
            let cleaned = manager.cleanup_idle(60).await;

            assert_eq!(cleaned, 1, "Should have cleaned 1 session");
            assert!(!manager.contains(&id), "Session should be removed");
            assert_eq!(manager.count(), 0, "Manager should be empty");
        }

        #[tokio::test]
        async fn cleanup_preserves_active_sessions() {
            let manager = SessionManager::new(10);

            // Create sessions
            let active_session = manager.create().unwrap();
            let idle_session = manager.create().unwrap();

            let active_id = active_session.id.clone();
            let idle_id = idle_session.id.clone();

            // Make one session idle
            {
                let mut last_activity = idle_session.last_activity.write().await;
                *last_activity = Instant::now() - Duration::from_secs(120);
            }

            // Active session should not be idle
            assert!(
                !active_session.is_idle(60).await,
                "Active session should not be idle"
            );

            // Cleanup
            let cleaned = manager.cleanup_idle(60).await;

            assert_eq!(cleaned, 1, "Should clean only idle session");
            assert!(manager.contains(&active_id), "Active session preserved");
            assert!(!manager.contains(&idle_id), "Idle session removed");
        }

        #[tokio::test]
        async fn cleanup_cancels_removed_session() {
            let manager = SessionManager::new(10);

            let session = manager.create().unwrap();
            let cancel_token = session.cancel_token.clone();

            // Make session idle
            {
                let mut last_activity = session.last_activity.write().await;
                *last_activity = Instant::now() - Duration::from_secs(120);
            }

            // Should not be cancelled yet
            assert!(
                !cancel_token.is_cancelled(),
                "Token should not be cancelled before cleanup"
            );

            // Cleanup
            manager.cleanup_idle(60).await;

            // Now should be cancelled
            assert!(
                cancel_token.is_cancelled(),
                "Token should be cancelled after cleanup"
            );
        }

        #[tokio::test]
        async fn touch_prevents_cleanup() {
            let manager = SessionManager::new(10);

            let session = manager.create().unwrap();
            let id = session.id.clone();

            // Make session appear old
            {
                let mut last_activity = session.last_activity.write().await;
                *last_activity = Instant::now() - Duration::from_secs(120);
            }

            // Touch to update activity
            session.touch().await;

            // Should no longer be idle
            assert!(!session.is_idle(60).await, "Touched session not idle");

            // Cleanup should not remove it
            let cleaned = manager.cleanup_idle(60).await;
            assert_eq!(cleaned, 0, "No sessions should be cleaned");
            assert!(manager.contains(&id), "Session should still exist");
        }

        #[tokio::test]
        async fn cleanup_with_no_idle_sessions() {
            let manager = SessionManager::new(10);

            // Create some active sessions
            let _s1 = manager.create().unwrap();
            let _s2 = manager.create().unwrap();

            // Cleanup with very short threshold - but sessions were just created
            // so they shouldn't be idle yet
            let cleaned = manager.cleanup_idle(1000).await;

            assert_eq!(cleaned, 0, "No sessions should be cleaned");
            assert_eq!(manager.count(), 2, "All sessions preserved");
        }
    }

    // =========================================================================
    // Additional tests for edge cases
    // =========================================================================

    mod edge_case_tests {
        use super::*;

        #[test]
        fn get_nonexistent_returns_none() {
            let manager = SessionManager::new(10);
            assert!(manager.get("nonexistent-id").is_none());
        }

        #[test]
        fn remove_nonexistent_returns_none() {
            let manager = SessionManager::new(10);
            assert!(manager.remove("nonexistent-id").is_none());
        }

        #[test]
        fn remove_cancels_token() {
            let manager = SessionManager::new(10);
            let session = manager.create().unwrap();

            let cancel_token = session.cancel_token.clone();
            assert!(!cancel_token.is_cancelled(), "Not cancelled initially");

            manager.remove(&session.id);
            assert!(cancel_token.is_cancelled(), "Cancelled after removal");
        }

        #[test]
        fn session_created_at_is_set() {
            let before = Instant::now();
            let session = Session::new();
            let after = Instant::now();

            // created_at should be between before and after
            assert!(session.created_at >= before);
            assert!(session.created_at <= after);
        }

        #[tokio::test]
        async fn session_last_activity_initialized() {
            let session = Session::new();
            let last = *session.last_activity.read().await;

            // Should be very recent (within 1 second)
            assert!(last.elapsed().as_secs() < 1);
        }

        #[test]
        fn default_max_sessions_constant() {
            assert_eq!(DEFAULT_MAX_SESSIONS, 10);
        }

        #[test]
        fn default_ttl_constant() {
            assert_eq!(DEFAULT_SESSION_TTL_SECS, 30 * 60);
        }
    }
}

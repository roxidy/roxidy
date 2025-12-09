// Commands module for AI agent interaction.
//
// This module provides Tauri command handlers for the AI agent system,
// organized into logical submodules for maintainability.
#![allow(dead_code)]

use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::{mpsc, RwLock};

use super::agent_bridge::AgentBridge;
use super::events::AiEvent;
use crate::runtime::{QbitRuntime, RuntimeEvent, TauriRuntime};
use crate::state::AppState;

pub mod config;
pub mod context;
pub mod core;
pub mod hitl;
pub mod loop_detection;
pub mod policy;
pub mod session;
pub mod workflow;

// Re-export all commands for easier access
pub use config::*;
pub use context::*;
pub use core::*;
pub use hitl::*;
pub use loop_detection::*;
pub use policy::*;
pub use session::*;
pub use workflow::*;

/// Shared AI state.
/// Uses tokio RwLock for async compatibility with AgentBridge methods.
pub struct AiState {
    pub bridge: Arc<RwLock<Option<AgentBridge>>>,
    /// Runtime abstraction for event emission and approval handling.
    /// Stored here for later phases when AgentBridge will use it directly.
    /// Currently created during init but the existing event_tx path is used.
    pub runtime: Arc<RwLock<Option<Arc<dyn QbitRuntime>>>>,
}

impl Default for AiState {
    fn default() -> Self {
        Self {
            bridge: Arc::new(RwLock::new(None)),
            runtime: Arc::new(RwLock::new(None)),
        }
    }
}

/// Error message for uninitialized AI agent.
pub const AI_NOT_INITIALIZED_ERROR: &str = "AI agent not initialized. Call init_ai_agent first.";

impl AiState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a read guard to the bridge, returning an error if not initialized.
    ///
    /// This helper reduces boilerplate in command handlers by providing
    /// a consistent way to access the bridge with proper error handling.
    pub async fn get_bridge(
        &self,
    ) -> Result<tokio::sync::RwLockReadGuard<'_, Option<AgentBridge>>, String> {
        let guard = self.bridge.read().await;
        if guard.is_none() {
            return Err(AI_NOT_INITIALIZED_ERROR.to_string());
        }
        Ok(guard)
    }

    /// Execute a closure with access to the bridge reference.
    ///
    /// This helper eliminates the two-step pattern of `get_bridge().await?.as_ref().unwrap()`.
    /// Only use for synchronous operations. For async operations, use `get_bridge()` directly.
    pub async fn with_bridge<F, T>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce(&AgentBridge) -> T,
    {
        let guard = self.bridge.read().await;
        let bridge = guard.as_ref().ok_or(AI_NOT_INITIALIZED_ERROR)?;
        Ok(f(bridge))
    }
}

/// Spawn an event forwarder task that sends AI events via a runtime abstraction.
///
/// This is the runtime-agnostic version that works with any `QbitRuntime` implementation.
/// Events are wrapped in `RuntimeEvent::Ai` before emission.
///
/// Returns the sender channel for dispatching events.
pub fn spawn_event_forwarder_runtime(
    runtime: Arc<dyn QbitRuntime>,
) -> mpsc::UnboundedSender<AiEvent> {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AiEvent>();

    tokio::spawn(async move {
        while let Some(ai_event) = event_rx.recv().await {
            if let Err(e) = runtime.emit(RuntimeEvent::Ai(Box::new(ai_event))) {
                tracing::warn!("Failed to emit event: {}", e);
            }
        }
        tracing::debug!("Event forwarder shut down");
    });

    event_tx
}

/// Spawn an event forwarder task that sends AI events to the frontend.
///
/// This is the Tauri-specific version that delegates to the runtime-based forwarder.
/// It creates a `TauriRuntime` from the `AppHandle` and uses `spawn_event_forwarder_runtime`.
///
/// Returns the sender channel for dispatching events.
pub fn spawn_event_forwarder(app: AppHandle) -> mpsc::UnboundedSender<AiEvent> {
    let runtime: Arc<dyn QbitRuntime> = Arc::new(TauriRuntime::new(app));
    spawn_event_forwarder_runtime(runtime)
}

/// Configure the agent bridge with shared services from AppState.
pub fn configure_bridge(bridge: &mut AgentBridge, state: &AppState) {
    bridge.set_pty_manager(state.pty_manager.clone());
    bridge.set_indexer_state(state.indexer_state.clone());
    bridge.set_tavily_state(state.tavily_state.clone());
    bridge.set_workflow_state(state.workflow_state.clone());
    bridge.set_sidecar_state(state.sidecar_state.clone());
}

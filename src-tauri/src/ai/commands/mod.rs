// Commands module for AI agent interaction.
//
// This module provides Tauri command handlers for the AI agent system,
// organized into logical submodules for maintainability.

use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, RwLock};

use super::agent_bridge::AgentBridge;
use super::events::AiEvent;
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
#[derive(Default)]
pub struct AiState {
    pub bridge: Arc<RwLock<Option<AgentBridge>>>,
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

/// Spawn an event forwarder task that sends AI events to the frontend.
///
/// Returns the sender channel for dispatching events.
pub fn spawn_event_forwarder(app: AppHandle) -> mpsc::UnboundedSender<AiEvent> {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AiEvent>();

    tokio::spawn(async move {
        while let Some(ai_event) = event_rx.recv().await {
            if let Err(e) = app.emit("ai-event", &ai_event) {
                tracing::error!("Failed to emit AI event: {}", e);
            }
        }
    });

    event_tx
}

/// Configure the agent bridge with shared services from AppState.
pub fn configure_bridge(bridge: &mut AgentBridge, state: &AppState) {
    bridge.set_pty_manager(state.pty_manager.clone());
    bridge.set_indexer_state(state.indexer_state.clone());
    bridge.set_tavily_state(state.tavily_state.clone());
    bridge.set_workflow_state(state.workflow_state.clone());
    bridge.set_sidecar_state(state.sidecar_state.clone());
}

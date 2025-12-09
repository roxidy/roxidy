//! Sidecar Context Capture System
//!
//! A background system that passively captures session context during Qbit agent
//! interactions, stores it semantically, and synthesizes useful outputs (commit
//! messages, documentation, session summaries) on demand.
//!
//! ## Architecture
//!
//! The sidecar operates in multiple layers:
//!
//! ### Layer 0: Raw Event Capture (synchronous, cheap)
//! - No LLM calls, just logging
//! - File changes, tool calls, agent reasoning, user feedback
//! - Persisted to LanceDB with embeddings
//!
//! ### Layer 1: Session State (async, interpreted)
//! - Maintains live session state derived from L0 events
//! - Tracks goals, decisions, file contexts, errors
//! - Provides injectable context for agent system prompts
//! - Uses sidecar LLM for interpretation (with rule-based fallback)
//!
//! ### Periodic Processing (async, batched)
//! - Runs during natural pauses
//! - Embed events for semantic search
//! - Generate checkpoint summaries
//!
//! ### On-Demand Synthesis (user-triggered)
//! - One-shot when user asks
//! - Commit messages, session summaries, history queries

pub mod capture;
#[cfg(feature = "tauri")]
pub mod commands;
pub mod config;
pub mod events;
pub mod layer1;
pub mod models;
pub mod processor;
pub mod prompts;
pub mod state;
pub mod storage;
pub mod synthesis;
pub mod synthesis_llm;

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod schema_verification_test;

pub use capture::CaptureContext;
#[cfg(feature = "tauri")]
pub use commands::*;
pub use state::SidecarState;

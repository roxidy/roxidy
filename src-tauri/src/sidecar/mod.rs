//! Sidecar Context Capture System
//!
//! A background system that passively captures session context during Qbit agent
//! interactions, stores it semantically, and synthesizes useful outputs (commit
//! messages, documentation, session summaries) on demand.
//!
//! ## Architecture
//!
//! The sidecar operates in three layers:
//!
//! 1. **Event Capture** (synchronous, cheap) - No LLM calls, just logging
//!    - File changes, tool calls, agent reasoning, user feedback
//!
//! 2. **Periodic Processing** (async, batched) - Runs during natural pauses
//!    - Embed events for semantic search
//!    - Generate checkpoint summaries
//!
//! 3. **On-Demand Synthesis** (user-triggered) - One-shot when user asks
//!    - Commit messages, session summaries, history queries

pub mod capture;
pub mod commands;
pub mod config;
pub mod events;
pub mod models;
pub mod processor;
pub mod prompts;
pub mod state;
pub mod storage;
pub mod synthesis;
pub mod synthesis_llm;

#[cfg(test)]
mod integration_tests;

pub use capture::CaptureContext;
pub use commands::*;
pub use state::SidecarState;

//! Sidecar Context Capture System
//!
//! A background system that passively captures session context during Qbit agent
//! interactions using a simple markdown-based storage approach.
//!
//! ## Architecture
//!
//! Each session is stored as a directory in `~/.qbit/sessions/{session_id}/`:
//!
//! ```text
//! ~/.qbit/sessions/{session_id}/
//!   meta.toml       # Machine-managed metadata (cwd, git info, timestamps)
//!   state.md        # LLM-managed current state (rewritten on each event)
//!   state.md.bak    # Previous state backup (for recovery)
//!   log.md          # Append-only event log with diffs
//!   events.jsonl    # Raw events (optional, for future use)
//! ```
//!
//! ### state.md
//! The current session state, maintained by an LLM. Contains:
//! - Current goal and sub-goals
//! - Narrative summary of progress
//! - Files in focus
//! - Open questions
//!
//! ### log.md
//! Chronological append-only log of events with timestamps and diffs.
//! Used for commit synthesis and audit trail.
//!
//! ### meta.toml
//! Machine-managed metadata that shouldn't be touched by LLM:
//! - Session ID, timestamps, status
//! - Working directory, git info
//! - Initial request

pub mod capture;
#[cfg(feature = "tauri")]
pub mod commands;
pub mod config;
pub mod events;
pub mod formats;
pub mod processor;
pub mod session;
pub mod state;

pub use capture::CaptureContext;
#[cfg(feature = "tauri")]
pub use commands::*;
#[allow(unused_imports)]
pub use config::SidecarConfig;

pub use state::SidecarState;

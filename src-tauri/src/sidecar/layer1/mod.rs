//! Layer 1: Session State Implementation
//!
//! Note: Many items in this module are public API intended for future use.
#![allow(dead_code)]
//!
//! This layer maintains a continuously-updated session state model derived from
//! L0 raw events. It provides a live understanding of the current session that
//! can be injected into agent context.
//!
//! ## Architecture
//!
//! ```text
//! L0 Events → StateProcessor → SessionState (in-memory)
//!                   ↓
//!             LanceDB (snapshots)
//! ```
//!
//! ## Key Components
//!
//! - **SessionState**: Root state model containing goals, narrative, decisions, etc.
//! - **StateProcessor**: Subscribes to L0 events and updates state via sidecar LLM
//! - **StateStorage**: Persists state snapshots to LanceDB
//! - **API**: Public functions for querying state and getting injectable context

pub mod api;
pub mod events;
pub mod processor;
pub mod prompt;
pub mod state;
pub mod storage;

#[cfg(test)]
mod verification_tests;

pub use api::*;
pub use events::Layer1Event;
pub use processor::{Layer1Config, Layer1Processor, Layer1Task};
pub use state::{
    // Main state types
    Decision,
    ErrorEntry,
    FileContext,
    Goal,
    OpenQuestion,
    SessionState,
};
pub use storage::Layer1Storage;

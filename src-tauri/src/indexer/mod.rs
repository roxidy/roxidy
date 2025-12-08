//! Code indexer module for semantic code analysis
//!
//! Integrates vtcode-indexer and vtcode-core's tree-sitter capabilities
//! to provide intelligent code understanding, navigation, and symbol extraction.

#[cfg(feature = "tauri")]
pub mod commands;
pub mod state;

#[cfg(feature = "tauri")]
pub use commands::*;
pub use state::IndexerState;

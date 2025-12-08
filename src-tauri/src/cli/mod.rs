//! CLI module for Qbit headless operation.
//!
//! This module provides a command-line interface that uses the same
//! services as the Tauri GUI application, enabling automated testing
//! and scripting.
//!
//! # Architecture
//!
//! The CLI uses the `QbitRuntime` abstraction to share code with the
//! Tauri application. Instead of emitting events to the frontend via
//! Tauri's event system, the CLI runtime sends events through a channel
//! that is consumed by the output handler.
//!
//! ```text
//! +-----------------+     +-------------+     +---------------+
//! | AgentBridge     | --> | CliRuntime  | --> | output.rs     |
//! | (shared logic)  |     | (emit())    |     | (print/JSON)  |
//! +-----------------+     +-------------+     +---------------+
//! ```
//!
//! # REPL Mode
//!
//! When no prompt is provided via `-e` or `-f`, the CLI enters
//! interactive REPL mode. See `repl.rs` for details.

mod args;
mod bootstrap;
mod output;
mod repl;
mod runner;

pub use args::Args;
pub use bootstrap::{initialize, CliContext};
pub use output::run_event_loop;
pub use repl::run_repl;
pub use runner::{execute_batch, execute_once};

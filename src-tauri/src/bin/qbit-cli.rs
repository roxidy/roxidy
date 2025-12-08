//! Qbit CLI - Headless interface for the Qbit AI agent
//!
//! This binary provides a command-line interface to the Qbit agent,
//! enabling automated testing, scripting, and headless operation.
//!
//! # Usage
//!
//! ```bash
//! # Build the CLI binary
//! cargo build --package qbit --features cli --no-default-features --bin qbit-cli
//!
//! # Execute a single prompt
//! ./target/debug/qbit-cli -e "What files are in this directory?"
//!
//! # With auto-approval for testing
//! ./target/debug/qbit-cli -e "Read Cargo.toml" --auto-approve
//!
//! # JSON output for scripting
//! ./target/debug/qbit-cli -e "Hello" --json --auto-approve | jq .
//!
//! # Quiet mode - only final response
//! ./target/debug/qbit-cli -e "What is 2+2?" --quiet --auto-approve
//!
//! # Interactive REPL mode (when no -e or -f provided)
//! ./target/debug/qbit-cli --auto-approve
//! ```
//!
//! # Features
//!
//! This binary requires the `cli` feature flag and is mutually exclusive
//! with the `tauri` feature (GUI application).

use anyhow::Result;
use clap::Parser;

use qbit_lib::cli::{execute_batch, execute_once, initialize, run_repl, Args};

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize the full Qbit stack
    let mut ctx = initialize(&args).await?;

    // Execute based on mode
    let result = if let Some(ref prompt) = args.execute {
        // Single prompt execution mode
        execute_once(&mut ctx, prompt).await
    } else if let Some(ref file) = args.file {
        // Batch file execution mode
        execute_batch(&mut ctx, file).await
    } else {
        // No prompt provided - enter interactive REPL mode
        run_repl(&mut ctx).await
    };

    // Graceful shutdown
    ctx.shutdown().await?;

    result
}

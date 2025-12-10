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
//! # Server Mode
//!
//! When built with the `server` feature, the CLI can also run as an HTTP server:
//!
//! ```bash
//! # Build with server feature
//! cargo build --package qbit --features server --no-default-features --bin qbit-cli
//!
//! # Run as HTTP server
//! ./target/debug/qbit-cli --server --port 8080
//!
//! # Use port 0 for random available port
//! ./target/debug/qbit-cli --server --port 0
//! ```
//!
//! # Features
//!
//! This binary requires the `cli` feature flag and is mutually exclusive
//! with the `tauri` feature (GUI application). The `server` feature extends
//! CLI mode with HTTP/SSE endpoints for eval framework integration.

use anyhow::Result;
use clap::Parser;

use qbit_lib::cli::{execute_batch, execute_once, initialize, run_repl, Args};

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Server mode - start HTTP server instead of CLI
    #[cfg(feature = "server")]
    if args.server {
        let workspace = args.resolve_workspace()?;
        let (addr, _shutdown_token) =
            qbit_lib::cli::server::start_server(args.port, workspace, args.max_sessions).await?;

        // Print bound address (useful for port=0)
        println!("Server listening on http://{}", addr);

        // Wait for shutdown signal
        tokio::signal::ctrl_c().await?;
        println!("\nShutting down...");
        return Ok(());
    }

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

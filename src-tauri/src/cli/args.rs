//! CLI argument parsing using clap.
//!
//! Defines the command-line interface for qbit-cli.

use clap::Parser;
use std::path::PathBuf;

/// Qbit CLI - Headless interface for the Qbit AI agent
#[derive(Parser, Debug, Clone)]
#[command(name = "qbit-cli")]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Working directory (default: current directory)
    #[arg(default_value = ".")]
    pub workspace: PathBuf,

    /// Execute a single prompt and exit
    #[arg(short = 'e', long, conflicts_with = "file")]
    pub execute: Option<String>,

    /// Execute prompts from a file (one per line) and exit
    #[arg(short = 'f', long, conflicts_with = "execute")]
    pub file: Option<PathBuf>,

    /// Override AI provider from settings
    ///
    /// Options: vertex_ai, openrouter, anthropic, openai
    #[arg(short = 'p', long)]
    pub provider: Option<String>,

    /// Override model from settings
    #[arg(short = 'm', long)]
    pub model: Option<String>,

    /// API key (overrides settings and env vars)
    #[arg(long, env = "QBIT_API_KEY")]
    pub api_key: Option<String>,

    /// Auto-approve all tool calls (DANGEROUS: for testing only)
    #[arg(long)]
    pub auto_approve: bool,

    /// Output events as JSON lines (for scripting/parsing)
    #[arg(long)]
    pub json: bool,

    /// Only output final response (suppress streaming)
    #[arg(long, short = 'q')]
    pub quiet: bool,

    /// Show verbose output (debug information)
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// Run as HTTP server for eval framework
    #[cfg(feature = "server")]
    #[arg(long, help = "Run as HTTP server")]
    pub server: bool,

    /// Server port (use 0 for random available port)
    #[cfg(feature = "server")]
    #[arg(long, default_value = "8080", help = "Server port")]
    pub port: u16,

    /// Maximum concurrent sessions
    #[cfg(feature = "server")]
    #[arg(long, default_value = "10", help = "Maximum concurrent sessions")]
    pub max_sessions: usize,
}

impl Args {
    /// Resolve the workspace path to an absolute path.
    ///
    /// Returns an error if the path does not exist or is not a directory.
    pub fn resolve_workspace(&self) -> anyhow::Result<PathBuf> {
        let canonical = self.workspace.canonicalize().map_err(|e| {
            anyhow::anyhow!(
                "Workspace '{}' does not exist or is not accessible: {}",
                self.workspace.display(),
                e
            )
        })?;

        if !canonical.is_dir() {
            anyhow::bail!("Workspace '{}' is not a directory", canonical.display());
        }

        Ok(canonical)
    }

    /// Create Args for a server session.
    ///
    /// This is used when creating CLI contexts for HTTP server sessions.
    /// Server sessions always have auto_approve=true and json=true for SSE.
    #[cfg(feature = "server")]
    pub fn for_server_session(workspace: PathBuf, auto_approve: bool) -> Self {
        Self {
            workspace,
            execute: None,
            file: None,
            provider: None,
            model: None,
            api_key: None,
            auto_approve,
            json: true, // Server mode uses JSON for SSE events
            quiet: false,
            verbose: false,
            server: false,
            port: 0,
            max_sessions: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_default_values() {
        let args = Args::parse_from(["qbit-cli"]);
        assert_eq!(args.workspace, PathBuf::from("."));
        assert!(!args.auto_approve);
        assert!(!args.json);
        assert!(!args.quiet);
        assert!(!args.verbose);
    }

    #[test]
    fn test_args_execute_flag() {
        let args = Args::parse_from(["qbit-cli", "-e", "Hello world"]);
        assert_eq!(args.execute, Some("Hello world".to_string()));
    }

    #[test]
    fn test_args_provider_and_model() {
        let args = Args::parse_from([
            "qbit-cli",
            "-p",
            "openrouter",
            "-m",
            "anthropic/claude-sonnet-4",
        ]);
        assert_eq!(args.provider, Some("openrouter".to_string()));
        assert_eq!(args.model, Some("anthropic/claude-sonnet-4".to_string()));
    }

    #[test]
    fn test_args_output_modes() {
        let args = Args::parse_from(["qbit-cli", "--json", "--quiet"]);
        assert!(args.json);
        assert!(args.quiet);
    }

    #[test]
    fn test_args_auto_approve() {
        let args = Args::parse_from(["qbit-cli", "--auto-approve"]);
        assert!(args.auto_approve);
    }

    #[cfg(feature = "server")]
    mod server_tests {
        use super::*;

        #[test]
        fn test_args_server_defaults() {
            let args = Args::parse_from(["qbit-cli"]);
            assert!(!args.server);
            assert_eq!(args.port, 8080);
            assert_eq!(args.max_sessions, 10);
        }

        #[test]
        fn test_args_server_flag() {
            let args = Args::parse_from(["qbit-cli", "--server"]);
            assert!(args.server);
        }

        #[test]
        fn test_args_server_port() {
            let args = Args::parse_from(["qbit-cli", "--server", "--port", "3000"]);
            assert!(args.server);
            assert_eq!(args.port, 3000);
        }

        #[test]
        fn test_args_server_port_zero() {
            let args = Args::parse_from(["qbit-cli", "--server", "--port", "0"]);
            assert!(args.server);
            assert_eq!(args.port, 0);
        }

        #[test]
        fn test_args_server_max_sessions() {
            let args = Args::parse_from(["qbit-cli", "--server", "--max-sessions", "20"]);
            assert!(args.server);
            assert_eq!(args.max_sessions, 20);
        }

        #[test]
        fn test_args_server_all_options() {
            let args = Args::parse_from([
                "qbit-cli",
                "--server",
                "--port",
                "9000",
                "--max-sessions",
                "5",
            ]);
            assert!(args.server);
            assert_eq!(args.port, 9000);
            assert_eq!(args.max_sessions, 5);
        }
    }
}

//! Lightweight REPL (Read-Eval-Print-Loop) for qbit-cli.
//!
//! Provides an interactive mode when no prompt is provided via `-e` or `-f`.
//! Supports minimal commands:
//! - `/quit`, `/exit`, `/q` - Exit the REPL
//!
//! Any other input is sent as a prompt to the agent.

use std::io::{self, BufRead, Write};

use anyhow::Result;

use super::bootstrap::CliContext;
use super::runner::execute_once;

/// REPL command variants.
#[derive(Debug, Clone, PartialEq)]
pub enum ReplCommand {
    /// Exit the REPL
    Quit,
    /// Unknown command (will show help)
    Unknown(String),
    /// Regular prompt to send to the agent
    Prompt(String),
    /// Empty input (skip)
    Empty,
}

impl ReplCommand {
    /// Parse user input into a REPL command.
    pub fn parse(input: &str) -> Self {
        let trimmed = input.trim();

        if trimmed.is_empty() {
            return ReplCommand::Empty;
        }

        if trimmed.starts_with('/') {
            match trimmed.to_lowercase().as_str() {
                "/quit" | "/exit" | "/q" => ReplCommand::Quit,
                _ => ReplCommand::Unknown(trimmed.to_string()),
            }
        } else {
            ReplCommand::Prompt(trimmed.to_string())
        }
    }
}

/// Run an interactive REPL session.
///
/// Supports:
/// - `/quit`, `/exit`, `/q` - Exit the REPL
/// - Any other input - Send as prompt to agent
///
/// Returns when the user exits or on EOF (Ctrl+D).
pub async fn run_repl(ctx: &mut CliContext) -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    // Print banner
    eprintln!("qbit-cli interactive mode");
    eprintln!("Type /quit to exit\n");

    loop {
        // Print prompt
        print!("> ");
        stdout.flush()?;

        // Read line
        let mut input = String::new();
        if stdin.lock().read_line(&mut input)? == 0 {
            // EOF (Ctrl+D)
            eprintln!("\nGoodbye!");
            break;
        }

        // Parse and handle command
        match ReplCommand::parse(&input) {
            ReplCommand::Empty => {
                continue;
            }
            ReplCommand::Quit => {
                eprintln!("Goodbye!");
                break;
            }
            ReplCommand::Unknown(cmd) => {
                eprintln!("Unknown command: {}", cmd);
                eprintln!("Available: /quit, /exit, /q");
                continue;
            }
            ReplCommand::Prompt(prompt) => {
                // Execute prompt via agent
                if let Err(e) = execute_once(ctx, &prompt).await {
                    eprintln!("Error: {}", e);
                }

                println!(); // Blank line between interactions
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ────────────────────────────────────────────────────────────────────────────────
    // Tests for ReplCommand::parse
    // ────────────────────────────────────────────────────────────────────────────────

    mod parse_tests {
        use super::*;

        #[test]
        fn parses_quit_command() {
            assert_eq!(ReplCommand::parse("/quit"), ReplCommand::Quit);
        }

        #[test]
        fn parses_exit_command() {
            assert_eq!(ReplCommand::parse("/exit"), ReplCommand::Quit);
        }

        #[test]
        fn parses_q_command() {
            assert_eq!(ReplCommand::parse("/q"), ReplCommand::Quit);
        }

        #[test]
        fn parses_quit_case_insensitive() {
            assert_eq!(ReplCommand::parse("/QUIT"), ReplCommand::Quit);
            assert_eq!(ReplCommand::parse("/Quit"), ReplCommand::Quit);
            assert_eq!(ReplCommand::parse("/EXIT"), ReplCommand::Quit);
            assert_eq!(ReplCommand::parse("/Q"), ReplCommand::Quit);
        }

        #[test]
        fn parses_unknown_slash_command() {
            assert_eq!(
                ReplCommand::parse("/help"),
                ReplCommand::Unknown("/help".to_string())
            );
            assert_eq!(
                ReplCommand::parse("/foo"),
                ReplCommand::Unknown("/foo".to_string())
            );
            assert_eq!(
                ReplCommand::parse("/tools"),
                ReplCommand::Unknown("/tools".to_string())
            );
        }

        #[test]
        fn parses_regular_prompt() {
            assert_eq!(
                ReplCommand::parse("Hello world"),
                ReplCommand::Prompt("Hello world".to_string())
            );
        }

        #[test]
        fn parses_prompt_with_slash_in_middle() {
            // Slash in middle should not be treated as command
            assert_eq!(
                ReplCommand::parse("Read /tmp/file.txt"),
                ReplCommand::Prompt("Read /tmp/file.txt".to_string())
            );
        }

        #[test]
        fn parses_empty_input() {
            assert_eq!(ReplCommand::parse(""), ReplCommand::Empty);
            assert_eq!(ReplCommand::parse("   "), ReplCommand::Empty);
            assert_eq!(ReplCommand::parse("\t\n"), ReplCommand::Empty);
        }

        #[test]
        fn trims_whitespace_from_prompt() {
            assert_eq!(
                ReplCommand::parse("  hello  "),
                ReplCommand::Prompt("hello".to_string())
            );
        }

        #[test]
        fn trims_whitespace_from_command() {
            assert_eq!(ReplCommand::parse("  /quit  "), ReplCommand::Quit);
        }

        #[test]
        fn handles_newline_in_input() {
            // This simulates input from stdin with trailing newline
            assert_eq!(
                ReplCommand::parse("hello\n"),
                ReplCommand::Prompt("hello".to_string())
            );
            assert_eq!(ReplCommand::parse("/quit\n"), ReplCommand::Quit);
        }
    }
}

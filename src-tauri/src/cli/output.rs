//! CLI output handling - Event receiver loop.
//!
//! This module is CRITICAL for the CLI to work. It receives events from the
//! agent via the runtime channel and renders them appropriately based on
//! output mode (terminal, JSON, or quiet).

use std::io::{self, Write};

use anyhow::Result;
use tokio::sync::mpsc;

use crate::ai::events::AiEvent;
use crate::runtime::RuntimeEvent;

/// Run the event loop, processing events until completion or error.
///
/// This function consumes the event receiver and processes events until
/// it sees a `Completed` or `Error` event from the AI agent.
///
/// # Arguments
///
/// * `event_rx` - Channel receiver for runtime events
/// * `json_mode` - If true, output events as JSON lines
/// * `quiet_mode` - If true, only output final response
pub async fn run_event_loop(
    mut event_rx: mpsc::UnboundedReceiver<RuntimeEvent>,
    json_mode: bool,
    quiet_mode: bool,
) -> Result<()> {
    while let Some(event) = event_rx.recv().await {
        match event {
            RuntimeEvent::Ai(ai_event) => {
                let should_break = handle_ai_event(&ai_event, json_mode, quiet_mode)?;
                if should_break {
                    break;
                }
            }
            RuntimeEvent::TerminalOutput { data, .. } => {
                // Write terminal output directly to stdout
                if !quiet_mode && !json_mode {
                    io::stdout().write_all(&data)?;
                    io::stdout().flush()?;
                }
            }
            RuntimeEvent::TerminalExit { session_id, code } => {
                if json_mode {
                    let json = serde_json::json!({
                        "type": "terminal_exit",
                        "session_id": session_id,
                        "code": code
                    });
                    println!("{}", json);
                }
            }
            RuntimeEvent::Custom { name, payload } => {
                if json_mode {
                    let json = serde_json::json!({
                        "type": "custom",
                        "name": name,
                        "payload": payload
                    });
                    println!("{}", json);
                }
            }
        }
    }

    Ok(())
}

/// Handle an AI event, returning true if the loop should exit.
fn handle_ai_event(event: &AiEvent, json_mode: bool, quiet_mode: bool) -> Result<bool> {
    if json_mode {
        // JSON mode: output each event as a JSON line
        println!("{}", serde_json::to_string(event)?);
        io::stdout().flush()?;
    } else if !quiet_mode {
        // Terminal mode: pretty-print events
        handle_ai_event_terminal(event)?;
    }

    // Check for completion/error events
    match event {
        AiEvent::Completed { response, .. } => {
            if quiet_mode && !json_mode {
                // In quiet mode, only print the final response
                println!("{}", response);
            } else if !json_mode {
                // Ensure we end with a newline after streaming
                println!();
            }
            Ok(true) // Exit loop
        }
        AiEvent::Error { message, .. } => {
            if !json_mode {
                eprintln!("Error: {}", message);
            }
            Ok(true) // Exit loop
        }
        _ => Ok(false), // Continue loop
    }
}

/// Handle AI events for terminal (non-JSON) output.
fn handle_ai_event_terminal(event: &AiEvent) -> Result<()> {
    match event {
        AiEvent::Started { .. } => {
            // Optionally show a spinner or indicator
        }
        AiEvent::TextDelta { delta, .. } => {
            // Stream text as it arrives
            print!("{}", delta);
            io::stdout().flush()?;
        }
        AiEvent::ToolRequest {
            tool_name, args, ..
        } => {
            eprintln!(
                "\n[tool] {} {}",
                tool_name,
                format_args_summary(args)
            );
        }
        AiEvent::ToolApprovalRequest {
            tool_name,
            args,
            risk_level,
            ..
        } => {
            eprintln!(
                "\n[{:?}] {} {}",
                risk_level,
                tool_name,
                format_args_summary(args)
            );
        }
        AiEvent::ToolAutoApproved {
            tool_name, reason, ..
        } => {
            eprintln!("[auto-approved] {} ({})", tool_name, reason);
        }
        AiEvent::ToolDenied {
            tool_name, reason, ..
        } => {
            eprintln!("[denied] {} ({})", tool_name, reason);
        }
        AiEvent::ToolResult {
            tool_name, success, ..
        } => {
            let icon = if *success { "ok" } else { "err" };
            eprintln!("[{}] {}", icon, tool_name);
        }
        AiEvent::Reasoning { content } => {
            // Show reasoning in a distinctive way
            eprintln!("[thinking] {}", truncate(content, 100));
        }
        AiEvent::SubAgentStarted {
            agent_name, task, ..
        } => {
            eprintln!("[sub-agent] {} starting: {}", agent_name, truncate(task, 80));
        }
        AiEvent::SubAgentCompleted {
            agent_id: _,
            response,
            duration_ms,
        } => {
            eprintln!(
                "[sub-agent] completed in {}ms: {}",
                duration_ms,
                truncate(response, 80)
            );
        }
        AiEvent::SubAgentError { agent_id, error } => {
            eprintln!("[sub-agent] {} error: {}", agent_id, error);
        }
        AiEvent::ContextWarning {
            utilization,
            total_tokens,
            max_tokens,
        } => {
            eprintln!(
                "[context] Warning: {:.1}% used ({}/{})",
                utilization * 100.0,
                total_tokens,
                max_tokens
            );
        }
        AiEvent::LoopWarning {
            tool_name,
            current_count,
            max_count,
            ..
        } => {
            eprintln!(
                "[loop] Warning: {} called {}/{} times",
                tool_name, current_count, max_count
            );
        }
        AiEvent::LoopBlocked {
            tool_name, message, ..
        } => {
            eprintln!("[loop] Blocked: {} - {}", tool_name, message);
        }
        AiEvent::WorkflowStarted { workflow_name, .. } => {
            eprintln!("[workflow] Starting: {}", workflow_name);
        }
        AiEvent::WorkflowStepStarted {
            step_name,
            step_index,
            total_steps,
            ..
        } => {
            eprintln!(
                "[workflow] Step {}/{}: {}",
                step_index + 1,
                total_steps,
                step_name
            );
        }
        AiEvent::WorkflowCompleted {
            final_output,
            total_duration_ms,
            ..
        } => {
            eprintln!(
                "[workflow] Completed in {}ms: {}",
                total_duration_ms,
                truncate(final_output, 100)
            );
        }
        AiEvent::WorkflowError { error, .. } => {
            eprintln!("[workflow] Error: {}", error);
        }
        // Events handled in the main match or not displayed in terminal mode
        AiEvent::Completed { .. } | AiEvent::Error { .. } => {}
        _ => {}
    }

    Ok(())
}

/// Format tool arguments for display (truncated summary).
fn format_args_summary(args: &serde_json::Value) -> String {
    let s = args.to_string();
    if s.len() > 60 {
        format!("{}...", &s[..57])
    } else {
        s
    }
}

/// Truncate a string to a maximum length.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world!", 8), "hello...");
    }

    #[test]
    fn test_format_args_summary() {
        let short = serde_json::json!({"path": "/tmp"});
        assert_eq!(format_args_summary(&short), r#"{"path":"/tmp"}"#);

        let long = serde_json::json!({
            "path": "/very/long/path/to/some/file/that/exceeds/the/limit.txt",
            "content": "some content"
        });
        let summary = format_args_summary(&long);
        assert!(summary.len() <= 63); // 60 + "..."
        assert!(summary.ends_with("..."));
    }
}

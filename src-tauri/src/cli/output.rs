//! CLI output handling - Event receiver loop.
//!
//! This module is CRITICAL for the CLI to work. It receives events from the
//! agent via the runtime channel and renders them appropriately based on
//! output mode (terminal, JSON, or quiet).
//!
//! ## Output Modes
//!
//! - **Terminal mode**: Human-readable output with box-drawing formatting
//! - **JSON mode**: Standardized JSONL format for programmatic parsing (NO TRUNCATION)
//! - **Quiet mode**: Only final response output
//!
//! ## Truncation Policy
//!
//! | Output Mode | Tool Input | Tool Output | Reasoning | Text Deltas |
//! |-------------|------------|-------------|-----------|-------------|
//! | Terminal    | No trunc   | 500 chars   | 2000 chars| No trunc    |
//! | JSON        | No trunc   | No trunc    | No trunc  | No trunc    |
//! | Quiet       | Not shown  | Not shown   | Not shown | Final only  |

use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::Serialize;
use tokio::sync::mpsc;

use crate::ai::events::AiEvent;
use crate::runtime::RuntimeEvent;

// ────────────────────────────────────────────────────────────────────────────────
// Constants for terminal mode truncation
// ────────────────────────────────────────────────────────────────────────────────

/// Maximum characters for tool output in terminal mode
const TERMINAL_TOOL_OUTPUT_MAX: usize = 500;

/// Maximum characters for reasoning content in terminal mode
const TERMINAL_REASONING_MAX: usize = 2000;

// ────────────────────────────────────────────────────────────────────────────────
// Standardized CLI JSON Event Format
// ────────────────────────────────────────────────────────────────────────────────

/// Standardized JSON output event for CLI.
///
/// This provides a consistent format for all CLI events that is easy to parse
/// in evaluation frameworks and scripts. Key differences from raw AiEvent:
///
/// - Uses `event` field instead of `type`
/// - Adds `timestamp` to all events
/// - Renames `args` to `input` and `result` to `output` for tool events
/// - NO TRUNCATION of any data (truncation only happens in terminal mode)
#[derive(Debug, Serialize)]
pub struct CliJsonEvent {
    /// Event type (started, text_delta, tool_call, tool_result, reasoning, completed, error, etc.)
    event: String,

    /// Unix timestamp in milliseconds
    timestamp: u64,

    /// Event-specific data (flattened into the top-level object)
    #[serde(flatten)]
    data: serde_json::Value,
}

impl CliJsonEvent {
    /// Create a new CLI JSON event with current timestamp.
    fn new(event: &str, data: serde_json::Value) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Self {
            event: event.to_string(),
            timestamp,
            data,
        }
    }
}

/// Convert an AiEvent to the standardized CLI JSON format.
///
/// IMPORTANT: This function does NOT truncate any data. All tool inputs,
/// outputs, reasoning content, and text deltas are passed through completely.
/// Truncation is only applied in terminal mode for readability.
pub fn convert_to_cli_json(event: &AiEvent) -> CliJsonEvent {
    match event {
        AiEvent::Started { turn_id } => {
            CliJsonEvent::new("started", serde_json::json!({ "turn_id": turn_id }))
        }

        AiEvent::TextDelta { delta, accumulated } => CliJsonEvent::new(
            "text_delta",
            serde_json::json!({
                "delta": delta,
                "accumulated": accumulated
            }),
        ),

        AiEvent::ToolRequest {
            tool_name,
            args,
            request_id,
            source,
        } => CliJsonEvent::new(
            "tool_call",
            serde_json::json!({
                "tool_name": tool_name,
                "input": args,  // Renamed from "args"
                "request_id": request_id,
                "source": source
            }),
        ),

        AiEvent::ToolApprovalRequest {
            request_id,
            tool_name,
            args,
            stats,
            risk_level,
            can_learn,
            suggestion,
            source,
        } => CliJsonEvent::new(
            "tool_approval",
            serde_json::json!({
                "request_id": request_id,
                "tool_name": tool_name,
                "input": args,  // Renamed from "args"
                "stats": stats,
                "risk_level": risk_level,
                "can_learn": can_learn,
                "suggestion": suggestion,
                "source": source
            }),
        ),

        AiEvent::ToolAutoApproved {
            request_id,
            tool_name,
            args,
            reason,
            source,
        } => CliJsonEvent::new(
            "tool_auto_approved",
            serde_json::json!({
                "request_id": request_id,
                "tool_name": tool_name,
                "input": args,  // Renamed from "args"
                "reason": reason,
                "source": source
            }),
        ),

        AiEvent::ToolDenied {
            request_id,
            tool_name,
            args,
            reason,
            source,
        } => CliJsonEvent::new(
            "tool_denied",
            serde_json::json!({
                "request_id": request_id,
                "tool_name": tool_name,
                "input": args,  // Renamed from "args"
                "reason": reason,
                "source": source
            }),
        ),

        AiEvent::ToolResult {
            tool_name,
            result,
            success,
            request_id,
            source,
        } => CliJsonEvent::new(
            "tool_result",
            serde_json::json!({
                "tool_name": tool_name,
                "output": result,  // Renamed from "result"
                "success": success,
                "request_id": request_id,
                "source": source
            }),
        ),

        AiEvent::Reasoning { content } => {
            CliJsonEvent::new("reasoning", serde_json::json!({ "content": content }))
        }

        AiEvent::Completed {
            response,
            tokens_used,
            duration_ms,
        } => CliJsonEvent::new(
            "completed",
            serde_json::json!({
                "response": response,
                "tokens_used": tokens_used,
                "duration_ms": duration_ms
            }),
        ),

        AiEvent::Error {
            message,
            error_type,
        } => CliJsonEvent::new(
            "error",
            serde_json::json!({
                "message": message,
                "error_type": error_type
            }),
        ),

        // Sub-agent events
        AiEvent::SubAgentStarted {
            agent_id,
            agent_name,
            task,
            depth,
        } => CliJsonEvent::new(
            "sub_agent_started",
            serde_json::json!({
                "agent_id": agent_id,
                "agent_name": agent_name,
                "task": task,
                "depth": depth
            }),
        ),

        AiEvent::SubAgentToolRequest {
            agent_id,
            tool_name,
            args,
        } => CliJsonEvent::new(
            "sub_agent_tool_request",
            serde_json::json!({
                "agent_id": agent_id,
                "tool_name": tool_name,
                "input": args  // Renamed from "args"
            }),
        ),

        AiEvent::SubAgentToolResult {
            agent_id,
            tool_name,
            success,
        } => CliJsonEvent::new(
            "sub_agent_tool_result",
            serde_json::json!({
                "agent_id": agent_id,
                "tool_name": tool_name,
                "success": success
            }),
        ),

        AiEvent::SubAgentCompleted {
            agent_id,
            response,
            duration_ms,
        } => CliJsonEvent::new(
            "sub_agent_completed",
            serde_json::json!({
                "agent_id": agent_id,
                "response": response,
                "duration_ms": duration_ms
            }),
        ),

        AiEvent::SubAgentError { agent_id, error } => CliJsonEvent::new(
            "sub_agent_error",
            serde_json::json!({
                "agent_id": agent_id,
                "error": error
            }),
        ),

        // Context management events
        AiEvent::ContextPruned {
            messages_removed,
            utilization_before,
            utilization_after,
        } => CliJsonEvent::new(
            "context_pruned",
            serde_json::json!({
                "messages_removed": messages_removed,
                "utilization_before": utilization_before,
                "utilization_after": utilization_after
            }),
        ),

        AiEvent::ContextWarning {
            utilization,
            total_tokens,
            max_tokens,
        } => CliJsonEvent::new(
            "context_warning",
            serde_json::json!({
                "utilization": utilization,
                "total_tokens": total_tokens,
                "max_tokens": max_tokens
            }),
        ),

        AiEvent::ToolResponseTruncated {
            tool_name,
            original_tokens,
            truncated_tokens,
        } => CliJsonEvent::new(
            "tool_response_truncated",
            serde_json::json!({
                "tool_name": tool_name,
                "original_tokens": original_tokens,
                "truncated_tokens": truncated_tokens
            }),
        ),

        // Loop protection events
        AiEvent::LoopWarning {
            tool_name,
            current_count,
            max_count,
            message,
        } => CliJsonEvent::new(
            "loop_warning",
            serde_json::json!({
                "tool_name": tool_name,
                "current_count": current_count,
                "max_count": max_count,
                "message": message
            }),
        ),

        AiEvent::LoopBlocked {
            tool_name,
            repeat_count,
            max_count,
            message,
        } => CliJsonEvent::new(
            "loop_blocked",
            serde_json::json!({
                "tool_name": tool_name,
                "repeat_count": repeat_count,
                "max_count": max_count,
                "message": message
            }),
        ),

        AiEvent::MaxIterationsReached {
            iterations,
            max_iterations,
            message,
        } => CliJsonEvent::new(
            "max_iterations_reached",
            serde_json::json!({
                "iterations": iterations,
                "max_iterations": max_iterations,
                "message": message
            }),
        ),

        // Workflow events
        AiEvent::WorkflowStarted {
            workflow_id,
            workflow_name,
            session_id,
        } => CliJsonEvent::new(
            "workflow_started",
            serde_json::json!({
                "workflow_id": workflow_id,
                "workflow_name": workflow_name,
                "session_id": session_id
            }),
        ),

        AiEvent::WorkflowStepStarted {
            workflow_id,
            step_name,
            step_index,
            total_steps,
        } => CliJsonEvent::new(
            "workflow_step_started",
            serde_json::json!({
                "workflow_id": workflow_id,
                "step_name": step_name,
                "step_index": step_index,
                "total_steps": total_steps
            }),
        ),

        AiEvent::WorkflowStepCompleted {
            workflow_id,
            step_name,
            output,
            duration_ms,
        } => CliJsonEvent::new(
            "workflow_step_completed",
            serde_json::json!({
                "workflow_id": workflow_id,
                "step_name": step_name,
                "output": output,
                "duration_ms": duration_ms
            }),
        ),

        AiEvent::WorkflowCompleted {
            workflow_id,
            final_output,
            total_duration_ms,
        } => CliJsonEvent::new(
            "workflow_completed",
            serde_json::json!({
                "workflow_id": workflow_id,
                "final_output": final_output,
                "total_duration_ms": total_duration_ms
            }),
        ),

        AiEvent::WorkflowError {
            workflow_id,
            step_name,
            error,
        } => CliJsonEvent::new(
            "workflow_error",
            serde_json::json!({
                "workflow_id": workflow_id,
                "step_name": step_name,
                "error": error
            }),
        ),
    }
}

// ────────────────────────────────────────────────────────────────────────────────
// Helper functions
// ────────────────────────────────────────────────────────────────────────────────

/// Format a JSON value with pretty printing (indented).
fn format_json_pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// Truncate a string to a maximum number of characters.
///
/// This is used for terminal mode output only. JSON mode does NOT truncate.
/// Handles unicode correctly by iterating over chars rather than bytes.
fn truncate_output(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars).collect()
    }
}

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
        // JSON mode: output standardized CLI JSON format (NO TRUNCATION)
        let cli_json = convert_to_cli_json(event);
        println!("{}", serde_json::to_string(&cli_json)?);
        io::stdout().flush()?;
    } else if !quiet_mode {
        // Terminal mode: pretty-print events with box-drawing format
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
///
/// Uses box-drawing characters for enhanced readability. Tool inputs are shown
/// in full, while tool outputs and reasoning are truncated for terminal display.
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
        // ─── Tool Request (box-drawing format with full input) ───
        AiEvent::ToolRequest {
            tool_name, args, ..
        } => {
            eprintln!();
            eprintln!("\x1b[2m{}\x1b[0m tool: {}", BOX_TOP, tool_name);
            eprintln!("\x1b[2m{}\x1b[0m input:", BOX_MID);
            for line in format_json_pretty(args).lines() {
                eprintln!("\x1b[2m{}\x1b[0m   {}", BOX_MID, line);
            }
            eprintln!("\x1b[2m{}\x1b[0m", BOX_BOT);
        }
        // ─── Tool Approval Request (shows risk level) ───
        AiEvent::ToolApprovalRequest {
            tool_name,
            args,
            risk_level,
            ..
        } => {
            let risk_str = format!("{:?}", risk_level).to_lowercase();
            eprintln!();
            eprintln!(
                "\x1b[2m{}\x1b[0m tool: {} \x1b[33m[{}]\x1b[0m",
                BOX_TOP, tool_name, risk_str
            );
            eprintln!("\x1b[2m{}\x1b[0m input:", BOX_MID);
            for line in format_json_pretty(args).lines() {
                eprintln!("\x1b[2m{}\x1b[0m   {}", BOX_MID, line);
            }
            eprintln!("\x1b[2m{}\x1b[0m", BOX_BOT);
        }
        AiEvent::ToolAutoApproved {
            tool_name, reason, ..
        } => {
            eprintln!("\x1b[32m[auto-approved]\x1b[0m {} ({})", tool_name, reason);
        }
        AiEvent::ToolDenied {
            tool_name, reason, ..
        } => {
            eprintln!("\x1b[31m[denied]\x1b[0m {} ({})", tool_name, reason);
        }
        // ─── Tool Result (box-drawing format with truncated output) ───
        AiEvent::ToolResult {
            tool_name,
            result,
            success,
            ..
        } => {
            let icon = if *success { "\x1b[32m+\x1b[0m" } else { "\x1b[31m!\x1b[0m" };
            eprintln!();
            eprintln!("\x1b[2m{}\x1b[0m {} {}", BOX_TOP, icon, tool_name);
            eprintln!("\x1b[2m{}\x1b[0m output:", BOX_MID);

            // Format and truncate output for terminal readability
            let output_str = format_json_pretty(result);
            let output_chars = output_str.chars().count();
            let truncated = truncate_output(&output_str, TERMINAL_TOOL_OUTPUT_MAX);

            for line in truncated.lines() {
                eprintln!("\x1b[2m{}\x1b[0m   {}", BOX_MID, line);
            }

            if output_chars > TERMINAL_TOOL_OUTPUT_MAX {
                eprintln!(
                    "\x1b[2m{}\x1b[0m   \x1b[2m... ({} chars total)\x1b[0m",
                    BOX_MID, output_chars
                );
            }
            eprintln!("\x1b[2m{}\x1b[0m", BOX_BOT);
        }
        // ─── Reasoning (box-drawing format with truncated content) ───
        AiEvent::Reasoning { content } => {
            eprintln!();
            eprintln!("\x1b[2m{}\x1b[0m \x1b[36mreasoning\x1b[0m", BOX_TOP);

            // Truncate reasoning for terminal readability
            let content_chars = content.chars().count();
            let truncated = truncate_output(content, TERMINAL_REASONING_MAX);

            for line in truncated.lines() {
                eprintln!("\x1b[2m{}\x1b[0m {}", BOX_MID, line);
            }

            if content_chars > TERMINAL_REASONING_MAX {
                eprintln!(
                    "\x1b[2m{}\x1b[0m \x1b[2m... ({} chars total)\x1b[0m",
                    BOX_MID, content_chars
                );
            }
            eprintln!("\x1b[2m{}\x1b[0m", BOX_BOT);
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

// ────────────────────────────────────────────────────────────────────────────────
// Box-drawing constants for terminal output
// ────────────────────────────────────────────────────────────────────────────────

const BOX_TOP: &str = "+-";
const BOX_MID: &str = "|";
const BOX_BOT: &str = "+-";

/// Format tool arguments for display (truncated summary).
///
/// NOTE: This is kept for backward compatibility in tests but is no longer
/// used in terminal output (we now show full tool inputs with `format_json_pretty`).
#[cfg(test)]
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

    // ────────────────────────────────────────────────────────────────────────────────
    // Tests for new helper functions
    // ────────────────────────────────────────────────────────────────────────────────

    mod format_json_pretty_tests {
        use super::*;

        #[test]
        fn formats_simple_object() {
            let value = serde_json::json!({"path": "Cargo.toml"});
            let pretty = format_json_pretty(&value);
            assert!(pretty.contains("\"path\""));
            assert!(pretty.contains("\"Cargo.toml\""));
            // Should be multi-line pretty format
            assert!(pretty.contains('\n'));
        }

        #[test]
        fn formats_nested_object() {
            let value = serde_json::json!({
                "path": "src/main.rs",
                "options": {
                    "recursive": true,
                    "limit": 100
                }
            });
            let pretty = format_json_pretty(&value);
            assert!(pretty.contains("\"path\""));
            assert!(pretty.contains("\"recursive\""));
            assert!(pretty.contains("true"));
        }

        #[test]
        fn formats_string_value() {
            let value = serde_json::json!("just a string");
            let pretty = format_json_pretty(&value);
            assert_eq!(pretty, "\"just a string\"");
        }

        #[test]
        fn formats_null_value() {
            let value = serde_json::Value::Null;
            let pretty = format_json_pretty(&value);
            assert_eq!(pretty, "null");
        }
    }

    mod truncate_output_tests {
        use super::*;

        #[test]
        fn does_not_truncate_short_string() {
            let short = "Hello, world!";
            let result = truncate_output(short, 500);
            assert_eq!(result, short);
        }

        #[test]
        fn truncates_long_string_at_limit() {
            let long = "a".repeat(1000);
            let result = truncate_output(&long, 500);
            assert_eq!(result.len(), 500);
            assert!(result.chars().all(|c| c == 'a'));
        }

        #[test]
        fn handles_exact_length_string() {
            let exact = "x".repeat(500);
            let result = truncate_output(&exact, 500);
            assert_eq!(result.len(), 500);
        }

        #[test]
        fn handles_empty_string() {
            let result = truncate_output("", 500);
            assert_eq!(result, "");
        }

        #[test]
        fn handles_unicode_correctly() {
            // Unicode characters can be multi-byte, ensure we don't split mid-character
            let unicode = "Hello ";
            let result = truncate_output(unicode, 10);
            // Should handle gracefully (either truncate cleanly or include full char)
            assert!(!result.is_empty());
        }
    }

    // ────────────────────────────────────────────────────────────────────────────────
    // Tests for CliJsonEvent standardized format
    // ────────────────────────────────────────────────────────────────────────────────

    mod cli_json_event_tests {
        use super::*;
        use crate::ai::hitl::RiskLevel;

        #[test]
        fn started_event_has_correct_format() {
            let ai_event = AiEvent::Started {
                turn_id: "test-turn-123".to_string(),
            };
            let cli_json = convert_to_cli_json(&ai_event);
            let json_str = serde_json::to_string(&cli_json).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

            assert_eq!(parsed["event"], "started");
            assert!(parsed["timestamp"].as_u64().is_some());
            assert_eq!(parsed["turn_id"], "test-turn-123");
        }

        #[test]
        fn text_delta_event_has_correct_format() {
            let ai_event = AiEvent::TextDelta {
                delta: "Hello".to_string(),
                accumulated: "Hello World".to_string(),
            };
            let cli_json = convert_to_cli_json(&ai_event);
            let json_str = serde_json::to_string(&cli_json).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

            assert_eq!(parsed["event"], "text_delta");
            assert!(parsed["timestamp"].as_u64().is_some());
            assert_eq!(parsed["delta"], "Hello");
            assert_eq!(parsed["accumulated"], "Hello World");
        }

        #[test]
        fn tool_request_uses_input_not_args() {
            let ai_event = AiEvent::ToolRequest {
                tool_name: "read_file".to_string(),
                args: serde_json::json!({"path": "Cargo.toml"}),
                request_id: "req-123".to_string(),
                source: Default::default(),
            };
            let cli_json = convert_to_cli_json(&ai_event);
            let json_str = serde_json::to_string(&cli_json).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

            assert_eq!(parsed["event"], "tool_call");
            assert_eq!(parsed["tool_name"], "read_file");
            // Should use "input" not "args"
            assert_eq!(parsed["input"]["path"], "Cargo.toml");
            assert!(parsed.get("args").is_none());
        }

        #[test]
        fn tool_result_uses_output_not_result() {
            let ai_event = AiEvent::ToolResult {
                tool_name: "read_file".to_string(),
                result: serde_json::json!("[package]\nname = \"qbit\""),
                success: true,
                request_id: "req-123".to_string(),
                source: Default::default(),
            };
            let cli_json = convert_to_cli_json(&ai_event);
            let json_str = serde_json::to_string(&cli_json).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

            assert_eq!(parsed["event"], "tool_result");
            assert_eq!(parsed["tool_name"], "read_file");
            assert_eq!(parsed["success"], true);
            // Should use "output" not "result"
            assert_eq!(parsed["output"], "[package]\nname = \"qbit\"");
            assert!(parsed.get("result").is_none());
        }

        #[test]
        fn tool_approval_request_uses_input_not_args() {
            let ai_event = AiEvent::ToolApprovalRequest {
                request_id: "req-456".to_string(),
                tool_name: "write_file".to_string(),
                args: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
                stats: None,
                risk_level: RiskLevel::High,
                can_learn: true,
                suggestion: Some("Approve this operation?".to_string()),
                source: Default::default(),
            };
            let cli_json = convert_to_cli_json(&ai_event);
            let json_str = serde_json::to_string(&cli_json).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

            assert_eq!(parsed["event"], "tool_approval");
            assert_eq!(parsed["tool_name"], "write_file");
            // Should use "input" not "args"
            assert_eq!(parsed["input"]["path"], "/tmp/test.txt");
            assert!(parsed.get("args").is_none());
        }

        #[test]
        fn reasoning_event_has_correct_format() {
            let ai_event = AiEvent::Reasoning {
                content: "Let me think about this step by step...".to_string(),
            };
            let cli_json = convert_to_cli_json(&ai_event);
            let json_str = serde_json::to_string(&cli_json).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

            assert_eq!(parsed["event"], "reasoning");
            assert_eq!(parsed["content"], "Let me think about this step by step...");
        }

        #[test]
        fn completed_event_has_correct_format() {
            let ai_event = AiEvent::Completed {
                response: "Here is the answer".to_string(),
                tokens_used: Some(150),
                duration_ms: Some(1234),
            };
            let cli_json = convert_to_cli_json(&ai_event);
            let json_str = serde_json::to_string(&cli_json).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

            assert_eq!(parsed["event"], "completed");
            assert_eq!(parsed["response"], "Here is the answer");
            assert_eq!(parsed["tokens_used"], 150);
            assert_eq!(parsed["duration_ms"], 1234);
        }

        #[test]
        fn error_event_has_correct_format() {
            let ai_event = AiEvent::Error {
                message: "Something went wrong".to_string(),
                error_type: "api_error".to_string(),
            };
            let cli_json = convert_to_cli_json(&ai_event);
            let json_str = serde_json::to_string(&cli_json).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

            assert_eq!(parsed["event"], "error");
            assert_eq!(parsed["message"], "Something went wrong");
            assert_eq!(parsed["error_type"], "api_error");
        }

        #[test]
        fn all_events_have_timestamp() {
            let events = vec![
                AiEvent::Started {
                    turn_id: "t1".to_string(),
                },
                AiEvent::TextDelta {
                    delta: "x".to_string(),
                    accumulated: "x".to_string(),
                },
                AiEvent::Reasoning {
                    content: "thinking".to_string(),
                },
            ];

            for ai_event in events {
                let cli_json = convert_to_cli_json(&ai_event);
                let json_str = serde_json::to_string(&cli_json).unwrap();
                let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

                assert!(
                    parsed["timestamp"].as_u64().is_some(),
                    "Event should have timestamp: {:?}",
                    parsed
                );
            }
        }
    }

    // ────────────────────────────────────────────────────────────────────────────────
    // Tests for NO TRUNCATION in JSON mode
    // ────────────────────────────────────────────────────────────────────────────────

    mod json_no_truncation_tests {
        use super::*;

        #[test]
        fn tool_output_not_truncated_in_json() {
            // Create a very large tool result (> 500 chars which is terminal limit)
            let large_output = "x".repeat(10000);
            let ai_event = AiEvent::ToolResult {
                tool_name: "read_file".to_string(),
                result: serde_json::json!(large_output),
                success: true,
                request_id: "req-large".to_string(),
                source: Default::default(),
            };
            let cli_json = convert_to_cli_json(&ai_event);
            let json_str = serde_json::to_string(&cli_json).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

            // Output should be FULL 10000 chars, NOT truncated
            let output = parsed["output"].as_str().unwrap();
            assert_eq!(output.len(), 10000, "JSON output should NOT be truncated");
            assert!(
                !output.contains("truncated"),
                "Should not contain truncation indicator"
            );
        }

        #[test]
        fn reasoning_not_truncated_in_json() {
            // Create very large reasoning content (> 2000 chars which is terminal limit)
            let large_reasoning = "thinking step ".repeat(500);
            let ai_event = AiEvent::Reasoning {
                content: large_reasoning.clone(),
            };
            let cli_json = convert_to_cli_json(&ai_event);
            let json_str = serde_json::to_string(&cli_json).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

            // Content should be FULL, NOT truncated
            let content = parsed["content"].as_str().unwrap();
            assert_eq!(
                content.len(),
                large_reasoning.len(),
                "JSON reasoning should NOT be truncated"
            );
            assert!(
                !content.contains("truncated"),
                "Should not contain truncation indicator"
            );
        }

        #[test]
        fn tool_input_not_truncated_in_json() {
            // Create a very large tool input
            let large_content = "y".repeat(5000);
            let ai_event = AiEvent::ToolRequest {
                tool_name: "write_file".to_string(),
                args: serde_json::json!({
                    "path": "/tmp/test.txt",
                    "content": large_content
                }),
                request_id: "req-large-input".to_string(),
                source: Default::default(),
            };
            let cli_json = convert_to_cli_json(&ai_event);
            let json_str = serde_json::to_string(&cli_json).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

            // Input content should be FULL 5000 chars
            let input_content = parsed["input"]["content"].as_str().unwrap();
            assert_eq!(
                input_content.len(),
                5000,
                "JSON tool input should NOT be truncated"
            );
        }
    }
}

# Qbit CLI Update Plan

**Goal**: Improve CLI output quality for debugging, testing, and evaluation.

**Scope**: Four focused changes to improve visibility and add lightweight REPL.

---

## 1. Enhanced Tool Output

### Problem

Current terminal output truncates tool information:
```rust
// output.rs:121-122 - truncates args to 60 chars
"[tool] {} {}", tool_name, format_args_summary(args)

// output.rs:148-151 - only shows success/fail, no result data
let icon = if *success { "ok" } else { "err" };
"[{}] {}", icon, tool_name
```

Developers cannot see what tools are actually doing without `--json` mode.

### Solution

Display full tool input and output in terminal mode:

```rust
// ─── Tool Request ───
AiEvent::ToolRequest { tool_name, args, .. } => {
    eprintln!();
    eprintln!("╭─ tool: {}", tool_name);
    eprintln!("│ input:");
    for line in format_json_pretty(args).lines() {
        eprintln!("│   {}", line);
    }
    eprintln!("╰─");
}

// ─── Tool Result ───
AiEvent::ToolResult { tool_name, result, success, .. } => {
    let icon = if *success { "✓" } else { "✗" };
    eprintln!();
    eprintln!("╭─ {} {}", icon, tool_name);
    eprintln!("│ output:");
    // Truncate to reasonable length (500 chars) for large results
    let output_str = format_json_pretty(result);
    let truncated = truncate_output(&output_str, 500);
    for line in truncated.lines() {
        eprintln!("│   {}", line);
    }
    if output_str.len() > 500 {
        eprintln!("│   ... (truncated, {} chars total)", output_str.len());
    }
    eprintln!("╰─");
}
```

### Files to Modify

- `src-tauri/src/cli/output.rs`:
  - Update `handle_ai_event_terminal()` for `ToolRequest` and `ToolResult`
  - Add `format_json_pretty()` helper function
  - Add `truncate_output()` helper with configurable max length

---

## 2. Enhanced Reasoning Output

### Problem

Reasoning/thinking content is truncated to 100 chars:
```rust
// output.rs:153-155
AiEvent::Reasoning { content } => {
    eprintln!("[thinking] {}", truncate(content, 100));
}
```

This hides most of the agent's reasoning process.

### Solution

Display full reasoning content (with optional max length):

```rust
AiEvent::Reasoning { content } => {
    eprintln!();
    eprintln!("╭─ reasoning");
    // Show full content up to 2000 chars
    let truncated = truncate_output(content, 2000);
    for line in truncated.lines() {
        eprintln!("│ {}", line);
    }
    if content.len() > 2000 {
        eprintln!("│ ... (truncated, {} chars total)", content.len());
    }
    eprintln!("╰─");
}
```

### Files to Modify

- `src-tauri/src/cli/output.rs`:
  - Update `handle_ai_event_terminal()` for `Reasoning` event

---

## 3. Standardized JSON Output Format

### Problem

Current JSON output uses raw `AiEvent` serialization:
```rust
// output.rs:74-75
println!("{}", serde_json::to_string(event)?);
```

This uses Rust's serde tags which may not be ideal for eval parsing.

### Solution

Create a standardized CLI output format with consistent structure.

**IMPORTANT**: JSON mode must have **NO TRUNCATION** for any data:
- Tool inputs: Full JSON, no truncation
- Tool outputs: Full result, no truncation
- Reasoning: Full content, no truncation
- Text deltas: Full content, no truncation

Truncation is only applied in terminal mode for readability.

```rust
/// Standardized JSON output event for CLI
#[derive(Serialize)]
struct CliJsonEvent {
    /// Event type (started, text_delta, tool_call, tool_result, reasoning, completed, error)
    #[serde(rename = "event")]
    event_type: String,

    /// Unix timestamp in milliseconds
    timestamp: u64,

    /// Event-specific data (flattened)
    #[serde(flatten)]
    data: serde_json::Value,
}
```

Example output:
```jsonl
{"event":"started","timestamp":1733680000000,"turn_id":"abc123"}
{"event":"text_delta","timestamp":1733680000100,"delta":"Hello","accumulated":"Hello"}
{"event":"tool_call","timestamp":1733680000200,"tool_name":"read_file","input":{"path":"Cargo.toml"}}
{"event":"tool_result","timestamp":1733680000500,"tool_name":"read_file","success":true,"output":"[package]\nname = \"qbit\"..."}
{"event":"reasoning","timestamp":1733680000600,"content":"I should read the file to understand..."}
{"event":"completed","timestamp":1733680001000,"response":"The file contains...","tokens_used":150}
```

### Key Changes

1. Use `event` instead of `type` (cleaner for eval parsing)
2. Add `timestamp` to all events
3. Flatten tool events to use `input`/`output` instead of `args`/`result`
4. Normalize event names to be more eval-friendly

### Files to Modify

- `src-tauri/src/cli/output.rs`:
  - Add `CliJsonEvent` struct
  - Add `convert_to_cli_json()` function
  - Update `handle_ai_event()` to use new format in JSON mode

### Eval-Friendly Event Names

| AiEvent Variant | CLI JSON `event` |
|-----------------|------------------|
| `Started` | `started` |
| `TextDelta` | `text_delta` |
| `ToolRequest` | `tool_call` |
| `ToolApprovalRequest` | `tool_approval` |
| `ToolAutoApproved` | `tool_auto_approved` |
| `ToolDenied` | `tool_denied` |
| `ToolResult` | `tool_result` |
| `Reasoning` | `reasoning` |
| `Completed` | `completed` |
| `Error` | `error` |
| `SubAgent*` | `sub_agent_*` |
| `Context*` | `context_*` |
| `Loop*` | `loop_*` |
| `Workflow*` | `workflow_*` |

---

## 4. Lightweight REPL Mode

### Problem

Currently no interactive mode - CLI exits after single prompt:
```rust
// qbit-cli.rs:50-58
} else {
    // No prompt provided - show help
    eprintln!("Error: No prompt provided.");
    // ...
}
```

### Solution

Add minimal REPL with only `/quit` command:

```rust
// src-tauri/src/cli/repl.rs

use std::io::{self, BufRead, Write};
use anyhow::Result;
use crate::cli::{bootstrap::CliContext, runner::execute_once};

/// Run an interactive REPL session.
///
/// Supports:
/// - `/quit` or `/exit` - Exit the REPL
/// - Any other input - Send as prompt to agent
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
            break;
        }

        let input = input.trim();

        // Handle empty input
        if input.is_empty() {
            continue;
        }

        // Handle commands
        if input.starts_with('/') {
            match input.to_lowercase().as_str() {
                "/quit" | "/exit" | "/q" => {
                    eprintln!("Goodbye!");
                    break;
                }
                _ => {
                    eprintln!("Unknown command: {}", input);
                    eprintln!("Available: /quit");
                    continue;
                }
            }
        }

        // Execute prompt
        if let Err(e) = execute_once(ctx, input).await {
            eprintln!("Error: {}", e);
        }

        println!(); // Blank line between interactions
    }

    Ok(())
}
```

### Entry Point Update

```rust
// qbit-cli.rs
} else {
    // No prompt provided - enter REPL mode
    run_repl(&mut ctx).await
}
```

### Files to Create/Modify

- `src-tauri/src/cli/repl.rs` (NEW)
- `src-tauri/src/cli/mod.rs` - Add `pub mod repl` and export `run_repl`
- `src-tauri/src/bin/qbit-cli.rs` - Call `run_repl` when no `-e` or `-f`

---

## Implementation Order

### Phase 1: Output Formatting (output.rs changes)

1. Add helper functions:
   - `format_json_pretty(value: &serde_json::Value) -> String`
   - `truncate_output(s: &str, max_len: usize) -> String` (terminal mode only)

2. Update `handle_ai_event_terminal()` (TERMINAL MODE - with truncation):
   - `ToolRequest` / `ToolApprovalRequest` - Show full input (no truncation for input)
   - `ToolResult` - Show output (truncated at 500 chars for readability)
   - `Reasoning` - Show content (truncated at 2000 chars for readability)

3. Add standardized JSON format (JSON MODE - NO truncation):
   - `CliJsonEvent` struct
   - `convert_to_cli_json()` function - **NO TRUNCATION**
   - All tool inputs, outputs, reasoning content passed through completely
   - Update JSON mode to use new format

### Phase 2: REPL Mode

1. Create `src-tauri/src/cli/repl.rs`
2. Update `mod.rs` exports
3. Update `qbit-cli.rs` entry point

---

## Truncation Policy

| Output Mode | Tool Input | Tool Output | Reasoning | Text Deltas |
|-------------|------------|-------------|-----------|-------------|
| **Terminal** | No truncation | 500 chars | 2000 chars | No truncation |
| **JSON** | **No truncation** | **No truncation** | **No truncation** | **No truncation** |
| **Quiet** | Not shown | Not shown | Not shown | Not shown (final only) |

JSON mode is designed for programmatic parsing (evals, CI, scripting) and must preserve all data.

---

## Acceptance Criteria

### Tool Output (Terminal Mode)
```bash
./target/debug/qbit-cli -e "Read the first line of Cargo.toml" --auto-approve

# Should show:
# ╭─ tool: read_file
# │ input:
# │   {
# │     "path": "Cargo.toml",
# │     "lines": 1
# │   }
# ╰─
#
# ╭─ ✓ read_file
# │ output:
# │   "[package]"
# ╰─
#
# The first line of Cargo.toml is: [package]
```

### Tool Output (JSON Mode) - NO TRUNCATION
```bash
./target/debug/qbit-cli -e "Read Cargo.toml" --auto-approve --json | head -5

# Should show (one per line):
# {"event":"started","timestamp":...,"turn_id":"..."}
# {"event":"tool_call","timestamp":...,"tool_name":"read_file","input":{"path":"Cargo.toml"}}
# {"event":"tool_result","timestamp":...,"tool_name":"read_file","success":true,"output":"[package]\nname = \"qbit\"\nversion = \"0.1.0\"...FULL CONTENT"}
# {"event":"text_delta","timestamp":...,"delta":"The","accumulated":"The"}

# IMPORTANT: output field contains FULL file contents, not truncated
# This enables eval tests to verify exact tool results
```

### Reasoning Output
```bash
./target/debug/qbit-cli -e "Think step by step: what is 15 * 17?" --auto-approve

# Should show full reasoning:
# ╭─ reasoning
# │ Let me calculate 15 * 17 step by step.
# │ First, I'll break this down:
# │ 15 * 17 = 15 * (10 + 7)
# │ = (15 * 10) + (15 * 7)
# │ = 150 + 105
# │ = 255
# ╰─
```

### REPL Mode
```bash
./target/debug/qbit-cli --auto-approve

# qbit-cli interactive mode
# Type /quit to exit
#
# > Hello
# Hello! How can I help you today?
#
# > /quit
# Goodbye!
```

### Eval Test Compatibility
```python
# In evals/test_cli.py
def test_json_format(self, cli):
    result = cli.run_prompt("Hello", json_output=True)
    events = [json.loads(line) for line in result.stdout.strip().split("\n") if line]

    # Verify standard format
    for event in events:
        assert "event" in event
        assert "timestamp" in event

    # Find tool events
    tool_calls = [e for e in events if e["event"] == "tool_call"]
    tool_results = [e for e in events if e["event"] == "tool_result"]

    for tc in tool_calls:
        assert "tool_name" in tc
        assert "input" in tc

    for tr in tool_results:
        assert "tool_name" in tr
        assert "success" in tr
        assert "output" in tr

def test_json_no_truncation(self, cli):
    """Verify JSON mode does not truncate tool output."""
    result = cli.run_prompt("Read Cargo.toml", json_output=True)
    events = [json.loads(line) for line in result.stdout.strip().split("\n") if line]

    tool_results = [e for e in events if e["event"] == "tool_result"]
    for tr in tool_results:
        # Output should NOT end with truncation indicator
        output = tr.get("output", "")
        assert "... (truncated" not in str(output)
        # For read_file, output should contain actual file content
        if tr["tool_name"] == "read_file":
            assert "[package]" in str(output)  # Cargo.toml starts with [package]

def test_reasoning_no_truncation(self, cli):
    """Verify JSON mode does not truncate reasoning."""
    result = cli.run_prompt("Think carefully about X", json_output=True)
    events = [json.loads(line) for line in result.stdout.strip().split("\n") if line]

    reasoning_events = [e for e in events if e["event"] == "reasoning"]
    for r in reasoning_events:
        content = r.get("content", "")
        assert "... (truncated" not in content
```

---

## Files Summary

| File | Action | Changes |
|------|--------|---------|
| `src-tauri/src/cli/output.rs` | Modify | New helpers, enhanced terminal output, standardized JSON |
| `src-tauri/src/cli/repl.rs` | Create | Lightweight REPL with /quit |
| `src-tauri/src/cli/mod.rs` | Modify | Export `repl` module |
| `src-tauri/src/bin/qbit-cli.rs` | Modify | Call `run_repl` when no args |

---

## Non-Goals (Out of Scope)

- `/help` command - Keep REPL minimal
- `/tools` command - Not needed for MVP
- Command history (rustyline) - Future enhancement
- Colored output - Future enhancement
- Session management commands - Already works via `--session` flag

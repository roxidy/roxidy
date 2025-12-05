# Browser-Only Development Mode

Develop and test the Qbit UI without running the Rust backend.

## Quick Start

```bash
# Start the Vite dev server (no Tauri)
pnpm dev

# Open http://localhost:1420 in your browser
```

When running outside of Tauri, the app automatically loads mock implementations for all IPC commands and events.

## MockDevTools Panel

A floating dev tools panel appears in the bottom-right corner (🔧 button).

### Presets Tab

Click any preset to simulate that scenario:

| Preset | Description |
|--------|-------------|
| 🌱 Fresh Start | Clean terminal with welcome message |
| 💬 Active Conversation | AI helping with a Rust project |
| 🔧 Tool Execution | AI using `read_file` and `glob` tools |
| ❌ Error State | Rate limit error mid-response |
| 📜 Command History | Series of git/cargo commands |
| 🔴 Build Failure | Compiler error + AI offering help |
| 👀 Code Review | AI reviewing code with suggestions |
| 📄 Long Output | 50+ test results (scroll testing) |

### Manual Controls

- **Terminal tab**: Emit raw output, command blocks, directory changes
- **AI tab**: Stream custom responses, trigger tool events, emit errors
- **Session tab**: Manage session IDs, end sessions

## Programmatic Usage

Import helpers directly for custom test scenarios:

```typescript
import {
  emitTerminalOutput,
  emitCommandBlock,
  emitAiEvent,
  simulateAiResponse,
} from "@/mocks";

// Emit terminal output
await emitTerminalOutput("session-id", "Hello world\n");

// Emit a command block
await emitCommandBlock("session-id", "ls -la", "file1.txt\nfile2.txt", 0);

// Simulate streaming AI response
await simulateAiResponse("This streams word by word.", 50);

// Emit individual AI events
await emitAiEvent({ type: "started", turn_id: "turn-1" });
await emitAiEvent({ type: "text_delta", delta: "Hello", accumulated: "Hello" });
await emitAiEvent({ type: "completed", response: "Hello", tokens_used: 10 });
```

## What's Mocked

### IPC Commands (47 total)

- **PTY**: `pty_create`, `pty_write`, `pty_resize`, `pty_destroy`, `pty_get_session`
- **AI Agent**: `init_ai_agent`, `send_ai_prompt`, `get_available_tools`, etc.
- **Sessions**: `list_ai_sessions`, `load_ai_session`, `restore_ai_session`, etc.
- **HITL**: `get_approval_patterns`, `respond_to_tool_approval`, etc.
- **Indexer**: `init_indexer`, `search_code`, `analyze_file`, etc.

### Events (5 channels)

- `terminal_output` - Terminal data
- `command_block` - Completed commands
- `directory_changed` - Working directory updates
- `session_ended` - Session termination
- `ai-event` - AI streaming (text, tools, errors)

## Running E2E Tests

```bash
# Run all Playwright tests
npx playwright test

# Run with UI
npx playwright test --ui

# Run specific test
npx playwright test e2e/tauri-mocks.spec.ts
```

## How It Works

1. `src/main.tsx` checks for `window.__TAURI_INTERNALS__`
2. If absent (browser mode), it dynamically imports `src/mocks.ts`
3. `setupMocks()` calls `mockWindows()` and `mockIPC()` from `@tauri-apps/api/mocks`
4. All `invoke()` calls are intercepted and return mock data
5. Events can be emitted using the helper functions

# Tauri Commands and Events

This document details the IPC layer between the React frontend and Rust backend.

## Philosophy

- **Commands** (invoke): Frontend → Backend, request/response pattern
- **Events** (emit/listen): Backend → Frontend, push notifications for async updates

Commands are for actions. Events are for streams and notifications.

## Commands

### PTY Commands

```rust
// src-tauri/src/commands/pty.rs

/// Create a new terminal session
#[tauri::command]
async fn pty_create(
    state: State<'_, AppState>,
    working_directory: Option<PathBuf>,
) -> Result<SessionInfo, String> {
    // Returns session_id, initial dimensions, etc.
}

/// Write bytes to PTY (user input)
#[tauri::command]
async fn pty_write(
    state: State<'_, AppState>,
    session_id: Uuid,
    data: Vec<u8>,
) -> Result<(), String> {
    // Forwards input to shell
}

/// Resize terminal
#[tauri::command]
async fn pty_resize(
    state: State<'_, AppState>,
    session_id: Uuid,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    // Updates PTY dimensions
}

/// Close a session
#[tauri::command]
async fn pty_close(
    state: State<'_, AppState>,
    session_id: Uuid,
) -> Result<(), String> {
    // Terminates shell, cleans up resources
}

/// Get all active sessions
#[tauri::command]
async fn pty_list_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<SessionInfo>, String> {
    // Returns all session metadata
}
```

### AI Commands

```rust
// src-tauri/src/commands/ai.rs

/// Send a prompt to the AI agent
#[tauri::command]
async fn ai_prompt(
    state: State<'_, AppState>,
    session_id: Uuid,
    message: String,
) -> Result<(), String> {
    // Starts streaming response via events
    // Returns immediately, response comes via ai_stream events
}

/// Cancel an in-progress AI response
#[tauri::command]
async fn ai_cancel(
    state: State<'_, AppState>,
    session_id: Uuid,
) -> Result<(), String> {
    // Stops generation
}

/// Approve a tool call that requires confirmation
#[tauri::command]
async fn ai_approve_tool(
    state: State<'_, AppState>,
    tool_call_id: Uuid,
    approved: bool,
) -> Result<(), String> {
    // For dangerous operations like file writes
}

/// Get conversation history
#[tauri::command]
async fn ai_get_history(
    state: State<'_, AppState>,
    session_id: Uuid,
) -> Result<Vec<AIMessage>, String> {
    // Returns conversation for session
}

/// Clear conversation history
#[tauri::command]
async fn ai_clear_history(
    state: State<'_, AppState>,
    session_id: Uuid,
) -> Result<(), String> {
    // Resets conversation
}
```

### Settings Commands

```rust
// src-tauri/src/commands/settings.rs

/// Get all settings
#[tauri::command]
async fn settings_get_all(
    state: State<'_, AppState>,
) -> Result<Settings, String> {
    // Returns full settings object
}

/// Update a setting
#[tauri::command]
async fn settings_set(
    state: State<'_, AppState>,
    key: String,
    value: serde_json::Value,
) -> Result<(), String> {
    // Persists to SQLite
}

/// Get available themes
#[tauri::command]
async fn themes_list(
    state: State<'_, AppState>,
) -> Result<Vec<ThemeInfo>, String> {
    // Built-in + user themes
}

/// Get a specific theme
#[tauri::command]
async fn theme_get(
    state: State<'_, AppState>,
    theme_id: String,
) -> Result<Theme, String> {
    // Full theme definition
}

/// Save a custom theme
#[tauri::command]
async fn theme_save(
    state: State<'_, AppState>,
    theme: Theme,
) -> Result<(), String> {
    // Saves to user themes directory
}
```

### Shell Integration Commands

```rust
// src-tauri/src/commands/shell.rs

/// Check if shell integration is installed
#[tauri::command]
async fn shell_integration_status() -> Result<IntegrationStatus, String> {
    // Returns installed/outdated/not_installed
}

/// Install or update shell integration
#[tauri::command]
async fn shell_integration_install() -> Result<(), String> {
    // Writes integration.zsh, updates .zshrc if needed
}

/// Uninstall shell integration
#[tauri::command]
async fn shell_integration_uninstall() -> Result<(), String> {
    // Removes integration files
}
```

### History Commands

```rust
// src-tauri/src/commands/history.rs

/// Search command history
#[tauri::command]
async fn history_search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<CommandBlock>, String> {
    // Full-text search on commands
}

/// Get recent commands for a session
#[tauri::command]
async fn history_recent(
    state: State<'_, AppState>,
    session_id: Option<Uuid>,
    limit: u32,
) -> Result<Vec<CommandBlock>, String> {
    // Most recent N commands
}

/// Delete a command block
#[tauri::command]
async fn history_delete(
    state: State<'_, AppState>,
    block_id: Uuid,
) -> Result<(), String> {
    // Removes from history
}
```

## Events

Events are emitted from Rust and listened to in TypeScript.

### PTY Events

```rust
// Terminal output (raw bytes for xterm.js)
#[derive(Clone, Serialize)]
struct TerminalOutputEvent {
    session_id: Uuid,
    data: Vec<u8>,  // Raw terminal output
}
// Event name: "terminal_output"

// Command block detected (semantic)
#[derive(Clone, Serialize)]
struct CommandBlockEvent {
    session_id: Uuid,
    block: CommandBlock,
}
// Event name: "command_block"

// Working directory changed
#[derive(Clone, Serialize)]
struct DirectoryChangedEvent {
    session_id: Uuid,
    path: PathBuf,
}
// Event name: "directory_changed"

// Session ended (shell exited)
#[derive(Clone, Serialize)]
struct SessionEndedEvent {
    session_id: Uuid,
    exit_code: Option<i32>,
}
// Event name: "session_ended"
```

### AI Events

```rust
// Streaming text delta
#[derive(Clone, Serialize)]
struct AIStreamEvent {
    session_id: Uuid,
    delta: Option<String>,      // Text chunk
    tool_call: Option<ToolCall>, // Tool being called
    done: bool,                  // Stream complete
    error: Option<String>,       // Error if failed
}
// Event name: "ai_stream"

// Tool execution started
#[derive(Clone, Serialize)]
struct ToolStartEvent {
    session_id: Uuid,
    tool_call_id: Uuid,
    tool_name: String,
    arguments: serde_json::Value,
    requires_approval: bool,
}
// Event name: "tool_start"

// Tool execution completed
#[derive(Clone, Serialize)]
struct ToolResultEvent {
    session_id: Uuid,
    tool_call_id: Uuid,
    result: serde_json::Value,
    success: bool,
}
// Event name: "tool_result"
```

## TypeScript Types and Wrappers

```typescript
// src/lib/tauri.ts

import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

// ============ Types ============

export interface SessionInfo {
  id: string;
  name: string;
  workingDirectory: string;
  createdAt: string;
}

export interface CommandBlock {
  id: string;
  sessionId: string;
  command: string;
  output: string;
  exitCode: number | null;
  startTime: string;
  durationMs: number | null;
  workingDirectory: string;
}

export interface AIMessage {
  role: "user" | "assistant" | "tool";
  content: string;
  toolCalls?: ToolCall[];
  toolResults?: ToolResult[];
}

export interface ToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
}

export interface Settings {
  aiProvider: string;
  aiModel: string;
  aiApiKey?: string;
  theme: string;
  fontSize: number;
  fontFamily: string;
}

// ============ PTY Commands ============

export async function ptyCreate(workingDirectory?: string): Promise<SessionInfo> {
  return invoke("pty_create", { workingDirectory });
}

export async function ptyWrite(sessionId: string, data: Uint8Array): Promise<void> {
  return invoke("pty_write", { sessionId, data: Array.from(data) });
}

export async function ptyResize(sessionId: string, rows: number, cols: number): Promise<void> {
  return invoke("pty_resize", { sessionId, rows, cols });
}

export async function ptyClose(sessionId: string): Promise<void> {
  return invoke("pty_close", { sessionId });
}

// ============ AI Commands ============

export async function aiPrompt(sessionId: string, message: string): Promise<void> {
  return invoke("ai_prompt", { sessionId, message });
}

export async function aiCancel(sessionId: string): Promise<void> {
  return invoke("ai_cancel", { sessionId });
}

export async function aiApproveTool(toolCallId: string, approved: boolean): Promise<void> {
  return invoke("ai_approve_tool", { toolCallId, approved });
}

// ============ Event Listeners ============

export interface TerminalOutputEvent {
  sessionId: string;
  data: number[];  // Will convert to Uint8Array
}

export interface CommandBlockEvent {
  sessionId: string;
  block: CommandBlock;
}

export interface AIStreamEvent {
  sessionId: string;
  delta?: string;
  toolCall?: ToolCall;
  done: boolean;
  error?: string;
}

export function onTerminalOutput(
  callback: (event: TerminalOutputEvent) => void
): Promise<UnlistenFn> {
  return listen("terminal_output", (event) => {
    callback(event.payload as TerminalOutputEvent);
  });
}

export function onCommandBlock(
  callback: (event: CommandBlockEvent) => void
): Promise<UnlistenFn> {
  return listen("command_block", (event) => {
    callback(event.payload as CommandBlockEvent);
  });
}

export function onAIStream(
  callback: (event: AIStreamEvent) => void
): Promise<UnlistenFn> {
  return listen("ai_stream", (event) => {
    callback(event.payload as AIStreamEvent);
  });
}

export function onToolStart(
  callback: (event: ToolStartEvent) => void
): Promise<UnlistenFn> {
  return listen("tool_start", (event) => {
    callback(event.payload as ToolStartEvent);
  });
}
```

## Registering Commands in Tauri

```rust
// src-tauri/src/lib.rs

mod commands;
mod pty;
mod ai;
mod db;
mod shell;

use commands::{pty::*, ai::*, settings::*, shell::*, history::*};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            // PTY
            pty_create,
            pty_write,
            pty_resize,
            pty_close,
            pty_list_sessions,
            // AI
            ai_prompt,
            ai_cancel,
            ai_approve_tool,
            ai_get_history,
            ai_clear_history,
            // Settings
            settings_get_all,
            settings_set,
            themes_list,
            theme_get,
            theme_save,
            // Shell
            shell_integration_status,
            shell_integration_install,
            shell_integration_uninstall,
            // History
            history_search,
            history_recent,
            history_delete,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

## App State

```rust
// src-tauri/src/state.rs

use std::sync::Arc;
use tokio::sync::RwLock;
use rusqlite::Connection;

pub struct AppState {
    pub pty_manager: Arc<RwLock<PtyManager>>,
    pub ai_manager: Arc<RwLock<AIManager>>,
    pub db: Arc<RwLock<Connection>>,
    pub settings: Arc<RwLock<Settings>>,
}

impl AppState {
    pub fn new() -> Self {
        let db_path = directories::ProjectDirs::from("dev", "roxidy", "roxidy")
            .unwrap()
            .data_dir()
            .join("roxidy.db");

        let db = Connection::open(&db_path).unwrap();
        db::schema::initialize(&db).unwrap();

        Self {
            pty_manager: Arc::new(RwLock::new(PtyManager::new())),
            ai_manager: Arc::new(RwLock::new(AIManager::new())),
            db: Arc::new(RwLock::new(db)),
            settings: Arc::new(RwLock::new(Settings::load(&db).unwrap())),
        }
    }
}
```

## Error Handling Pattern

All commands return `Result<T, String>` for Tauri compatibility. Use a helper:

```rust
// src-tauri/src/error.rs

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RoxidyError {
    #[error("PTY error: {0}")]
    Pty(String),

    #[error("AI error: {0}")]
    AI(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Session not found: {0}")]
    SessionNotFound(Uuid),
}

// Convert to String for Tauri
impl From<RoxidyError> for String {
    fn from(err: RoxidyError) -> Self {
        err.to_string()
    }
}

// Helper trait for commands
pub trait IntoTauriResult<T> {
    fn into_tauri(self) -> Result<T, String>;
}

impl<T, E: Into<RoxidyError>> IntoTauriResult<T> for Result<T, E> {
    fn into_tauri(self) -> Result<T, String> {
        self.map_err(|e| e.into().to_string())
    }
}
```

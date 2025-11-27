# Roxidy Architecture

Roxidy is a Warp.dev alternative built with Tauri 2, React/TypeScript frontend, and Rust backend. AI agent integration via [rig](https://github.com/0xPlaygrounds/rig) is planned for a future release.

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Vite/React Frontend                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │ CommandInput│  │ Command     │  │ TabBar              │  │
│  │ (custom)    │  │ BlockList   │  │ (session management)│  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
└────────────────────────┬────────────────────────────────────┘
                         │ Tauri IPC (invoke + events)
┌────────────────────────┴────────────────────────────────────┐
│                    Rust Backend (Tauri 2)                    │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐   │
│  │ PTY Manager  │  │ VTE Parser   │  │ Shell Integration│   │
│  │ (portable-   │  │ (OSC 133,    │  │ (zsh hooks,      │   │
│  │  pty)        │  │  OSC 7)      │  │  auto-install)   │   │
│  └──────────────┘  └──────────────┘  └──────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

## Current Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| PTY Manager | ✅ Complete | Multi-session support, resize handling |
| VTE Parser | ✅ Complete | OSC 133 + OSC 7 parsing with tests |
| Shell Integration | ✅ Complete | Auto-install to ~/.config/roxidy/ |
| Command Blocks | ✅ Complete | Collapsible, ANSI rendering, exit codes |
| Multi-Tab | ✅ Complete | Cmd+T new tab, tab switching |
| CommandInput | ✅ Complete | History, Ctrl+C/D/L, tab completion passthrough |
| Interactive Commands | ❌ Blocked | Shows toast error for vim, htop, etc. |
| AI Integration | ❌ Not Started | Designed but not implemented |
| SQLite Storage | ❌ Not Started | Planned for persistence |

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Shell support | zsh only (initial) | Focus on one shell, expand later |
| Terminal emulation | Custom input + blocks | Block-based UI for command history. Interactive programs blocked for now. |
| Command input | Custom React input | Simpler than xterm.js for non-interactive use |
| AI providers | User-configurable (planned) | Support Vertex, OpenRouter, Anthropic, OpenAI, local |
| History storage | SQLite (planned) | Persistent, queryable, reliable |
| Theming | Tokyo Night (hardcoded) | Single theme for POC, system planned |

## Core Components

### 1. PTY Manager (Rust)

Manages pseudo-terminal sessions using `portable-pty`.

**Responsibilities:**
- Spawn shell processes with environment configuration
- Handle input/output streaming
- Manage terminal resize events
- Support multiple concurrent sessions (tabs)

**Key crate:** `portable-pty = "0.8"`

### 2. VTE Parser (Rust)

Parses terminal output using the VTE state machine, with special handling for OSC 133 semantic sequences.

**OSC 133 Markers:**
| Marker | Meaning | Emitted When |
|--------|---------|--------------|
| `A` | Prompt start | Shell begins rendering prompt |
| `B` | Prompt end | Prompt rendered, awaiting input |
| `C` | Command start | User pressed enter, command executing |
| `D;N` | Command end | Command finished with exit code N |

**Key crate:** `vte = "0.13"`

### 3. Shell Integration

A zsh script that emits OSC 133 sequences, installed to `~/.config/roxidy/integration.zsh`:

```zsh
# OSC 133 sequences for semantic shell integration
_osc() { printf '\e]133;%s\e\\' "$1" }

_prompt_start() { _osc "A" }
_prompt_end() { _osc "B" }
_cmd_start() { _osc "C" }
_cmd_finished() { _osc "D;$?" }
_report_cwd() { printf '\e]7;file://%s%s\e\\' "$HOST" "$PWD" }

_roxidy_preexec() { _cmd_start }
_roxidy_precmd() {
  local exit_code=$?
  _cmd_finished
  _report_cwd
  _prompt_start
}
_roxidy_line_init() { _prompt_end }

autoload -Uz add-zsh-hook
add-zsh-hook preexec _roxidy_preexec
add-zsh-hook precmd _roxidy_precmd
zle -N zle-line-init _roxidy_line_init
```

### 4. Command Block Model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandBlock {
    pub id: Uuid,
    pub session_id: Uuid,
    pub command: String,
    pub output: String,
    pub exit_code: Option<i32>,
    pub start_time: DateTime<Utc>,
    pub duration: Option<Duration>,
    pub working_directory: PathBuf,
}
```

### 5. AI Agent System (Rig)

Multi-provider agent with terminal-aware tools.

**Supported Providers:**
- Google Vertex AI
- OpenRouter
- Anthropic (Claude)
- OpenAI
- Local models (Ollama)

**Agent Tools:**

| Tool | Description |
|------|-------------|
| `run_command` | Execute shell command in current PTY |
| `read_file` | Read file contents for context |
| `write_file` | Create or overwrite file |
| `edit_file` | Apply targeted edits to file |
| `list_directory` | List directory contents |
| `get_command_history` | Retrieve recent command blocks |
| `search_history` | Search command history by pattern |
| `explain_error` | Parse error output and suggest fixes |
| `get_current_directory` | Get PWD of active session |

### 6. SQLite Storage

**Tables:**

```sql
-- Terminal sessions
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    name TEXT,
    working_directory TEXT,
    created_at TEXT,
    last_accessed_at TEXT
);

-- Command history with full context
CREATE TABLE command_blocks (
    id TEXT PRIMARY KEY,
    session_id TEXT REFERENCES sessions(id),
    command TEXT,
    output TEXT,
    exit_code INTEGER,
    start_time TEXT,
    duration_ms INTEGER,
    working_directory TEXT
);

-- User settings
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT
);

-- AI conversation history
CREATE TABLE ai_conversations (
    id TEXT PRIMARY KEY,
    session_id TEXT REFERENCES sessions(id),
    messages TEXT, -- JSON array
    created_at TEXT
);
```

### 7. Theme System

Themes defined as JSON/TOML with semantic color tokens:

```json
{
  "name": "Roxidy Dark",
  "colors": {
    "background": "#1a1b26",
    "foreground": "#c0caf5",
    "cursor": "#c0caf5",
    "selection": "#33467c",
    "black": "#15161e",
    "red": "#f7768e",
    "green": "#9ece6a",
    "yellow": "#e0af68",
    "blue": "#7aa2f7",
    "magenta": "#bb9af7",
    "cyan": "#7dcfff",
    "white": "#a9b1d6",
    "brightBlack": "#414868",
    "brightRed": "#f7768e",
    "brightGreen": "#9ece6a",
    "brightYellow": "#e0af68",
    "brightBlue": "#7aa2f7",
    "brightMagenta": "#bb9af7",
    "brightCyan": "#7dcfff",
    "brightWhite": "#c0caf5"
  },
  "ui": {
    "panelBackground": "#16161e",
    "border": "#27293d",
    "commandBlockBackground": "#1f2335",
    "commandBlockBorder": "#3b4261",
    "successBadge": "#9ece6a",
    "errorBadge": "#f7768e",
    "aiPanelBackground": "#1a1b26"
  }
}
```

## Data Flow

### Terminal I/O

```
User keystroke
    ↓
React captures keydown
    ↓
invoke("pty_write", { session_id, data })
    ↓
PTY receives input, shell executes
    ↓
Shell emits output + OSC 133 sequences
    ↓
VTE parser extracts CommandBlock
    ↓
emit("terminal_output", { session_id, raw_output })
emit("command_block", { session_id, block })
    ↓
React updates terminal view + block list
```

### AI Interaction

```
User submits prompt
    ↓
invoke("ai_prompt", { session_id, message })
    ↓
Rig agent processes with context:
  - Recent command blocks
  - Current working directory
  - Conversation history
    ↓
Agent may call tools (run_command, read_file, etc.)
    ↓
emit("ai_stream", { delta, tool_calls, done })
    ↓
React renders streaming response
    ↓
On tool execution, emit("tool_result", { tool, result })
```

## Frontend State Management

### Core State Slices

```typescript
interface RoxidyState {
  sessions: SessionsState;
  terminal: TerminalState;
  ai: AIState;
  settings: SettingsState;
  theme: ThemeState;
}

interface SessionsState {
  sessions: Record<string, Session>;
  activeSessionId: string | null;
}

interface TerminalState {
  // Per-session terminal state
  buffers: Record<string, TerminalBuffer>;
  commandBlocks: Record<string, CommandBlock[]>;
}

interface AIState {
  conversations: Record<string, AIConversation>;
  isStreaming: boolean;
  pendingToolCalls: ToolCall[];
}
```

### Recommended: Zustand

Lightweight, TypeScript-friendly, works well with Tauri events:

```typescript
const useStore = create<RoxidyState>((set, get) => ({
  // State and actions
}));

// Listen to Tauri events
listen("command_block", (event) => {
  useStore.getState().addCommandBlock(event.payload);
});
```

## Rust Crate Dependencies

```toml
[dependencies]
tauri = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
anyhow = "1.0"
thiserror = "1.0"              # Derive Error trait for custom errors

# Terminal
portable-pty = "0.8"
vte = "0.13"

# AI
rig-core = "0.6"

# Storage
rusqlite = { version = "0.31", features = ["bundled"] }

# Utilities
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
dirs = "5"                     # Home/config directory paths
```

## Frontend Dependencies

### Currently Installed

These dependencies are already in `package.json`:

```json
{
  "dependencies": {
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-opener": "^2",
    "react": "^19.1.0",
    "react-dom": "^19.1.0",

    // Styling
    "tailwindcss": "^4.1.17",
    "@tailwindcss/vite": "^4.1.17",
    "tailwind-merge": "^3.4.0",
    "clsx": "^2.1.1",
    "class-variance-authority": "^0.7.1",

    // UI Components (Radix primitives via shadcn)
    "@radix-ui/react-collapsible": "^1.1.12",
    "@radix-ui/react-context-menu": "^2.2.16",
    "@radix-ui/react-dialog": "^1.1.15",
    "@radix-ui/react-dropdown-menu": "^2.1.16",
    "@radix-ui/react-popover": "^1.1.15",
    "@radix-ui/react-scroll-area": "^1.2.10",
    "@radix-ui/react-separator": "^1.1.8",
    "@radix-ui/react-slot": "^1.2.4",
    "@radix-ui/react-switch": "^1.2.6",
    "@radix-ui/react-tabs": "^1.1.13",
    "@radix-ui/react-toggle": "^1.1.10",
    "@radix-ui/react-tooltip": "^1.2.8",
    "cmdk": "^1.1.1",
    "react-resizable-panels": "^3.0.6",
    "sonner": "^2.0.7",
    "lucide-react": "^0.555.0",
    "next-themes": "^0.4.6"
  }
}
```

### To Be Installed (Before POC)

These dependencies are required but not yet added:

```bash
# Terminal emulation
pnpm add @xterm/xterm @xterm/addon-fit @xterm/addon-webgl @xterm/addon-web-links

# State management
pnpm add zustand immer
```

| Package | Version | Purpose |
|---------|---------|---------|
| `@xterm/xterm` | ^5 | Terminal emulator canvas |
| `@xterm/addon-fit` | ^0.10 | Auto-resize terminal to container |
| `@xterm/addon-webgl` | ^0.18 | GPU-accelerated rendering |
| `@xterm/addon-web-links` | ^0.11 | Clickable URLs in terminal |
| `zustand` | ^5 | Lightweight state management |
| `immer` | ^10 | Immutable state updates |

### Installed shadcn Components

Located in `src/components/ui/`:

| Category | Components |
|----------|------------|
| Terminal & Blocks | `command`, `collapsible`, `card`, `scroll-area`, `separator`, `badge`, `skeleton`, `resizable` |
| AI Interface | `textarea`, `input`, `popover`, `tooltip` |
| App Chrome | `sidebar`, `tabs`, `dialog`, `sheet`, `dropdown-menu`, `context-menu`, `toggle`, `switch`, `button` |
| Feedback | `sonner` |

## File Structure

### Current State (Implemented)

```
roxidy/
├── docs/                         # Documentation
│   ├── architecture.md           # This file
│   ├── decisions.md              # Architecture Decision Records
│   ├── tauri-ipc.md              # Tauri commands/events spec
│   ├── shell-integration.md      # OSC 133 shell integration
│   ├── ai-integration.md         # Rig agent setup (planned)
│   ├── frontend-state.md         # Zustand store design
│   ├── security.md               # Security considerations
│   └── gaps-and-todos.md         # Known issues and roadmap
├── src/                          # React frontend
│   ├── components/
│   │   ├── CommandBlock/         # Command block display
│   │   │   ├── CommandBlock.tsx  # Individual block with collapse
│   │   │   ├── CommandBlockList.tsx
│   │   │   └── index.ts
│   │   ├── CommandInput/         # Custom command input
│   │   │   ├── CommandInput.tsx  # Input with history, shortcuts
│   │   │   └── index.ts
│   │   ├── TabBar/               # Session tabs
│   │   │   ├── TabBar.tsx
│   │   │   └── index.ts
│   │   ├── Terminal/             # xterm.js wrapper (unused)
│   │   │   └── Terminal.tsx
│   │   └── ui/                   # shadcn components (22 files)
│   ├── hooks/
│   │   ├── use-mobile.ts         # Responsive detection
│   │   └── useTauriEvents.ts     # Tauri event listeners
│   ├── store/
│   │   └── index.ts              # Zustand store with immer
│   ├── lib/
│   │   ├── utils.ts              # cn() class merger
│   │   ├── tauri.ts              # Typed invoke wrappers
│   │   └── ansi.ts               # OSC sequence stripping
│   ├── App.tsx                   # Main app component
│   ├── index.css                 # Tailwind + CSS variables
│   ├── main.tsx                  # React entry point
│   └── vite-env.d.ts
├── src-tauri/
│   ├── src/
│   │   ├── main.rs               # Tauri entry point
│   │   ├── lib.rs                # Plugin registration
│   │   ├── state.rs              # AppState with PtyManager
│   │   ├── error.rs              # Error types
│   │   ├── pty/
│   │   │   ├── mod.rs
│   │   │   ├── manager.rs        # PTY lifecycle + events
│   │   │   └── parser.rs         # VTE + OSC 133 parsing
│   │   └── commands/
│   │       ├── mod.rs
│   │       ├── pty.rs            # pty_create, pty_write, etc.
│   │       └── shell.rs          # shell_integration_* commands
│   ├── capabilities/
│   │   └── default.json
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── build.rs
├── CHANGELOG.md
├── README.md
├── components.json               # shadcn configuration
├── package.json
├── pnpm-lock.yaml
├── tsconfig.json
├── tsconfig.node.json
├── vite.config.ts
└── biome.json                    # Linter/formatter config
```

### Planned Additions

```
src-tauri/src/
├── ai/                           # AI agent (planned)
│   ├── mod.rs
│   ├── agent.rs                  # Rig agent setup
│   ├── tools.rs                  # Tool definitions
│   └── providers.rs              # Multi-provider config
└── db/                           # SQLite storage (planned)
    ├── mod.rs
    ├── schema.rs
    └── queries.rs

src/components/
├── AI/                           # AI panel (planned)
│   ├── AIPanel.tsx
│   ├── ChatMessage.tsx
│   └── ToolCallDisplay.tsx
└── Settings/                     # Settings dialog (planned)
    ├── SettingsDialog.tsx
    └── ThemePicker.tsx
```

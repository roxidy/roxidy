# Architecture Decisions Record

This document captures key technical decisions made during roxidy development.

---

## ADR-001: PTY Implementation

**Date:** 2025-11-26

**Status:** Accepted

**Context:**
Roxidy needs to spawn and manage interactive shell sessions with full terminal emulation support (vim, htop, etc.).

**Decision:** Use `portable-pty` exclusively for all terminal operations.

**Rationale:**

`portable-pty` is the industry-standard Rust library for pseudo-terminal management, used by Alacritty and other terminal emulators. It provides:

- **Interactive sessions** — Stdin remains open for continuous input
- **Terminal resize** — Handles SIGWINCH for dynamic resizing
- **Full ANSI/VT100 support** — Works with curses-based applications
- **Cross-platform** — macOS, Linux, Windows support

**Alternatives Rejected:**

| Option | Why Rejected |
|--------|--------------|
| `tauri-plugin-shell` | Designed for one-shot commands, not interactive sessions. Closes stdin after spawn. No terminal resize support. |
| Raw `std::process::Command` | No PTY support, cannot run interactive programs |

**Consequences:**
- All terminal operations (user sessions + AI agent commands) use `portable-pty`
- Consistent behavior across all command execution
- PTY lifecycle management handled in Rust backend
- AI agent's `run_command` tool writes to PTY like a user would

---

## ADR-002: UI Component Library

**Date:** 2025-11-26

**Status:** Accepted

**Context:**
Need a component library for the React frontend that:
- Works well with Tailwind CSS
- Provides accessible, composable primitives
- Doesn't add runtime bundle bloat
- Supports theming

**Decision:** Use shadcn/ui with Tailwind CSS v4.

**Rationale:**
- **Not a dependency** — Components are copied into the project as source files
- **Full control** — Can modify components directly without fighting a library
- **Tailwind v4 native** — Uses CSS variables and `@theme` for theming
- **Accessible** — Built on Radix UI primitives
- **TypeScript first** — Full type safety

**Alternatives Considered:**
- **Radix UI directly** — More work to style, shadcn wraps this already
- **Chakra UI** — Runtime CSS-in-JS, heavier bundle
- **Headless UI** — Fewer components, would need more custom work
- **Ant Design / MUI** — Opinionated styling, harder to customize

**Consequences:**
- Components live in `src/components/ui/`
- Theming via CSS variables in `src/index.css`
- Can add/remove components as needed with `pnpm dlx shadcn@latest add <component>`

---

## ADR-003: Component Selection

**Date:** 2025-11-26

**Status:** Accepted

**Context:**
Which shadcn components to install for roxidy's feature set.

**Decision:** Install 22 components covering terminal UI, AI interface, and app chrome.

### Terminal & Command Blocks

| Component | Purpose |
|-----------|---------|
| `command` | Cmd+K command palette |
| `collapsible` | Expand/collapse command output |
| `card` | Command block container |
| `scroll-area` | Terminal scrollback with custom scrollbar |
| `separator` | Visual divider between blocks |
| `badge` | Exit code indicators (success/error) |
| `skeleton` | Loading placeholder for streaming output |
| `resizable` | Drag to resize terminal/AI panel split |

### AI Agent Interface

| Component | Purpose |
|-----------|---------|
| `textarea` | Multi-line AI prompt input |
| `input` | Single-line inputs |
| `popover` | Autocomplete suggestions |
| `tooltip` | Hover information |

### App Chrome & Navigation

| Component | Purpose |
|-----------|---------|
| `sidebar` | Session/tab list |
| `tabs` | Multiple terminal sessions |
| `dialog` | Modal dialogs (settings, confirmations) |
| `sheet` | Slide-out panels |
| `dropdown-menu` | Action menus |
| `context-menu` | Right-click menus on blocks |
| `toggle` | View mode switches |
| `switch` | Boolean settings |
| `button` | Actions |

### Feedback

| Component | Purpose |
|-----------|---------|
| `sonner` | Toast notifications (command complete, errors) |

### Not Installed (May Add Later)

| Component | Reason to Skip |
|-----------|----------------|
| `menubar` | Native Tauri menus preferred for desktop feel |
| `breadcrumb` | No deep navigation hierarchy |
| `avatar` | No user avatars currently planned |
| `progress` | `skeleton` covers loading states |
| `alert` | `sonner` toasts handle notifications |

**Consequences:**
- 22 component files in `src/components/ui/`
- Dependencies added: `cmdk`, `sonner`, `react-resizable-panels`, Radix primitives
- Can add more components later with zero config

---

## ADR-004: State Management

**Date:** 2025-11-26

**Status:** Accepted

**Context:**
Need state management for:
- Terminal sessions and command blocks
- AI conversations and streaming state
- Application settings
- UI state (panel sizes, active tabs)

**Decision:** Use Zustand with immer middleware.

**Rationale:**
- **Lightweight** — ~1KB vs Redux's ~7KB
- **No boilerplate** — No action creators, reducers, or providers
- **TypeScript native** — Excellent type inference
- **Immer integration** — Immutable updates with mutable syntax
- **Tauri-friendly** — Easy to connect to Tauri events
- **Selective subscriptions** — Components only re-render when their slice changes

**Store Structure:**
```
RoxidyStore
├── sessions     — Terminal sessions (tabs)
├── terminal     — Command blocks per session
├── ai           — Conversations, streaming state
└── settings     — User preferences
```

**Consequences:**
- Single store in `src/store/index.ts`
- Slices in `src/store/*.ts`
- Tauri events connected via `useTauriEvents` hook
- Selectors for performance optimization

---

## ADR-005: AI Provider Architecture

**Date:** 2025-11-26

**Status:** Accepted

**Context:**
User requirement to support multiple AI providers:
- Google Vertex AI
- OpenRouter
- Anthropic
- OpenAI
- Local models (Ollama)

**Decision:** Use [rig](https://github.com/0xPlaygrounds/rig) for provider abstraction.

**Rationale:**
- **Rust-native** — Runs in Tauri backend, no Node.js dependency
- **Multi-provider** — Single API for different LLM providers
- **Tool support** — Built-in function calling / tool use
- **Streaming** — Native async streaming support
- **Extensible** — Can add custom providers via OpenAI-compatible API

**Provider Configuration:**
```rust
enum AIProvider {
    Anthropic,    // Direct API
    OpenAI,       // Direct API
    Google,       // Vertex AI
    OpenRouter,   // OpenAI-compatible
    Ollama,       // Local, OpenAI-compatible
    Custom,       // Any OpenAI-compatible endpoint
}
```

**Consequences:**
- AI processing happens in Rust, not JavaScript
- Streaming responses via Tauri events
- API keys stored in SQLite (encrypted) or environment variables
- Tool definitions in Rust with `rig::Tool` trait

---

## ADR-006: Shell Integration Protocol

**Date:** 2025-11-26

**Status:** Accepted

**Context:**
Need to detect command boundaries, exit codes, and working directory changes to create Warp-like command blocks.

**Decision:** Use OSC 133 (FinalTerm) semantic prompt sequences.

**Rationale:**
- **Industry standard** — Used by iTerm2, VSCode, Warp, Windows Terminal
- **Shell-agnostic** — Works with any shell that implements the sequences
- **Non-invasive** — Escape sequences are invisible to other terminals
- **Rich information** — Provides prompt start/end, command start/end, exit codes

**Sequence Format:**
```
ESC ] 133 ; A ST    → Prompt start
ESC ] 133 ; B ST    → Prompt end (user can type)
ESC ] 133 ; C ST    → Command execution start
ESC ] 133 ; D ; N ST → Command end with exit code N
```

**Implementation:**
- Shell integration script at `~/.config/roxidy/integration.zsh`
- VTE parser in Rust extracts sequences from PTY output
- Only active when `$ROXIDY` environment variable is set

**Consequences:**
- Requires shell integration script to be sourced
- First-run onboarding prompts user to install
- Fallback: terminal still works without blocks, just raw output
- Starting with zsh only; bash/fish can be added later

---

## ADR-007: Data Persistence

**Date:** 2025-11-26

**Status:** Accepted

**Context:**
Need to persist:
- Command history with output
- User settings
- AI conversation history
- Session state

**Decision:** Use SQLite via `rusqlite`.

**Rationale:**
- **Single file** — Easy backup, no server process
- **Rust-native** — `rusqlite` with bundled SQLite
- **Queryable** — Can search command history
- **Reliable** — ACID transactions, corruption-resistant
- **Proven** — Used by Firefox, Chrome, iOS, countless apps

**Schema Highlights:**
```sql
sessions          — Terminal session metadata
command_blocks    — Full command history with output
settings          — Key-value user preferences
ai_conversations  — Conversation history per session
```

**Database Location:**
- macOS: `~/Library/Application Support/dev.roxidy.roxidy/roxidy.db`
- Linux: `~/.local/share/roxidy/roxidy.db`
- Windows: `%APPDATA%\roxidy\roxidy.db`

**Consequences:**
- All persistence handled in Rust backend
- Frontend requests data via Tauri commands
- Can implement search, analytics, export features
- Need migration strategy for schema changes

---

## Future Decisions (Not Yet Made)

| Topic | Options | Notes |
|-------|---------|-------|
| Terminal renderer | xterm.js vs custom | xterm.js for compatibility, custom for Warp-like blocks |
| Theme format | JSON vs TOML | Need to decide theme file structure |
| Plugin system | WASM vs Lua vs none | For user extensions |
| Sync | None vs cloud | Command history sync across devices |
| Keybinding config | JSON vs custom DSL | How users customize shortcuts |

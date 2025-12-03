AI-powered terminal emulator built with Tauri 2 (Rust backend, React 19 frontend).

## About This Project

This is **Qbit's own codebase**. If you are Qbit, then you are the AI agent being developed here. The system prompt you operate under is defined in `src-tauri/crates/vtcode-core/src/prompts/system.md`. When working on this project, you have unique insight into how changes will affect your own behavior.

Key self-referential files:
- `src-tauri/crates/vtcode-core/src/prompts/` - Your prompts and tool definitions
- `src-tauri/crates/vtcode-core/src/agent/` - Your agent orchestration logic
- `src-tauri/src/ai/` - Tauri bridge connecting you to the frontend
- `src/components/AgentChat/` - How users see your responses

## Commands

```bash
# Development
just dev              # Start full app (frontend + Tauri)
just dev-fe           # Frontend only (Vite dev server)

# Testing
just test             # All tests (frontend + Rust)
just test-fe          # Frontend tests (Vitest)
just test-rust        # Rust tests (cargo test)
pnpm test             # Frontend watch mode

# Code Quality
just check            # All checks (biome + clippy + fmt)
just fix              # Auto-fix frontend (biome)
just fmt              # Format all code
cd src-tauri && cargo clippy -- -D warnings  # Rust lint

# Build
just build            # Production build
just build-rust       # Rust only (debug)
```

## Project Structure

```
src/                      # React frontend
  components/
    ui/                   # shadcn/ui primitives (do not edit directly)
    AgentChat/            # AI chat UI (messages, tool cards, approval dialogs)
    UnifiedTimeline/      # Main content view (commands + agent messages)
    UnifiedInput/         # Mode-switching input (terminal/agent toggle)
  hooks/
    useTauriEvents.ts     # Subscribes to Rust backend events
    useAiEvents.ts        # Subscribes to AI streaming events
  lib/
    tauri.ts              # Typed wrappers for invoke() calls
    ai.ts                 # AI-specific invoke wrappers
  store/index.ts          # Zustand store (single file, all state)

src-tauri/src/            # Rust backend
  ai/                     # AI agent system (vtcode-core integration)
    agent_bridge.rs       # Bridge between Tauri and vtcode agent
    sub_agent.rs          # Sub-agent orchestration
    commands.rs           # Tauri command handlers
  pty/                    # Terminal management
    manager.rs            # PTY session lifecycle
    parser.rs             # VTE/OSC sequence parsing
  commands.rs             # All Tauri commands (exported from lib.rs)
  state.rs                # AppState (shared across commands)

src-tauri/crates/
  rig-anthropic-vertex/   # Custom crate for Anthropic on Vertex AI
```

## Conventions

### TypeScript/React
- Path alias: `@/*` maps to `./src/*`
- Components: PascalCase directories with `index.ts` barrel exports
- State: Single Zustand store with Immer middleware
- Tauri calls: Always use typed wrappers from `lib/tauri.ts` or `lib/ai.ts`
- Formatting: Biome (2-space indent, double quotes, semicolons, trailing commas)

### Rust
- Module structure: `mod.rs` re-exports public items
- Error handling: `anyhow::Result` for commands, `thiserror` for custom errors
- Async: Tokio runtime (full features)
- Events: Use `app.emit()` for frontend communication
- Logging: `tracing` crate with `debug!`, `info!`, `warn!`, `error!`

### Tauri Integration
- Commands defined in `src-tauri/src/commands.rs`, registered in `lib.rs`
- Events: snake_case names (`terminal_output`, `command_block`, `ai-event`)
- Frontend listens via `@tauri-apps/api/event`

## Key Dependencies

| Purpose | Package |
|---------|---------|
| AI/LLM | vtcode-core, rig-core |
| Terminal | portable-pty, vte, @xterm/xterm |
| UI | shadcn/ui, Radix primitives, Tailwind v4 |
| State | Zustand + Immer |

## Testing

- Frontend: Vitest + React Testing Library
- Tauri mocks: `src/test/mocks/tauri-event.ts`
- Run `pnpm test:coverage` for coverage report
- Rust: Standard `cargo test` (no special setup)

## Gotchas

- Shell integration uses OSC 133 sequences; test with real shell sessions
- AI initialization requires Vertex AI credentials at hardcoded path (see `App.tsx`)
- The `ui/` components are shadcn-generated; modify via shadcn CLI, not directly
- Streaming blocks use interleaved text/tool pattern; check `streamingBlocks` in store

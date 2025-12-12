AI-powered terminal emulator built with Tauri 2 (Rust backend, React 19 frontend).

## About This Project

This is **Qbit's own codebase**. If you are Qbit, then you are the AI agent being developed here. 
The system prompt you operate under is defined in `src-tauri/crates/vtcode-core/src/prompts/system.md`. 
When working on this project, you have unique insight into how changes will affect your own behavior.

## Commands

```bash
# Development
just dev              # Full app (in current directory)
just dev ~/Code/foo   # Full app (opens in specified directory)
just dev-fe           # Frontend only (Vite on port 1420)

# Testing
just test             # All tests (frontend + Rust)
just test-fe          # Frontend tests (Vitest, single run)
just test-watch       # Frontend tests (watch mode)
just test-rust        # Rust tests
pnpm test:coverage    # Frontend coverage report

# Code Quality
just check            # All checks (biome + clippy + fmt)
just fix              # Auto-fix frontend (biome --write)
just fmt              # Format all (frontend + Rust)

# Build
just build            # Production build
just build-rust       # Rust only (debug)

# CLI Binary (headless mode)
cargo build -p qbit --features cli --no-default-features --bin qbit-cli
./target/debug/qbit-cli -e "prompt" --auto-approve
```

## Architecture

```
React Frontend (src/)
        |
        v (invoke / listen)
  Tauri Commands & Events
        |
        v
   Rust Backend (src-tauri/src/)
        |
        +-- PTY Manager (terminal sessions)
        +-- AI Module (agent orchestration)
        |       +-- vtcode-core (external crate)
        |       +-- rig-anthropic-vertex (local crate)
        +-- Sidecar (context capture + LanceDB)
        +-- Settings (TOML config)
```

## Project Structure

```
src/                      # React frontend
  components/
    ui/                   # shadcn/ui primitives (modify via shadcn CLI only)
    AgentChat/            # AI chat UI (messages, tool cards, approval dialogs)
    UnifiedTimeline/      # Main content view (commands + agent messages)
    UnifiedInput/         # Mode-switching input (terminal/agent toggle)
    Sidecar/              # Context capture panel
    WorkflowTree/         # Multi-step workflow visualization
  hooks/
    useTauriEvents.ts     # Terminal/PTY event subscriptions
    useAiEvents.ts        # AI streaming event subscriptions (30+ event types)
  lib/
    tauri.ts              # Typed wrappers for PTY/shell invoke() calls
    ai.ts                 # AI-specific invoke wrappers
    sidecar.ts            # Sidecar invoke wrappers
    settings.ts           # Settings invoke wrappers
  store/index.ts          # Zustand store (single file, Immer middleware)

src-tauri/src/            # Rust backend
  ai/                     # AI agent system
    agent_bridge.rs       # Bridge between Tauri and vtcode agent
    agentic_loop.rs       # Main agent execution loop
    tool_executors.rs     # Tool implementation handlers
    tool_definitions.rs   # Tool schemas and configs
    events.rs             # AiEvent enum (30+ event types)
    hitl/                 # Human-in-the-loop approval system
    workflow/             # Multi-step workflow system (graph-flow)
    commands/             # AI-specific Tauri commands
  pty/                    # Terminal management
    manager.rs            # PTY session lifecycle
    parser.rs             # VTE/OSC sequence parsing
  sidecar/                # Context capture system
    storage.rs            # LanceDB vector storage
    synthesis.rs          # Commit message / summary generation
    layer1/               # Event processing pipeline
  settings/               # TOML settings
    schema.rs             # QbitSettings struct definitions
    loader.rs             # File loading with env var interpolation
  commands/               # General Tauri commands (PTY, shell, themes, files)
  cli/                    # CLI-specific code (args, runner, output)
  bin/qbit-cli.rs         # Headless CLI binary entry point
  lib.rs                  # Command registration and app entry point

src-tauri/crates/
  rig-anthropic-vertex/   # Custom crate for Anthropic on Vertex AI

evals/                    # LLM evaluation framework (Python)
  test_cli.py             # DeepEval test cases for qbit-cli
  conftest.py             # pytest fixtures (cli runner, eval model)

docs/                     # Documentation
  eval-setup.md           # Evaluation framework setup guide
  cli-plan.md             # CLI development roadmap
```

## Feature Flags

| Flag | Description | Default |
|------|-------------|---------|
| `tauri` | GUI application (Tauri window) | Yes |
| `cli` | Headless CLI binary | No |
| `local-llm` | Local LLM via mistral.rs (Metal GPU) | No |

Flags `tauri` and `cli` are mutually exclusive.

## Environment Setup

Create `.env` in project root:
```bash
# Required for Vertex AI (or set in ~/.qbit/settings.toml)
GOOGLE_APPLICATION_CREDENTIALS=/path/to/service-account.json
VERTEX_AI_PROJECT_ID=your-project-id
VERTEX_AI_LOCATION=us-east5

# Optional: for web search tool
TAVILY_API_KEY=your-key
```

Settings file: `~/.qbit/settings.toml` (auto-generated on first run, see `src-tauri/src/settings/template.toml`)

Sessions stored in: `~/.qbit/sessions/` (override with `VT_SESSION_DIR` env var)

Workspace override: `just dev /path/to/project` or set `QBIT_WORKSPACE` env var

## Event System

### Terminal Events
| Event | Payload | Description |
|-------|---------|-------------|
| `terminal_output` | `{session_id, data}` | Raw PTY output |
| `command_block` | `CommandBlock` | Parsed command with output |

### AI Events (emitted as `ai-event`)
| Event Type | Key Fields | Description |
|------------|------------|-------------|
| `started` | `turn_id` | Agent turn started |
| `text_delta` | `delta`, `accumulated` | Streaming text chunk |
| `tool_approval_request` | `request_id`, `tool_name`, `args`, `risk_level` | Requires user approval |
| `tool_auto_approved` | `request_id`, `reason` | Auto-approved by pattern |
| `tool_result` | `request_id`, `success`, `result` | Tool execution completed |
| `reasoning` | `content` | Extended thinking content |
| `completed` | `response`, `tokens_used` | Turn finished |
| `error` | `message`, `error_type` | Error occurred |
| `workflow_*` | `workflow_id`, `step_*` | Workflow lifecycle events |
| `context_*` | utilization metrics | Context window management |
| `loop_*` | detection stats | Loop protection events |

## Conventions

### TypeScript/React
- Path alias: `@/*` maps to `./src/*`
- Components: PascalCase directories with `index.ts` barrel exports
- State: Single Zustand store with Immer middleware (`enableMapSet()` for Set/Map)
- Tauri calls: Always use typed wrappers from `lib/*.ts`, never raw `invoke()`
- Formatting: Biome (2-space indent, double quotes, semicolons, trailing commas ES5)

### Rust
- Module structure: `mod.rs` re-exports public items
- Error handling: `anyhow::Result` for commands, `thiserror` for domain errors
- Async: Tokio runtime (full features)
- Events: `app.emit("event-name", payload)` for frontend communication
- Logging: `tracing` crate (`debug!`, `info!`, `warn!`, `error!`)

### Tauri Integration
- Commands distributed across modules:
  - `src-tauri/src/commands/*.rs` - PTY, shell, themes, files
  - `src-tauri/src/ai/commands/*.rs` - AI agent commands
  - `src-tauri/src/settings/commands.rs` - Settings commands
  - `src-tauri/src/sidecar/commands.rs` - Sidecar commands
  - `src-tauri/src/indexer/commands.rs` - Code indexer commands
- All commands registered in `lib.rs`
- Frontend listens via `@tauri-apps/api/event`

## Key Dependencies

| Purpose | Package |
|---------|---------|
| AI/LLM | vtcode-core (external crate), rig-core |
| AI routing | rig-anthropic-vertex (local crate) |
| Terminal | portable-pty, vte, @xterm/xterm |
| Vector DB | LanceDB, fastembed |
| Workflows | graph-flow |
| UI | shadcn/ui, Radix primitives, Tailwind v4 |
| State | Zustand + Immer |

## Testing

- Frontend: Vitest + React Testing Library + jsdom
- Tauri mocks: `src/test/mocks/tauri-event.ts` (aliased in vitest.config.ts)
- Rust: Standard `cargo test` (includes proptest for property-based tests)
- Setup file: `src/test/setup.ts`

## Evaluations (evals/)

LLM-based evaluation framework using [DeepEval](https://deepeval.com/) for testing `qbit-cli` responses.

```bash
cd evals

# Setup (one-time)
uv venv .venv && source .venv/bin/activate
uv pip install -e .

# Run basic CLI tests (no API key needed)
pytest test_cli.py -v -k "TestCliBasics"

# Run full suite with LLM evaluation (requires OpenAI key)
RUN_API_TESTS=1 pytest test_cli.py -v

# Verbose output
RUN_API_TESTS=1 VERBOSE=1 pytest test_cli.py -v
```

**Configuration**: Set `OPENAI_API_KEY` env var or add to `~/.qbit/settings.toml`:
```toml
[eval]
model = "gpt-4o-mini"  # Recommended for routine testing
api_key = "sk-..."
```

**Key files**:
- `test_cli.py` - Test cases using GEval metrics
- `conftest.py` - Fixtures (`cli`, `eval_model`)
- `inspect_qbit_eval.py` - Inspection utilities

See `docs/eval-setup.md` for full documentation on writing evaluations.

## Gotchas

- Shell integration uses OSC 133 sequences; test with real shell sessions
- The `ui/` components are shadcn-generated; modify via `pnpm dlx shadcn@latest`, not directly
- vtcode-core is an external dependency (not in `src-tauri/crates/`); check crates.io for docs
- Streaming blocks use interleaved text/tool pattern; see `streamingBlocks` in store
- LanceDB requires rustls crypto provider init before any TLS operations (done in `lib.rs`)
- Feature flags are mutually exclusive: `--features tauri` (default) vs `--features cli`

## Adding New Features

### New Tauri Command
1. Create function in appropriate `commands.rs` file (or `ai/commands/*.rs`)
2. Annotate with `#[tauri::command]`
3. Add to `tauri::generate_handler![]` in `lib.rs`
4. Add typed wrapper in `src/lib/*.ts`

### New AI Tool
1. Add tool definition in `ai/tool_definitions.rs`
2. Add executor in `ai/tool_executors.rs`
3. Register in the tool registry

### New AI Event
1. Add variant to `AiEvent` enum in `ai/events.rs`
2. Emit via `app.emit("ai-event", event)`
3. Handle in `src/hooks/useAiEvents.ts`

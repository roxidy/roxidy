# Roxidy

A modern terminal emulator inspired by [Warp](https://warp.dev), built with Tauri 2, React/TypeScript, and Rust.

## Features

### Implemented (POC v0.1)

- **Command Blocks** - Commands and their output are displayed as discrete, collapsible blocks with:
  - Exit code indicators (green check/red X)
  - Execution duration
  - ANSI color rendering via `ansi-to-react`
  - Collapsible output sections

- **Multi-Tab Support** - Multiple terminal sessions with:
  - Tab bar with session management
  - Keyboard shortcut: `Cmd+T` for new tab
  - Working directory shown in tab name

- **Shell Integration** - Automatic OSC 133 sequence parsing for:
  - Command boundary detection (prompt start/end, command start/end)
  - Working directory tracking (OSC 7)
  - Command text capture with exit codes

- **Modern UI** - Tokyo Night-inspired dark theme with:
  - Tailwind CSS v4
  - shadcn/ui components
  - Toast notifications via Sonner

### Limitations

- **Interactive Commands Blocked** - vim, htop, ssh, and similar interactive programs show an error toast. Full terminal emulation for these programs is planned for a future release.
- **zsh Only** - Currently only supports zsh. bash/fish support planned.
- **No AI Integration Yet** - AI assistant panel is designed but not implemented.

## Architecture

```
┌────────────────────────────────────────────────────────────┐
│                  React Frontend (Vite)                      │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │ CommandInput│  │ Command     │  │ TabBar              │ │
│  │ (custom)    │  │ BlockList   │  │ (session management)│ │
│  └─────────────┘  └─────────────┘  └─────────────────────┘ │
└───────────────────────────┬────────────────────────────────┘
                            │ Tauri IPC
┌───────────────────────────┴────────────────────────────────┐
│                    Rust Backend (Tauri 2)                   │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │ PTY Manager  │  │ VTE Parser   │  │ Shell Integration│  │
│  │ (portable-   │  │ (OSC 133,    │  │ (zsh hooks,      │  │
│  │  pty)        │  │  OSC 7)      │  │  auto-install)   │  │
│  └──────────────┘  └──────────────┘  └──────────────────┘  │
└────────────────────────────────────────────────────────────┘
```

## Getting Started

### Prerequisites

- Node.js 18+
- pnpm
- Rust 1.70+
- macOS (Linux/Windows support planned)

### Development

```bash
# Install dependencies
pnpm install

# Start development server
pnpm tauri dev
```

### Build

```bash
pnpm tauri build
```

## Project Structure

```
roxidy/
├── src/                          # React frontend
│   ├── components/
│   │   ├── CommandBlock/         # Command block display
│   │   ├── CommandInput/         # Custom command input
│   │   ├── TabBar/               # Tab management
│   │   ├── Terminal/             # xterm.js wrapper (unused)
│   │   └── ui/                   # shadcn components
│   ├── hooks/
│   │   └── useTauriEvents.ts     # Tauri event listeners
│   ├── store/
│   │   └── index.ts              # Zustand state management
│   ├── lib/
│   │   ├── tauri.ts              # Typed Tauri IPC wrappers
│   │   ├── ansi.ts               # OSC sequence stripping
│   │   └── utils.ts              # Utilities
│   └── App.tsx                   # Main application
├── src-tauri/
│   └── src/
│       ├── pty/
│       │   ├── manager.rs        # PTY lifecycle management
│       │   └── parser.rs         # VTE + OSC 133 parsing
│       ├── commands/
│       │   ├── pty.rs            # PTY Tauri commands
│       │   └── shell.rs          # Shell integration commands
│       ├── state.rs              # Application state
│       ├── error.rs              # Error types
│       └── lib.rs                # Tauri plugin setup
└── docs/                         # Documentation
```

## Documentation

See the `docs/` directory for detailed documentation:

- [Architecture](docs/architecture.md) - System design and components
- [Frontend State](docs/frontend-state.md) - Zustand store structure
- [Shell Integration](docs/shell-integration.md) - OSC 133 sequences
- [Tauri IPC](docs/tauri-ipc.md) - Command and event specs
- [Security](docs/security.md) - Security considerations
- [AI Integration](docs/ai-integration.md) - Planned AI features
- [Gaps and TODOs](docs/gaps-and-todos.md) - Known issues and roadmap

## Tech Stack

### Frontend
- React 19
- TypeScript
- Tailwind CSS v4
- Zustand + Immer (state management)
- shadcn/ui (components)
- lucide-react (icons)
- ansi-to-react (ANSI rendering)
- sonner (toasts)

### Backend
- Tauri 2
- Rust
- portable-pty (PTY management)
- vte (terminal parsing)
- parking_lot (synchronization)
- serde (serialization)

## License

MIT

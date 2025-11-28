# Roxidy

**A powerful open-source agentic terminal, built with Rust.**

Warp showed us what a modern terminal could be. Roxidy takes that vision and makes it yours—fully open source and built for developers who want to own their tools.

<!-- TODO: Add screenshot or demo GIF here -->

## Why Roxidy?

- **Open Source** — No black boxes. Read the code, modify it, make it yours.
- **AI-Native** — Built-in agent system supporting all [vtcode](https://github.com/vinhnx/vtcode) providers (OpenRouter, OpenAI, Google Gemini, and more) plus Anthropic on Vertex AI. Bring your own API keys.
- **Modern Stack** — Rust backend for speed, React frontend for hackability.

## Features

- **Command Blocks** — Output organized into collapsible blocks with exit codes and timing. No more scrolling through walls of text.
- **Multi-Tab Sessions** — Spawn tabs with `Cmd+T`. Each tab is an independent PTY.
- **Shell Integration** — Automatic command boundary detection via OSC 133. Works with your existing shell.
- **Agentic AI** — Not just chat. The AI can analyze code, run commands, and execute multi-step workflows autonomously.

## Quick Start

```bash
pnpm install
pnpm tauri dev
```

**Requirements:** Node.js 18+, pnpm, Rust 1.70+, macOS

## Architecture

```
src/              # React frontend
  components/     # UI (shadcn + custom)
  stores/         # Zustand state management
src-tauri/src/    # Rust backend
  terminal/       # PTY management, OSC parsing
  ai/             # Agent system, tools, workflows
```

### Tech Stack

| Layer | Tech |
|-------|------|
| Framework | [Tauri 2](https://tauri.app) |
| Frontend | React 19, TypeScript, Vite, Tailwind v4, Zustand |
| Terminal | [xterm.js](https://xtermjs.org), [portable-pty](https://github.com/wez/wezterm/tree/main/pty), [vte](https://docs.rs/vte) |
| AI | [rig](https://github.com/0xPlaygrounds/rig) (LLM), [graph-flow](https://github.com/jkhoel/graph-flow) (orchestration) |
| UI | [shadcn/ui](https://ui.shadcn.com) |

### AI Tools

Powered by [vtcode](https://github.com/vinhnx/vtcode), a production-grade agentic toolkit for Rust. The AI agent has access to:

- **File Operations** — Read, write, and refactor code with unified diff output
- **Shell Execution** — Run commands with security controls and allowlists
- **Code Analysis** — Semantic understanding via Tree-sitter (Rust, Python, TypeScript, Go, Java, Swift)
- **Context Management** — Smart token budgeting for efficient LLM interactions
- **MCP Support** — Extend with Model Context Protocol tools

All tools run with workspace isolation, audit logging, and human-in-the-loop approval when needed.

## Roadmap

| Feature | Status |
|---------|--------|
| PTY + multi-session | Done |
| Command blocks UI | Done |
| Shell integration (OSC 133) | Done |
| AI agentic loop | Done |
| Interactive commands (vim, htop) | In progress |
| SQLite persistence | Planned |
| Plugin system | Planned |
| Custom keybindings | Planned |
| Theme engine | Planned |

## Contributing

Roxidy is early-stage and moving fast. If you're interested in building the future of terminal emulators, we'd love your help.

```bash
pnpm check:fix    # Lint + format (Biome)
pnpm test         # Run tests (Vitest)
```

## License

MIT

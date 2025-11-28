# Roxidy

A modern terminal emulator inspired by [Warp](https://warp.dev), built with Tauri 2, React, and Rust.

## Features

- **Command Blocks** - Commands and output displayed as discrete, collapsible blocks with exit codes and execution duration
- **Multi-Tab Support** - Multiple terminal sessions with `Cmd+T` shortcut
- **Shell Integration** - OSC 133 parsing for command boundaries and working directory tracking
- **AI Assistant** - Integrated AI chat with tool execution and human-in-the-loop approval

## Getting Started

### Prerequisites

- Node.js 18+
- pnpm
- Rust 1.70+
- macOS

### Development

```bash
pnpm install
pnpm tauri dev
```

### Build

```bash
pnpm tauri build
```

## Tech Stack

**Frontend:** React 19, TypeScript, Tailwind CSS v4, Zustand, shadcn/ui

**Backend:** Tauri 2, Rust, portable-pty, vte

## License

MIT

# Changelog

All notable changes to roxidy will be documented in this file.

## [Unreleased]

### Added

#### Critical Design Decisions
- **Command Text Capture:** Extended OSC 133;C marker to include command payload from preexec
- **Terminal Rendering:** Dual-mode strategy - xterm.js base with React block overlays, alternate screen detection for fullscreen apps
- **AI Command Execution:** Same PTY, queued until prompt ready (B marker), with visual execution indicator
- **Rig API Verification:** Verified actual rig crate API, updated documentation to match

#### Documentation
- `docs/architecture.md` — High-level system architecture, component diagram, file structure
- `docs/tauri-ipc.md` — Tauri commands and events specification
- `docs/shell-integration.md` — OSC 133 shell integration script and installation
- `docs/ai-integration.md` — Rig agent setup, multi-provider config, tool definitions
- `docs/frontend-state.md` — Zustand store structure and Tauri event binding
- `docs/decisions.md` — Architecture Decision Records (ADRs)
- `docs/gaps-and-todos.md` — Documentation gaps review and implementation TODOs

### Changed

#### AI Integration (`docs/ai-integration.md`)
- Fixed provider module: `google` → `gemini` for Google Gemini API
- Fixed env var: `GOOGLE_API_KEY` → `GEMINI_API_KEY`
- Fixed Tool trait: `definition()` is now `async fn definition(&self, _prompt: String)`
- Fixed streaming types: `StreamItem::*` → `MultiTurnStreamItem::StreamItem(StreamedAssistantContent::*)`
- Added `ToolCallDelta` and `Reasoning` stream handling
- Added API verification note with confirmed patterns

#### Shell Integration (`docs/shell-integration.md`)
- Fixed exit code bug: now passes exit code explicitly to `__roxidy_cmd_end`
- Added command capture: OSC 133;C marker now includes command text from preexec
- Documented OSC 133 extensions table

#### Architecture (`docs/architecture.md`)
- Added `thiserror = "1.0"` to Cargo dependencies
- Changed `directories = "5"` to `dirs = "5"` for home/config paths
- Split frontend deps into "Currently Installed" and "To Be Installed" sections
- Added installation commands for xterm.js and zustand

#### Frontend State (`docs/frontend-state.md`)
- Added complete `ThemeSlice` with terminal colors and UI colors
- Added "Type Serialization Notes" section documenting Rust↔TypeScript type mappings

#### ADRs (`docs/decisions.md`)
- Fixed all date typos: "2024-11-26" → "2025-11-26"

### Added

#### Security Documentation
- Created `docs/security.md` covering:
  - API key storage strategies (env vars, SQLite, OS keychain)
  - Command history sanitization with pattern detection
  - AI tool approval flow with risk classification
  - Destructive command detection patterns
  - File system sandboxing and path validation
  - Threat model and security checklist

#### Frontend Setup
- Installed Tailwind CSS v4 with `@tailwindcss/vite` plugin
- Configured TypeScript path aliases (`@/*` → `./src/*`)
- Initialized shadcn/ui with new-york style and neutral base color
- Installed 22 shadcn components:
  - Terminal: `command`, `collapsible`, `card`, `scroll-area`, `separator`, `badge`, `skeleton`, `resizable`
  - AI: `textarea`, `input`, `popover`, `tooltip`
  - Chrome: `sidebar`, `tabs`, `dialog`, `sheet`, `dropdown-menu`, `context-menu`, `toggle`, `switch`, `button`
  - Feedback: `sonner`
- Created `src/lib/utils.ts` with `cn()` class merging utility
- Created `src/hooks/use-mobile.ts` for responsive detection

#### Configuration
- Updated `vite.config.ts` with Tailwind plugin and path aliases
- Updated `tsconfig.json` with baseUrl and paths configuration
- Created `components.json` for shadcn CLI configuration
- Updated `src/index.css` with Tailwind imports and CSS variables for theming

### Technical Decisions

- **PTY**: Use `portable-pty` exclusively for all terminal operations
- **UI**: shadcn/ui components with Tailwind CSS v4
- **State**: Zustand with immer middleware
- **AI**: Rig library for multi-provider LLM support
- **Shell Integration**: OSC 133 semantic prompt sequences
- **Storage**: SQLite via rusqlite

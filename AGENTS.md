# Repository Guidelines

## Project Structure & Module Organization

- `src/`: React 19 + TypeScript frontend (UI components, hooks, typed Tauri invoke wrappers, Zustand store).
- `src/components/ui/`: shadcn/ui primitives; regenerate via `pnpm dlx shadcn@latest`, don’t hand‑edit.
- `src-tauri/src/`: Rust backend for Tauri (AI agent system, PTY/terminal, sidecar context capture, settings, headless CLI).
- `evals/`: Python evaluation suite (pytest) that drives `qbit-cli`.
- `e2e/`: Playwright end‑to‑end tests. `docs/` and `public/` contain documentation and static assets.
- `dist/` is build output; don’t commit manual edits.

## Build, Test, and Development Commands

Prefer `just` (run `just --list` for all recipes):

- `just dev` / `just dev-fe`: run full app (Vite + Tauri) or frontend only.
- `just test`, `just test-fe`, `just test-rust`: Vitest unit tests and Rust tests.
- `just check` / `just fix` / `just fmt`: Biome lint/format + Rust clippy/rustfmt.
- `just build` / `just build-fe`: production builds.

Other useful commands: `pnpm install`, `pnpm preview`, `pnpm exec playwright test`, `just eval-fast`.

## Coding Style & Naming Conventions

- Frontend uses Biome (`biome.json`): 2‑space indent, 100‑column lines, double quotes, semicolons.
- TypeScript/React: prefer functional components; hooks live in `src/hooks/`; global state in `src/store/`.
- Rust: follow rustfmt defaults; keep clippy clean (`cargo clippy -D warnings`).
- Naming: `PascalCase` for React components, `kebab-case` for folders, `snake_case.rs` for Rust files.

## Testing Guidelines

- Frontend: Vitest + React Testing Library; name tests `*.test.ts(x)` near the code they cover.
- Rust: `cargo test` runs inline `#[cfg(test)]` modules under `src-tauri/src/`.
- Evals: `uv run pytest` in `evals/`; tests needing live LLM calls are marked `requires_api`.
- E2E: Playwright specs `e2e/*.spec.ts`.

## Commit & Pull Request Guidelines

- Use Conventional‑Commits‑style messages: `feat(scope): …`, `fix: …`, `refactor: …`, `chore: …`.
- PRs should describe intent, link issues, note tests run (`just precommit` is a good baseline), and include screenshots for UI changes.

## Configuration Notes

Local AI requires a root `.env` with Vertex AI credentials. Never commit secrets. User settings and sessions live under `~/.qbit/` by default.


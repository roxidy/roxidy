# Sidecar Simplification Plan

**Created**: 2025-01-XX
**Updated**: 2025-01-11
**Status**: Phase 2 Complete (Integration & Wiring)

## Quick Reference - Files to Delete

| File | Lines | Action |
|------|-------|--------|
| `sidecar/layer1/storage.rs` | 2,639 | DELETE |
| `sidecar/layer1/processor.rs` | 1,555 | DELETE |
| `sidecar/layer1/state.rs` | 1,317 | DELETE |
| `sidecar/layer1/verification_tests.rs` | 790 | DELETE |
| `sidecar/layer1/prompt.rs` | 477 | DELETE |
| `sidecar/layer1/api.rs` | 368 | DELETE |
| `sidecar/layer1/events.rs` | 59 | DELETE |
| `sidecar/layer1/mod.rs` | 47 | DELETE |
| `sidecar/storage.rs` | 1,848 | DELETE |
| `sidecar/integration_tests.rs` | 1,642 | DELETE |
| `sidecar/events.rs` | 1,508 | SIMPLIFY |
| `sidecar/capture.rs` | 1,088 | REWRITE |
| `sidecar/commands.rs` | 1,013 | REWRITE |
| `sidecar/state.rs` | 851 | REWRITE |
| `sidecar/synthesis.rs` | 733 | DELETE |
| `sidecar/models.rs` | 670 | DELETE |
| `sidecar/processor.rs` | 537 | DELETE |
| `sidecar/config.rs` | 492 | SIMPLIFY |
| `sidecar/prompts.rs` | 458 | DELETE |
| `sidecar/schema_verification_test.rs` | 408 | DELETE |
| `sidecar/synthesis_llm.rs` | 271 | DELETE |
| `sidecar/mod.rs` | 55 | REWRITE |
| `evals/sidecar/db.py` | ~862 | DELETE |
| `evals/test_sidecar_db.py` | ~500 | DELETE |
| `src/lib/sidecar.ts` | ~500 | REWRITE |
| `src/hooks/useLayer1Events.ts` | 75 | DELETE |
| `src/components/Sidecar/*` | ~400 | DELETE |

**Total to delete/rewrite: ~18,800+ lines**

---

> Replace the complex LanceDB + Layer1 state machine architecture with a simple markdown-based session tracking system.

## Overview

### Current Architecture (Complex)
- ~19,000 lines of Rust code
- LanceDB vector storage with multiple tables
- Complex type hierarchies (15+ enums, 10+ structs)
- Embedding model integration (fastembed)
- Local LLM integration (mistral.rs)
- JSON serialization/deserialization layers

### New Architecture (Simple)
- Markdown files as the source of truth
- Two files per session: `state.md` (rewritten) + `log.md` (append-only)
- Machine-managed `meta.toml` for metadata
- Optional `events.jsonl` for raw event storage (future semantic search)
- LLM interprets events and updates markdown directly

### New Session Structure
```
~/.qbit/sessions/{session_id}/
  meta.toml       # Machine-managed metadata (cwd, git info, timestamps)
  state.md        # LLM-managed current state (rewritten on each event)
  state.md.bak    # Previous state backup (for recovery)
  log.md          # Append-only event log with diffs
  events.jsonl    # Raw L0 events (optional, for future use)
```

---

## Deletion Checklist

### Rust Backend (`src-tauri/src/sidecar/`)

#### Layer 1 - DELETE ENTIRELY
- [ ] `layer1/state.rs` (1,317 lines) - Complex state structs and enums
- [ ] `layer1/storage.rs` (2,639 lines) - LanceDB table management
- [ ] `layer1/processor.rs` (1,555 lines) - Event processing with LLM/rules
- [ ] `layer1/prompt.rs` (477 lines) - 144-line system prompt for state interpreter
- [ ] `layer1/api.rs` (368 lines) - Layer1 API functions
- [ ] `layer1/events.rs` (59 lines) - Layer1 event types
- [ ] `layer1/mod.rs` (47 lines) - Module exports
- [ ] `layer1/verification_tests.rs` (790 lines) - Layer1 tests

**Subtotal: ~7,252 lines**

#### Core Sidecar - DELETE
- [ ] `storage.rs` (1,848 lines) - LanceDB storage (events, checkpoints, sessions tables)
- [ ] `models.rs` (670 lines) - Embedding + LLM model management
- [ ] `processor.rs` (537 lines) - Background processor (flush, embed, checkpoint)
- [ ] `synthesis.rs` (733 lines) - Commit/summary synthesis with templates
- [ ] `synthesis_llm.rs` (271 lines) - LLM-based synthesis
- [ ] `prompts.rs` (458 lines) - Prompt templates for synthesis
- [ ] `integration_tests.rs` (1,642 lines) - Integration tests
- [ ] `schema_verification_test.rs` (408 lines) - Schema tests

**Subtotal: ~6,567 lines**

#### Core Sidecar - SIMPLIFY/REWRITE
- [ ] `state.rs` (851 lines) - `SidecarState` needs complete rewrite
- [ ] `capture.rs` (1,088 lines) - `CaptureContext` needs simplification
- [ ] `events.rs` (1,508 lines) - Keep event types, remove complex serialization
- [ ] `config.rs` (492 lines) - Simplify config options
- [ ] `commands.rs` (1,013 lines) - Rewrite Tauri commands for new API
- [ ] `mod.rs` (55 lines) - Update exports

**Subtotal: ~5,007 lines to rewrite**

### Frontend (`src/`)

#### Components - DELETE ENTIRELY
- [ ] `src/components/Sidecar/CommitDraft.tsx` - Uses old synthesis API
- [ ] `src/components/Sidecar/ContextPanel.tsx` - Uses Layer1 state
- [ ] `src/components/Sidecar/SessionHistory.tsx` - Uses LanceDB queries
- [ ] `src/components/Sidecar/SidecarStatus.tsx` - Uses old status API
- [ ] `src/components/Sidecar/index.ts` - Barrel export

#### Hooks - DELETE
- [ ] `src/hooks/useLayer1Events.ts` (75 lines) - Layer1 event subscription

#### Lib - REWRITE
- [ ] `src/lib/sidecar.ts` (~500 lines) - Complete rewrite for new API

### Evals (`evals/`)

#### DELETE ENTIRELY
- [ ] `evals/sidecar/` directory
  - [ ] `evals/sidecar/__init__.py`
  - [ ] `evals/sidecar/assertions.py`
  - [ ] `evals/sidecar/db.py` (~862 lines) - LanceDB query utilities
- [ ] `evals/test_sidecar_db.py` - LanceDB integration tests

### Documentation (`docs/`)

#### DELETE/REWRITE
- [ ] `docs/sidecar.md` - Complete rewrite for new architecture

### Settings

#### MODIFY
- [ ] `src-tauri/src/settings/schema.rs` - Simplify `SidecarSettings`
- [ ] `src-tauri/src/settings/template.toml` - Update sidecar section
- [ ] `src/lib/settings.ts` - Update TypeScript types

---

## Files to KEEP (with modifications)

### Integration Points (require updates)

| File | Changes Needed |
|------|----------------|
| `src-tauri/src/lib.rs` | Remove old sidecar commands, add new ones |
| `src-tauri/src/state.rs` | Update `SidecarState` initialization |
| `src-tauri/src/ai/agent_bridge.rs` | Update sidecar integration |
| `src-tauri/src/ai/agentic_loop.rs` | Update event capture calls |
| `src-tauri/src/ai/commands/config.rs` | Update sidecar initialization |
| `src-tauri/src/ai/commands/mod.rs` | Update `configure_bridge` |
| `src-tauri/src/cli/bootstrap.rs` | Update CLI sidecar initialization |

### Keep As-Is

| File | Reason |
|------|--------|
| `src-tauri/src/sidecar/events.rs` | Event types are still useful (simplify serialization) |

---

## New Implementation Structure

```
src-tauri/src/sidecar/
├── mod.rs              # Public exports
├── session.rs          # Session struct & file operations (meta.toml, state.md, log.md)
├── processor.rs        # Event processing + LLM calls (~200 lines)
├── prompt.rs           # Single LLM prompt for state updates (~50 lines)
├── formats.rs          # File format templates (meta.toml, state.md, log.md)
├── events.rs           # Simplified event types (keep from old, remove complex fields)
├── config.rs           # Simplified config
└── commands.rs         # New Tauri commands

Estimated: ~800-1000 lines total (vs ~19,000 current)
```

---

## Migration Steps

### Phase 1: Create New Module ✅ COMPLETE
1. [x] Create new sidecar module structure (in-place, no _v2 suffix)
2. [x] Implement `session.rs` with file operations
3. [x] Implement `processor.rs` with rule-based state updates
4. [x] Implement `formats.rs` with templates (state.md, log.md, meta.toml)
5. [x] Write basic tests (59 passing)

### Phase 2: Wire Up Integration ✅ COMPLETE
1. [x] Create new Tauri commands in `commands.rs` (13 commands)
2. [x] Update `lib.rs` to register new commands
3. [x] Update `agent_bridge.rs` to use new sidecar
4. [x] Update `agentic_loop.rs` capture calls
5. [x] Update `state.rs` with `SidecarState` in `AppState`
6. [x] Backend compiles and tests pass

### Phase 3: Frontend Updates ✅ COMPLETE
1. [x] Simplify `src/lib/sidecar.ts` (566 → 196 lines)
   - Removed all Layer 1 types (Goal, Decision, FileContext, etc.)
   - Removed legacy API functions (synthesis backend, queries, etc.)
   - Added new types: `SidecarStatus`, `SessionMeta`, `SidecarConfig`
   - Added new functions: `getSessionState`, `getSessionLog`, `getSessionMeta`
2. [x] Create new `src/components/Sidecar/` directory
   - `SidecarStatus.tsx` - Minimal status indicator
   - `ContextPanel.tsx` - Slide-over panel showing state.md/log.md
   - `index.ts` - Barrel exports
3. [x] Fix `AiSettings.tsx` - Removed synthesis backend API calls
4. [x] TypeScript compiles successfully

### Phase 4: Cleanup (TODO)
1. [ ] Delete remaining dead code (old Layer 1 tests if any)
2. [ ] Clean up unused warning items in Rust
3. [ ] Delete old evals (`evals/sidecar/`, `evals/test_sidecar_db.py`)
4. [ ] Update documentation
5. [ ] Run full E2E testing

---

## Tauri Commands - Old vs New

### Commands to REMOVE (~50 commands)
```rust
// Layer 0 commands (LanceDB-based)
sidecar_get_session_events
sidecar_get_session_checkpoints
sidecar_search_events
sidecar_storage_stats
sidecar_create_indexes
sidecar_index_status

// Model management (no longer needed)
sidecar_models_status
sidecar_download_models

// Complex synthesis (replaced with simpler approach)
sidecar_query_history

// Layer 1 cross-session queries (LanceDB-based)
layer1_search_similar_decisions
layer1_get_decisions_by_category
layer1_get_unresolved_errors
layer1_search_similar_errors
layer1_list_sessions
layer1_search_goals
layer1_get_state_history

// Layer 1 state accessors (replaced with markdown read)
sidecar_get_session_state
sidecar_get_injectable_context
sidecar_get_goals
sidecar_get_file_contexts
sidecar_get_decisions
sidecar_get_errors
sidecar_get_open_questions
sidecar_answer_question
sidecar_complete_goal

// Export/import (different format now)
sidecar_export_session
sidecar_export_session_to_file
sidecar_import_session
sidecar_import_session_from_file
```

### Commands to KEEP/MODIFY
```rust
// Core lifecycle (simplified)
sidecar_status           // Simplified status
sidecar_initialize       // Initialize sessions directory
sidecar_start_session    // Create session directory + meta.toml
sidecar_end_session      // Mark session complete
sidecar_current_session  // Get current session ID

// Synthesis (simplified)
sidecar_generate_commit  // Read log.md, generate commit
sidecar_generate_summary // Read state.md

// Config
sidecar_get_config
sidecar_set_config
sidecar_shutdown
```

### Commands to ADD
```rust
// New markdown-based commands
sidecar_get_state_markdown     // Read state.md
sidecar_get_log_markdown       // Read log.md
sidecar_get_session_meta       // Read meta.toml
sidecar_list_sessions          // Scan ~/.qbit/sessions/
```

---

## Settings Changes

### Current `SidecarSettings`
```rust
pub struct SidecarSettings {
    pub enabled: bool,
    pub synthesis_enabled: bool,
    pub synthesis_backend: String,
    pub synthesis_vertex: SynthesisVertexSettings,
    pub synthesis_openai: SynthesisOpenAiSettings,
    pub synthesis_grok: SynthesisGrokSettings,
    pub retention_days: u32,
    pub capture_tool_calls: bool,
    pub capture_reasoning: bool,
}
```

### New `SidecarSettings` (Simplified)
```rust
pub struct SidecarSettings {
    pub enabled: bool,
    pub sessions_dir: Option<PathBuf>,  // Default: ~/.qbit/sessions
    pub retention_days: u32,
    pub state_max_size_kb: u32,         // Max size for state.md (context budget)
    pub keep_event_log: bool,           // Whether to write events.jsonl
}
```

---

## Estimated Line Count Reduction

| Category | Current | After | Reduction |
|----------|---------|-------|-----------|
| Rust sidecar | ~19,000 | ~1,000 | -18,000 |
| Frontend sidecar | ~800 | ~300 | -500 |
| Evals | ~1,000 | 0 | -1,000 |
| **Total** | **~20,800** | **~1,300** | **~19,500 lines** |

---

## Open Questions

1. **LLM Provider for state updates**: Use main agent's provider or separate config?
   - Current: Rule-based only (`use_llm_for_state: false`)
   - Future: Could enable LLM via `StateUpdateContext` in processor.rs
2. **State backup retention**: Keep last N backups or just one?
   - Current: Single backup (state.md.bak)
3. **Events.jsonl**: Keep for potential future semantic search, or skip entirely?
   - Current: Enabled by default (`write_raw_events: true`)
4. **Cross-session queries**: Do we need to search across sessions? (Probably not initially)
   - Current: Not implemented, would require re-adding vector storage

## Implementation Notes (Phase 2)

### Files Changed
- `src-tauri/src/sidecar/` - Fully rewritten (9 files, ~1500 lines total)
- `src-tauri/src/lib.rs` - 13 new sidecar commands registered
- `src-tauri/src/state.rs` - `SidecarState` in `AppState`
- `src-tauri/src/ai/agent_bridge.rs` - Session start/end integration
- `src-tauri/src/ai/agentic_loop.rs` - `CaptureContext` integration
- `src/lib/sidecar.ts` - Simplified from 566 to 196 lines
- `src/components/Sidecar/` - New directory with 3 files
- `src/components/Settings/AiSettings.tsx` - Removed synthesis backend API

### Test Status
- 59 sidecar tests passing
- 260 total tests passing (1 flaky unrelated test)
- TypeScript compiles with 1 unrelated error (ajv types)

---

## Appendix: Full File Line Counts

### Rust Backend (`src-tauri/src/sidecar/`)

```
  1088 sidecar/capture.rs
  1013 sidecar/commands.rs
   492 sidecar/config.rs
  1508 sidecar/events.rs
  1642 sidecar/integration_tests.rs
    55 sidecar/mod.rs
   670 sidecar/models.rs
   537 sidecar/processor.rs
   458 sidecar/prompts.rs
   408 sidecar/schema_verification_test.rs
   851 sidecar/state.rs
  1848 sidecar/storage.rs
   733 sidecar/synthesis.rs
   271 sidecar/synthesis_llm.rs
   368 sidecar/layer1/api.rs
    59 sidecar/layer1/events.rs
    47 sidecar/layer1/mod.rs
  1555 sidecar/layer1/processor.rs
   477 sidecar/layer1/prompt.rs
  1317 sidecar/layer1/state.rs
  2639 sidecar/layer1/storage.rs
   790 sidecar/layer1/verification_tests.rs
------
 18826 total
```

### Frontend (`src/`)

```
  ~500 lib/sidecar.ts
    75 hooks/useLayer1Events.ts
  ~400 components/Sidecar/* (4 files + index)
------
  ~975 total
```

### Evals (`evals/`)

```
  ~862 sidecar/db.py
  ~500 test_sidecar_db.py
------
 ~1362 total
```

### Grand Total: ~21,163 lines
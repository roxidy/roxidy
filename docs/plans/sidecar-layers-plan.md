# Sidecar Layers Implementation Plan

## Overview

The sidecar system uses a layered architecture for context capture and synthesis:

- **L1 (Event Capture)**: ✅ Complete — Markdown-based session storage (`state.md` with YAML frontmatter)
- **L2 (Staged Commits)**: ✅ Complete — Git format-patch based commit staging with LLM synthesis
- **L3 (Project Artifacts)**: ✅ Complete — Auto-maintained README.md/CLAUDE.md with LLM synthesis

**Remaining:** End-to-end testing of the full L2 → L3 cascade flow.

## Current Architecture (L1 + L2)

```
~/.qbit/sessions/{session_id}/
  state.md          # YAML frontmatter (metadata) + markdown body (context)
  patches/
    staged/         # Pending patches in git format-patch style
      0001-*.patch        # Standard git patch file
      0001-*.meta.toml    # Qbit metadata (timestamp, boundary reason)
    applied/        # Applied patches (moved after git am)
```

### state.md Format

```markdown
---
session_id: abc123
cwd: /Users/xlyk/Code/qbit
git_root: /Users/xlyk/Code/qbit
git_branch: main
initial_request: "Add authentication module"
created_at: 2025-12-10T14:30:00Z
status: active
---

# Session Context

## Current Goal
Implementing JWT-based authentication...

## Progress
- Created auth module structure
- Added token validation

## Files in Focus
- src/auth.rs
- src/lib.rs
```

Key components:
- `Session` — File I/O for session directory, YAML frontmatter parsing
- `Processor` — Async event processing via channel
- `SessionEvent` — Rich event types with semantic information
- `CommitBoundaryDetector` — Detects logical commit points
- `PatchManager` — Manages staged/applied patches in git format-patch style

---

## L2: Staged Commits (Git Format-Patch)

### Purpose

Automatically synthesize well-formed git commits from captured session activity using standard git format-patch files that can be applied with `git am`.

### File Format

Each patch is a standard git format-patch file in `patches/staged/`:

```patch
From 0000000000000000000000000000000000000000 Mon Sep 17 00:00:00 2001
From: Qbit Agent <agent@qbit.dev>
Date: Tue, 10 Dec 2025 14:30:00 +0000
Subject: [PATCH] feat(auth): add JWT authentication module

Implements token generation and validation with configurable expiry.
---
 src/auth.rs | 25 +++++++++++++++++++++++++
 src/lib.rs  |  1 +
 2 files changed, 26 insertions(+)
 create mode 100644 src/auth.rs

diff --git a/src/auth.rs b/src/auth.rs
new file mode 100644
...
--
2.39.0
```

Alongside each `.patch` file is a `.meta.toml` sidecar:

```toml
id = 1
created_at = "2025-12-10T14:30:00Z"
boundary_reason = "completion_signal"
```

### Benefits of Git Format-Patch

- **Standard format** — Native git format, well-documented
- **Self-contained** — Complete patch with author, date, message, and diff
- **Directly applicable** — Use `git am` to apply patches
- **Tooling support** — Works with existing git tools and workflows
- **Email-compatible** — Can be sent/received via email (git's original use case)

### Trigger Conditions

1. **CommitBoundaryDetector fires** — Already implemented, detects:
   - Completion signals in reasoning ("done", "complete", "finished")
   - User approval events
   - Session end
   - Activity pause (configurable threshold)

2. **User requests** — Explicit command to stage current changes

3. **Session end** — Auto-stage any remaining changes

### Processing Flow

```
Session events → CommitBoundaryDetector → boundary detected
                                               ↓
                               Collect changes since last boundary
                                               ↓
                               Generate commit message (rule-based or LLM)
                                               ↓
                               git format-patch → patches/staged/NNNN-*.patch
```

### Commands (Tauri)

| Command | Description | Status |
|---------|-------------|--------|
| `sidecar_get_staged_patches` | List all patches in `staged/` | ✅ |
| `sidecar_get_applied_patches` | List all patches in `applied/` | ✅ |
| `sidecar_get_patch(id)` | Read specific patch | ✅ |
| `sidecar_apply_patch(id)` | Execute `git am`, move to `applied/`, trigger L3 | ✅ |
| `sidecar_apply_all_patches` | Apply all staged patches in order, trigger L3 | ✅ |
| `sidecar_discard_patch(id)` | Delete patch files | ✅ |
| `sidecar_get_current_staged_patches` | Get staged patches for active session | ✅ |
| `sidecar_regenerate_patch(id)` | Regenerate patch message using LLM | ✅ |
| `sidecar_update_patch_message(id, msg)` | Manually update patch commit message | ✅ |

### Git Integration

When applying a patch:
1. Read patch from `staged/{id}-*.patch`
2. Run `git am --3way {patch_file}` in git root
3. Capture resulting commit SHA
4. Move `.patch` and `.meta.toml` to `applied/`
5. Update `.meta.toml` with `applied_sha`

---

## L3: Project Artifacts

### Purpose

Auto-maintain project documentation (README.md, CLAUDE.md) based on session activity. Proposes updates that users review and apply.

### File Format

Artifacts in `artifacts/pending/` mirror project files:

```
artifacts/
  pending/
    README.md      # Proposed update to project README
    CLAUDE.md      # Proposed update to project CLAUDE.md
  applied/
    README.md      # Previous versions after applying
    CLAUDE.md
```

Each file includes a metadata header:

```markdown
<!--
Target: /Users/xlyk/Code/qbit/README.md
Created: 2025-12-10 14:30
Reason: Added authentication feature
Based on patches: 0001, 0002
-->

# Qbit

AI-powered terminal emulator...

## Features

- **Authentication** ← NEW: Added JWT-based auth
...
```

### Trigger Conditions

1. **Patch applied** — L2 → L3 cascade
2. **Session end** — Generate proposals for any changes
3. **User request** — Explicit command to update artifacts

### Processing Flow

```
Patch applied (L2)
       ↓
Read current project README.md / CLAUDE.md
       ↓
Read state.md context
       ↓
LLM generates updated version
       ↓
Write to artifacts/pending/
```

### LLM Prompts

**README.md Update:**
```
You are updating a project README based on recent changes.

## Current README
{current_readme}

## Recent Changes
{patch_summaries}

## Session Context
{state_summary}

## Guidelines
- Preserve existing structure and tone
- Add/update sections for new features
- Keep it concise and user-focused
- Don't remove existing content unless outdated

Return the COMPLETE updated README.md.
```

**CLAUDE.md Update:**
```
You are updating a CLAUDE.md file (AI assistant instructions) based on recent changes.

## Current CLAUDE.md
{current_claude_md}

## Recent Changes
{patch_summaries}

## Guidelines
- Add new conventions discovered during implementation
- Update file structure if it changed
- Add new commands or workflows
- Keep instructions actionable and specific

Return the COMPLETE updated CLAUDE.md.
```

### Commands (Tauri)

| Command | Description | Status |
|---------|-------------|--------|
| `sidecar_get_pending_artifacts` | List artifacts in `pending/` | ✅ |
| `sidecar_get_applied_artifacts` | List artifacts in `applied/` | ✅ |
| `sidecar_get_artifact(name)` | Read specific artifact | ✅ |
| `sidecar_preview_artifact(name)` | Show diff against current project file | ✅ |
| `sidecar_apply_artifact(name)` | Copy to project, git stage, move to `applied/` | ✅ |
| `sidecar_apply_all_artifacts` | Apply all pending artifacts | ✅ |
| `sidecar_discard_artifact(name)` | Delete artifact file | ✅ |
| `sidecar_regenerate_artifacts` | Re-run LLM generation with backend override | ✅ |
| `sidecar_get_current_pending_artifacts` | Get pending artifacts for active session | ✅ |

### Git Integration

When applying an artifact:
1. Read artifact from `pending/{name}`
2. Copy to project path (from metadata header)
3. `git add {path}`
4. Move artifact to `applied/` with timestamp
5. Emit event for potential L2 patch

---

## Implementation Status

### Phase 1: L2 Foundation ✅
1. ✅ Simplified session structure (`state.md` with YAML frontmatter)
2. ✅ Created `PatchManager` for git format-patch style patches
3. ✅ Wire `CommitBoundaryDetector` events to patch generation
4. ✅ Implement rule-based commit message generation
5. ✅ Add Tauri commands: `sidecar_get_staged_patches`, `sidecar_discard_patch`, etc.

### Phase 2: L2 Git Integration ✅
1. ✅ Implement `sidecar_apply_patch` with `git am`
2. ✅ Add patch file movement (staged → applied)
3. ✅ Add SHA tracking in applied patches
4. ✅ Implement `sidecar_apply_all_patches`

### Phase 3: L2 LLM Integration ✅
1. ✅ Add LLM prompt for commit message synthesis (`synthesis.rs`)
2. ✅ Implement `sidecar_regenerate_patch` command
3. ✅ Implement `sidecar_update_patch_message` command (manual update)
4. ✅ Add configuration for LLM vs rule-based (`SynthesisBackend` enum)
5. ✅ Multiple LLM backends: Template, VertexAnthropic, OpenAI, Grok
6. ✅ Wire processor to auto-generate patches on boundary detection (`CaptureContext` in `capture.rs`)

### Phase 4: L3 Foundation ✅
1. ✅ Add `artifacts/` directory structure to `Session`
2. ✅ Create `ArtifactFile` struct with metadata header parsing
3. ✅ Create `ArtifactMeta` struct with HTML comment header format
4. ✅ Implement `ArtifactManager` for artifact lifecycle
5. ✅ Implement artifact generation (rule-based)
6. ✅ Add commands: `sidecar_get_pending_artifacts`, `sidecar_get_applied_artifacts`, `sidecar_get_artifact`, `sidecar_discard_artifact`

### Phase 5: L3 Git Integration ✅
1. ✅ Implement `sidecar_apply_artifact` with file copy + git add
2. ✅ Add artifact file movement (pending → applied)
3. ✅ Implement `sidecar_preview_artifact` (diff view)
4. ✅ Implement `sidecar_apply_all_artifacts`
5. ✅ Wire L2 → L3 cascade (patch applied triggers artifact regeneration via `regenerate_from_patches_with_config`)
6. ✅ Add `sidecar_get_current_pending_artifacts` for active session

### Phase 6: L3 LLM Integration ✅
1. ✅ Add LLM prompts for README.md synthesis (`README_SYSTEM_PROMPT`, `README_USER_PROMPT`)
2. ✅ Add LLM prompts for CLAUDE.md synthesis (`CLAUDE_MD_SYSTEM_PROMPT`, `CLAUDE_MD_USER_PROMPT`)
3. ✅ Implement `sidecar_regenerate_artifacts` command with backend override
4. ✅ Implement `synthesize_readme` and `synthesize_claude_md` async functions
5. ✅ Multiple LLM backends: Template, VertexAnthropic, OpenAI, Grok
6. ✅ Automatic fallback to template when LLM fails
7. ✅ `ArtifactSynthesisConfig` with settings integration

### Phase 7: Integration & Polish ✅ (Mostly Complete)
1. ✅ Frontend UI for viewing/managing staged patches (`SidecarPanel.tsx`)
2. ✅ Frontend UI for viewing/applying pending artifacts (`SidecarPanel.tsx`)
3. ✅ TypeScript types and API wrappers (`src/lib/sidecar.ts`)
4. ✅ Sidecar event hook (`useSidecarEvents.ts`) with type guards
5. ✅ `SidecarEvent` enum with emission in commands (backend)
6. ✅ Real-time UI updates via event subscription
7. ✅ Merge conflicts resolved
8. ⬜ End-to-end testing of L2 → L3 cascade flow

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Session metadata | YAML frontmatter in state.md | Single file, standard format, human-readable |
| Patch storage | Git format-patch | Standard format, directly applicable with `git am` |
| Patch metadata | TOML sidecar files | Keeps patches pristine, Qbit-specific info separate |
| Cross-session | Clean slate | Simpler mental model, avoids stale state |
| Artifact application | Git-aware (stage changes) | Integrates with existing workflow |
| Artifact types | README.md, CLAUDE.md | Start simple, make configurable later |

---

## Future Considerations

- **Configurable artifact types** — Let users specify which files to auto-maintain
- **Cross-session queries** — "What did I change in auth last week?"
- **Patch squashing** — Combine multiple staged patches before applying
- **Artifact templates** — Custom templates for different project types
- **Conflict resolution** — Handle cases where project file changed since proposal
- **Interactive patch editing** — Allow users to edit patch content before applying

# Documentation Gaps and Implementation TODOs

Last updated: 2025-11-27

## Current Implementation Status

### POC v0.1 - COMPLETE ✓

| Feature | Status | Notes |
|---------|--------|-------|
| PTY spawn + basic I/O | ✅ Complete | Multi-session support |
| Shell integration script | ✅ Complete | Auto-install to ~/.config/roxidy/ |
| OSC 133 parsing | ✅ Complete | With command capture via C marker |
| Command block UI | ✅ Complete | Collapsible, ANSI rendering |
| Multi-session tabs | ✅ Complete | Cmd+T new tab |
| CommandInput | ✅ Complete | History, Ctrl+C/D/L, tab completion |
| Working directory tracking | ✅ Complete | OSC 7 parsing |

### Current Limitations

| Issue | Status | Notes |
|-------|--------|-------|
| Interactive commands | ❌ Blocked | vim, htop, ssh show toast error |
| xterm.js terminal | ⚠️ Unused | Terminal component exists but not integrated |
| SQLite persistence | ❌ Not Started | Command history not persisted |
| AI integration | ❌ Not Started | Panel designed, not implemented |
| Settings dialog | ❌ Not Started | Theme hardcoded |

---

## Design Decisions Made

### 1. Command Text Capture - IMPLEMENTED ✓

Extended OSC 133 to emit command text with C marker:

```zsh
__roxidy_cmd_start() {
    local cmd="$1"
    if [[ -n "$cmd" ]]; then
        __roxidy_osc "C;$cmd"
    else
        __roxidy_osc "C"
    fi
}
```

### 2. Terminal Rendering Strategy - SIMPLIFIED

**Original plan:** xterm.js as base with React block overlays.

**Current implementation:** Custom `CommandInput` component with React-rendered blocks. xterm.js Terminal component exists but is unused.

**Reason:** Simpler to implement for non-interactive commands. Interactive command support deferred.

### 3. Interactive Commands - BLOCKED

**Decision:** Block interactive commands (vim, htop, ssh, etc.) with toast error for now.

**Blocked commands:**
- vim, vi, nvim, nano, emacs, pico
- less, more, man
- htop, top, btop
- ssh, telnet, ftp, sftp
- python, python3, node, irb, ruby, ghci
- mysql, psql, sqlite3, redis-cli, mongo
- tmux, screen, watch

**Future:** Implement proper terminal mode switching with xterm.js.

---

## Previously Resolved Issues

### Command capture ✓
Extended OSC 133;C with command payload - implemented in shell integration.

### Shell Integration Exit Code ✓
`__roxidy_precmd` captures `$?` before any commands run.

### VTE Parser Tests ✓
Unit tests for OSC 133 and OSC 7 parsing in `src-tauri/src/pty/parser.rs`.

---

## Next Steps (POC v0.2)

### Priority 1: Interactive Command Support

To support vim, htop, etc.:

1. Detect alternate screen buffer (DECSET 1049)
2. Switch UI to xterm.js Terminal component
3. Return to block view on alternate screen exit
4. Handle React StrictMode double-mount issues (fix exists in Terminal.tsx)

### Priority 2: Persistence

1. Add SQLite for command history
2. Persist sessions across restarts
3. Settings storage

### Priority 3: AI Integration

1. Implement AI panel UI
2. Add rig agent with tools
3. Command suggestion flow

---

## Files Updated

| File | Last Updated | Status |
|------|--------------|--------|
| `README.md` | 2025-11-27 | ✅ Updated with current features |
| `architecture.md` | 2025-11-27 | ✅ Updated with implementation status |
| `frontend-state.md` | 2025-11-27 | ✅ Updated with actual store |
| `gaps-and-todos.md` | 2025-11-27 | ✅ This file |
| `shell-integration.md` | 2025-11-26 | ✅ Complete |
| `security.md` | 2025-11-26 | ✅ Complete |
| `ai-integration.md` | 2025-11-26 | ⚠️ Design only, not implemented |
| `decisions.md` | 2025-11-26 | ✅ Complete |
| `tauri-ipc.md` | 2025-11-26 | ✅ Complete |

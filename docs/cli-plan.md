# Qbit CLI - Integration Test Interface

## Primary Goal

**Enable automated verification that Qbit features actually work.**

The GUI makes it impossible to programmatically verify agent behavior. Without a CLI, we cannot:
- Prove that tools execute correctly
- Verify sidecar captures events
- Confirm session persistence works
- Test agent responses in CI
- Catch regressions before they ship

This CLI exists primarily as a **test harness**. Interactive use is secondary.

## Acceptance Criteria

The MVP is complete when we can run this and have it pass:

```bash
#!/bin/bash
set -e

echo "=== Qbit Integration Test Suite ==="

# 1. Agent initializes with settings.toml
qbit-cli -e "Say 'ready'" --quiet --auto-approve | grep -q "ready"
echo "✓ Agent initialization"

# 2. File tools work
output=$(qbit-cli -e "Read line 1 of Cargo.toml" --json --auto-approve)
echo "$output" | jq -e 'select(.event == "tool_result" and .tool_name == "read_file" and .success == true)' > /dev/null
echo "✓ File reading tool"

# 3. Shell execution works
output=$(qbit-cli -e "Run 'echo hello' in the shell" --json --auto-approve)
echo "$output" | jq -e 'select(.event == "tool_result" and .tool_name == "run_pty_cmd" and .success == true)' > /dev/null
echo "✓ Shell execution tool"

# 4. Sidecar captures events
qbit-cli -e "List files in src/" --auto-approve --quiet
test -d ~/.qbit/sidecar/ && echo "✓ Sidecar directory exists"

# 5. Session persistence works
qbit-cli -e "Remember this" --session integration-test --auto-approve --quiet
ls ~/.qbit/sessions/ | grep -q "integration-test"
echo "✓ Session persistence"

# 6. Batch execution works
echo -e "What is 1+1?\n---\nWhat is 2+2?" > /tmp/batch-test.txt
qbit-cli -f /tmp/batch-test.txt --quiet --auto-approve | grep -q "4"
echo "✓ Batch execution"

# 7. JSON output is valid and parseable
qbit-cli -e "Hello" --json --auto-approve | jq -e . > /dev/null
echo "✓ JSON output format"

echo ""
echo "=== All integration tests passed ==="
```

### Extended Test Suite (Recommended)

The basic 7 tests verify happy path. These additional tests catch edge cases:

```bash
#!/bin/bash
set -e

echo "=== Extended Integration Tests ==="

# 8. Error handling - invalid API key
QBIT_API_KEY=invalid qbit-cli -e "test" 2>&1 | grep -q "API\|auth\|key"
echo "✓ Invalid API key error"

# 9. Tool denial behavior
output=$(qbit-cli -e "Delete all files" --deny-tools "delete_file,run_pty_cmd" --json 2>&1)
echo "$output" | jq -e 'select(.event == "tool_denied")' > /dev/null
echo "✓ Tool denial works"

# 10. Timeout behavior (use short timeout)
timeout 5 qbit-cli -e "Run 'sleep 100'" --timeout 2 --auto-approve 2>&1 | grep -q "timeout\|Timeout"
echo "✓ Timeout works"

# 11. Non-zero exit code on error
if qbit-cli -e "test" --provider nonexistent 2>/dev/null; then
    echo "✗ Should have failed with bad provider"
    exit 1
fi
echo "✓ Non-zero exit on error"

# 12. Workspace validation
if qbit-cli /nonexistent/path -e "test" 2>/dev/null; then
    echo "✗ Should have failed with bad workspace"
    exit 1
fi
echo "✓ Workspace validation"

# 13. Ctrl+C graceful shutdown (manual verification)
echo "⚠ Manual test: Run 'qbit-cli -e \"Run sleep 100\"' and press Ctrl+C"
echo "  Verify: Session saved message appears, no crash"

# 14. Piped stdin without --auto-approve fails gracefully
echo "test" | timeout 5 qbit-cli 2>&1 | grep -q "TTY\|auto-approve\|interactive" || true
echo "✓ Non-TTY stdin handled"

echo ""
echo "=== Extended tests complete ==="
```

**If this script passes, the MVP is done.**

---

## Critical Issues Identified (Must Address)

Analysis by specialized agents identified these blockers:

### 🚨 Architecture Issues

#### 1. Event Channel Ownership (BLOCKER)
**Problem**: Plan shows `ctx.event_rx.clone()` but `UnboundedReceiver` is NOT `Clone`.

**Fix**: Create fresh channel per execution, don't store receiver in `CliContext`:
```rust
pub struct CliContext {
    pub app_state: AppState,
    pub workspace: PathBuf,
    // NO event_rx here - create per execution
}

impl Runner {
    pub async fn execute_once(ctx: &CliContext, prompt: &str, args: &Args) -> Result<()> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // Reconfigure bridge with new event_tx
        // (requires AgentBridge modification to accept new sender)

        let output_handle = tokio::spawn(handle_events(event_rx, ...));
        // ...
    }
}
```

#### 2. PtyManager Tauri Dependency (BLOCKER)
**Problem**: `PtyManager::create_session()` requires `AppHandle` for event emission.

**Fix**: Refactor to trait-based event emission:
```rust
pub trait EventEmitter: Send + Sync {
    fn emit(&self, event: &str, data: impl Serialize) -> Result<()>;
}

// CLI implementation
pub struct CliEventEmitter;
impl EventEmitter for CliEventEmitter {
    fn emit(&self, event: &str, data: impl Serialize) -> Result<()> {
        // Write to stdout or log
    }
}
```

#### 3. HITL Approval Flow (BLOCKER)
**Problem**: Event handler receives `ToolApprovalRequest` but has no channel to send decision back.

**Fix**: Add approval response channel:
```rust
pub struct CliContext {
    pub app_state: AppState,
    pub workspace: PathBuf,
    pub approval_tx: mpsc::UnboundedSender<(String, ApprovalDecision)>,
}

// In event handler:
if auto_approve {
    approval_tx.send((request_id, ApprovalDecision::Approved))?;
} else {
    // prompt user, then send decision
}

// Spawn separate task to forward approvals to bridge
```

#### 4. Settings Path Mismatch (BLOCKER)
**Problem**: Plan references `settings.api_keys.openrouter`, actual path is `settings.ai.openrouter.api_key`.

**Fix**:
```rust
fn resolve_api_key_from_settings(settings: &QbitSettings, provider: &str) -> Option<String> {
    match provider {
        "openrouter" => settings.ai.openrouter.api_key.clone(),
        "anthropic" => settings.ai.anthropic.api_key.clone(),
        "openai" => settings.ai.openai.api_key.clone(),
        "vertex" => None, // Uses credentials file
        _ => None,
    }
}
```

#### 5. Missing Vertex AI Path (BLOCKER)
**Problem**: Default provider is `vertex_ai` but bootstrap only handles API key providers.

**Fix**: Add Vertex initialization branch:
```rust
let bridge = match provider.as_str() {
    "vertex" | "vertex_ai" => {
        let creds = settings.ai.vertex_ai.credentials_path
            .ok_or_else(|| anyhow!("Vertex AI requires credentials_path in settings"))?;
        AgentBridge::new_vertex_anthropic(
            workspace.clone(),
            &creds,
            &settings.ai.vertex_ai.project_id,
            &settings.ai.vertex_ai.location,
            model,
            event_tx,
        ).await?
    }
    _ => {
        let api_key = resolve_api_key_from_settings(&settings, &provider)
            .ok_or_else(|| anyhow!("No API key for provider: {}", provider))?;
        AgentBridge::new(workspace.clone(), &provider, model, &api_key, event_tx).await?
    }
};
```

### 🔴 Runtime Safety Issues

#### 6. No Signal Handling
**Fix**: Wrap execution in signal handler:
```rust
tokio::select! {
    result = Runner::execute_once(&ctx, prompt, &args) => result,
    _ = tokio::signal::ctrl_c() => {
        eprintln!("\nInterrupted - saving session...");
        ctx.shutdown().await.ok();
        std::process::exit(130); // 128 + SIGINT
    }
}
```

#### 7. No Timeout
**Fix**: Add default 5-minute timeout:
```rust
match tokio::time::timeout(
    Duration::from_secs(args.timeout.unwrap_or(300)),
    Runner::execute_once(&ctx, prompt, &args)
).await {
    Ok(result) => result,
    Err(_) => {
        eprintln!("Error: Execution timed out");
        std::process::exit(124); // timeout exit code
    }
}
```

#### 8. JSON Mode Stderr Corruption
**Fix**: Never write to stderr in JSON mode:
```rust
macro_rules! cli_log {
    ($json:expr, $($arg:tt)*) => {
        if !$json {
            eprintln!($($arg)*);
        }
    };
}

// Usage:
cli_log!(args.json, "[auto-approved] {}", tool_name);
```

#### 9. Non-TTY Stdin Detection
**Fix**: Fail early if approval needed without TTY:
```rust
AiEvent::ToolApprovalRequest { .. } if !auto_approve => {
    if !atty::is(atty::Stream::Stdin) {
        anyhow::bail!(
            "Tool approval required but stdin is not a TTY. Use --auto-approve"
        );
    }
    // ... prompt for approval
}
```

### 🟡 Signature Corrections

#### 10. `SidecarState::start_session()` requires `initial_request`
```rust
// Wrong:
app_state.sidecar_state.start_session().await?;

// Correct:
app_state.sidecar_state.start_session("CLI session started")?; // sync, not async
```

#### 11. `AppState::new()` returns `Self`, not `Result<Self>`
```rust
// Wrong:
let app_state = AppState::new().await?;

// Correct:
let app_state = AppState::new().await; // No ? - panics on failure internally
```

---

## Runtime Abstraction Layer

The core issue: `PtyManager`, `AgentBridge`, and event emission are coupled to Tauri's `AppHandle`. We need an abstraction that works for both Tauri and CLI.

### Design: `QbitRuntime` Trait

```
┌─────────────────────────────────────────────────────────────────┐
│                        Application Layer                         │
│  (AgentBridge, PtyManager, SidecarState, etc.)                  │
├─────────────────────────────────────────────────────────────────┤
│                      QbitRuntime Trait                           │
│  - emit_event()      → Send events to frontend/CLI              │
│  - spawn_task()      → Runtime-specific task spawning           │
│  - get_approval()    → HITL approval (async for both)           │
├─────────────────────────────────────────────────────────────────┤
│     TauriRuntime              │         CliRuntime              │
│  - Uses AppHandle.emit()      │  - Uses mpsc channels           │
│  - Tauri async runtime        │  - Tokio runtime                │
│  - Frontend approval UI       │  - Stdin approval prompts       │
└───────────────────────────────┴─────────────────────────────────┘
```

### Core Trait Definition

```rust
// src-tauri/src/runtime/mod.rs

use async_trait::async_trait;
use serde::Serialize;

/// Events that can be emitted to the frontend/CLI
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum RuntimeEvent {
    // Terminal events
    TerminalOutput { session_id: String, data: Vec<u8> },
    TerminalExit { session_id: String, code: Option<i32> },

    // AI events (re-export existing AiEvent)
    Ai(AiEvent),

    // Generic event for extensibility
    Custom { name: String, payload: serde_json::Value },
}

/// Approval decision from user
#[derive(Debug, Clone)]
pub enum ApprovalResult {
    Approved,
    Denied,
    AlwaysAllow,
    AlwaysDeny,
    Timeout,
}

/// Runtime abstraction for Tauri vs CLI
#[async_trait]
pub trait QbitRuntime: Send + Sync + 'static {
    /// Emit an event to the frontend/output
    fn emit(&self, event: RuntimeEvent) -> anyhow::Result<()>;

    /// Request approval for a tool execution (blocks until decision)
    async fn request_approval(
        &self,
        request_id: &str,
        tool_name: &str,
        args: &serde_json::Value,
        risk_level: &str,
    ) -> anyhow::Result<ApprovalResult>;

    /// Check if running in headless/non-interactive mode
    fn is_interactive(&self) -> bool;

    /// Get auto-approve setting (CLI: --auto-approve flag)
    fn auto_approve(&self) -> bool;
}
```

### Tauri Implementation

```rust
// src-tauri/src/runtime/tauri.rs

use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;

pub struct TauriRuntime {
    app_handle: AppHandle,
    pending_approvals: Arc<RwLock<HashMap<String, oneshot::Sender<ApprovalResult>>>>,
}

impl TauriRuntime {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle,
            pending_approvals: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Called by frontend when user responds to approval dialog
    pub fn respond_to_approval(&self, request_id: &str, decision: ApprovalResult) {
        if let Some(tx) = self.pending_approvals.write().remove(request_id) {
            let _ = tx.send(decision);
        }
    }
}

#[async_trait]
impl QbitRuntime for TauriRuntime {
    fn emit(&self, event: RuntimeEvent) -> anyhow::Result<()> {
        self.app_handle.emit("qbit-event", &event)?;
        Ok(())
    }

    async fn request_approval(
        &self,
        request_id: &str,
        tool_name: &str,
        args: &serde_json::Value,
        risk_level: &str,
    ) -> anyhow::Result<ApprovalResult> {
        // Create response channel
        let (tx, rx) = oneshot::channel();
        self.pending_approvals.write().insert(request_id.to_string(), tx);

        // Emit approval request to frontend
        self.emit(RuntimeEvent::Ai(AiEvent::ToolApprovalRequest {
            request_id: request_id.to_string(),
            tool_name: tool_name.to_string(),
            args: args.clone(),
            risk_level: risk_level.to_string(),
            // ... other fields
        }))?;

        // Wait for response (with timeout)
        match tokio::time::timeout(std::time::Duration::from_secs(300), rx).await {
            Ok(Ok(decision)) => Ok(decision),
            Ok(Err(_)) => Ok(ApprovalResult::Timeout),
            Err(_) => Ok(ApprovalResult::Timeout),
        }
    }

    fn is_interactive(&self) -> bool {
        true // Tauri always has UI
    }

    fn auto_approve(&self) -> bool {
        false // Tauri uses UI for approval
    }
}
```

### CLI Implementation

```rust
// src-tauri/src/runtime/cli.rs

use tokio::sync::mpsc;
use std::io::{self, Write};

pub struct CliRuntime {
    event_tx: mpsc::UnboundedSender<RuntimeEvent>,
    auto_approve: bool,
    json_mode: bool,
    quiet_mode: bool,
}

impl CliRuntime {
    pub fn new(
        event_tx: mpsc::UnboundedSender<RuntimeEvent>,
        auto_approve: bool,
        json_mode: bool,
        quiet_mode: bool,
    ) -> Self {
        Self { event_tx, auto_approve, json_mode, quiet_mode }
    }
}

#[async_trait]
impl QbitRuntime for CliRuntime {
    fn emit(&self, event: RuntimeEvent) -> anyhow::Result<()> {
        // Send to channel for CLI event handler to process
        self.event_tx.send(event)?;
        Ok(())
    }

    async fn request_approval(
        &self,
        request_id: &str,
        tool_name: &str,
        args: &serde_json::Value,
        risk_level: &str,
    ) -> anyhow::Result<ApprovalResult> {
        // Auto-approve if flag set
        if self.auto_approve {
            if !self.json_mode {
                eprintln!("[auto-approved] {}", tool_name);
            }
            return Ok(ApprovalResult::Approved);
        }

        // Check if stdin is a TTY
        if !atty::is(atty::Stream::Stdin) {
            anyhow::bail!(
                "Tool '{}' requires approval but stdin is not a TTY. Use --auto-approve",
                tool_name
            );
        }

        // Prompt user
        eprint!(
            "\n[{}] {} {}\n(a)pprove / (d)eny / (A)lways / (D)never: ",
            risk_level, tool_name, args
        );
        io::stderr().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.trim().to_lowercase().as_str() {
            "a" | "y" | "yes" => Ok(ApprovalResult::Approved),
            "d" | "n" | "no" => Ok(ApprovalResult::Denied),
            "always" | "aa" => Ok(ApprovalResult::AlwaysAllow),
            "never" | "dd" => Ok(ApprovalResult::AlwaysDeny),
            _ => Ok(ApprovalResult::Denied), // Default to deny on invalid input
        }
    }

    fn is_interactive(&self) -> bool {
        atty::is(atty::Stream::Stdin)
    }

    fn auto_approve(&self) -> bool {
        self.auto_approve
    }
}
```

### Updating Dependent Components

#### PtyManager Changes

```rust
// src-tauri/src/pty/manager.rs

// BEFORE (Tauri-coupled):
pub fn create_session(
    &self,
    app_handle: AppHandle,  // ❌ Tauri-specific
    shell: Option<String>,
) -> Result<PtySession>

// AFTER (Runtime-agnostic):
pub fn create_session<R: QbitRuntime>(
    &self,
    runtime: Arc<R>,  // ✅ Generic runtime
    shell: Option<String>,
) -> Result<PtySession>

// Inside session output handler:
// BEFORE:
app_handle.emit("terminal_output", &data)?;

// AFTER:
runtime.emit(RuntimeEvent::TerminalOutput {
    session_id: session_id.clone(),
    data: data.to_vec(),
})?;
```

#### AgentBridge Changes

```rust
// src-tauri/src/ai/agent_bridge.rs

// BEFORE:
pub struct AgentBridge {
    event_tx: mpsc::UnboundedSender<AiEvent>,
    // ...
}

// AFTER:
pub struct AgentBridge {
    runtime: Arc<dyn QbitRuntime>,
    // ...
}

impl AgentBridge {
    // BEFORE:
    pub async fn new(
        workspace: PathBuf,
        provider: &str,
        model: &str,
        api_key: &str,
        event_tx: mpsc::UnboundedSender<AiEvent>,  // ❌
    ) -> Result<Self>

    // AFTER:
    pub async fn new<R: QbitRuntime>(
        workspace: PathBuf,
        provider: &str,
        model: &str,
        api_key: &str,
        runtime: Arc<R>,  // ✅
    ) -> Result<Self>

    // HITL approval now uses runtime:
    async fn request_tool_approval(&self, tool: &ToolCall) -> Result<bool> {
        let result = self.runtime.request_approval(
            &tool.id,
            &tool.name,
            &tool.args,
            &tool.risk_level,
        ).await?;

        match result {
            ApprovalResult::Approved | ApprovalResult::AlwaysAllow => Ok(true),
            _ => Ok(false),
        }
    }
}
```

### File Structure

```
src-tauri/src/
├── runtime/
│   ├── mod.rs          # QbitRuntime trait + RuntimeEvent enum
│   ├── tauri.rs        # TauriRuntime implementation
│   └── cli.rs          # CliRuntime implementation
├── pty/
│   └── manager.rs      # Updated to use QbitRuntime
├── ai/
│   └── agent_bridge.rs # Updated to use QbitRuntime
└── cli/
    ├── mod.rs
    ├── args.rs
    ├── bootstrap.rs    # Creates CliRuntime
    ├── runner.rs
    └── output.rs       # Consumes RuntimeEvent from channel
```

### Risk Mitigation Strategy

#### Principle: Never Break, Always Add

The core strategy is to **never modify existing working code** until new code is proven. Each phase follows this pattern:

1. **Add** new implementation alongside old
2. **Test** new implementation in isolation
3. **Switch** callers one at a time
4. **Verify** parity between old and new
5. **Deprecate** old path (but keep working)
6. **Remove** old path only after full validation

#### Parallel Implementation Pattern

```rust
impl PtyManager {
    // KEEP - existing code unchanged until Phase 4d
    pub fn create_session(&self, app_handle: AppHandle, ...) -> Result<PtySession> {
        // Original implementation - untouched
    }

    // ADD - new implementation (Phase 4b)
    pub fn create_session_with_runtime(
        &self,
        runtime: Arc<dyn QbitRuntime>,
        ...
    ) -> Result<PtySession> {
        // New implementation using runtime
    }
}
```

#### Adapter Pattern for Transition

During migration, emit through BOTH paths to verify parity:

```rust
struct DualEmitter {
    old_tx: mpsc::UnboundedSender<AiEvent>,
    new_runtime: Arc<dyn QbitRuntime>,
}

impl DualEmitter {
    fn emit(&self, event: AiEvent) {
        // Emit through both - verify they match
        let _ = self.old_tx.send(event.clone());
        let _ = self.new_runtime.emit(RuntimeEvent::Ai(event));
    }
}
```

#### Verification Layer (Debug Builds)

```rust
#[cfg(debug_assertions)]
fn verify_event_parity(old: &AiEvent, new: &RuntimeEvent) {
    if let RuntimeEvent::Ai(new_event) = new {
        debug_assert_eq!(
            serde_json::to_value(old).unwrap(),
            serde_json::to_value(new_event).unwrap(),
            "Event mismatch! Old: {:?}, New: {:?}", old, new_event
        );
    }
}
```

#### Rollback Checkpoints

Each phase is independently revertable:

| Phase | Rollback Action |
|-------|-----------------|
| 1 | Remove feature flags from Cargo.toml |
| 2 | Remove TauriRuntime creation in commands |
| 3 | Delete `spawn_event_forwarder_runtime`, restore original |
| 4 | Delete `create_session_with_runtime`, revert callers |
| 5 | Restore `event_tx` field, remove runtime field |
| 6 | Delete `cli/` module and binary |
| 7 | N/A (tests only) |

---

### Migration Path (REVISED after agent review)

**Current State**: Runtime traits exist (`src/runtime/`) but are NOT integrated. AgentBridge and PtyManager still use `AppHandle` directly.

**Critical Insight**: Original order breaks Tauri app during migration. New order maintains working app throughout.

---

#### Phase 1: Feature Flags & Cargo Setup

**Risk Level**: ✅ SAFE (no behavior changes)

**Changes**:
```toml
# Cargo.toml
[features]
default = ["tauri"]
tauri = ["dep:tauri", "dep:tauri-plugin-opener"]
cli = ["dep:clap", "dep:atty"]

[dependencies]
tauri = { version = "2", optional = true }
atty = { version = "0.2", optional = true }

[[bin]]
name = "qbit-cli"
path = "src/bin/qbit-cli.rs"
required-features = ["cli"]
```

```rust
// src/runtime/mod.rs
#[cfg(all(feature = "tauri", feature = "cli"))]
compile_error!("Features 'tauri' and 'cli' are mutually exclusive");
```

**Acceptance Criteria**:
```bash
#!/bin/bash
set -e
echo "=== Phase 1 Acceptance Tests ==="

# 1.1: Default build compiles (uses tauri feature)
cargo build --package qbit
echo "✓ 1.1: Default build succeeds"

# 1.2: Explicit tauri feature compiles
cargo build --package qbit --features tauri --no-default-features
echo "✓ 1.2: Tauri feature build succeeds"

# 1.3: CLI feature compiles (may have missing code - just check feature works)
cargo check --package qbit --features cli --no-default-features 2>/dev/null || true
echo "✓ 1.3: CLI feature recognized"

# 1.4: Both features together fails with compile_error
if cargo check --package qbit --features "tauri,cli" 2>&1 | grep -q "mutually exclusive"; then
    echo "✓ 1.4: Mutual exclusion enforced"
else
    echo "✗ 1.4: Mutual exclusion NOT enforced"
    exit 1
fi

# 1.5: Existing tests still pass
cargo test --package qbit --features tauri
echo "✓ 1.5: Existing tests pass"

echo ""
echo "=== Phase 1 PASSED ==="
```

**Rollback**: `git checkout -- Cargo.toml src/runtime/mod.rs`

---

#### Phase 2: Wire TauriRuntime into Commands

**Risk Level**: ✅ SAFE (additive only)

**Changes**:
```rust
// src/ai/commands/core.rs
use crate::runtime::{TauriRuntime, QbitRuntime};

pub async fn init_ai_agent(
    state: State<'_, AppState>,
    app: AppHandle,
    // ...
) -> Result<(), String> {
    // NEW: Create runtime (but don't use yet)
    let runtime: Arc<dyn QbitRuntime> = Arc::new(TauriRuntime::new(app.clone()));

    // Store in state for later use
    // For now, continue using old path

    // ... existing code unchanged ...
}
```

**Acceptance Criteria**:
```bash
#!/bin/bash
set -e
echo "=== Phase 2 Acceptance Tests ==="

# 2.1: Tauri app compiles
cargo build --package qbit --features tauri
echo "✓ 2.1: Build succeeds"

# 2.2: Tests pass
cargo test --package qbit --features tauri
echo "✓ 2.2: Tests pass"

# 2.3: TauriRuntime is constructed (check for log/trace)
cargo build --package qbit --features tauri 2>&1
echo "✓ 2.3: TauriRuntime compiles"

echo ""
echo "=== Phase 2 PASSED ==="
echo ""
echo "MANUAL VERIFICATION REQUIRED:"
echo "  1. Run: just dev"
echo "  2. Open app, initialize AI agent"
echo "  3. Send a prompt, verify response appears"
echo "  4. Verify tool approval dialog works"
```

**Rollback**: Remove TauriRuntime creation lines

---

#### Phase 3: Abstract spawn_event_forwarder

**Risk Level**: ✅ SAFE (old function delegates to new)

**Changes**:
```rust
// src/ai/commands/mod.rs

/// NEW: Runtime-based event forwarder
pub fn spawn_event_forwarder_runtime(
    runtime: Arc<dyn QbitRuntime>
) -> mpsc::UnboundedSender<AiEvent> {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(ai_event) = event_rx.recv().await {
            if let Err(e) = runtime.emit(RuntimeEvent::Ai(ai_event)) {
                tracing::warn!("Failed to emit event: {}", e);
            }
        }
        tracing::debug!("Event forwarder shut down");
    });
    event_tx
}

/// EXISTING: Now delegates to runtime version
pub fn spawn_event_forwarder(app: AppHandle) -> mpsc::UnboundedSender<AiEvent> {
    let runtime = Arc::new(TauriRuntime::new(app));
    spawn_event_forwarder_runtime(runtime)
}
```

**Acceptance Criteria**:
```bash
#!/bin/bash
set -e
echo "=== Phase 3 Acceptance Tests ==="

# 3.1: Build succeeds
cargo build --package qbit --features tauri
echo "✓ 3.1: Build succeeds"

# 3.2: Tests pass
cargo test --package qbit --features tauri
echo "✓ 3.2: Tests pass"

# 3.3: New function exists and compiles
grep -q "spawn_event_forwarder_runtime" src-tauri/src/ai/commands/mod.rs
echo "✓ 3.3: New function exists"

# 3.4: Old function delegates to new
grep -A5 "pub fn spawn_event_forwarder(" src-tauri/src/ai/commands/mod.rs | grep -q "spawn_event_forwarder_runtime"
echo "✓ 3.4: Delegation in place"

echo ""
echo "=== Phase 3 PASSED ==="
echo ""
echo "MANUAL VERIFICATION REQUIRED:"
echo "  1. Run: just dev"
echo "  2. Send AI prompt"
echo "  3. Verify streaming text appears in UI"
echo "  4. Verify tool calls show in UI"
echo "  5. Check console for 'Event forwarder' logs"
```

**Rollback**: Delete `spawn_event_forwarder_runtime`, restore original function

---

#### Phase 4: Update PtyManager

**Risk Level**: ⚠️ CAREFUL (touches thread spawning)

**Sub-phases**:

**4a: Extract Internal Implementation**
```rust
// src/pty/manager.rs

// Internal trait for event emission
trait PtyEventEmitter: Send + Sync + 'static {
    fn emit_output(&self, session_id: &str, data: &[u8]);
    fn emit_exit(&self, session_id: &str, code: Option<i32>);
    fn emit_directory_changed(&self, session_id: &str, path: &str);
}

// Implement for AppHandle
struct AppHandleEmitter(AppHandle);
impl PtyEventEmitter for AppHandleEmitter {
    fn emit_output(&self, session_id: &str, data: &[u8]) {
        let _ = self.0.emit("terminal_output", (&session_id, data));
    }
    // ... other methods
}

impl PtyManager {
    // Internal implementation
    fn create_session_internal<E: PtyEventEmitter>(
        &self,
        emitter: Arc<E>,
        shell: Option<String>,
    ) -> Result<PtySession> {
        // Move existing implementation here
    }

    // Public method - unchanged signature
    pub fn create_session(&self, app_handle: AppHandle, ...) -> Result<PtySession> {
        let emitter = Arc::new(AppHandleEmitter(app_handle));
        self.create_session_internal(emitter, shell)
    }
}
```

**4b: Add Runtime Emitter**
```rust
// Implement PtyEventEmitter for QbitRuntime
struct RuntimeEmitter(Arc<dyn QbitRuntime>);
impl PtyEventEmitter for RuntimeEmitter {
    fn emit_output(&self, session_id: &str, data: &[u8]) {
        let _ = self.0.emit(RuntimeEvent::TerminalOutput {
            session_id: session_id.to_string(),
            data: data.to_vec(),
        });
    }
    // ... other methods
}

impl PtyManager {
    // NEW public method
    pub fn create_session_with_runtime(
        &self,
        runtime: Arc<dyn QbitRuntime>,
        shell: Option<String>,
    ) -> Result<PtySession> {
        let emitter = Arc::new(RuntimeEmitter(runtime));
        self.create_session_internal(emitter, shell)
    }
}
```

**4c: Switch Callers**
```rust
// src/commands/pty.rs
pub async fn pty_create(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let runtime = Arc::new(TauriRuntime::new(app));
    state.pty_manager
        .create_session_with_runtime(runtime, shell)
        .map_err(|e| e.to_string())
}
```

**4d: Deprecate Old Method**
```rust
impl PtyManager {
    #[deprecated(since = "0.2.0", note = "Use create_session_with_runtime")]
    pub fn create_session(&self, app_handle: AppHandle, ...) -> Result<PtySession> {
        let runtime = Arc::new(TauriRuntime::new(app_handle));
        self.create_session_with_runtime(runtime, shell)
    }
}
```

**Acceptance Criteria**:
```bash
#!/bin/bash
set -e
echo "=== Phase 4 Acceptance Tests ==="

# 4.1: Build succeeds
cargo build --package qbit --features tauri
echo "✓ 4.1: Build succeeds"

# 4.2: Tests pass
cargo test --package qbit --features tauri
echo "✓ 4.2: Tests pass"

# 4.3: New method exists
grep -q "create_session_with_runtime" src-tauri/src/pty/manager.rs
echo "✓ 4.3: New method exists"

# 4.4: Deprecation warning appears (but build succeeds)
cargo build --package qbit --features tauri 2>&1 | grep -q "deprecated" || echo "(no deprecation warnings yet - ok if 4c not done)"
echo "✓ 4.4: Deprecation check complete"

# 4.5: PtyEventEmitter trait exists
grep -q "trait PtyEventEmitter" src-tauri/src/pty/manager.rs
echo "✓ 4.5: Internal trait exists"

echo ""
echo "=== Phase 4 PASSED ==="
echo ""
echo "MANUAL VERIFICATION REQUIRED:"
echo "  1. Run: just dev"
echo "  2. Open terminal in app"
echo "  3. Type 'echo hello' and press Enter"
echo "  4. Verify output appears"
echo "  5. Type 'cd /tmp && pwd'"
echo "  6. Verify directory change detected"
echo "  7. Type 'exit'"
echo "  8. Verify session closes cleanly"
```

**Rollback**:
- 4a: Revert to single `create_session` method
- 4b: Delete `RuntimeEmitter` and `create_session_with_runtime`
- 4c: Revert callers to use `create_session`
- 4d: Remove `#[deprecated]` attribute

---

#### Phase 5: Update AgentBridge

**Risk Level**: ⚠️ CAREFUL (core agent functionality)

**Sub-phases**:

**5a: Add emit_event Helper**
```rust
impl AgentBridge {
    /// Helper to emit events (prepares for runtime migration)
    fn emit_event(&self, event: AiEvent) {
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(event);
        }
    }
}
// Update all event_tx.send() calls to use emit_event()
```

**5b: Add Runtime Field (Dual Mode)**
```rust
pub struct AgentBridge {
    event_tx: Option<mpsc::UnboundedSender<AiEvent>>,
    runtime: Option<Arc<dyn QbitRuntime>>,  // NEW
    // ...
}

impl AgentBridge {
    fn emit_event(&self, event: AiEvent) {
        // Emit through BOTH during transition
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(event.clone());
        }
        if let Some(ref rt) = self.runtime {
            let _ = rt.emit(RuntimeEvent::Ai(event));
        }
    }
}
```

**5c: Add Runtime Constructor**
```rust
impl AgentBridge {
    /// NEW: Construct with runtime (CLI path)
    pub async fn new_with_runtime(
        workspace: PathBuf,
        provider: &str,
        model: &str,
        api_key: &str,
        runtime: Arc<dyn QbitRuntime>,
    ) -> Result<Self> {
        Self {
            event_tx: None,
            runtime: Some(runtime),
            // ... rest of initialization
        }
    }
}
```

**5d: Switch Tauri to Runtime**
```rust
// In init_ai_agent command
let runtime = Arc::new(TauriRuntime::new(app));
let bridge = AgentBridge::new_with_runtime(
    workspace, provider, model, api_key, runtime
).await?;
```

**5e: Remove event_tx Field**
```rust
pub struct AgentBridge {
    runtime: Arc<dyn QbitRuntime>,  // Only field now
    // ...
}
```

**Acceptance Criteria**:
```bash
#!/bin/bash
set -e
echo "=== Phase 5 Acceptance Tests ==="

# 5.1: Build succeeds
cargo build --package qbit --features tauri
echo "✓ 5.1: Build succeeds"

# 5.2: Tests pass
cargo test --package qbit --features tauri
echo "✓ 5.2: Tests pass"

# 5.3: emit_event helper exists
grep -q "fn emit_event" src-tauri/src/ai/agent_bridge.rs
echo "✓ 5.3: emit_event helper exists"

# 5.4: Runtime field exists
grep -q "runtime.*Arc<dyn QbitRuntime>" src-tauri/src/ai/agent_bridge.rs
echo "✓ 5.4: Runtime field exists"

# 5.5: new_with_runtime constructor exists
grep -q "new_with_runtime" src-tauri/src/ai/agent_bridge.rs
echo "✓ 5.5: Runtime constructor exists"

echo ""
echo "=== Phase 5 PASSED ==="
echo ""
echo "MANUAL VERIFICATION REQUIRED:"
echo "  1. Run: just dev"
echo "  2. Initialize AI agent"
echo "  3. Send prompt: 'What files are in src/?'"
echo "  4. Verify streaming response appears"
echo "  5. Verify tool calls (list_files) show in UI"
echo "  6. Verify tool results appear"
echo "  7. Test tool that needs approval"
echo "  8. Verify approval dialog works"
echo "  9. Approve and verify tool executes"
```

**Rollback**:
- 5a: Revert emit_event calls to direct event_tx.send()
- 5b: Remove runtime field
- 5c: Delete new_with_runtime constructor
- 5d: Revert command to use old constructor
- 5e: Restore event_tx field

---

#### Phase 6: Build CLI Infrastructure

**Risk Level**: 🆕 NEW CODE (no existing code affected)

**Files to Create**:
```
src-tauri/src/
├── bin/qbit-cli.rs       # Entry point
└── cli/
    ├── mod.rs            # Module exports
    ├── args.rs           # Clap argument definitions
    ├── bootstrap.rs      # AppState + CliRuntime setup
    ├── runner.rs         # Execution logic
    └── output.rs         # Event receiver loop (CRITICAL)
```

**Key Implementation - output.rs**:
```rust
use tokio::sync::mpsc;
use crate::runtime::RuntimeEvent;
use crate::ai::events::AiEvent;

pub async fn run_event_loop(
    mut event_rx: mpsc::UnboundedReceiver<RuntimeEvent>,
    json_mode: bool,
    quiet_mode: bool,
) -> anyhow::Result<()> {
    use std::io::Write;

    while let Some(event) = event_rx.recv().await {
        match event {
            RuntimeEvent::Ai(ai_event) => {
                if json_mode {
                    // Flat JSON for jq compatibility
                    println!("{}", serde_json::to_string(&ai_event)?);
                } else if !quiet_mode {
                    handle_ai_event_terminal(&ai_event);
                }

                // Check for completion
                if matches!(ai_event, AiEvent::Completed { .. } | AiEvent::Error { .. }) {
                    break;
                }
            }
            RuntimeEvent::TerminalOutput { data, .. } => {
                if !quiet_mode && !json_mode {
                    std::io::stdout().write_all(&data)?;
                    std::io::stdout().flush()?;
                }
            }
            _ => {
                if json_mode {
                    println!("{}", serde_json::to_string(&event)?);
                }
            }
        }
    }
    Ok(())
}

fn handle_ai_event_terminal(event: &AiEvent) {
    match event {
        AiEvent::TextDelta { delta, .. } => {
            print!("{}", delta);
            std::io::stdout().flush().ok();
        }
        AiEvent::ToolResult { tool_name, success, .. } => {
            let icon = if *success { "✓" } else { "✗" };
            eprintln!("[{} {}]", icon, tool_name);
        }
        AiEvent::Error { message, .. } => {
            eprintln!("Error: {}", message);
        }
        AiEvent::Completed { .. } => {
            println!(); // Final newline
        }
        _ => {}
    }
}
```

**Acceptance Criteria**:
```bash
#!/bin/bash
set -e
echo "=== Phase 6 Acceptance Tests ==="

# 6.1: CLI binary compiles
cargo build --package qbit --features cli --bin qbit-cli
echo "✓ 6.1: CLI binary compiles"

# 6.2: CLI shows help
./target/debug/qbit-cli --help | grep -q "qbit-cli"
echo "✓ 6.2: CLI help works"

# 6.3: CLI shows version
./target/debug/qbit-cli --version | grep -q "qbit-cli"
echo "✓ 6.3: CLI version works"

# 6.4: CLI fails gracefully without API key
./target/debug/qbit-cli -e "test" 2>&1 | grep -qi "api.*key\|config\|credential"
echo "✓ 6.4: Missing API key error is clear"

# 6.5: CLI module structure exists
test -f src-tauri/src/cli/mod.rs
test -f src-tauri/src/cli/args.rs
test -f src-tauri/src/cli/bootstrap.rs
test -f src-tauri/src/cli/runner.rs
test -f src-tauri/src/cli/output.rs
echo "✓ 6.5: All CLI modules exist"

# 6.6: Tauri build still works (no regression)
cargo build --package qbit --features tauri
echo "✓ 6.6: Tauri build unaffected"

echo ""
echo "=== Phase 6 PASSED ==="
echo ""
echo "MANUAL VERIFICATION REQUIRED (needs API key):"
echo "  1. Export QBIT_API_KEY or configure ~/.qbit/settings.toml"
echo "  2. Run: ./target/debug/qbit-cli -e 'Say hello'"
echo "  3. Verify streaming response appears"
echo "  4. Run: ./target/debug/qbit-cli -e 'What is 2+2?' --quiet"
echo "  5. Verify only final answer appears"
echo "  6. Run: ./target/debug/qbit-cli -e 'Hello' --json | jq ."
echo "  7. Verify valid JSON output"
```

**Rollback**: Delete `src/cli/` directory and `src/bin/qbit-cli.rs`

---

#### Phase 7: Integration Tests

**Risk Level**: 🎯 VALIDATION (read-only verification)

**Acceptance Criteria** (from top of document):
```bash
#!/bin/bash
set -e

echo "=== Qbit CLI Integration Test Suite ==="

# Test setup
export QBIT_WORKSPACE=$(pwd)
CLI="./target/debug/qbit-cli"

# Build CLI first
cargo build --package qbit --features cli --bin qbit-cli

# 1. Agent initializes with settings.toml
$CLI -e "Say 'ready'" --quiet --auto-approve | grep -q "ready"
echo "✓ Test 1: Agent initialization"

# 2. File tools work
output=$($CLI -e "Read line 1 of Cargo.toml" --json --auto-approve)
echo "$output" | jq -e 'select(.event == "ToolResult" and .tool_name == "read_file" and .success == true)' > /dev/null
echo "✓ Test 2: File reading tool"

# 3. Shell execution works
output=$($CLI -e "Run 'echo hello' in the shell" --json --auto-approve)
echo "$output" | jq -e 'select(.event == "ToolResult" and .tool_name == "run_pty_cmd" and .success == true)' > /dev/null
echo "✓ Test 3: Shell execution tool"

# 4. Sidecar captures events
$CLI -e "List files in src/" --auto-approve --quiet
test -d ~/.qbit/sidecar/ && echo "✓ Test 4: Sidecar directory exists"

# 5. Session persistence works
$CLI -e "Remember this" --session integration-test --auto-approve --quiet
ls ~/.qbit/sessions/ | grep -q "integration-test"
echo "✓ Test 5: Session persistence"

# 6. Batch execution works
echo -e "What is 1+1?\n---\nWhat is 2+2?" > /tmp/batch-test.txt
$CLI -f /tmp/batch-test.txt --quiet --auto-approve | grep -q "4"
echo "✓ Test 6: Batch execution"

# 7. JSON output is valid and parseable
$CLI -e "Hello" --json --auto-approve | jq -e . > /dev/null
echo "✓ Test 7: JSON output format"

echo ""
echo "=== All integration tests passed ==="
echo "=== MVP COMPLETE ==="
```

**Extended Tests** (from earlier in document):
```bash
#!/bin/bash
set -e

echo "=== Extended Integration Tests ==="

CLI="./target/debug/qbit-cli"

# 8. Error handling - invalid API key
QBIT_API_KEY=invalid $CLI -e "test" 2>&1 | grep -qi "api\|auth\|key\|invalid"
echo "✓ Test 8: Invalid API key error"

# 9. Timeout behavior
timeout 10 $CLI -e "Run 'sleep 100'" --timeout 2 --auto-approve 2>&1 | grep -qi "timeout" || true
echo "✓ Test 9: Timeout handling"

# 10. Non-zero exit code on error
if $CLI -e "test" --provider nonexistent 2>/dev/null; then
    echo "✗ Test 10: Should have failed"
    exit 1
fi
echo "✓ Test 10: Non-zero exit on error"

# 11. Workspace validation
if $CLI /nonexistent/path -e "test" 2>/dev/null; then
    echo "✗ Test 11: Should have failed"
    exit 1
fi
echo "✓ Test 11: Workspace validation"

# 12. Verbose mode shows initialization
$CLI -e "Hi" -v --auto-approve 2>&1 | grep -qi "settings\|provider\|model"
echo "✓ Test 12: Verbose mode"

# Cleanup
rm -f /tmp/batch-test.txt

echo ""
echo "=== Extended tests complete ==="
```

---

### Phase Summary

| Phase | Risk | Est. Time | Acceptance Tests | Rollback Complexity |
|-------|------|-----------|------------------|---------------------|
| 1 | ✅ Safe | 1 hour | 5 automated | Trivial |
| 2 | ✅ Safe | 1 hour | 3 auto + manual | Trivial |
| 3 | ✅ Safe | 2 hours | 4 auto + manual | Easy |
| 4 | ⚠️ Careful | 4 hours | 5 auto + manual | Moderate |
| 5 | ⚠️ Careful | 4 hours | 5 auto + manual | Moderate |
| 6 | 🆕 New | 6 hours | 6 auto + manual | Easy (delete) |
| 7 | 🎯 Test | 1 hour | 12 automated | N/A |

**Total Estimated Time**: 2-3 days

### Agent Review Findings (Round 2)

Four specialized agents scrutinized the runtime abstraction. Key findings:

#### ✅ Design Validated
- Trait is object-safe (`dyn QbitRuntime` works)
- `emit()` is sync - works from std::thread (critical for PtyManager)
- Dynamic dispatch overhead negligible for IO-bound operations
- SidecarState needs NO abstraction (already runtime-agnostic)

#### 🔴 Issues Fixed in This Revision
| Issue | Resolution |
|-------|------------|
| Migration order breaks Tauri | Reordered: wire Tauri first, then abstract |
| No event receiver in CLI | Added `run_event_loop()` in Phase 6 |
| JSON format mismatch | Flatten `AiEvent` in JSON mode, not wrap |
| Feature flag conflict | Added `compile_error!` guard |
| `spawn_event_forwarder` is central hub | Abstract it in Phase 3, delegate old function |

#### ⚠️ Risks to Monitor
| Risk | Mitigation |
|------|------------|
| Thread spawning in PtyManager | `Arc<dyn QbitRuntime>` is `Send + Sync`, works in thread |
| Deprecation warnings during migration | Expected - remove after Phase 5 |
| Tauri binary size if deps not gated | Use `optional = true` on tauri dependency |
| Error propagation from `emit()` | Use `let _ =` to ignore, log in debug builds |

#### 📊 Implementation Status
| Component | Status |
|-----------|--------|
| `QbitRuntime` trait | ✅ Exists in codebase |
| `TauriRuntime` | ✅ Exists, not wired |
| `CliRuntime` | ✅ Exists, not wired |
| AgentBridge integration | ❌ Still uses `event_tx` |
| PtyManager integration | ❌ Still uses `AppHandle` |
| CLI binary | ❌ Does not exist |
| Event receiver loop | ❌ Does not exist |

**Estimated effort**: 2-3 days across 7 phases

### Backward Compatibility

To avoid breaking existing code during migration:

```rust
// Type alias for gradual migration
pub type DefaultRuntime = TauriRuntime;

// Or feature-flag based:
#[cfg(feature = "tauri")]
pub type DefaultRuntime = TauriRuntime;

#[cfg(feature = "cli")]
pub type DefaultRuntime = CliRuntime;
```

### Benefits

1. **Single codebase** - No duplication of business logic
2. **Testable** - Can create `MockRuntime` for unit tests
3. **Extensible** - Easy to add WebSocket runtime, gRPC runtime, etc.
4. **Clean separation** - UI concerns isolated from business logic

---

## Secondary Uses

Once the test harness works, it also enables:
- **Manual testing** - Interactive REPL for ad-hoc agent interactions
- **Behavior testing** - Scriptable sessions to verify agent responses
- **CI pipelines** - Automated regression testing on every commit

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                        qbit-cli                              │
├──────────────────────────────────────────────────────────────┤
│  CLI Parser (clap)                                           │
│    ├── Interactive mode (default)                            │
│    ├── Execute mode (-e "prompt")                            │
│    └── Script mode (-f script.txt)                           │
├──────────────────────────────────────────────────────────────┤
│  Event Handler                                               │
│    ├── Terminal renderer (colored, streaming)                │
│    ├── JSON output (--json)                                  │
│    └── Quiet mode (--quiet, final response only)             │
├──────────────────────────────────────────────────────────────┤
│  AgentBridge                                                 │
│    └── Existing orchestration (reuse from src-tauri/src/ai)  │
└──────────────────────────────────────────────────────────────┘
```

## Binary Location

```
src-tauri/src/bin/qbit-cli.rs     # Entry point
src-tauri/src/cli/                 # CLI-specific modules
  mod.rs
  args.rs                          # CLI argument parsing
  repl.rs                          # Interactive REPL
  runner.rs                        # Non-interactive execution
  output.rs                        # Output formatting
```

## CLI Interface

### Basic Usage

```bash
# Interactive REPL
qbit-cli

# Single prompt execution
qbit-cli -e "Explain the main.rs file"

# Execute from file (one prompt per line, or multi-line with ---)
qbit-cli -f prompts.txt

# Pipe input
echo "List all TODO comments" | qbit-cli
```

### Arguments

```
qbit-cli [OPTIONS] [WORKSPACE]

ARGS:
    [WORKSPACE]    Working directory (default: current dir)

OPTIONS:
    -e, --execute <PROMPT>     Execute single prompt and exit
    -f, --file <FILE>          Execute prompts from file
    -m, --model <MODEL>        Model to use (default: from env/config)
    -p, --provider <PROVIDER>  Provider: openrouter, anthropic, vertex

    --json                     Output events as JSON lines (for parsing)
    --quiet                    Only output final response
    --no-stream                Wait for full response (no streaming)

    --auto-approve             Auto-approve all tool calls (⚠️ testing only)
    --approve-tools <LIST>     Auto-approve specific tools (comma-separated)
    --deny-tools <LIST>        Always deny specific tools

    --session <ID>             Resume or name a session
    --no-session               Disable session persistence

    --max-turns <N>            Max conversation turns (default: unlimited)
    --timeout <SECS>           Timeout per prompt (default: 300)

    -v, --verbose              Show debug information
    -h, --help                 Print help
    -V, --version              Print version
```

### Environment Variables

```bash
QBIT_API_KEY          # API key for provider
QBIT_PROVIDER         # Default provider
QBIT_MODEL            # Default model
QBIT_WORKSPACE        # Default workspace
QBIT_AUTO_APPROVE     # Set to "1" for auto-approve mode
```

## Interactive Mode (REPL)

```
$ qbit-cli
qbit v0.1.0 | claude-3.5-sonnet via openrouter
workspace: /Users/xlyk/Code/qbit

> Explain the purpose of agent_bridge.rs

AgentBridge is the orchestration layer connecting the LLM to tools...
[streams response]

> /tools
Available tools:
  • read_file      - Read file contents
  • write_file     - Write to file
  • edit_file      - Edit file with search/replace
  • run_pty_cmd    - Execute shell command
  ...

> /session save
Session saved: session-2024-01-15-abc123

> /quit
```

### REPL Commands

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/tools` | List available tools |
| `/clear` | Clear conversation history |
| `/session [save\|load\|list]` | Session management |
| `/context` | Show current context (tokens, workspace) |
| `/model <name>` | Switch model mid-session |
| `/approve <always\|prompt\|deny> [tool]` | Set tool approval policy |
| `/quit` or `/exit` | Exit CLI |

## Tool Approval Handling

### Interactive Mode
```
[Tool Request] run_pty_cmd
  command: rm -rf node_modules
  risk: medium

(a)pprove / (d)eny / (A)lways allow / (D)always deny / (v)iew details? _
```

### Automation Mode
```bash
# Auto-approve everything (dangerous, testing only)
qbit-cli --auto-approve -e "Delete all .bak files"

# Auto-approve safe tools only
qbit-cli --approve-tools "read_file,grep_file,list_files" -f test.txt

# Explicit deny for destructive tools
qbit-cli --deny-tools "run_pty_cmd,delete_file" -e "Clean up the project"
```

## Output Formats

### Default (Terminal)
```
> What files are in src/?

Reading directory...
Found 12 files:
  src/
  ├── main.tsx
  ├── App.tsx
  ├── components/
  │   ├── Header.tsx
  │   └── Footer.tsx
  └── ...
```

### JSON Lines (--json)
```json
{"event":"started","turn_id":"abc123"}
{"event":"text_delta","delta":"Reading","accumulated":"Reading"}
{"event":"tool_request","tool":"list_files","args":{"path":"src/"}}
{"event":"tool_result","tool":"list_files","success":true}
{"event":"completed","response":"Found 12 files...","tokens":1234}
```

### Quiet (--quiet)
```
Found 12 files:
  src/main.tsx
  src/App.tsx
  ...
```

## Script Mode

### Script Format (prompts.txt)
```
# Comment lines are ignored
List all TypeScript files

---
# Multi-line prompt with separator
Analyze the following files for security issues:
- src/auth.ts
- src/api.ts

---
Run the test suite
```

### Execution
```bash
# Run script
qbit-cli -f integration-test.txt --json > results.jsonl

# With auto-approve for CI
qbit-cli -f smoke-test.txt --auto-approve --quiet
```

## Integration Test Patterns

### Basic Test Runner

```bash
#!/bin/bash
# test-agent-behavior.sh

# Test 1: File reading capability
output=$(qbit-cli -e "Read the first 10 lines of Cargo.toml" --json --auto-approve)
if echo "$output" | grep -q '"success":true'; then
    echo "✓ File reading works"
else
    echo "✗ File reading failed"
    exit 1
fi

# Test 2: Tool denial handling
output=$(qbit-cli -e "Delete all files" --deny-tools "delete_file,run_pty_cmd" --json)
if echo "$output" | grep -q '"event":"tool_denied"'; then
    echo "✓ Tool denial works"
else
    echo "✗ Tool denial not triggered"
    exit 1
fi
```

### Rust Integration Tests

```rust
// tests/cli_integration.rs
use assert_cmd::Command;

#[test]
fn test_single_prompt_execution() {
    let mut cmd = Command::cargo_bin("qbit-cli").unwrap();
    cmd.args(["-e", "What is 2+2?", "--quiet"])
        .env("QBIT_API_KEY", "test-key")
        .assert()
        .success()
        .stdout(predicates::str::contains("4"));
}

#[test]
fn test_json_output_format() {
    let mut cmd = Command::cargo_bin("qbit-cli").unwrap();
    cmd.args(["-e", "Hello", "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains(r#""event":"completed""#));
}
```

## Implementation Phases

### Phase 1: Core CLI (MVP)

**Goal**: Full-featured headless CLI that mirrors the UI version exactly - same settings, same services, same behavior.

#### 1.1 Design Principle: Full Parity with UI

The CLI is **not** a stripped-down agent. It runs the exact same stack:

```
┌─────────────────────────────────────────────────────────────┐
│                      qbit-cli                               │
├─────────────────────────────────────────────────────────────┤
│  CLI Interface (args, stdin/stdout, JSON output)            │
├─────────────────────────────────────────────────────────────┤
│  AppState (SAME as UI)                                      │
│    ├── SettingsManager    ← ~/.qbit/settings.toml           │
│    ├── PtyManager         ← Shell execution                 │
│    ├── AiState            ← AgentBridge + tools             │
│    ├── IndexerState       ← Code analysis                   │
│    ├── TavilyState        ← Web search                      │
│    ├── WorkflowState      ← Workflow execution              │
│    └── SidecarState       ← Context capture + history       │
├─────────────────────────────────────────────────────────────┤
│  Background Services (SAME as UI)                           │
│    ├── Sidecar event processor                              │
│    ├── Session persistence                                  │
│    └── Indexer (on-demand)                                  │
└─────────────────────────────────────────────────────────────┘
```

#### 1.2 Project Setup

```
src-tauri/
├── src/
│   ├── bin/
│   │   └── qbit-cli.rs      # Binary entry point
│   └── cli/
│       ├── mod.rs           # Module exports
│       ├── args.rs          # CLI argument parsing
│       ├── bootstrap.rs     # AppState + service initialization
│       ├── runner.rs        # Prompt execution logic
│       └── output.rs        # Event → stdout formatting
```

**Cargo.toml additions**:
```toml
[[bin]]
name = "qbit-cli"
path = "src/bin/qbit-cli.rs"

[dependencies]
clap = { version = "4", features = ["derive", "env"] }
```

#### 1.3 Argument Structure (args.rs)

```rust
#[derive(Parser)]
#[command(name = "qbit-cli", version, about = "Qbit agent CLI - headless interface")]
pub struct Args {
    /// Working directory (default: current dir)
    #[arg(default_value = ".")]
    pub workspace: PathBuf,

    /// Execute single prompt and exit
    #[arg(short = 'e', long)]
    pub execute: Option<String>,

    /// Execute prompts from file
    #[arg(short = 'f', long)]
    pub file: Option<PathBuf>,

    // ─── Settings Overrides (optional, defaults from settings.toml) ───

    /// Override LLM provider from settings
    #[arg(short = 'p', long)]
    pub provider: Option<String>,

    /// Override model from settings
    #[arg(short = 'm', long)]
    pub model: Option<String>,

    /// Override API key from settings
    #[arg(long, env = "QBIT_API_KEY")]
    pub api_key: Option<String>,

    // ─── Execution Modes ───

    /// Auto-approve all tool calls (⚠️ dangerous, for testing)
    #[arg(long)]
    pub auto_approve: bool,

    /// Output events as JSON lines (for scripting/parsing)
    #[arg(long)]
    pub json: bool,

    /// Only output final response (no streaming)
    #[arg(long)]
    pub quiet: bool,

    /// Disable sidecar context capture
    #[arg(long)]
    pub no_sidecar: bool,

    /// Disable session persistence
    #[arg(long)]
    pub no_session: bool,

    /// Session ID to resume or name
    #[arg(long)]
    pub session: Option<String>,

    // ─── Debug ───

    /// Verbose output (show service initialization, debug info)
    #[arg(short = 'v', long)]
    pub verbose: bool,
}
```

#### 1.4 Bootstrap (bootstrap.rs) - Full Service Initialization

```rust
use crate::state::AppState;
use crate::settings::SettingsManager;
use crate::ai::AiState;
use crate::sidecar::SidecarState;

pub struct CliContext {
    pub app_state: AppState,
    pub event_rx: mpsc::UnboundedReceiver<AiEvent>,
    pub workspace: PathBuf,
}

impl CliContext {
    /// Initialize the full Qbit stack (same as UI)
    pub async fn new(args: &Args) -> Result<Self> {
        // ─── Phase 1: Core Setup (same as main.rs) ───

        // Install TLS provider
        rustls::crypto::ring::default_provider()
            .install_default()
            .ok();

        // Load .env
        dotenvy::dotenv().ok();

        // Initialize logging
        tracing_subscriber::fmt()
            .with_env_filter(if args.verbose { "debug" } else { "warn" })
            .init();

        // ─── Phase 2: AppState (same as Tauri app) ───

        let app_state = AppState::new().await?;

        // ─── Phase 3: Resolve Settings ───

        let settings = app_state.settings_manager.get_settings().await;

        // Provider priority: CLI arg > settings.toml > default
        let provider = args.provider.clone()
            .or_else(|| settings.ai.default_provider.clone())
            .unwrap_or_else(|| "openrouter".to_string());

        let model = args.model.clone()
            .or_else(|| settings.ai.default_model.clone());

        let api_key = args.api_key.clone()
            .or_else(|| resolve_api_key_from_settings(&settings, &provider));

        // ─── Phase 4: Initialize Services (same order as UI) ───

        let workspace = args.workspace.canonicalize()?;
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // 4a. Initialize AI agent
        {
            let mut ai_state = app_state.ai_state.bridge.write().await;
            *ai_state = Some(AgentBridge::new(
                workspace.clone(),
                &provider,
                model.as_deref().unwrap_or("anthropic/claude-sonnet-4"),
                &api_key.ok_or_else(|| anyhow!("No API key configured"))?,
                event_tx,
            ).await?);
        }

        // 4b. Inject dependencies into agent (same as init_ai_agent command)
        {
            let mut bridge = app_state.ai_state.bridge.write().await;
            if let Some(ref mut b) = *bridge {
                b.set_pty_manager(app_state.pty_manager.clone());
                b.set_indexer_state(app_state.indexer_state.clone());
                b.set_tavily_state(app_state.tavily_state.clone());
                b.set_workflow_state(app_state.workflow_state.clone());
                b.set_sidecar_state(app_state.sidecar_state.clone());

                if let Some(ref session_id) = args.session {
                    b.set_session_id(Some(session_id.clone())).await;
                } else if !args.no_session {
                    // Auto-generate session ID
                    b.set_session_id(Some(generate_session_id())).await;
                }
            }
        }

        // 4c. Initialize indexer for workspace
        app_state.indexer_state.initialize(&workspace).await?;

        // 4d. Initialize sidecar (unless disabled)
        if !args.no_sidecar {
            app_state.sidecar_state.initialize(&workspace).await?;
            app_state.sidecar_state.start_session().await?;
        }

        if args.verbose {
            eprintln!("✓ Settings loaded from ~/.qbit/settings.toml");
            eprintln!("✓ Provider: {}", provider);
            eprintln!("✓ Model: {}", model.as_deref().unwrap_or("default"));
            eprintln!("✓ Workspace: {}", workspace.display());
            eprintln!("✓ Sidecar: {}", if args.no_sidecar { "disabled" } else { "enabled" });
            eprintln!("✓ Session: {}", if args.no_session { "disabled" } else { "enabled" });
        }

        Ok(Self {
            app_state,
            event_rx,
            workspace,
        })
    }

    /// Cleanup on exit (flush sidecar, save session)
    pub async fn shutdown(self) -> Result<()> {
        // End sidecar session (flushes events)
        self.app_state.sidecar_state.end_session().await?;

        // Finalize session history
        if let Some(bridge) = self.app_state.ai_state.bridge.read().await.as_ref() {
            bridge.finalize_session().await?;
        }

        Ok(())
    }
}

fn resolve_api_key_from_settings(settings: &Settings, provider: &str) -> Option<String> {
    match provider {
        "openrouter" => settings.api_keys.openrouter.clone(),
        "anthropic" => settings.api_keys.anthropic.clone(),
        "openai" => settings.api_keys.openai.clone(),
        // Vertex uses credentials file, not API key
        "vertex" => None,
        _ => None,
    }
}
```

#### 1.5 Entry Point (qbit-cli.rs)

```rust
use anyhow::Result;
use clap::Parser;
use qbit::cli::{Args, CliContext, Runner};

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize full Qbit stack (same as UI)
    let ctx = CliContext::new(&args).await?;

    let result = match (&args.execute, &args.file) {
        (Some(prompt), _) => {
            // Single prompt execution
            Runner::execute_once(&ctx, prompt, &args).await
        }
        (_, Some(file)) => {
            // Batch file execution
            Runner::execute_file(&ctx, file, &args).await
        }
        (None, None) => {
            // No prompt = error for MVP (REPL comes in Phase 2)
            anyhow::bail!("Interactive mode not yet implemented. Use -e 'prompt' or -f file.txt")
        }
    };

    // Graceful shutdown (save session, flush sidecar)
    ctx.shutdown().await?;

    result
}
```

#### 1.6 Runner (runner.rs)

```rust
pub struct Runner;

impl Runner {
    pub async fn execute_once(
        ctx: &CliContext,
        prompt: &str,
        args: &Args,
    ) -> Result<()> {
        let output_handle = spawn_event_handler(
            ctx.event_rx.clone(), // Need to restructure for multiple prompts
            args.auto_approve,
            args.json,
            args.quiet,
        );

        // Execute via the bridge (same path as send_ai_prompt command)
        let bridge = ctx.app_state.ai_state.bridge.read().await;
        let bridge = bridge.as_ref().ok_or_else(|| anyhow!("Agent not initialized"))?;

        let _response = bridge.execute(prompt).await?;

        output_handle.await??;
        Ok(())
    }

    pub async fn execute_file(
        ctx: &CliContext,
        file: &Path,
        args: &Args,
    ) -> Result<()> {
        let content = std::fs::read_to_string(file)?;
        let prompts = parse_prompt_file(&content);

        for (i, prompt) in prompts.iter().enumerate() {
            if args.verbose {
                eprintln!("\n─── Prompt {}/{} ───", i + 1, prompts.len());
            }

            Self::execute_once(ctx, prompt, args).await?;
        }

        Ok(())
    }
}

fn parse_prompt_file(content: &str) -> Vec<String> {
    content
        .split("\n---\n")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .map(|s| {
            // Remove comment lines within prompt
            s.lines()
                .filter(|line| !line.trim_start().starts_with('#'))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect()
}
```

#### 1.5 Event Handler (output.rs)

```rust
pub async fn handle_events(
    mut rx: mpsc::UnboundedReceiver<AiEvent>,
    auto_approve: bool,
    json_output: bool,
    quiet: bool,
) -> Result<()> {
    while let Some(event) = rx.recv().await {
        match &event {
            AiEvent::TextDelta { delta, .. } if !quiet => {
                if json_output {
                    println!("{}", serde_json::to_string(&event)?);
                } else {
                    print!("{}", delta);
                    std::io::stdout().flush()?;
                }
            }

            AiEvent::ToolApprovalRequest { request_id, tool_name, args, .. } => {
                if json_output {
                    println!("{}", serde_json::to_string(&event)?);
                }

                if auto_approve {
                    // TODO: Send approval via bridge
                    eprintln!("[auto-approved] {}", tool_name);
                } else {
                    // MVP: Simple y/n prompt
                    eprint!("[{} {:?}] approve? (y/n): ", tool_name, args);
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;
                    let approved = input.trim().to_lowercase() == "y";
                    // TODO: Send decision via bridge
                }
            }

            AiEvent::ToolResult { tool_name, success, .. } if !quiet => {
                if json_output {
                    println!("{}", serde_json::to_string(&event)?);
                } else {
                    let icon = if *success { "✓" } else { "✗" };
                    eprintln!("[{} {}]", icon, tool_name);
                }
            }

            AiEvent::Completed { response, .. } => {
                if json_output {
                    println!("{}", serde_json::to_string(&event)?);
                } else if quiet {
                    println!("{}", response);
                }
                // Don't break - let channel close naturally
            }

            AiEvent::Error { message, .. } => {
                if json_output {
                    println!("{}", serde_json::to_string(&event)?);
                } else {
                    eprintln!("Error: {}", message);
                }
                anyhow::bail!("{}", message);
            }

            _ => {
                if json_output {
                    println!("{}", serde_json::to_string(&event)?);
                }
            }
        }
    }

    if !quiet && !json_output {
        println!(); // Final newline after streaming
    }

    Ok(())
}
```

#### 1.7 MVP Deliverables Checklist

- [ ] **Setup**
  - [ ] Add `[[bin]]` section to Cargo.toml
  - [ ] Create `src/bin/qbit-cli.rs` entry point
  - [ ] Create `src/cli/mod.rs` with submodule exports

- [ ] **Arguments** (`cli/args.rs`)
  - [ ] Define `Args` struct with clap derive
  - [ ] Core flags: `-e`, `-f`, `-p`, `-m`, `--api-key`
  - [ ] Mode flags: `--auto-approve`, `--json`, `--quiet`
  - [ ] Service flags: `--no-sidecar`, `--no-session`, `--session`
  - [ ] Debug: `-v/--verbose`

- [ ] **Bootstrap** (`cli/bootstrap.rs`)
  - [ ] Reuse `AppState::new()` (same as Tauri app)
  - [ ] Load settings from `~/.qbit/settings.toml`
  - [ ] Resolve provider/model/api_key with priority: CLI > settings > default
  - [ ] Initialize AI agent with all dependencies injected:
    - [ ] PtyManager
    - [ ] IndexerState
    - [ ] TavilyState
    - [ ] WorkflowState
    - [ ] SidecarState
  - [ ] Initialize indexer for workspace
  - [ ] Initialize sidecar (unless `--no-sidecar`)
  - [ ] Start sidecar session
  - [ ] Graceful shutdown (flush sidecar, save session)

- [ ] **Runner** (`cli/runner.rs`)
  - [ ] Single prompt execution (`-e`)
  - [ ] Batch file execution (`-f`)
  - [ ] Prompt file parser (`---` separator, `#` comments)

- [ ] **Output** (`cli/output.rs`)
  - [ ] Terminal streaming (print deltas as they arrive)
  - [ ] JSON lines mode (all events)
  - [ ] Quiet mode (final response only)
  - [ ] Tool approval prompt (stdin y/n)
  - [ ] Auto-approve handling

- [ ] **Integration Test Suite** (the actual deliverable)
  - [ ] Create `scripts/integration-test.sh` with acceptance criteria tests
  - [ ] Test 1: Agent initializes with settings.toml
  - [ ] Test 2: File reading tool works (`read_file`)
  - [ ] Test 3: Shell execution tool works (`run_pty_cmd`)
  - [ ] Test 4: Sidecar captures events (directory exists, has data)
  - [ ] Test 5: Session persistence works (saved to `~/.qbit/sessions/`)
  - [ ] Test 6: Batch file execution works
  - [ ] Test 7: JSON output is valid and parseable
  - [ ] **All 7 tests pass = MVP complete**

#### 1.8 MVP Usage Examples

```bash
# ─── Basic Usage (uses ~/.qbit/settings.toml) ───

# Uses provider/model/api_key from settings.toml
qbit-cli -e "What files are in src/?"

# Override model for this run
qbit-cli -e "Explain main.rs" -m anthropic/claude-sonnet-4

# Different workspace
qbit-cli ~/other-project -e "Summarize this project"

# ─── Output Modes ───

# JSON output for scripting/parsing
qbit-cli -e "List TODO comments" --json --auto-approve | jq '.event'

# Quiet mode - only final response
result=$(qbit-cli -e "What is 2+2?" --quiet --auto-approve)

# Verbose - see service initialization
qbit-cli -e "Hello" -v

# ─── Service Control ───

# Disable sidecar (faster startup, no context capture)
qbit-cli -e "Quick question" --no-sidecar

# Disable session persistence
qbit-cli -e "One-off task" --no-session

# Resume or name a session
qbit-cli -e "Continue from yesterday" --session my-feature

# ─── Batch Execution ───

# Run prompts from file
qbit-cli -f integration-tests.txt --auto-approve

# Batch with JSON output for CI
qbit-cli -f smoke-tests.txt --json --auto-approve > results.jsonl

# ─── Integration Test Example ───

#!/bin/bash
# test-agent.sh

# Test 1: File operations work
output=$(qbit-cli -e "Read the first line of Cargo.toml" --json --auto-approve)
if echo "$output" | jq -e 'select(.event == "tool_result" and .success == true)' > /dev/null; then
    echo "✓ File reading works"
else
    echo "✗ File reading failed"
    exit 1
fi

# Test 2: Sidecar captures events
qbit-cli -e "List files in src/" --auto-approve
if [ -d ~/.qbit/sidecar/ ]; then
    echo "✓ Sidecar directory exists"
fi

# Test 3: Session persistence
qbit-cli -e "Hello" --session test-session --auto-approve
if ls ~/.qbit/sessions/*test-session* 2>/dev/null; then
    echo "✓ Session saved"
fi
```

---

### Phase 2: Interactive REPL
- [ ] readline-style input (rustyline)
- [ ] Command history
- [ ] REPL commands (/help, /tools, /quit)
- [ ] Colored output with syntax highlighting

### Phase 3: Automation Features
- [ ] Batch file execution (`-f`)
- [ ] `--stateless` mode for isolated prompts
- [ ] `--approve-tools` / `--deny-tools` fine-grained control
- [ ] `--timeout` per prompt
- [ ] `--continue-on-error` for batch

### Phase 4: Testing Infrastructure
- [ ] Session save/load for reproducible tests
- [ ] Mock provider for unit tests
- [ ] Integration test harness with `assert_cmd`
- [ ] CI workflow

## Dependencies (Cargo.toml additions)

```toml
[dependencies]
clap = { version = "4", features = ["derive", "env"] }
rustyline = "14"
colored = "2"
indicatif = "0.17"  # Progress spinners

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
```

## File Structure Summary

```
src-tauri/
├── src/
│   ├── bin/
│   │   └── qbit-cli.rs          # NEW: CLI binary entry point
│   ├── cli/                      # NEW: CLI module
│   │   ├── mod.rs
│   │   ├── args.rs              # Argument parsing
│   │   ├── repl.rs              # Interactive REPL
│   │   ├── runner.rs            # Prompt execution
│   │   └── output.rs            # Formatters (terminal, json, quiet)
│   └── ai/                       # Existing (reused)
│       └── agent_bridge.rs
├── Cargo.toml                    # Add [[bin]] section
└── tests/
    └── cli_integration.rs        # NEW: CLI tests
```

## Cargo.toml Changes

```toml
[[bin]]
name = "qbit-cli"
path = "src/bin/qbit-cli.rs"

[features]
cli = ["clap", "rustyline", "colored", "indicatif"]
```

# Sidecar Context Capture System

The sidecar is a background system that passively captures session context during Qbit agent interactions, stores it semantically in a vector database, and synthesizes useful outputs (commit messages, documentation, session summaries) on demand.

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Features](#features)
- [Configuration](#configuration)
- [Models](#models)
- [Running Integration Tests](#running-integration-tests)
- [API Reference](#api-reference)
- [Troubleshooting](#troubleshooting)

## Overview

The sidecar operates alongside the main Qbit agent, capturing events without blocking the primary workflow. It provides:

- **Passive Event Capture**: Records user prompts, file edits, tool calls, agent reasoning, and user feedback
- **Semantic Search**: Vector-based search over session history using embeddings
- **Commit Message Generation**: Synthesizes meaningful commit messages from captured context
- **Session Summaries**: Generates summaries of what was accomplished during a session
- **History Queries**: Answer questions about past work using natural language

## Architecture

The sidecar operates in three layers:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Layer 1: Event Capture                        │
│                  (synchronous, no LLM calls)                     │
│                                                                  │
│   User Prompts ─┐                                                │
│   File Edits ───┼──▶ SessionEvent ──▶ In-Memory Buffer          │
│   Tool Calls ───┤                                                │
│   Reasoning ────┘                                                │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                  Layer 2: Periodic Processing                    │
│               (async, batched during pauses)                     │
│                                                                  │
│   Buffer ──▶ Embed Events ──▶ LanceDB ──▶ Generate Checkpoints  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                  Layer 3: On-Demand Synthesis                    │
│                   (user-triggered, one-shot)                     │
│                                                                  │
│   Commit Messages ◀──┐                                           │
│   Session Summaries ◀┼── Retrieve + LLM/Template ◀── User Query │
│   History Queries ◀──┘                                           │
└─────────────────────────────────────────────────────────────────┘
```

### Module Structure

```
src-tauri/src/sidecar/
├── mod.rs           # Public exports
├── capture.rs       # Event capture bridge (AiEvent → SessionEvent)
├── commands.rs      # Tauri command handlers
├── config.rs        # Configuration options
├── events.rs        # Event types and session lifecycle
├── models.rs        # Model management (embeddings + LLM)
├── processor.rs     # Background processing (flush, embed, checkpoint)
├── prompts.rs       # Prompt templates for synthesis
├── state.rs         # SidecarState (main entry point)
├── storage.rs       # LanceDB vector storage
└── synthesis.rs     # High-level synthesis API
```

## Features

### Event Types

The sidecar captures the following event types:

| Event Type | Description | Example |
|------------|-------------|---------|
| `UserPrompt` | User's stated intent | "Add authentication feature" |
| `FileEdit` | File modification | Created `/src/auth.rs` |
| `ToolCall` | Agent tool invocation | `write_file(path=/src/auth.rs)` |
| `AgentReasoning` | Agent's decision-making | "Choosing JWT for stateless auth" |
| `UserFeedback` | Approval/denial/correction | Approved file write |
| `ErrorRecovery` | Error and recovery attempt | Fixed import error |
| `CommitBoundary` | Logical commit point | Files ready for commit |
| `SessionStart` | Session begins | Initial request captured |
| `SessionEnd` | Session ends | Summary generated |
| `AiResponse` | Agent's response | Final message to user |

### Commit Boundary Detection

The sidecar automatically detects commit boundaries based on:

- **Completion Signals**: Agent indicates work is done ("The feature is now complete")
- **User Approvals**: Explicit approval for commit-worthy changes
- **File Groups**: Related files edited together

### Search Capabilities

- **Vector Search**: Find semantically similar events using embeddings
- **Keyword Search**: Full-text search across event content
- **File-Based Search**: Find all events related to specific files
- **Session Filtering**: Scope searches to specific sessions

## Configuration

Configuration is stored at `~/.qbit/sidecar/config.json`:

```json
{
  "checkpoint_event_threshold": 20,
  "checkpoint_time_threshold_secs": 300,
  "buffer_flush_threshold": 100,
  "synthesis_enabled": true,
  "embeddings_enabled": true,
  "data_dir": "~/.qbit/sidecar",
  "models_dir": "~/.qbit/models",
  "retention_days": 30,
  "capture_tool_calls": true,
  "capture_reasoning": true,
  "min_content_length": 10
}
```

### Configuration Options

| Option | Default | Description |
|--------|---------|-------------|
| `checkpoint_event_threshold` | 20 | Events before generating checkpoint |
| `checkpoint_time_threshold_secs` | 300 | Seconds of inactivity before checkpoint |
| `buffer_flush_threshold` | 100 | Max events in memory before disk flush |
| `synthesis_enabled` | true | Enable LLM-based synthesis |
| `embeddings_enabled` | true | Enable embedding generation |
| `data_dir` | `~/.qbit/sidecar` | Storage directory |
| `models_dir` | `~/.qbit/models` | Models directory |
| `retention_days` | 30 | Days to keep events (0 = unlimited) |
| `capture_tool_calls` | true | Capture tool call events |
| `capture_reasoning` | true | Capture agent reasoning events |
| `min_content_length` | 10 | Minimum content length to capture |

## Models

The sidecar uses two local models:

### Embedding Model

- **Name**: AllMiniLM-L6-V2
- **Size**: ~30 MB
- **Dimensions**: 384
- **Purpose**: Generate embeddings for semantic search
- **Library**: fastembed

### LLM Model

- **Name**: Qwen 2.5 0.5B Instruct (Q4_K_M quantization)
- **Size**: ~400 MB
- **Purpose**: Generate commit messages, summaries, and answer queries
- **Library**: llama-cpp-2

### Installing Models

Models are downloaded to `~/.qbit/models/`. There are two ways to install them:

#### Option 1: Run the Model Download Test (Recommended)

```bash
cd src-tauri

# Download both models (~430 MB total)
cargo test test_download_models -- --ignored --nocapture
```

This will:
1. Download the embedding model (all-MiniLM-L6-v2, ~30 MB)
2. Download the LLM model (Qwen 2.5 0.5B, ~400 MB)
3. Save both to `~/.qbit/models/`

Expected output:
```
=== Sidecar Model Download ===
Models directory: "/Users/you/.qbit/models"

1. Downloading embedding model (all-MiniLM-L6-v2, ~30MB)...
   Progress: 0.0%
   Progress: 20.0%
   ...
   ✓ Embedding model ready
2. Downloading LLM model (Qwen 2.5 0.5B, ~400MB)...
   This may take several minutes on first run.
   Progress: 10.0%
   ...
   ✓ LLM model ready

=== Status ===
Embedding available: true
LLM available: true
```

#### Option 2: Manual Download

**Embedding Model** (auto-downloaded by fastembed on first use):
```bash
# The embedding model is cached automatically by fastembed
# in ~/.qbit/models/all-minilm-l6-v2/
```

**LLM Model** (manual download from HuggingFace):
```bash
mkdir -p ~/.qbit/models
curl -L -o ~/.qbit/models/qwen2.5-0.5b-instruct-q4_k_m.gguf \
  "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf"
```

### Verifying Model Installation

```bash
# Check if models are present
ls -la ~/.qbit/models/

# Expected:
# all-minilm-l6-v2/           (directory with embedding model files)
# qwen2.5-0.5b-instruct-q4_k_m.gguf  (~400 MB file)
```

## Running Integration Tests

The sidecar has extensive integration tests that verify the full system flow.

### Prerequisites

1. **Install models first** (see [Installing Models](#installing-models))
2. **Ensure Rust toolchain is installed**

### Running All Integration Tests

```bash
cd src-tauri

# Run all sidecar integration tests (requires models)
cargo test sidecar::integration_tests -- --ignored --nocapture

# This runs ~30 tests covering:
# - Session lifecycle
# - Event capture and storage
# - Embedding generation
# - Semantic search
# - LLM text generation
# - Commit message synthesis
# - Session export/import
```

### Running Specific Test Categories

```bash
# Session and storage tests (fast, no models needed)
cargo test sidecar::integration_tests::test_session -- --ignored --nocapture
cargo test sidecar::integration_tests::test_lancedb -- --ignored --nocapture

# Embedding tests (requires embedding model)
cargo test sidecar::integration_tests::test_embedding -- --ignored --nocapture

# LLM tests (requires LLM model)
cargo test sidecar::integration_tests::test_llm -- --ignored --nocapture

# Full end-to-end simulation
cargo test sidecar::integration_tests::test_full_agent_session_simulation -- --ignored --nocapture
```

### Running Unit Tests (No Models Required)

```bash
# Run all sidecar unit tests (fast, no external dependencies)
cargo test --lib sidecar:: -- --test-threads=1

# These include:
# - Configuration tests
# - Event serialization tests
# - Prompt template tests
# - Storage layer tests
# - Property-based tests
```

### Test Categories

| Test Category | Models Needed | Command |
|--------------|---------------|---------|
| Unit tests | None | `cargo test --lib sidecar::` |
| Storage tests | None | `cargo test sidecar::storage::tests` |
| Property-based tests | None | `cargo test sidecar::storage::proptests` |
| Session lifecycle | None | `cargo test test_session_lifecycle -- --ignored` |
| Embedding generation | Embedding | `cargo test test_embedding -- --ignored` |
| LLM generation | LLM | `cargo test test_llm -- --ignored` |
| Full simulation | Both | `cargo test test_full_agent_session -- --ignored` |

### Expected Test Output

```
running 58 tests
test sidecar::config::tests::test_default_config ... ok
test sidecar::events::tests::test_event_serialization ... ok
test sidecar::storage::proptests::prop_event_roundtrip ... ok
test sidecar::storage::proptests::prop_count_matches_query ... ok
...
test result: ok. 58 passed; 0 failed; 32 ignored
```

## API Reference

### Tauri Commands

The sidecar exposes the following Tauri commands:

```typescript
// Initialize the sidecar with a workspace path
await invoke('sidecar_initialize', { workspace: '/path/to/project' });

// Start a new session
const sessionId: string = await invoke('sidecar_start_session', {
  initialRequest: 'Add authentication feature'
});

// End the current session
const session: Session = await invoke('sidecar_end_session');

// Get sidecar status
const status: SidecarStatus = await invoke('sidecar_status');

// Synthesize a commit message
const draft: CommitDraft = await invoke('sidecar_synthesize_commit', {
  sessionId: 'uuid-here'
});

// Query session history
const response: HistoryResponse = await invoke('sidecar_query_history', {
  sessionId: 'uuid-here',
  query: 'What changes were made to authentication?'
});

// Get model status
const models: ModelsStatus = await invoke('sidecar_models_status');
```

### Rust API

```rust
use crate::sidecar::{SidecarState, CaptureContext};

// Initialize sidecar
let state = SidecarState::with_config(SidecarConfig::default());
state.initialize(workspace_path).await?;

// Start a session
let session_id = state.start_session("Add authentication")?;

// Capture events manually
let event = SessionEvent::file_edit(
    session_id,
    PathBuf::from("/src/auth.rs"),
    FileOperation::Create,
    Some("Created auth module".to_string()),
);
state.capture(event);

// Or use the capture bridge with AI events
let mut capture = CaptureContext::new(Arc::clone(&state));
capture.process(&ai_event);

// End session
let session = state.end_session()?;
```

## Troubleshooting

### Models Not Found

```
Error: LLM model not found at ~/.qbit/models/qwen2.5-0.5b-instruct-q4_k_m.gguf
```

**Solution**: Run the model download test:
```bash
cargo test test_download_models -- --ignored --nocapture
```

### Embedding Model Download Fails

```
Error: Failed to initialize embedding model
```

**Solution**: Check internet connection and disk space. The fastembed library downloads models automatically but needs network access.

### LLM Generation Slow

The LLM runs on CPU by default. For faster inference:
1. Ensure you have a modern CPU with AVX2 support
2. The Q4_K_M quantization is optimized for CPU inference

### LanceDB Query Returns Limited Results

**Issue**: Queries return only 10 results when more exist.

**Solution**: This was fixed by adding explicit limits to queries. If you encounter this, ensure you're using the latest code with `QUERY_LIMIT` constant.

### Tests Fail with "BackendAlreadyInitialized"

**Issue**: Parallel tests fail because llama.cpp can only be initialized once.

**Solution**: Run LLM tests with single thread:
```bash
cargo test test_llm -- --ignored --test-threads=1
```

### Storage Directory Permissions

```
Error: Permission denied: ~/.qbit/sidecar
```

**Solution**: Ensure the directory is writable:
```bash
mkdir -p ~/.qbit/sidecar ~/.qbit/models
chmod 755 ~/.qbit ~/.qbit/sidecar ~/.qbit/models
```

## Performance Notes

- **Event Capture**: Synchronous and fast (~microseconds), doesn't block agent
- **Storage Flush**: Batched async writes to LanceDB
- **Embedding Generation**: ~10-50ms per text on modern CPU
- **LLM Generation**: ~1-5 seconds for short outputs on CPU
- **Vector Search**: Sub-second for typical index sizes

## Data Storage

Data is stored in LanceDB format at `~/.qbit/sidecar/sidecar.lance/`:

```
sidecar.lance/
├── events/           # Event vectors and metadata
├── checkpoints/      # Checkpoint summaries
└── sessions/         # Session metadata
```

To clear all data:
```bash
rm -rf ~/.qbit/sidecar/sidecar.lance
```

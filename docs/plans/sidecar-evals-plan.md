# Sidecar Evaluation Framework Plan

This plan outlines how to leverage the DeepEval-based evaluation framework to test the sidecar context capture system and LanceDB storage.

## Background

### Current State

**Eval Framework** (`evals/`):
- Uses DeepEval with GEval metrics for semantic evaluation
- `CliRunner` class executes prompts and captures stdout/stderr
- Extensible via new test files and fixtures
- Supports batch execution for multi-turn conversations

**Sidecar System** (`src-tauri/src/sidecar/`):
- Full LanceDB storage with `events`, `checkpoints`, `sessions` tables
- Rich Tauri command API for querying state
- Synthesis capabilities for commit messages and summaries
- Event types: UserPrompt, FileEdit, ToolCall, AgentReasoning, UserFeedback, etc.

**CLI-Sidecar Integration**:
- CLI already uses sidecar when `settings.sidecar.enabled = true`
- Events captured during execution and stored in `~/.qbit/sidecar/sidecar.lance`
- **Gap**: No CLI interface to query sidecar state after execution

## Goals

1. Verify sidecar correctly captures events during CLI execution
2. Validate database state consistency and persistence
3. Evaluate synthesis quality (commit messages, summaries) using LLM-as-judge
4. Enable regression testing for sidecar functionality

---

## Phase 1: Enable Sidecar Querying from Python

### 1.1 Create Python LanceDB Helper

**File**: `evals/sidecar_utils.py`

```python
"""Utilities for querying sidecar LanceDB from Python tests."""

import os
from pathlib import Path
from typing import Optional
import lancedb

SIDECAR_DB_PATH = Path.home() / ".qbit" / "sidecar" / "sidecar.lance"


def connect_sidecar_db(db_path: Optional[Path] = None) -> lancedb.DBConnection:
    """Connect to the sidecar LanceDB database."""
    path = db_path or SIDECAR_DB_PATH
    if not path.exists():
        raise FileNotFoundError(f"Sidecar database not found at {path}")
    return lancedb.connect(str(path))


def get_last_session(db: lancedb.DBConnection) -> Optional[dict]:
    """Get the most recently created session."""
    sessions = db.open_table("sessions")
    df = sessions.to_pandas().sort_values("started_at_ms", ascending=False)
    if df.empty:
        return None
    return df.iloc[0].to_dict()


def get_session_events(db: lancedb.DBConnection, session_id: str) -> list[dict]:
    """Get all events for a specific session."""
    events = db.open_table("events")
    df = events.to_pandas()
    session_events = df[df["session_id"] == session_id]
    return session_events.sort_values("timestamp_ms").to_dict("records")


def search_events_keyword(db: lancedb.DBConnection, query: str, limit: int = 10) -> list[dict]:
    """Search events by keyword in content."""
    events = db.open_table("events")
    df = events.to_pandas()
    matches = df[df["content"].str.contains(query, case=False, na=False)]
    return matches.head(limit).to_dict("records")


def get_storage_stats(db: lancedb.DBConnection) -> dict:
    """Get storage statistics."""
    return {
        "event_count": len(db.open_table("events").to_pandas()),
        "checkpoint_count": len(db.open_table("checkpoints").to_pandas()),
        "session_count": len(db.open_table("sessions").to_pandas()),
    }


def list_sessions(db: lancedb.DBConnection, limit: int = 10) -> list[dict]:
    """List recent sessions."""
    sessions = db.open_table("sessions")
    df = sessions.to_pandas().sort_values("started_at_ms", ascending=False)
    return df.head(limit).to_dict("records")


def get_session(db: lancedb.DBConnection, session_id: str) -> Optional[dict]:
    """Get a specific session by ID."""
    sessions = db.open_table("sessions")
    df = sessions.to_pandas()
    match = df[df["id"] == session_id]
    if match.empty:
        return None
    return match.iloc[0].to_dict()
```

### 1.2 Add Sidecar Fixtures

**File**: `evals/conftest.py` (additions)

```python
import time
from sidecar_utils import connect_sidecar_db, get_last_session

@pytest.fixture(scope="function")
def sidecar_db():
    """Connection to sidecar LanceDB database."""
    # Small delay to ensure async flush completes
    time.sleep(0.3)
    return connect_sidecar_db()


@pytest.fixture(scope="function")
def sidecar_session_before(sidecar_db):
    """Capture session state before test for comparison."""
    return get_last_session(sidecar_db)


@pytest.fixture(scope="function")
def clean_sidecar(tmp_path):
    """Provide a clean temporary sidecar database for isolated tests."""
    import lancedb
    db_path = tmp_path / "test_sidecar.lance"
    db = lancedb.connect(str(db_path))
    # Initialize empty tables with correct schema
    # ... schema setup ...
    return db
```

---

## Phase 2: Create Sidecar Test Suite

### 2.1 New Test File

**File**: `evals/test_sidecar.py`

```python
"""Sidecar integration tests using DeepEval metrics."""

import pytest
import time
from deepeval import evaluate
from deepeval.test_case import LLMTestCase
from deepeval.metrics import GEval

from conftest import CliRunner, get_last_response
from sidecar_utils import (
    connect_sidecar_db,
    get_last_session,
    get_session_events,
    search_events_keyword,
    get_storage_stats,
)


def requires_sidecar(func):
    """Decorator to skip tests if sidecar is not available."""
    return pytest.mark.skipif(
        not connect_sidecar_db(),
        reason="Sidecar database not available"
    )(func)


@pytest.mark.requires_api
class TestEventCapture:
    """Verify events are captured correctly during CLI execution."""

    def test_user_prompt_captured(self, cli: CliRunner):
        """Verify user prompts appear in sidecar events."""
        marker = f"test-marker-{int(time.time())}"
        cli.run_prompt(f"Remember this marker: {marker}", quiet=True)

        time.sleep(0.5)  # Wait for async flush
        db = connect_sidecar_db()
        session = get_last_session(db)
        events = get_session_events(db, session["id"])

        user_prompts = [e for e in events if e["event_type"] == "UserPrompt"]
        assert len(user_prompts) > 0, "No UserPrompt events captured"
        assert any(marker in e["content"] for e in user_prompts), \
            f"Marker '{marker}' not found in captured prompts"

    def test_tool_execution_captured(self, cli: CliRunner):
        """Verify tool calls are captured with outputs."""
        cli.run_prompt(
            "List the files in the current directory using the Bash tool",
            auto_approve=True,
            quiet=True
        )

        time.sleep(0.5)
        db = connect_sidecar_db()
        session = get_last_session(db)
        events = get_session_events(db, session["id"])

        tool_events = [e for e in events if e["event_type"] == "ToolCall"]
        assert len(tool_events) > 0, "No ToolCall events captured"

    def test_file_edit_captured(self, cli: CliRunner, tmp_path):
        """Verify file edits are captured with diffs."""
        test_file = tmp_path / "test_edit.txt"
        test_file.write_text("original content")

        cli.run_prompt(
            f"Add a new line 'hello world' to {test_file}",
            auto_approve=True,
            quiet=True
        )

        time.sleep(0.5)
        db = connect_sidecar_db()
        session = get_last_session(db)
        events = get_session_events(db, session["id"])

        file_events = [e for e in events if e["event_type"] == "FileEdit"]
        # May also appear as ToolCall with file modification
        tool_events = [e for e in events if str(test_file) in str(e.get("files_modified", []))]

        assert len(file_events) > 0 or len(tool_events) > 0, \
            "No file modification events captured"


@pytest.mark.requires_api
class TestSessionLifecycle:
    """Verify session start/end and metadata persistence."""

    def test_session_created_on_prompt(self, cli: CliRunner):
        """Each CLI execution creates a new session."""
        db = connect_sidecar_db()
        initial_count = get_storage_stats(db)["session_count"]

        cli.run_prompt("Hello", quiet=True)
        time.sleep(0.5)

        db = connect_sidecar_db()
        new_count = get_storage_stats(db)["session_count"]
        assert new_count > initial_count, "No new session created"

    def test_session_has_initial_request(self, cli: CliRunner):
        """Session captures the initial request."""
        marker = f"initial-request-{int(time.time())}"
        cli.run_prompt(marker, quiet=True)

        time.sleep(0.5)
        db = connect_sidecar_db()
        session = get_last_session(db)

        assert session is not None
        assert marker in session.get("initial_request", ""), \
            "Initial request not captured in session"

    def test_session_has_workspace(self, cli: CliRunner):
        """Session captures the workspace path."""
        cli.run_prompt("Hello", quiet=True)

        time.sleep(0.5)
        db = connect_sidecar_db()
        session = get_last_session(db)

        assert session is not None
        assert session.get("workspace_path"), "Workspace path not captured"


@pytest.mark.requires_api
class TestSearchFunctionality:
    """Verify keyword and semantic search accuracy."""

    def test_keyword_search_finds_events(self, cli: CliRunner):
        """Keyword search returns matching events."""
        unique_term = f"uniqueterm{int(time.time())}"
        cli.run_prompt(f"Remember: {unique_term}", quiet=True)

        time.sleep(0.5)
        db = connect_sidecar_db()
        results = search_events_keyword(db, unique_term)

        assert len(results) > 0, f"Search for '{unique_term}' returned no results"

    def test_search_no_false_positives(self, cli: CliRunner):
        """Search doesn't return unrelated events."""
        db = connect_sidecar_db()
        nonsense = "xyzzy12345nonexistent"
        results = search_events_keyword(db, nonsense)

        assert len(results) == 0, f"Search for '{nonsense}' should return empty"


@pytest.mark.requires_api
class TestSynthesisQuality:
    """Evaluate generated commit messages and summaries using LLM-as-judge."""

    def test_commit_message_describes_changes(self, cli: CliRunner, eval_model, tmp_path):
        """Generated commit message accurately describes changes."""
        # Create a test file
        test_file = tmp_path / "feature.py"
        test_file.write_text("# placeholder")

        # Execute file editing task
        cli.run_prompt(
            f"Replace the content of {test_file} with a hello world function",
            auto_approve=True,
            quiet=True
        )

        time.sleep(0.5)
        db = connect_sidecar_db()
        session = get_last_session(db)
        events = get_session_events(db, session["id"])

        # Build context from events for commit message evaluation
        event_summaries = [e["content"] for e in events[:10]]

        test_case = LLMTestCase(
            input="Generate a commit message for these changes",
            actual_output=session.get("final_summary", "") or "\n".join(event_summaries),
            expected_output="A commit message describing the hello world function addition",
            context=event_summaries,
        )

        metric = GEval(
            name="Commit Relevance",
            criteria="The summary should accurately reflect the changes made (creating/editing a Python file with hello world function)",
            evaluation_steps=[
                "Check if the summary mentions file changes",
                "Verify it describes the nature of the change (adding functionality)",
                "Ensure it's concise and actionable",
            ],
            threshold=0.6,
            model=eval_model,
        )

        results = evaluate([test_case], [metric])
        assert all(r.success for r in results.test_results)


@pytest.mark.requires_api
class TestStorageIntegrity:
    """Verify database state consistency."""

    def test_event_count_matches_session(self, cli: CliRunner):
        """Session's event_count matches actual event count in storage."""
        cli.run_prompt("Do something simple", auto_approve=True, quiet=True)

        time.sleep(0.5)
        db = connect_sidecar_db()
        session = get_last_session(db)
        events = get_session_events(db, session["id"])

        # Allow some tolerance for async processing
        assert abs(session.get("event_count", 0) - len(events)) <= 1, \
            f"Session event_count ({session.get('event_count')}) != actual ({len(events)})"

    def test_sessions_persist_across_runs(self, cli: CliRunner):
        """Sessions persist and remain queryable after new CLI runs."""
        # Create first session with unique marker
        marker1 = f"persist-test-{int(time.time())}"
        cli.run_prompt(marker1, quiet=True)

        time.sleep(0.3)
        db = connect_sidecar_db()
        first_session = get_last_session(db)
        first_session_id = first_session["id"]

        # Create second session
        cli.run_prompt("Another prompt", quiet=True)

        time.sleep(0.3)
        db = connect_sidecar_db()

        # First session should still exist
        from sidecar_utils import get_session
        original = get_session(db, first_session_id)
        assert original is not None, "First session was lost after second run"

    def test_no_duplicate_events(self, cli: CliRunner):
        """Events should have unique IDs within a session."""
        cli.run_prompt("Test for duplicates", quiet=True)

        time.sleep(0.5)
        db = connect_sidecar_db()
        session = get_last_session(db)
        events = get_session_events(db, session["id"])

        event_ids = [e["id"] for e in events]
        assert len(event_ids) == len(set(event_ids)), "Duplicate event IDs found"
```

---

## Phase 3: Database State Verification

### 3.1 Custom Scorers

**File**: `evals/sidecar_scorers.py`

```python
"""Custom scorers for sidecar state verification."""

from typing import Callable
from sidecar_utils import connect_sidecar_db, get_session_events, get_session


def verify_min_event_count(expected_min: int) -> Callable:
    """Scorer to verify minimum event count for a session."""
    def scorer(session_id: str) -> tuple[bool, str]:
        db = connect_sidecar_db()
        events = get_session_events(db, session_id)
        passed = len(events) >= expected_min
        reason = f"Found {len(events)} events (expected >= {expected_min})"
        return passed, reason
    return scorer


def verify_files_tracked(expected_files: list[str]) -> Callable:
    """Scorer to verify specific files appear in session's tracked files."""
    def scorer(session_id: str) -> tuple[bool, str]:
        db = connect_sidecar_db()
        session = get_session(db, session_id)
        if not session:
            return False, "Session not found"

        tracked = session.get("files_touched_json", "[]")
        if isinstance(tracked, str):
            import json
            tracked = json.loads(tracked)

        missing = [f for f in expected_files if f not in tracked]
        if missing:
            return False, f"Missing files: {missing}"
        return True, "All expected files tracked"
    return scorer


def verify_event_types_present(expected_types: list[str]) -> Callable:
    """Scorer to verify specific event types were captured."""
    def scorer(session_id: str) -> tuple[bool, str]:
        db = connect_sidecar_db()
        events = get_session_events(db, session_id)

        found_types = set(e["event_type"] for e in events)
        missing = set(expected_types) - found_types

        if missing:
            return False, f"Missing event types: {missing}"
        return True, f"All expected types found: {expected_types}"
    return scorer


def verify_session_ended() -> Callable:
    """Scorer to verify session has ended_at timestamp."""
    def scorer(session_id: str) -> tuple[bool, str]:
        db = connect_sidecar_db()
        session = get_session(db, session_id)
        if not session:
            return False, "Session not found"

        if session.get("ended_at_ms"):
            return True, "Session properly ended"
        return False, "Session missing ended_at timestamp"
    return scorer
```

---

## Phase 4: Optional CLI Enhancements

### 4.1 Add `--export-session` Flag

**File**: `src-tauri/src/cli/args.rs` (modification)

```rust
/// Export the session data to a JSON file after execution
#[arg(long, value_name = "PATH")]
pub export_session: Option<PathBuf>,
```

**File**: `src-tauri/src/cli/runner.rs` (modification)

```rust
// After session ends, export if flag is set
if let Some(export_path) = &args.export_session {
    if let Some(session_id) = current_session_id {
        let export_data = sidecar_state.export_session(session_id).await?;
        std::fs::write(export_path, export_data)?;
        info!("Session exported to {}", export_path.display());
    }
}
```

### 4.2 Add CLI Sidecar Subcommands

**File**: `src-tauri/src/cli/sidecar_commands.rs` (new)

```rust
use clap::Subcommand;

#[derive(Subcommand)]
pub enum SidecarCommand {
    /// List recent sessions
    ListSessions {
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
    /// Show events for a session
    ShowEvents {
        session_id: String,
        #[arg(short, long)]
        json: bool,
    },
    /// Generate commit message for a session
    GenerateCommit {
        session_id: String,
    },
    /// Export session to JSON file
    Export {
        session_id: String,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Show storage statistics
    Stats,
}
```

---

## Implementation Priority

| Priority | Task | Effort | Description |
|----------|------|--------|-------------|
| **P0** | Create `sidecar_utils.py` | Low | Python helpers for LanceDB queries |
| **P0** | Add sidecar fixtures to `conftest.py` | Low | `sidecar_db`, `sidecar_session_before` |
| **P1** | Create `test_sidecar.py` | Medium | Basic event capture and session tests |
| **P1** | Add synthesis quality tests | Medium | GEval metrics for commit messages |
| **P2** | Create `sidecar_scorers.py` | Low | Custom verification scorers |
| **P2** | Storage integrity test suite | Medium | Persistence, consistency tests |
| **P3** | CLI `--export-session` flag | Medium | Export session data after run |
| **P3** | CLI sidecar subcommands | High | `list-sessions`, `show-events`, etc. |

---

## Dependencies

Add to `evals/pyproject.toml`:

```toml
[project]
dependencies = [
    "pytest>=7.0",
    "deepeval>=0.20",
    "lancedb>=0.3.0",
    "pandas>=2.0",
]
```

---

## Configuration

Ensure `~/.qbit/settings.toml` includes sidecar configuration:

```toml
[sidecar]
enabled = true
synthesis_enabled = true
synthesis_backend = "template"  # No API keys needed for template mode
retention_days = 30
capture_tool_calls = true
capture_reasoning = true
min_content_length = 10
```

---

## Running the Tests

```bash
cd evals

# Setup (one-time)
uv venv .venv && source .venv/bin/activate
uv pip install -e .

# Run sidecar tests (requires API and sidecar enabled)
RUN_API_TESTS=1 pytest test_sidecar.py -v

# Run specific test category
RUN_API_TESTS=1 pytest test_sidecar.py -v -k "TestEventCapture"

# Run with verbose output
RUN_API_TESTS=1 VERBOSE=1 pytest test_sidecar.py -v
```

---

## Success Metrics

1. **Event Capture Coverage**: 100% of defined event types testable
2. **Synthesis Quality**: Commit messages score >= 0.7 on GEval metrics
3. **Storage Reliability**: Zero data loss across test runs
4. **Search Accuracy**: Keyword search returns expected results with no false positives

---

## Future Enhancements

1. **Vector Search Testing**: Test semantic similarity search once embeddings are enabled
2. **Checkpoint Testing**: Verify automatic checkpoint generation
3. **Multi-Session Tests**: Test cross-session queries and history
4. **Performance Benchmarks**: Track event capture latency and storage efficiency

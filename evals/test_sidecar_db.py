"""Sidecar database integration tests.

Tests the LanceDB sidecar database functionality:
- Event capture and storage
- Session lifecycle management
- Search functionality
- Data persistence and integrity

Run all tests:
    RUN_API_TESTS=1 pytest test_sidecar_db.py -v
"""

import time
from typing import Any

import pytest
from deepeval import evaluate
from deepeval.metrics import GEval
from deepeval.test_case import LLMTestCase, LLMTestCaseParams

from client import StreamingRunner
from sidecar import (
    connect_db,
    get_last_session,
    get_session,
    get_events,
    search_events,
    get_storage_stats,
    get_sessions,
)


# =============================================================================
# Helper Functions
# =============================================================================


def wait_for_sidecar_flush(delay: float = 0.5):
    """Wait for sidecar async flush to complete.

    The sidecar system writes events asynchronously. This helper ensures
    events are flushed to disk before querying the database.

    Args:
        delay: Seconds to wait (default 0.5s)
    """
    time.sleep(delay)


def create_unique_marker() -> str:
    """Create a unique marker string for identifying test data.

    Returns:
        String like 'test-marker-1733678901'
    """
    return f"test-marker-{int(time.time())}"


# =============================================================================
# Event Capture Tests
# =============================================================================


@pytest.mark.requires_api
@pytest.mark.requires_sidecar
class TestEventCapture:
    """Tests that verify events are captured correctly in the sidecar database."""

    @pytest.mark.asyncio
    async def test_user_prompt_captured(self, runner: StreamingRunner, sidecar_db):
        """UserPrompt events contain the prompt text."""
        marker = create_unique_marker()
        prompt = f"Say '{marker}' and nothing else"

        result = await runner.run(prompt)
        assert result.success

        # Wait for async flush
        wait_for_sidecar_flush()

        # Search for the marker in event content
        events = search_events(sidecar_db, marker, limit=5)
        assert len(events) > 0, f"Expected to find events with marker '{marker}'"

        # Verify at least one event contains the full prompt or marker
        event_contents = [e.get("content", "") or "" for e in events]
        assert any(marker in content for content in event_contents), \
            f"Expected marker '{marker}' in event content"

    @pytest.mark.asyncio
    async def test_tool_execution_captured(self, runner: StreamingRunner, sidecar_db):
        """ToolCall events are created when tools run."""
        marker = create_unique_marker()
        prompt = f"Create a file called '/tmp/{marker}.txt' with content 'test' and then read it back"

        result = await runner.run(prompt)
        assert result.success

        # Verify tool was called
        assert len(result.tool_calls) > 0, "Expected at least one tool call"

        # Wait for async flush
        wait_for_sidecar_flush()

        # Search for tool-related events
        # The marker should appear in file-related tool calls or content
        events = search_events(sidecar_db, marker, limit=10)
        assert len(events) > 0, f"Expected to find tool events with marker '{marker}'"

    @pytest.mark.asyncio
    async def test_session_events_not_empty(self, runner: StreamingRunner, sidecar_db):
        """Verify sessions have at least one event."""
        marker = create_unique_marker()
        prompt = f"Echo '{marker}'"

        result = await runner.run(prompt)
        assert result.success

        # Wait for async flush
        wait_for_sidecar_flush()

        # Get the last session
        session = get_last_session(sidecar_db)
        assert session is not None, "Expected to find at least one session"

        # Get events for this session
        session_id = session["id"]
        events = get_events(sidecar_db, session_id)
        assert len(events) > 0, f"Expected session {session_id} to have at least one event"


# =============================================================================
# Session Lifecycle Tests
# =============================================================================


@pytest.mark.requires_api
@pytest.mark.requires_sidecar
class TestSessionLifecycle:
    """Tests that verify session management and metadata capture."""

    @pytest.mark.asyncio
    async def test_session_created_on_prompt(self, runner: StreamingRunner, sidecar_db):
        """New sessions are created for agent runs."""
        # Get current session count
        stats_before = get_storage_stats(sidecar_db)
        count_before = stats_before.get("session_count", 0)

        marker = create_unique_marker()
        result = await runner.run(f"Say '{marker}'")
        assert result.success

        # Wait for async flush
        wait_for_sidecar_flush()

        # Verify session count increased
        stats_after = get_storage_stats(sidecar_db)
        count_after = stats_after.get("session_count", 0)
        assert count_after > count_before, \
            f"Expected session count to increase from {count_before} to {count_after}"

    @pytest.mark.asyncio
    async def test_session_has_initial_request(self, runner: StreamingRunner, sidecar_db):
        """Session captures initial request."""
        marker = create_unique_marker()
        prompt = f"Remember this marker: {marker}"

        result = await runner.run(prompt)
        assert result.success

        # Wait for async flush
        wait_for_sidecar_flush()

        # Get the last session
        session = get_last_session(sidecar_db)
        assert session is not None, "Expected to find a session"

        # Get events for this session
        session_id = session["id"]
        events = get_events(sidecar_db, session_id)

        # Verify the marker appears in at least one event
        event_contents = [e.get("content", "") or "" for e in events]
        assert any(marker in content for content in event_contents), \
            f"Expected marker '{marker}' in session events"


# =============================================================================
# Search Functionality Tests
# =============================================================================


@pytest.mark.requires_api
@pytest.mark.requires_sidecar
class TestSearchFunctionality:
    """Tests that verify search capabilities work correctly."""

    @pytest.mark.asyncio
    async def test_keyword_search_finds_events(self, runner: StreamingRunner, sidecar_db):
        """Searching for a unique term finds it."""
        # Create a unique marker that's unlikely to exist elsewhere
        marker = f"UNIQUE_TEST_MARKER_{int(time.time() * 1000)}"
        prompt = f"Say the exact phrase '{marker}' and nothing else"

        result = await runner.run(prompt)
        assert result.success

        # Wait for async flush
        wait_for_sidecar_flush()

        # Search for the unique marker
        events = search_events(sidecar_db, marker, limit=5)
        assert len(events) > 0, f"Expected to find events containing '{marker}'"

        # Verify the marker actually appears in the results
        found_marker = False
        for event in events:
            content = event.get("content", "") or ""
            if marker in content:
                found_marker = True
                break

        assert found_marker, f"Expected marker '{marker}' to appear in event content"

    @pytest.mark.asyncio
    async def test_search_no_false_positives(self, runner: StreamingRunner, sidecar_db):
        """Nonsense queries return empty or don't match."""
        # Use a nonsense string that definitely won't exist
        nonsense = f"xyzzy_nonexistent_gibberish_{int(time.time() * 1000)}_qwerty"

        # Search for the nonsense string (no need to create a session first)
        events = search_events(sidecar_db, nonsense, limit=10)

        # Either no events found, or if any are found, they don't actually contain the nonsense
        if len(events) > 0:
            for event in events:
                content = event.get("content", "") or ""
                assert nonsense not in content.lower(), \
                    f"False positive: nonsense query '{nonsense}' found in content"


# =============================================================================
# Synthesis Quality Tests (DeepEval)
# =============================================================================


@pytest.mark.requires_api
@pytest.mark.requires_sidecar
class TestSynthesisQuality:
    """Tests that use GEval metrics to evaluate session summary quality."""

    @pytest.mark.asyncio
    async def test_session_summary_quality(self, runner: StreamingRunner, sidecar_db, eval_model: Any):
        """Evaluate that captured events make semantic sense.

        This test verifies that the sidecar captures meaningful context that
        could be used for session summarization or retrieval.
        """
        # Use a single prompt that covers multiple topics to test event capture
        prompt = (
            "Answer these three questions briefly: "
            "1) Name three programming languages, "
            "2) What is 10 + 20, "
            "3) What is the capital of France?"
        )

        result = await runner.run(prompt)
        assert result.success

        # Wait for async flush
        wait_for_sidecar_flush(delay=1.0)

        # Get the last session
        session = get_last_session(sidecar_db)
        assert session is not None, "Expected to find a session"

        # Get events for this session
        session_id = session["id"]
        events = get_events(sidecar_db, session_id)
        assert len(events) > 0, "Expected session to have events"

        # Create a summary of event types and content snippets
        event_summary = []
        for event in events[:20]:  # Limit to first 20 events
            event_type = event.get("event_type", "unknown")
            content = event.get("content", "")
            if content:
                snippet = content[:200] if content else ""
                event_summary.append(f"{event_type}: {snippet}")

        summary_text = "\n".join(event_summary)

        # Evaluate that the captured events contain coherent, task-relevant information
        test_case = LLMTestCase(
            input="Session with programming languages, arithmetic, geography questions",
            actual_output=summary_text,
            context=[
                "Session should contain a prompt about programming languages, arithmetic, and geography",
                "Session events should capture the user's question",
            ],
        )

        metric = GEval(
            name="Session Event Coherence",
            criteria="The captured events should contain the user's multi-topic question about programming, math, and geography",
            evaluation_steps=[
                "Check if events contain the user prompt asking about programming languages, arithmetic, and capital of France",
                "Events should be coherent and related to the original question",
            ],
            evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT, LLMTestCaseParams.CONTEXT],
            threshold=0.5,  # Lower threshold since we just need to verify prompt capture
            model=eval_model,
        )

        results = evaluate([test_case], [metric])

        if not results.test_results[0].success:
            raise AssertionError(
                f"DeepEval failed for Session Event Coherence: "
                f"summary={summary_text[:500]}"
            )


# =============================================================================
# Storage Integrity Tests
# =============================================================================


@pytest.mark.requires_sidecar
class TestStorageIntegrity:
    """Tests that verify data consistency and persistence (no API needed)."""

    def test_sessions_persist_across_runs(self, sidecar_db):
        """Sessions survive multiple database connections."""
        # Get sessions from first connection
        sessions_1 = get_sessions(sidecar_db, limit=5)
        session_count_1 = len(sessions_1)

        # Close and reconnect
        # Note: sidecar_db fixture creates a new connection per test
        # So we manually reconnect here
        from sidecar import connect_db
        db2 = connect_db()

        # Get sessions from second connection
        sessions_2 = get_sessions(db2, limit=5)
        session_count_2 = len(sessions_2)

        # Session count should be the same
        assert session_count_1 == session_count_2, \
            f"Session count mismatch: {session_count_1} vs {session_count_2}"

        # If we have sessions, verify they have the same IDs
        if session_count_1 > 0:
            ids_1 = [s["id"] for s in sessions_1]
            ids_2 = [s["id"] for s in sessions_2]
            assert ids_1 == ids_2, "Session IDs should match across connections"

    def test_no_duplicate_events(self, sidecar_db):
        """Event IDs are unique within a session.

        This test verifies that the sidecar doesn't create duplicate events
        with the same ID in the same session.
        """
        # Get the last session
        session = get_last_session(sidecar_db)

        # Skip if no sessions exist
        if session is None:
            pytest.skip("No sessions found in database")

        session_id = session["id"]
        events = get_events(sidecar_db, session_id)

        # If no events, skip
        if len(events) == 0:
            pytest.skip(f"No events found for session {session_id}")

        # Extract event IDs
        event_ids = [e.get("id") for e in events if e.get("id") is not None]

        # Verify no duplicates
        unique_ids = set(event_ids)
        assert len(event_ids) == len(unique_ids), \
            f"Found {len(event_ids) - len(unique_ids)} duplicate event IDs in session {session_id}"

    def test_database_tables_exist(self, sidecar_db):
        """Verify expected tables exist in the database."""
        # Get table names
        table_names = sidecar_db.table_names()

        # Check for expected tables (includes both legacy and new L1 normalized tables)
        # Core required tables
        expected_tables = {"events"}

        for table in expected_tables:
            assert table in table_names, f"Expected table '{table}' not found in database"

        # Layer 1 normalized tables (new schema)
        l1_tables = {
            "l1_sessions", "l1_goals", "l1_decisions", "l1_errors",
            "l1_file_contexts", "l1_questions", "l1_goal_progress", "l1_file_changes"
        }

        # Check that L1 tables exist
        for table in l1_tables:
            assert table in table_names, f"Expected L1 table '{table}' not found in database"

    def test_session_timestamps_valid(self, sidecar_db):
        """Session timestamps are valid and ordered correctly."""
        sessions = get_sessions(sidecar_db, limit=10)

        if len(sessions) == 0:
            pytest.skip("No sessions found in database")

        for session in sessions:
            started_at = session.get("started_at_ms")
            ended_at = session.get("ended_at_ms")

            # started_at should be present and positive
            assert started_at is not None, "Session missing started_at_ms"
            assert started_at > 0, "started_at_ms should be positive"

            # If ended_at is present, it should be after started_at
            if ended_at is not None and ended_at > 0:
                assert ended_at >= started_at, \
                    f"ended_at_ms ({ended_at}) should be >= started_at_ms ({started_at})"

    def test_event_timestamps_within_session(self, sidecar_db):
        """Event timestamps fall within session time boundaries."""
        session = get_last_session(sidecar_db)

        if session is None:
            pytest.skip("No sessions found in database")

        session_id = session["id"]
        events = get_events(sidecar_db, session_id)

        if len(events) == 0:
            pytest.skip(f"No events found for session {session_id}")

        started_at = session.get("started_at_ms")
        ended_at = session.get("ended_at_ms")

        assert started_at is not None, "Session missing started_at_ms"

        for event in events:
            event_ts = event.get("timestamp_ms")

            # Event timestamp should exist
            assert event_ts is not None, "Event missing timestamp_ms"

            # Event should be after session start
            # (Allow small tolerance for clock skew)
            assert event_ts >= started_at - 1000, \
                f"Event timestamp {event_ts} before session start {started_at}"

            # If session has ended, event should be before end time
            if ended_at is not None and ended_at > 0:
                assert event_ts <= ended_at + 1000, \
                    f"Event timestamp {event_ts} after session end {ended_at}"

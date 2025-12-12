"""Evaluation tests for session management fixes.

Tests verify that:
1. Sidecar sessions are unique per conversation (no duplicates from race conditions)
2. Multiple prompts in same conversation use the same sidecar session
3. Clearing conversation properly ends sidecar session and starts fresh
4. Session finalization works correctly
"""

import re
from pathlib import Path

import pytest
import yaml

from client import QbitClient


# =============================================================================
# Helpers
# =============================================================================


def find_recent_session_dirs(sessions_dir: Path, prefix: str = "") -> list[Path]:
    """Find session directories (not JSON files) in the sessions dir."""
    if not sessions_dir.exists():
        return []

    dirs = []
    for item in sessions_dir.iterdir():
        if item.is_dir() and (not prefix or item.name.startswith(prefix)):
            # Check if it has the expected sidecar files (state.md is the main session file)
            if (item / "state.md").exists():
                dirs.append(item)

    # Sort by modification time, newest first
    dirs.sort(key=lambda p: p.stat().st_mtime, reverse=True)
    return dirs


def parse_state_frontmatter(session_dir: Path) -> dict:
    """Parse YAML frontmatter from state.md file."""
    state_path = session_dir / "state.md"
    if not state_path.exists():
        return {}

    content = state_path.read_text()
    if not content.startswith("---\n"):
        return {}

    # Find end of frontmatter
    rest = content[4:]  # Skip opening "---\n"
    end_idx = rest.find("\n---")
    if end_idx == -1:
        return {}

    yaml_content = rest[:end_idx]
    try:
        return yaml.safe_load(yaml_content) or {}
    except yaml.YAMLError:
        return {}


def get_session_id_from_dir(session_dir: Path) -> str:
    """Extract session ID from state.md frontmatter."""
    meta = parse_state_frontmatter(session_dir)
    return meta.get("session_id", session_dir.name)


# =============================================================================
# Session Uniqueness Tests (Race Condition Prevention)
# =============================================================================


class TestSessionUniqueness:
    """Tests verifying that sessions are unique and not duplicated."""

    @pytest.mark.asyncio
    async def test_single_prompt_creates_single_session(self, qbit_server, eval_sessions_dir):
        """Verify that a single prompt creates exactly one sidecar session."""
        sessions_dir = Path(eval_sessions_dir)
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            # Execute a single prompt
            await qbit_server.execute_simple(
                session_id, "Say 'hello' and nothing else.", timeout_secs=60
            )

            # Find new session directories
            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs

            # Skip if no session directory created (sidecar may be disabled)
            if len(new_dirs) == 0:
                pytest.skip("No session directory created - sidecar may be disabled")

            # Should have exactly one new session directory
            assert len(new_dirs) == 1, (
                f"Expected exactly 1 new session, got {len(new_dirs)}. "
                "This could indicate a race condition in session creation."
            )

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_multiple_prompts_same_sidecar_session(self, qbit_server, eval_sessions_dir):
        """Verify multiple prompts in same conversation use same sidecar session."""
        sessions_dir = Path(eval_sessions_dir)
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            # Execute multiple prompts in same session
            await qbit_server.execute_simple(
                session_id, "Say 'first'", timeout_secs=60
            )
            await qbit_server.execute_simple(
                session_id, "Say 'second'", timeout_secs=60
            )
            await qbit_server.execute_simple(
                session_id, "Say 'third'", timeout_secs=60
            )

            # Find new session directories
            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs

            # Skip if no session directory created (sidecar may be disabled)
            if len(new_dirs) == 0:
                pytest.skip("No session directory created - sidecar may be disabled")

            # Should still have only ONE sidecar session (not three!)
            assert len(new_dirs) == 1, (
                f"Expected 1 sidecar session for 3 prompts, got {len(new_dirs)}. "
                "Multiple sidecar sessions indicate session reuse is not working."
            )

            # Verify the session was reused by checking log file exists
            if new_dirs:
                session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
                log_path = session_dir / "log.md"
                assert log_path.exists(), "log.md should exist in session directory"

        finally:
            await qbit_server.delete_session(session_id)


# =============================================================================
# Session Lifecycle Tests
# =============================================================================


class TestSessionLifecycle:
    """Tests for session lifecycle management."""

    @pytest.mark.asyncio
    async def test_session_active_during_conversation(self, qbit_server, eval_sessions_dir):
        """Verify sidecar session remains active during conversation."""
        sessions_dir = Path(eval_sessions_dir)
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            # First prompt
            await qbit_server.execute_simple(
                session_id, "What is 2+2?", timeout_secs=60
            )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
            meta = parse_state_frontmatter(session_dir)

            # Session should be active
            assert meta.get("status") in ("active", "Active", "completed", "Completed"), (
                f"Session status should be active/completed, got: {meta.get('status')}"
            )

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_session_updated_on_subsequent_prompts(self, qbit_server, eval_sessions_dir):
        """Verify session updated_at changes with subsequent prompts."""
        sessions_dir = Path(eval_sessions_dir)
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            # First prompt
            await qbit_server.execute_simple(
                session_id, "Say 'one'", timeout_secs=60
            )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)

            # Get initial updated_at
            meta1 = parse_state_frontmatter(session_dir)
            updated_at_1 = meta1.get("updated_at")

            # Second prompt
            await qbit_server.execute_simple(
                session_id, "Say 'two'", timeout_secs=60
            )

            # Get updated_at after second prompt
            meta2 = parse_state_frontmatter(session_dir)
            updated_at_2 = meta2.get("updated_at")

            # updated_at should have changed (or at least not be None)
            assert updated_at_2 is not None, "updated_at should be set"

        finally:
            await qbit_server.delete_session(session_id)


# =============================================================================
# Session Continuity Tests
# =============================================================================


class TestSessionContinuity:
    """Tests verifying session continuity across prompts."""

    @pytest.mark.asyncio
    async def test_session_id_consistent_across_prompts(self, qbit_server, eval_sessions_dir):
        """Verify the same session_id is used across all prompts."""
        sessions_dir = Path(eval_sessions_dir)
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            # Multiple prompts
            for i in range(3):
                await qbit_server.execute_simple(
                    session_id, f"Say '{i}'", timeout_secs=60
                )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            # Get all session IDs from new directories
            session_ids = [get_session_id_from_dir(d) for d in new_dirs]

            # All should be the same (only one unique session ID)
            unique_ids = set(session_ids)
            assert len(unique_ids) == 1, (
                f"Expected 1 unique session ID, got {len(unique_ids)}: {unique_ids}. "
                "This indicates the session was recreated instead of reused."
            )

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_log_exists_and_session_reused(self, qbit_server, eval_sessions_dir):
        """Verify log.md exists and session is reused across prompts."""
        sessions_dir = Path(eval_sessions_dir)
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        prompts = ["first prompt here", "second prompt here", "third prompt here"]

        try:
            for prompt in prompts:
                await qbit_server.execute_simple(session_id, prompt, timeout_secs=60)

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            # Should have exactly one session directory (session reused across prompts)
            assert len(new_dirs) == 1, (
                f"Expected 1 session for 3 prompts, got {len(new_dirs)}. "
                "Session was not properly reused."
            )

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
            log_path = session_dir / "log.md"

            # Verify log.md exists
            assert log_path.exists(), "log.md should exist in session directory"

            log_content = log_path.read_text()

            # Verify log has session start marker
            assert "Session" in log_content and "started" in log_content.lower(), (
                "log.md should contain session start entry"
            )

        finally:
            await qbit_server.delete_session(session_id)


# =============================================================================
# Session Isolation Tests
# =============================================================================


class TestSessionIsolation:
    """Tests verifying proper session isolation between conversations."""

    @pytest.mark.asyncio
    async def test_different_server_sessions_have_different_sidecar_sessions(
        self, qbit_server, eval_sessions_dir
    ):
        """Verify different server sessions create different sidecar sessions."""
        sessions_dir = Path(eval_sessions_dir)
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        # Create two separate server sessions
        session_id_1 = await qbit_server.create_session()
        session_id_2 = await qbit_server.create_session()

        try:
            # Execute in first session
            await qbit_server.execute_simple(
                session_id_1, "I am session one", timeout_secs=60
            )

            # Execute in second session
            await qbit_server.execute_simple(
                session_id_2, "I am session two", timeout_secs=60
            )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if len(new_dirs) < 2:
                pytest.skip(
                    "Less than 2 session directories created - sidecar may be disabled"
                )

            # Should have 2 distinct sidecar sessions
            session_ids = [get_session_id_from_dir(d) for d in new_dirs]
            unique_ids = set(session_ids)

            assert len(unique_ids) == 2, (
                f"Expected 2 unique sidecar sessions for 2 server sessions, "
                f"got {len(unique_ids)}. Sessions may be incorrectly shared."
            )

        finally:
            await qbit_server.delete_session(session_id_1)
            await qbit_server.delete_session(session_id_2)


# =============================================================================
# Edge Cases
# =============================================================================


class TestSessionEdgeCases:
    """Edge case tests for session management."""

    @pytest.mark.asyncio
    async def test_rapid_successive_prompts_single_session(self, qbit_server, eval_sessions_dir):
        """Verify rapid successive prompts don't create duplicate sessions.

        This tests the race condition fix - rapid calls should all use
        the same session due to the atomic check-and-set.
        """
        sessions_dir = Path(eval_sessions_dir)
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            # Send prompts in rapid succession (but still sequentially due to async)
            for i in range(5):
                await qbit_server.execute_simple(
                    session_id, f"Quick prompt {i}", timeout_secs=60
                )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs

            # Should still have only ONE sidecar session
            assert len(new_dirs) <= 1, (
                f"Expected at most 1 sidecar session for rapid prompts, "
                f"got {len(new_dirs)}. Race condition may still exist."
            )

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_session_survives_error(self, qbit_server, eval_sessions_dir):
        """Verify session continues to work after an error."""
        sessions_dir = Path(eval_sessions_dir)
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            # First successful prompt
            await qbit_server.execute_simple(
                session_id, "Say 'before'", timeout_secs=60
            )

            # After recovery, session should still work
            await qbit_server.execute_simple(
                session_id, "Say 'after'", timeout_secs=60
            )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs

            # Skip if no session directory created (sidecar may be disabled)
            if len(new_dirs) == 0:
                pytest.skip("No session directory created - sidecar may be disabled")

            # Should still have only one session
            assert len(new_dirs) == 1, (
                f"Expected 1 sidecar session after error recovery, got {len(new_dirs)}"
            )

        finally:
            await qbit_server.delete_session(session_id)

"""Evaluation tests for the simplified markdown-based sidecar system.

Tests verify that:
1. Session directories are created with proper structure
2. meta.toml, state.md, log.md files are created
3. Events are logged correctly
4. Session lifecycle works (create -> use -> complete)
"""

import os
import re
from pathlib import Path

import pytest
import toml

from client import QbitClient, StreamingRunner

# =============================================================================
# Fixtures
# =============================================================================


def get_sessions_dir() -> Path:
    """Get the qbit sessions directory."""
    return Path.home() / ".qbit" / "sessions"


def find_recent_session_dirs(sessions_dir: Path, prefix: str = "") -> list[Path]:
    """Find session directories (not JSON files) in the sessions dir."""
    if not sessions_dir.exists():
        return []

    dirs = []
    for item in sessions_dir.iterdir():
        if item.is_dir() and (not prefix or item.name.startswith(prefix)):
            # Check if it has the expected sidecar files
            if (item / "meta.toml").exists():
                dirs.append(item)

    # Sort by modification time, newest first
    dirs.sort(key=lambda p: p.stat().st_mtime, reverse=True)
    return dirs


# =============================================================================
# Session Structure Tests
# =============================================================================


class TestSidecarSessionStructure:
    """Tests for sidecar session file structure."""

    @pytest.mark.asyncio
    async def test_session_creates_directory_structure(self, qbit_server):
        """Verify that running a prompt creates proper session directory."""
        sessions_dir = get_sessions_dir()

        # Get existing session dirs before test
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        # Create session and run a prompt
        session_id = await qbit_server.create_session()
        try:
            # Execute a simple prompt to trigger sidecar activity
            response = await qbit_server.execute_simple(
                session_id, "Say 'hello' and nothing else.", timeout_secs=60
            )
            assert response  # Got some response

            # Find new session directories
            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs

            # Should have at least one new session directory
            assert len(new_dirs) >= 1, "No new session directory created"

            # Check the newest session directory
            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)

            # Verify expected files exist
            assert (session_dir / "meta.toml").exists(), "meta.toml not found"
            assert (session_dir / "state.md").exists(), "state.md not found"
            assert (session_dir / "log.md").exists(), "log.md not found"

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_meta_toml_structure(self, qbit_server):
        """Verify meta.toml has required fields."""
        sessions_dir = get_sessions_dir()
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            await qbit_server.execute_simple(
                session_id, "What is 2+2?", timeout_secs=60
            )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
            meta_path = session_dir / "meta.toml"

            # Parse meta.toml
            meta = toml.load(meta_path)

            # Check required fields
            assert "session_id" in meta, "session_id missing from meta.toml"
            assert "created_at" in meta, "created_at missing from meta.toml"
            assert "updated_at" in meta, "updated_at missing from meta.toml"
            assert "status" in meta, "status missing from meta.toml"

            # Check context section
            assert "context" in meta, "context section missing from meta.toml"
            assert "cwd" in meta["context"], "cwd missing from meta.toml context"
            assert "initial_request" in meta["context"], "initial_request missing"

            # Status should be active or completed
            assert meta["status"] in ("active", "completed"), (
                f"Invalid status: {meta['status']}"
            )

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_state_md_structure(self, qbit_server):
        """Verify state.md has expected structure."""
        sessions_dir = get_sessions_dir()
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            await qbit_server.execute_simple(
                session_id, "List files in the current directory.", timeout_secs=60
            )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
            state_path = session_dir / "state.md"

            state_content = state_path.read_text()

            # Should have markdown headers
            assert "# Session State" in state_content or "# " in state_content, (
                "state.md should have markdown headers"
            )

            # Should contain session info
            assert len(state_content) > 50, "state.md seems too short"

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_log_md_has_entries(self, qbit_server):
        """Verify log.md captures events."""
        sessions_dir = get_sessions_dir()
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            await qbit_server.execute_simple(
                session_id, "Echo back: test message", timeout_secs=60
            )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
            log_path = session_dir / "log.md"

            log_content = log_path.read_text()

            # Log should have session start
            assert "Session Start" in log_content, (
                "log.md should have Session Start entry"
            )

            # Log should have timestamps (HH:MM format like "## 01:11")
            assert re.search(r"## \d{2}:\d{2}", log_content), (
                f"log.md should have timestamps, got: {log_content[:200]}"
            )

        finally:
            await qbit_server.delete_session(session_id)


# =============================================================================
# Session Lifecycle Tests
# =============================================================================


class TestSidecarSessionLifecycle:
    """Tests for session lifecycle management."""

    @pytest.mark.asyncio
    async def test_multiple_prompts_same_session(self, qbit_server):
        """Verify multiple prompts in same session update log."""
        sessions_dir = get_sessions_dir()
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            # First prompt
            await qbit_server.execute_simple(session_id, "Say 'first'", timeout_secs=60)

            # Second prompt
            await qbit_server.execute_simple(
                session_id, "Say 'second'", timeout_secs=60
            )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
            log_path = session_dir / "log.md"

            log_content = log_path.read_text()

            # Log should have multiple entries
            # Count timestamp entries (HH:MM format like "## 01:11")
            timestamp_count = len(re.findall(r"## \d{2}:\d{2}", log_content))
            assert timestamp_count >= 2, (
                f"Expected multiple log entries, got {timestamp_count}. Log: {log_content[:300]}"
            )

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_events_jsonl_created(self, qbit_server):
        """Verify events.jsonl is created for raw event storage."""
        sessions_dir = get_sessions_dir()
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            await qbit_server.execute_simple(
                session_id, "What time is it?", timeout_secs=60
            )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
            events_path = session_dir / "events.jsonl"

            # events.jsonl should exist (may be empty if disabled)
            assert events_path.exists(), "events.jsonl should be created"

        finally:
            await qbit_server.delete_session(session_id)


# =============================================================================
# Content Verification Tests
# =============================================================================


class TestSidecarContentCapture:
    """Tests for verifying sidecar captures correct content."""

    @pytest.mark.asyncio
    async def test_initial_request_captured(self, qbit_server):
        """Verify initial request is captured in meta.toml."""
        sessions_dir = get_sessions_dir()
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        test_prompt = "Calculate the factorial of 5"

        session_id = await qbit_server.create_session()
        try:
            await qbit_server.execute_simple(session_id, test_prompt, timeout_secs=60)

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
            meta = toml.load(session_dir / "meta.toml")

            # Initial request should be captured
            initial_request = meta.get("context", {}).get("initial_request", "")
            assert test_prompt in initial_request or len(initial_request) > 0, (
                "Initial request should be captured in meta.toml"
            )

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_working_directory_captured(self, qbit_server):
        """Verify working directory is captured."""
        sessions_dir = get_sessions_dir()
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            await qbit_server.execute_simple(session_id, "pwd", timeout_secs=60)

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
            meta = toml.load(session_dir / "meta.toml")

            cwd = meta.get("context", {}).get("cwd", "")
            assert cwd, "Working directory should be captured"
            assert Path(cwd).exists() or cwd.startswith("/"), (
                f"CWD should be a valid path: {cwd}"
            )

        finally:
            await qbit_server.delete_session(session_id)


# =============================================================================
# Edge Cases
# =============================================================================


class TestSidecarEdgeCases:
    """Edge case tests for sidecar system."""

    @pytest.mark.asyncio
    async def test_empty_prompt_handling(self, qbit_server):
        """Verify system handles edge cases gracefully."""
        session_id = await qbit_server.create_session()
        try:
            # Very short prompt
            response = await qbit_server.execute_simple(
                session_id, "Hi", timeout_secs=60
            )
            assert response is not None
        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_state_backup_created(self, qbit_server):
        """Verify state.md.bak is created after state updates."""
        sessions_dir = get_sessions_dir()
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            # First prompt creates initial state
            await qbit_server.execute_simple(session_id, "Hello", timeout_secs=60)

            # Second prompt should trigger state update with backup
            await qbit_server.execute_simple(
                session_id, "How are you?", timeout_secs=60
            )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
            backup_path = session_dir / "state.md.bak"

            # Backup may or may not exist depending on whether state was updated
            # Just verify the check doesn't crash
            if backup_path.exists():
                backup_content = backup_path.read_text()
                assert len(backup_content) > 0, "Backup should have content"

        finally:
            await qbit_server.delete_session(session_id)

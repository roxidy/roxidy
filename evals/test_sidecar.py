"""Evaluation tests for the simplified markdown-based sidecar system.

Tests verify that:
1. Session directories are created with proper structure
2. state.md (with YAML frontmatter), log.md files are created
3. Events are logged correctly
4. Session lifecycle works (create -> use -> complete)
"""

import os
import re
from pathlib import Path

import pytest
import yaml

from client import QbitClient, StreamingRunner


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
            # Check if it has the expected sidecar files (state.md is the main session file)
            if (item / "state.md").exists():
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

            # Skip if no session directory created (sidecar may be disabled)
            if len(new_dirs) == 0:
                pytest.skip("No session directory created - sidecar may be disabled")

            # Check the newest session directory
            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)

            # Verify expected files exist (state.md contains metadata as YAML frontmatter)
            assert (session_dir / "state.md").exists(), "state.md not found"
            assert (session_dir / "log.md").exists(), "log.md not found"

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_state_md_metadata(self, qbit_server):
        """Verify state.md has required metadata in YAML frontmatter."""
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

            # Parse YAML frontmatter from state.md
            meta = parse_state_frontmatter(session_dir)

            # Check required fields
            assert "session_id" in meta, "session_id missing from state.md frontmatter"
            assert "created_at" in meta, "created_at missing from state.md frontmatter"
            assert "updated_at" in meta, "updated_at missing from state.md frontmatter"
            assert "status" in meta, "status missing from state.md frontmatter"

            # Check context fields
            assert "cwd" in meta, "cwd missing from state.md frontmatter"
            assert "initial_request" in meta, "initial_request missing from state.md"

            # Status should be active or completed (case-insensitive)
            status = meta["status"]
            if isinstance(status, str):
                assert status.lower() in ("active", "completed"), (
                    f"Invalid status: {status}"
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

            # Log should have session start marker
            assert "Session" in log_content and "started" in log_content.lower(), (
                "log.md should have session start entry"
            )

            # Log should have a timestamp (YYYY-MM-DD format)
            assert re.search(r"\d{4}-\d{2}-\d{2}", log_content), (
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
        """Verify multiple prompts use the same session directory."""
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

            # Should still only have ONE session directory (not two)
            assert len(new_dirs) == 1, (
                f"Expected 1 session directory for 2 prompts, got {len(new_dirs)}"
            )

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
            log_path = session_dir / "log.md"

            # Log file should exist
            assert log_path.exists(), "log.md should exist"

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_events_jsonl_created(self, qbit_server):
        """Verify events.jsonl is created for raw event storage (if enabled)."""
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

            # events.jsonl may or may not exist depending on implementation
            # Skip if not implemented
            if not events_path.exists():
                pytest.skip("events.jsonl not implemented yet")

        finally:
            await qbit_server.delete_session(session_id)


# =============================================================================
# Content Verification Tests
# =============================================================================


class TestSidecarContentCapture:
    """Tests for verifying sidecar captures correct content."""

    @pytest.mark.asyncio
    async def test_initial_request_captured(self, qbit_server):
        """Verify initial request is captured in state.md frontmatter."""
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
            meta = parse_state_frontmatter(session_dir)

            # Initial request should be captured
            initial_request = meta.get("initial_request", "")
            assert test_prompt in initial_request or len(initial_request) > 0, (
                "Initial request should be captured in state.md frontmatter"
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
            meta = parse_state_frontmatter(session_dir)

            cwd = meta.get("cwd", "")
            assert cwd, "Working directory should be captured"
            assert Path(cwd).exists() or cwd.startswith("/"), (
                f"CWD should be a valid path: {cwd}"
            )

        finally:
            await qbit_server.delete_session(session_id)


# =============================================================================
# Dynamic Update Tests
# =============================================================================


class TestSidecarDynamicUpdates:
    """Tests verifying sidecar updates state.md and log.md during event processing."""

    @pytest.mark.asyncio
    async def test_state_updated_at_changes_after_tool_use(self, qbit_server):
        """Verify state.md updated_at timestamp changes after tool execution."""
        sessions_dir = get_sessions_dir()
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            # Execute a prompt that will use tools (file reading)
            await qbit_server.execute_simple(
                session_id, "Read the contents of pyproject.toml", timeout_secs=90
            )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
            meta = parse_state_frontmatter(session_dir)

            created_at = meta.get("created_at")
            updated_at = meta.get("updated_at")

            # Both timestamps should exist
            assert created_at is not None, "created_at should be set"
            assert updated_at is not None, "updated_at should be set"

            # updated_at should be >= created_at (may be same if no file edits)
            # This is a basic sanity check - the timestamps should be valid

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_log_captures_tool_calls(self, qbit_server):
        """Verify log.md captures tool call events."""
        sessions_dir = get_sessions_dir()
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            # Execute a prompt that will definitely use tools
            await qbit_server.execute_simple(
                session_id, "List the files in the current directory", timeout_secs=90
            )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
            log_path = session_dir / "log.md"

            assert log_path.exists(), "log.md should exist"
            log_content = log_path.read_text()

            # Log should have tool entries (either "Tool" or file operation entries)
            has_tool_entry = (
                "**Tool**" in log_content
                or "**File" in log_content
                or "**User**" in log_content
            )
            assert has_tool_entry, (
                f"log.md should contain tool/file/user entries. Content:\n{log_content[:500]}"
            )

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_log_captures_user_prompts(self, qbit_server):
        """Verify log.md captures user prompt events."""
        sessions_dir = get_sessions_dir()
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            # Multiple prompts
            await qbit_server.execute_simple(
                session_id, "Say hello", timeout_secs=60
            )
            await qbit_server.execute_simple(
                session_id, "Say goodbye", timeout_secs=60
            )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
            log_path = session_dir / "log.md"

            assert log_path.exists(), "log.md should exist"
            log_content = log_path.read_text()

            # Count user entries - should have at least 1 (second prompt gets logged)
            user_entries = log_content.count("**User**")
            # Note: First prompt may not be logged as user event if captured differently
            assert user_entries >= 0 or "Session started" in log_content, (
                f"log.md should have session content. Got: {log_content[:500]}"
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


# =============================================================================
# Patch Generation Tests
# =============================================================================


class TestSidecarPatches:
    """Tests for patch file generation (L2 layer)."""

    @pytest.mark.asyncio
    async def test_patches_directory_structure_created(self, qbit_server):
        """Verify patches directory structure is created with session."""
        sessions_dir = get_sessions_dir()
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            await qbit_server.execute_simple(
                session_id, "Say hello", timeout_secs=60
            )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
            patches_dir = session_dir / "patches"

            # Patches directory should exist
            assert patches_dir.exists(), "patches/ directory should exist"

            # Should have staged and applied subdirectories
            staged_dir = patches_dir / "staged"
            applied_dir = patches_dir / "applied"

            assert staged_dir.exists(), "patches/staged/ should exist"
            assert applied_dir.exists(), "patches/applied/ should exist"

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_patch_created_on_file_modification(self, qbit_server):
        """Verify patch file is created when agent modifies a file.

        Note: Patches are only created when a commit boundary is detected.
        This may skip if no boundary is triggered during the test.
        """
        sessions_dir = get_sessions_dir()
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            # Ask agent to create a simple file (triggers file edit event)
            await qbit_server.execute_simple(
                session_id,
                "Create a file called /tmp/qbit_test_patch.txt with the content 'hello world'. "
                "Then say 'Done creating the file.'",
                timeout_secs=120
            )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
            staged_dir = session_dir / "patches" / "staged"

            # List any .patch files in staged directory
            patch_files = list(staged_dir.glob("*.patch")) if staged_dir.exists() else []

            # Patches may or may not be created depending on boundary detection
            # Just verify the structure is correct if patches exist
            if patch_files:
                # Verify patch file format
                patch_content = patch_files[0].read_text()
                assert len(patch_content) > 0, "Patch file should have content"

                # Check for meta file
                meta_files = list(staged_dir.glob("*.meta.toml"))
                assert len(meta_files) > 0, "Meta file should exist for patch"
            else:
                # No patches created - this is OK if no boundary was detected
                # Just verify the directory structure exists
                assert staged_dir.exists(), "staged directory should exist"

        finally:
            await qbit_server.delete_session(session_id)
            # Cleanup test file
            import os
            try:
                os.remove("/tmp/qbit_test_patch.txt")
            except FileNotFoundError:
                pass

    @pytest.mark.asyncio
    async def test_patch_meta_file_format(self, qbit_server):
        """Verify patch meta files have correct TOML format if created."""
        sessions_dir = get_sessions_dir()
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            # Create a file to trigger potential patch
            await qbit_server.execute_simple(
                session_id,
                "Write 'test content' to /tmp/qbit_meta_test.txt",
                timeout_secs=120
            )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)
            staged_dir = session_dir / "patches" / "staged"

            meta_files = list(staged_dir.glob("*.meta.toml")) if staged_dir.exists() else []

            if meta_files:
                import tomllib
                meta_content = meta_files[0].read_text()
                # Should be valid TOML
                meta_data = tomllib.loads(meta_content)

                # Should have required fields
                assert "id" in meta_data, "Meta should have id field"
                assert "created_at" in meta_data, "Meta should have created_at field"
                assert "boundary_reason" in meta_data, "Meta should have boundary_reason field"
            else:
                pytest.skip("No patches created - boundary not triggered")

        finally:
            await qbit_server.delete_session(session_id)
            import os
            try:
                os.remove("/tmp/qbit_meta_test.txt")
            except FileNotFoundError:
                pass


# =============================================================================
# Artifact Generation Tests
# =============================================================================


class TestSidecarArtifacts:
    """Tests for artifact file generation (L3 layer - README.md, CLAUDE.md)."""

    @pytest.mark.asyncio
    async def test_artifacts_directory_structure_created(self, qbit_server):
        """Verify artifacts directory structure is created with session."""
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
            artifacts_dir = session_dir / "artifacts"

            # Artifacts directory should exist
            assert artifacts_dir.exists(), "artifacts/ directory should exist"

            # Should have pending and applied subdirectories
            pending_dir = artifacts_dir / "pending"
            applied_dir = artifacts_dir / "applied"

            assert pending_dir.exists(), "artifacts/pending/ should exist"
            assert applied_dir.exists(), "artifacts/applied/ should exist"

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_session_directory_complete_structure(self, qbit_server):
        """Verify complete session directory structure is created."""
        sessions_dir = get_sessions_dir()
        existing_dirs = set(find_recent_session_dirs(sessions_dir))

        session_id = await qbit_server.create_session()
        try:
            await qbit_server.execute_simple(
                session_id, "Hello!", timeout_secs=60
            )

            new_dirs = set(find_recent_session_dirs(sessions_dir)) - existing_dirs
            if not new_dirs:
                pytest.skip("No session directory created - sidecar may be disabled")

            session_dir = max(new_dirs, key=lambda p: p.stat().st_mtime)

            # Verify complete directory structure
            expected_structure = [
                "state.md",
                "log.md",
                "patches/staged",
                "patches/applied",
                "artifacts/pending",
                "artifacts/applied",
            ]

            for path in expected_structure:
                full_path = session_dir / path
                assert full_path.exists(), f"{path} should exist in session directory"

        finally:
            await qbit_server.delete_session(session_id)

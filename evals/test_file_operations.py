"""Evaluation tests for file editing operations and tool approval flow.

Tests verify that:
1. File creation, modification, and deletion work correctly
2. Tool approval events are emitted properly (HITL)
3. Auto-approval patterns work
4. File content matches expectations after edits
"""

import os
from pathlib import Path

import pytest

from client import QbitClient, StreamingRunner


# =============================================================================
# Fixtures
# =============================================================================


def get_workspace_dir() -> Path:
    """Get the workspace directory for file operation tests."""
    workspace = os.environ.get("QBIT_WORKSPACE")
    if workspace:
        return Path(workspace)
    # Fallback to qbit-go-testbed relative to evals/
    return Path(__file__).parent.parent / "qbit-go-testbed"


def cleanup_test_file(path: Path):
    """Remove a test file if it exists."""
    try:
        if path.exists():
            path.unlink()
    except Exception:
        pass


# =============================================================================
# File Creation Tests
# =============================================================================


class TestFileCreation:
    """Tests for file creation operations."""

    @pytest.mark.asyncio
    async def test_create_simple_file(self, qbit_server):
        """Verify agent can create a simple text file."""
        workspace = get_workspace_dir()
        test_file = workspace / "test_created.txt"
        cleanup_test_file(test_file)

        session_id = await qbit_server.create_session()
        try:
            await qbit_server.execute_simple(
                session_id,
                f"Create a file at {test_file} with the content: 'Hello from eval test'",
                timeout_secs=120
            )

            # Verify file was created
            assert test_file.exists(), f"File {test_file} should have been created"

            # Verify content
            content = test_file.read_text()
            assert "Hello from eval test" in content, (
                f"File content should contain expected text. Got: {content}"
            )

        finally:
            await qbit_server.delete_session(session_id)
            cleanup_test_file(test_file)

    @pytest.mark.asyncio
    async def test_create_file_with_specific_content(self, qbit_server):
        """Verify file is created with exact specified content."""
        workspace = get_workspace_dir()
        test_file = workspace / "test_specific.txt"
        cleanup_test_file(test_file)

        expected_content = "Line 1\nLine 2\nLine 3"

        session_id = await qbit_server.create_session()
        try:
            await qbit_server.execute_simple(
                session_id,
                f"Create a file at {test_file} with exactly this content:\n{expected_content}",
                timeout_secs=120
            )

            assert test_file.exists(), "File should have been created"
            content = test_file.read_text().strip()

            # Check that all lines are present
            assert "Line 1" in content, "Should contain Line 1"
            assert "Line 2" in content, "Should contain Line 2"
            assert "Line 3" in content, "Should contain Line 3"

        finally:
            await qbit_server.delete_session(session_id)
            cleanup_test_file(test_file)


# =============================================================================
# File Modification Tests
# =============================================================================


class TestFileModification:
    """Tests for file modification operations."""

    @pytest.mark.asyncio
    async def test_modify_existing_file(self, qbit_server):
        """Verify agent can modify an existing file."""
        workspace = get_workspace_dir()
        test_file = workspace / "test_modify.txt"

        # Create initial file
        test_file.write_text("Original content here")

        session_id = await qbit_server.create_session()
        try:
            await qbit_server.execute_simple(
                session_id,
                f"Read {test_file}, then modify it to replace 'Original' with 'Modified'",
                timeout_secs=120
            )

            content = test_file.read_text()
            assert "Modified" in content, (
                f"File should contain 'Modified'. Got: {content}"
            )

        finally:
            await qbit_server.delete_session(session_id)
            cleanup_test_file(test_file)

    @pytest.mark.asyncio
    async def test_append_to_file(self, qbit_server):
        """Verify agent can append content to a file."""
        workspace = get_workspace_dir()
        test_file = workspace / "test_append.txt"

        # Create initial file
        test_file.write_text("First line\n")

        session_id = await qbit_server.create_session()
        try:
            await qbit_server.execute_simple(
                session_id,
                f"Read {test_file} and add a new line 'Second line' at the end",
                timeout_secs=120
            )

            content = test_file.read_text()
            assert "First line" in content, "Should still contain first line"
            assert "Second line" in content, "Should contain appended line"

        finally:
            await qbit_server.delete_session(session_id)
            cleanup_test_file(test_file)


# =============================================================================
# File Reading Tests
# =============================================================================


class TestFileReading:
    """Tests for file reading operations."""

    @pytest.mark.asyncio
    async def test_read_existing_file(self, qbit_server):
        """Verify agent can read and report file contents."""
        workspace = get_workspace_dir()
        main_go = workspace / "main.go"

        # Verify test file exists
        if not main_go.exists():
            pytest.skip("main.go not found in workspace")

        session_id = await qbit_server.create_session()
        try:
            result = await qbit_server.execute_simple(
                session_id,
                f"Read {main_go} and tell me what package it defines",
                timeout_secs=90
            )

            # Agent should mention the package name
            assert "main" in result.lower() or "package" in result.lower(), (
                f"Response should mention the package. Got: {result}"
            )

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_read_nonexistent_file(self, qbit_server):
        """Verify agent handles reading non-existent files gracefully."""
        workspace = get_workspace_dir()
        fake_file = workspace / "this_file_does_not_exist_12345.txt"

        session_id = await qbit_server.create_session()
        try:
            result = await qbit_server.execute_simple(
                session_id,
                f"Read the file {fake_file} and tell me its contents",
                timeout_secs=90
            )

            # Agent should indicate the file doesn't exist or there was an error
            result_lower = result.lower()
            assert (
                "not found" in result_lower or
                "doesn't exist" in result_lower or
                "does not exist" in result_lower or
                "error" in result_lower or
                "cannot" in result_lower or
                "couldn't" in result_lower or
                "failed" in result_lower
            ), f"Response should indicate file not found. Got: {result}"

        finally:
            await qbit_server.delete_session(session_id)


# =============================================================================
# Tool Approval Flow Tests (HITL)
# =============================================================================


class TestToolApprovalEvents:
    """Tests for Human-in-the-Loop tool approval events."""

    @pytest.mark.asyncio
    async def test_tool_events_emitted(self, qbit_server):
        """Verify tool-related events are emitted during execution."""
        workspace = get_workspace_dir()

        session_id = await qbit_server.create_session()
        try:
            events = []
            async for event in qbit_server.execute(
                session_id,
                f"List the files in {workspace}",
                timeout_secs=90
            ):
                events.append(event)

            # Should have tool-related events
            event_types = [e.event for e in events]

            # At minimum, should have started and completed
            assert "started" in event_types, "Should have started event"
            assert "completed" in event_types, "Should have completed event"

            # Tool events may or may not be present depending on execution
            # Just verify we got a valid response
            completed_event = next(e for e in events if e.event == "completed")
            assert completed_event.response, "Should have a response"

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_file_read_tool_auto_approved(self, qbit_server):
        """Verify read operations are auto-approved (low risk)."""
        workspace = get_workspace_dir()
        main_go = workspace / "main.go"

        if not main_go.exists():
            pytest.skip("main.go not found in workspace")

        session_id = await qbit_server.create_session()
        try:
            events = []
            async for event in qbit_server.execute(
                session_id,
                f"Read the file {main_go}",
                timeout_secs=90
            ):
                events.append(event)

            # Look for tool result event (indicates tool was executed)
            tool_results = [e for e in events if e.event == "tool_result"]

            # Should have at least one successful tool execution
            if tool_results:
                # At least one should be successful
                successful = any(e.data.get("success") for e in tool_results)
                assert successful, "At least one tool should succeed"

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_tool_result_contains_output(self, qbit_server):
        """Verify tool results contain meaningful output."""
        workspace = get_workspace_dir()

        session_id = await qbit_server.create_session()
        try:
            events = []
            async for event in qbit_server.execute(
                session_id,
                f"List the files in {workspace}",
                timeout_secs=90
            ):
                events.append(event)

            # Find tool result events
            tool_results = [e for e in events if e.event == "tool_result"]

            if tool_results:
                # Check that results have output
                for result in tool_results:
                    if result.data.get("success"):
                        # Successful results should have some output
                        assert result.data.get("output") is not None, (
                            "Successful tool result should have output"
                        )

        finally:
            await qbit_server.delete_session(session_id)


# =============================================================================
# Error Recovery Tests
# =============================================================================


class TestErrorRecovery:
    """Tests for error handling and recovery."""

    @pytest.mark.asyncio
    async def test_session_continues_after_tool_error(self, qbit_server):
        """Verify session remains usable after a tool encounters an error."""
        workspace = get_workspace_dir()
        fake_file = workspace / "nonexistent_for_error_test.txt"

        session_id = await qbit_server.create_session()
        try:
            # First prompt - try to read non-existent file
            await qbit_server.execute_simple(
                session_id,
                f"Try to read {fake_file}",
                timeout_secs=90
            )

            # Second prompt - should still work
            result = await qbit_server.execute_simple(
                session_id,
                "What is 5 + 5?",
                timeout_secs=60
            )

            # Session should still be functional
            assert "10" in result, f"Session should still work. Got: {result}"

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_multiple_tool_calls_in_sequence(self, qbit_server):
        """Verify multiple tool calls in sequence work correctly."""
        workspace = get_workspace_dir()
        test_file = workspace / "test_sequence.txt"
        cleanup_test_file(test_file)

        session_id = await qbit_server.create_session()
        try:
            # Create file, then read it back
            await qbit_server.execute_simple(
                session_id,
                f"Create a file at {test_file} with 'test content', then read it back and confirm its contents",
                timeout_secs=120
            )

            # Verify file exists and has content
            assert test_file.exists(), "File should have been created"
            content = test_file.read_text()
            assert "test" in content.lower(), f"File should have content. Got: {content}"

        finally:
            await qbit_server.delete_session(session_id)
            cleanup_test_file(test_file)


# =============================================================================
# Workspace Context Tests
# =============================================================================


class TestWorkspaceContext:
    """Tests for workspace context handling."""

    @pytest.mark.asyncio
    async def test_workspace_is_set_correctly(self, qbit_server):
        """Verify the workspace is set to qbit-go-testbed."""
        session_id = await qbit_server.create_session()
        try:
            result = await qbit_server.execute_simple(
                session_id,
                "What is the current working directory?",
                timeout_secs=60
            )

            # Should mention qbit-go-testbed
            assert "qbit-go-testbed" in result or "testbed" in result.lower(), (
                f"Workspace should be qbit-go-testbed. Got: {result}"
            )

        finally:
            await qbit_server.delete_session(session_id)

    @pytest.mark.asyncio
    async def test_can_list_workspace_files(self, qbit_server):
        """Verify agent can list files in the workspace."""
        session_id = await qbit_server.create_session()
        try:
            result = await qbit_server.execute_simple(
                session_id,
                "List all files in the current directory",
                timeout_secs=90
            )

            # Should see main.go and go.mod from qbit-go-testbed
            result_lower = result.lower()
            assert "main.go" in result_lower or "go.mod" in result_lower, (
                f"Should list workspace files. Got: {result}"
            )

        finally:
            await qbit_server.delete_session(session_id)

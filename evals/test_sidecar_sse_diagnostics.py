"""Diagnostic tests for sidecar patch/artifact generation using SSE server.

This module uses the existing eval client infrastructure and reads server logs
after execution to diagnose the patch generation flow.

Run:
    RUST_LOG=debug RUN_API_TESTS=1 pytest test_sidecar_sse_diagnostics.py -v -s

The tests check each stage of the event flow:
1. Event capture (CaptureContext)
2. Event forwarding (SidecarState)
3. File tracking (Processor)
4. Patch generation (on session end)
"""

import os
import re
import subprocess
import tempfile
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

import httpx
import pytest

from client import QbitClient, StreamingRunner
from config import get_binary_path
from conftest import get_eval_sessions_dir


@dataclass
class SidecarDiagnostics:
    """Collected diagnostics from sidecar log analysis."""

    # Capture stage
    tool_requests_captured: int = 0
    tool_results_captured: int = 0
    files_extracted: list[str] = field(default_factory=list)
    files_extraction_failures: list[str] = field(default_factory=list)

    # State forwarding stage
    events_forwarded: int = 0
    events_with_files_modified: int = 0
    processor_missing_warnings: int = 0

    # Processor stage
    file_edits_tracked: int = 0
    tool_calls_tracked: int = 0
    tool_calls_empty_files: int = 0
    file_tracker_counts: list[int] = field(default_factory=list)

    # Session end stage
    end_session_received: bool = False
    generate_patches_enabled: bool = False
    files_at_session_end: int = 0
    patch_generation_attempted: bool = False
    patch_created: bool = False
    patch_id: Optional[int] = None

    # Errors
    errors: list[str] = field(default_factory=list)

    def summary(self) -> str:
        """Generate a diagnostic summary."""
        lines = [
            "=" * 60,
            "SIDECAR DIAGNOSTICS SUMMARY",
            "=" * 60,
            "",
            "1. CAPTURE STAGE (CaptureContext)",
            f"   Tool requests captured: {self.tool_requests_captured}",
            f"   Tool results captured: {self.tool_results_captured}",
            f"   Files extracted: {len(self.files_extracted)}",
        ]

        if self.files_extracted:
            for f in self.files_extracted[:5]:
                lines.append(f"     - {f}")
            if len(self.files_extracted) > 5:
                lines.append(f"     ... and {len(self.files_extracted) - 5} more")

        if self.files_extraction_failures:
            lines.append(f"   ⚠ Extraction failures: {len(self.files_extraction_failures)}")
            for f in self.files_extraction_failures[:3]:
                lines.append(f"     - {f[:100]}...")

        lines.extend([
            "",
            "2. STATE FORWARDING STAGE (SidecarState)",
            f"   Events forwarded to processor: {self.events_forwarded}",
            f"   Events with files_modified > 0: {self.events_with_files_modified}",
        ])

        if self.processor_missing_warnings > 0:
            lines.append(f"   ⚠ Processor missing warnings: {self.processor_missing_warnings}")

        lines.extend([
            "",
            "3. PROCESSOR STAGE (track_file_changes)",
            f"   FileEdit events tracked: {self.file_edits_tracked}",
            f"   ToolCall events tracked: {self.tool_calls_tracked}",
            f"   ToolCall events with empty files_modified: {self.tool_calls_empty_files}",
        ])

        if self.file_tracker_counts:
            lines.append(f"   File tracker progression: {' -> '.join(map(str, self.file_tracker_counts[-10:]))}")

        lines.extend([
            "",
            "4. SESSION END STAGE",
            f"   EndSession received: {'✓' if self.end_session_received else '✗'}",
            f"   generate_patches enabled: {'✓' if self.generate_patches_enabled else '✗'}",
            f"   Files tracked at end: {self.files_at_session_end}",
            f"   Patch generation attempted: {'✓' if self.patch_generation_attempted else '✗'}",
            f"   Patch created: {'✓' if self.patch_created else '✗'}",
        ])

        if self.patch_id is not None:
            lines.append(f"   Patch ID: {self.patch_id}")

        if self.errors:
            lines.extend([
                "",
                "ERRORS:",
            ])
            for err in self.errors[:10]:
                lines.append(f"   - {err}")

        lines.extend([
            "",
            "=" * 60,
            "DIAGNOSIS:",
        ])

        # Provide diagnosis
        if self.tool_results_captured == 0:
            lines.append("   ⚠ No tool results captured - check if tools are being executed")
        elif len(self.files_extracted) == 0 and self.files_extraction_failures:
            lines.append("   ⚠ Files not being extracted from tool args")
            lines.append("     Check parameter names in vtcode-core tools")
        elif self.events_forwarded == 0:
            lines.append("   ⚠ Events not being forwarded to processor")
            lines.append("     Check if sidecar is enabled and processor is initialized")
        elif self.processor_missing_warnings > 0:
            lines.append("   ⚠ Processor not available when events are captured")
            lines.append("     Check sidecar initialization timing")
        elif self.file_tracker_counts and max(self.file_tracker_counts) == 0:
            lines.append("   ⚠ Files not being tracked in processor")
            lines.append("     Check files_modified field population")
        elif not self.end_session_received:
            lines.append("   ⚠ Session end not received by processor")
            lines.append("     Check session lifecycle management")
        elif not self.generate_patches_enabled:
            lines.append("   ⚠ Patch generation is disabled in config")
        elif self.files_at_session_end == 0:
            lines.append("   ⚠ No files in tracker at session end")
            lines.append("     Files may be cleared prematurely or not tracked")
        elif not self.patch_generation_attempted:
            lines.append("   ⚠ Patch generation not attempted despite tracked files")
        elif not self.patch_created:
            lines.append("   ⚠ Patch generation attempted but failed")
            lines.append("     Check git repository state and file paths")
        else:
            lines.append("   ✓ Patch generation appears to be working!")

        lines.append("=" * 60)

        return "\n".join(lines)


def parse_logs(log_output: str) -> SidecarDiagnostics:
    """Parse sidecar log messages and extract diagnostics."""
    diag = SidecarDiagnostics()

    for line in log_output.split("\n"):
        # Capture stage
        if "[sidecar-capture] Tool request:" in line:
            diag.tool_requests_captured += 1

        if "[sidecar-capture] Tool result:" in line:
            diag.tool_results_captured += 1

        if "[sidecar-capture] Extracted" in line and "files for write tool" in line:
            match = re.search(r"Extracted (\d+) files.*?: \[(.*?)\]", line)
            if match:
                count = int(match.group(1))
                files_str = match.group(2)
                if files_str:
                    diag.files_extracted.extend(f.strip().strip('"') for f in files_str.split(",") if f.strip())

        if "[sidecar-capture] No files extracted for write tool" in line:
            diag.files_extraction_failures.append(line)

        # State forwarding stage
        if "[sidecar-state] Capturing event:" in line:
            diag.events_forwarded += 1
            match = re.search(r"files_modified: (\d+)", line)
            if match and int(match.group(1)) > 0:
                diag.events_with_files_modified += 1

        if "[sidecar-state] No processor available" in line:
            diag.processor_missing_warnings += 1

        # Processor stage
        if "[processor] FileEdit event for path:" in line:
            diag.file_edits_tracked += 1

        if "[processor] ToolCall" in line and "tracking" in line and "file(s)" in line:
            diag.tool_calls_tracked += 1

        if "[processor] ToolCall" in line and "files_modified is empty" in line:
            diag.tool_calls_empty_files += 1

        if "[processor] File tracker now has" in line:
            match = re.search(r"has (\d+) file", line)
            if match:
                diag.file_tracker_counts.append(int(match.group(1)))

        # Session end stage
        if "[processor] EndSession task received" in line:
            diag.end_session_received = True

        if "[processor] Session" in line and "ending:" in line:
            match = re.search(r"generate_patches=(\w+)", line)
            if match:
                diag.generate_patches_enabled = match.group(1).lower() == "true"
            match = re.search(r"file_tracker has (\d+) file", line)
            if match:
                diag.files_at_session_end = int(match.group(1))

        if "[processor] Generating patch for session" in line:
            diag.patch_generation_attempted = True

        if "[processor] Patch" in line and "created successfully" in line:
            diag.patch_created = True
            match = re.search(r"Patch (\d+) created", line)
            if match:
                diag.patch_id = int(match.group(1))

        if "generate_patch called for session" in line:
            diag.patch_generation_attempted = True

        # Errors
        if "error" in line.lower() and "[processor]" in line:
            diag.errors.append(line.strip())

    return diag


# =============================================================================
# Server with Log Capture
# =============================================================================

@pytest.fixture(scope="module")
def server_with_logs(tmp_path_factory):
    """Start server with debug logging captured to a file.

    This fixture starts the qbit-cli server with RUST_LOG=debug and
    captures stderr to a log file for post-test analysis.
    """
    binary_path = get_binary_path()
    if not os.path.exists(binary_path):
        pytest.skip(f"Binary not found at {binary_path}. Run: just build-server")

    # Create log file in a temp directory
    log_dir = tmp_path_factory.mktemp("logs")
    log_file = log_dir / "server_debug.log"

    # Create workspace for file operations
    workspace = tmp_path_factory.mktemp("workspace")

    # Initialize git in workspace
    subprocess.run(["git", "init"], cwd=workspace, capture_output=True, check=True)
    subprocess.run(["git", "config", "user.email", "test@test.com"], cwd=workspace, capture_output=True)
    subprocess.run(["git", "config", "user.name", "Test"], cwd=workspace, capture_output=True)
    (workspace / "README.md").write_text("# Test Project\n")
    subprocess.run(["git", "add", "."], cwd=workspace, capture_output=True)
    subprocess.run(["git", "commit", "-m", "Initial commit"], cwd=workspace, capture_output=True)

    # Build environment with debug logging
    server_env = os.environ.copy()
    server_env["RUST_LOG"] = "debug"
    server_env["QBIT_WORKSPACE"] = str(workspace)
    # Use temp directory for sessions to prevent polluting ~/.qbit/sessions
    server_env["VT_SESSION_DIR"] = get_eval_sessions_dir()

    # Open log file for stderr capture
    log_handle = open(log_file, "w")

    # Start server with stderr going to log file
    proc = subprocess.Popen(
        [str(binary_path), "--server", "--port", "0"],
        stdout=subprocess.PIPE,
        stderr=log_handle,
        text=True,
        env=server_env,
    )

    try:
        # Read address from stdout
        line = proc.stdout.readline()
        match = re.search(r"http://([^:]+):(\d+)", line)
        if not match:
            proc.terminate()
            log_handle.close()
            pytest.fail(f"Could not parse server address from: {line}")

        host, port = match.groups()
        base_url = f"http://{host}:{port}"

        # Wait for server to be ready
        for _ in range(30):
            try:
                resp = httpx.get(f"{base_url}/health", timeout=1.0)
                if resp.status_code == 200:
                    break
            except httpx.RequestError:
                pass
            time.sleep(0.5)
        else:
            proc.terminate()
            log_handle.close()
            pytest.fail("Server did not become ready within 15 seconds")

        yield {
            "base_url": base_url,
            "log_file": log_file,
            "workspace": workspace,
            "process": proc,
        }

    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        log_handle.close()


@pytest.fixture
async def diag_client(server_with_logs):
    """Async client connected to the diagnostic server."""
    async with QbitClient(server_with_logs["base_url"]) as client:
        yield client, server_with_logs


# =============================================================================
# Diagnostic Tests
# =============================================================================

class TestSidecarSSEDiagnostics:
    """Diagnostic tests for sidecar patch generation using SSE server."""

    @pytest.mark.asyncio
    @pytest.mark.requires_api
    async def test_diagnose_patch_generation(self, diag_client):
        """Run a file write operation and diagnose patch generation."""
        client, server_info = diag_client
        workspace = server_info["workspace"]
        log_file = server_info["log_file"]

        # Create session
        session_id = await client.create_session(workspace=str(workspace))
        print(f"\n[Diag] Session: {session_id}")
        print(f"[Diag] Workspace: {workspace}")

        try:
            # Execute file write prompt
            prompt = (
                "Create a new file called 'test.txt' in the current directory "
                "with the content 'Hello from test'. Just create the file, "
                "don't explain anything."
            )

            print(f"[Diag] Executing prompt...")
            response = await client.execute_simple(session_id, prompt, timeout_secs=120)
            print(f"[Diag] Response: {response[:200]}...")

        finally:
            # Delete session to trigger EndSession
            print("[Diag] Deleting session to trigger EndSession...")
            await client.delete_session(session_id)

        # Give async processor time to complete
        import asyncio
        await asyncio.sleep(2)

        # Read and parse logs
        logs = log_file.read_text()
        diag = parse_logs(logs)

        # Print summary
        print("\n" + diag.summary())

        # Check if file was created
        test_file = workspace / "test.txt"
        if test_file.exists():
            print(f"\n✓ File created: {test_file}")
            print(f"  Content: {test_file.read_text()[:100]}")
        else:
            print(f"\n✗ File not created: {test_file}")

        # Save full logs
        print(f"\nFull logs at: {log_file}")

    @pytest.mark.asyncio
    @pytest.mark.requires_api
    async def test_diagnose_multiple_files(self, diag_client):
        """Test with multiple file operations to trigger boundary detection."""
        client, server_info = diag_client
        workspace = server_info["workspace"]
        log_file = server_info["log_file"]

        session_id = await client.create_session(workspace=str(workspace))
        print(f"\n[Diag] Session: {session_id}")

        try:
            # Create multiple files to exceed min_events threshold (3)
            prompt = (
                "Create three files: 'file1.txt' with 'content 1', "
                "'file2.txt' with 'content 2', and 'file3.txt' with 'content 3'. "
                "Create all three files. Don't explain, just do it."
            )

            print(f"[Diag] Executing prompt...")
            response = await client.execute_simple(session_id, prompt, timeout_secs=180)
            print(f"[Diag] Response: {response[:200]}...")

        finally:
            await client.delete_session(session_id)

        import asyncio
        await asyncio.sleep(2)

        logs = log_file.read_text()
        diag = parse_logs(logs)

        print("\n" + diag.summary())

        # Check which files were created
        files_created = []
        for i in range(1, 4):
            f = workspace / f"file{i}.txt"
            if f.exists():
                files_created.append(f.name)

        print(f"\nFiles created in workspace: {files_created}")
        print(f"Full logs at: {log_file}")

    @pytest.mark.asyncio
    @pytest.mark.requires_api
    async def test_diagnose_edit_operation(self, diag_client):
        """Test with an edit operation to check diff tracking."""
        client, server_info = diag_client
        workspace = server_info["workspace"]
        log_file = server_info["log_file"]

        # First create a file to edit
        test_file = workspace / "edit_target.txt"
        test_file.write_text("Line 1\nLine 2\nLine 3\n")
        subprocess.run(["git", "add", "."], cwd=workspace, capture_output=True)
        subprocess.run(
            ["git", "commit", "-m", "Add edit target"],
            cwd=workspace,
            capture_output=True,
        )

        session_id = await client.create_session(workspace=str(workspace))
        print(f"\n[Diag] Session: {session_id}")

        try:
            prompt = (
                "Edit the file 'edit_target.txt' and change 'Line 2' to 'Modified Line 2'. "
                "Don't explain, just make the edit."
            )

            print(f"[Diag] Executing prompt...")
            response = await client.execute_simple(session_id, prompt, timeout_secs=120)
            print(f"[Diag] Response: {response[:200]}...")

        finally:
            await client.delete_session(session_id)

        import asyncio
        await asyncio.sleep(2)

        logs = log_file.read_text()
        diag = parse_logs(logs)

        print("\n" + diag.summary())

        # Check if file was modified
        if test_file.exists():
            content = test_file.read_text()
            print(f"\nFile content after edit:\n{content}")

        print(f"Full logs at: {log_file}")


# =============================================================================
# Simple Tests Using Existing Infrastructure
# =============================================================================

class TestSidecarWithExistingRunner:
    """Tests using the existing runner fixture.

    These tests use the standard eval infrastructure and check logs
    via environment variables. Set RUST_LOG=debug when running.
    """

    @pytest.mark.asyncio
    @pytest.mark.requires_api
    async def test_file_creation_events(self, runner: StreamingRunner):
        """Test that file creation triggers sidecar events."""
        prompt = "Create a file called 'hello.txt' with content 'Hello World'"

        result = await runner.run(prompt)

        print(f"\n[Test] Success: {result.success}")
        print(f"[Test] Response: {result.response[:200]}...")
        print(f"[Test] Tool calls: {len(result.tool_calls)}")

        for tc in result.tool_calls:
            print(f"  - {tc.get('name', 'unknown')}")

        # The actual sidecar behavior can be verified by checking RUST_LOG output
        # or by checking for patches in the session directory

    @pytest.mark.asyncio
    @pytest.mark.requires_api
    async def test_multiple_tool_calls(self, runner: StreamingRunner):
        """Test multiple tool calls to trigger commit boundary."""
        prompt = (
            "Create three files: test1.txt, test2.txt, and test3.txt. "
            "Each should contain 'Test content N' where N is the file number."
        )

        result = await runner.run(prompt)

        print(f"\n[Test] Success: {result.success}")
        print(f"[Test] Tool calls: {len(result.tool_calls)}")

        for tc in result.tool_calls:
            print(f"  - {tc.get('name', 'unknown')}: {tc.get('input', {}).get('path', 'N/A')}")


if __name__ == "__main__":
    pytest.main([__file__, "-v", "-s"])

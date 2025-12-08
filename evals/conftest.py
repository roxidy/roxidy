"""Pytest configuration and fixtures for CLI integration tests."""

import json
import os
import subprocess
import tempfile
import tomllib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Generator

import pytest
from deepeval.models import GPTModel


# =============================================================================
# JSON Output Parsing Utilities
# =============================================================================


@dataclass
class CliJsonEvent:
    """Structured representation of a CLI JSON event.

    The CLI outputs events in JSONL format (one JSON object per line).
    Each event has an 'event' type, 'timestamp', and event-specific fields.
    """

    event: str
    timestamp: int
    data: dict = field(default_factory=dict)

    @classmethod
    def from_dict(cls, d: dict) -> "CliJsonEvent":
        """Create a CliJsonEvent from a parsed JSON dict."""
        event = d.pop("event", "unknown")
        timestamp = d.pop("timestamp", 0)
        return cls(event=event, timestamp=timestamp, data=d)

    def __getitem__(self, key: str):
        """Allow dict-like access to data fields."""
        return self.data.get(key)

    def get(self, key: str, default=None):
        """Get a data field with optional default."""
        return self.data.get(key, default)


def parse_json_output(stdout: str) -> list[CliJsonEvent]:
    """Parse JSONL output from CLI into structured events.

    Args:
        stdout: Raw stdout from CLI with --json flag

    Returns:
        List of CliJsonEvent objects in order received

    Raises:
        ValueError: If any line contains invalid JSON
    """
    events = []
    for line in stdout.strip().split("\n"):
        line = line.strip()
        if not line:
            continue
        try:
            data = json.loads(line)
            events.append(CliJsonEvent.from_dict(data))
        except json.JSONDecodeError as e:
            raise ValueError(f"Invalid JSON in CLI output: {line!r}") from e
    return events


def get_response_from_json(events: list[CliJsonEvent]) -> str:
    """Extract the final response text from JSON events.

    Looks for a 'completed' event and returns its 'response' field.
    Falls back to accumulating text_delta events if no completed event.

    Args:
        events: List of parsed CLI events

    Returns:
        The final response string, or empty string if not found
    """
    # First try to find completed event
    for event in reversed(events):
        if event.event == "completed":
            return event.get("response", "")

    # Fall back to accumulated text from last text_delta
    for event in reversed(events):
        if event.event == "text_delta":
            return event.get("accumulated", "")

    return ""


def get_tool_events(events: list[CliJsonEvent]) -> list[CliJsonEvent]:
    """Extract tool-related events from the event stream.

    Args:
        events: List of parsed CLI events

    Returns:
        List of events with type: tool_call, tool_result, tool_approval, etc.
    """
    tool_event_types = {
        "tool_call",
        "tool_result",
        "tool_approval",
        "tool_auto_approved",
        "tool_denied",
    }
    return [e for e in events if e.event in tool_event_types]


def get_tool_calls(events: list[CliJsonEvent]) -> list[CliJsonEvent]:
    """Extract only tool_call events."""
    return [e for e in events if e.event == "tool_call"]


def get_tool_results(events: list[CliJsonEvent]) -> list[CliJsonEvent]:
    """Extract only tool_result events."""
    return [e for e in events if e.event == "tool_result"]


def filter_events_by_type(events: list[CliJsonEvent], event_type: str) -> list[CliJsonEvent]:
    """Filter events by their type."""
    return [e for e in events if e.event == event_type]


@dataclass
class JsonRunResult:
    """Result of running CLI in JSON mode.

    Contains both the parsed events and convenience accessors.
    """

    events: list[CliJsonEvent]
    response: str
    returncode: int
    stderr: str

    @property
    def tool_calls(self) -> list[CliJsonEvent]:
        """Get all tool_call events."""
        return get_tool_calls(self.events)

    @property
    def tool_results(self) -> list[CliJsonEvent]:
        """Get all tool_result events."""
        return get_tool_results(self.events)

    @property
    def completed_event(self) -> CliJsonEvent | None:
        """Get the completed event if present."""
        for event in reversed(self.events):
            if event.event == "completed":
                return event
        return None

    @property
    def error_event(self) -> CliJsonEvent | None:
        """Get the error event if present."""
        for event in reversed(self.events):
            if event.event == "error":
                return event
        return None

    @property
    def tokens_used(self) -> int | None:
        """Get tokens_used from completed event."""
        if completed := self.completed_event:
            return completed.get("tokens_used")
        return None

    @property
    def duration_ms(self) -> int | None:
        """Get duration_ms from completed event."""
        if completed := self.completed_event:
            return completed.get("duration_ms")
        return None

    def has_tool(self, tool_name: str) -> bool:
        """Check if a specific tool was called."""
        return any(tc.get("tool_name") == tool_name for tc in self.tool_calls)

    def get_tool_output(self, tool_name: str) -> str | None:
        """Get the output of a specific tool call."""
        for tr in self.tool_results:
            if tr.get("tool_name") == tool_name:
                return tr.get("output")
        return None


def load_settings() -> dict:
    """Load settings from settings.toml."""
    settings_path = "~/.qbit/settings.toml"
    if not os.path.exists(settings_path):
        return {"eval": {"model": "gpt-4o-mini", "temperature": 0}}
    with open(settings_path, "rb") as f:
        return tomllib.load(f)


def create_eval_model():
    """Create the OpenAI evaluation model from settings."""
    settings = load_settings()
    eval_settings = settings.get("eval", {})

    # Set API key from settings if provided (overrides env var)
    if api_key := eval_settings.get("api_key"):
        os.environ["OPENAI_API_KEY"] = api_key

    return GPTModel(
        model=eval_settings.get("model", "gpt-4o-mini"),
        temperature=eval_settings.get("temperature", 0),
    )


def get_last_response(stdout: str) -> str:
    """Extract the last response from batch output.

    Batch mode outputs each response on a separate line.
    This returns only the final response for evaluation.
    """
    lines = [line for line in stdout.strip().split("\n") if line.strip()]
    return lines[-1] if lines else ""


def get_cli_path() -> str:
    """Get the path to the qbit-cli binary."""
    # Check environment variable first
    if cli_path := os.environ.get("QBIT_CLI_PATH"):
        return cli_path

    # Default to debug build
    return str("../src-tauri/target/debug/qbit-cli")


@pytest.fixture(scope="session")
def cli_path() -> str:
    """Path to the CLI binary."""
    path = get_cli_path()
    if not os.path.exists(path):
        pytest.skip(f"CLI binary not found at {path}. Run: cargo build --no-default-features --features cli --bin qbit-cli")
    return path


@pytest.fixture
def temp_prompt_file() -> Generator[Path, None, None]:
    """Create a temporary file for prompts."""
    with tempfile.NamedTemporaryFile(mode="w", suffix=".txt", delete=False) as f:
        yield Path(f.name)
    os.unlink(f.name)


class CliRunner:
    """Helper class to run CLI commands."""

    def __init__(self, cli_path: str, verbose: bool = False):
        self.cli_path = cli_path
        self.verbose = verbose

    def _log(self, *args, **kwargs):
        """Print if verbose mode is enabled."""
        if self.verbose:
            print(*args, **kwargs)

    def run(
        self,
        *args: str,
        timeout: int = 120,
        check: bool = False,
    ) -> subprocess.CompletedProcess:
        """Run the CLI with given arguments."""
        cmd = [self.cli_path, *args]
        self._log(f"\n{'='*60}")
        self._log(f"CMD: {' '.join(cmd)}")

        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=check,
        )

        self._log(f"EXIT: {result.returncode}")
        if result.stdout:
            self._log(f"STDOUT:\n{result.stdout}")
        if result.stderr:
            self._log(f"STDERR:\n{result.stderr}")
        self._log('='*60)

        return result

    def run_prompt(
        self,
        prompt: str,
        auto_approve: bool = True,
        quiet: bool = False,
        json_output: bool = False,
        timeout: int = 120,
    ) -> subprocess.CompletedProcess:
        """Run a single prompt."""
        self._log(f"\n>>> PROMPT: {prompt}")

        args = ["-e", prompt]
        if auto_approve:
            args.append("--auto-approve")
        if quiet:
            args.append("--quiet")
        if json_output:
            args.append("--json")

        result = self.run(*args, timeout=timeout)

        self._log(f"<<< RESPONSE: {result.stdout.strip()}")
        return result

    def run_prompt_json(
        self,
        prompt: str,
        auto_approve: bool = True,
        timeout: int = 120,
    ) -> JsonRunResult:
        """Run a single prompt in JSON mode and return parsed results.

        This is the preferred method for tests that need to inspect
        tool calls, timing, or other structured event data.

        Args:
            prompt: The prompt to execute
            auto_approve: Whether to auto-approve tool calls
            timeout: Command timeout in seconds

        Returns:
            JsonRunResult with parsed events and convenience accessors
        """
        result = self.run_prompt(
            prompt,
            auto_approve=auto_approve,
            quiet=False,
            json_output=True,
            timeout=timeout,
        )

        events = parse_json_output(result.stdout) if result.stdout else []
        response = get_response_from_json(events)

        return JsonRunResult(
            events=events,
            response=response,
            returncode=result.returncode,
            stderr=result.stderr,
        )

    def run_batch(
        self,
        prompts: list[str],
        auto_approve: bool = True,
        quiet: bool = False,
        timeout: int = 300,
    ) -> subprocess.CompletedProcess:
        """Run multiple prompts from a temp file."""
        with tempfile.NamedTemporaryFile(mode="w", suffix=".txt", delete=False) as f:
            for prompt in prompts:
                f.write(prompt + "\n")
            f.flush()
            temp_path = f.name

        try:
            args = ["-f", temp_path]
            if auto_approve:
                args.append("--auto-approve")
            # In verbose mode, don't use quiet so we see sequential execution
            if quiet and not self.verbose:
                args.append("--quiet")

            self._log(f"\n{'='*60}")
            self._log(f"BATCH: {len(prompts)} prompts")
            self._log('='*60)

            result = self.run(*args, timeout=timeout)

            # Parse and display the sequential Q&A from stderr
            if self.verbose and result.stderr:
                self._log("\n--- Sequential Execution ---")
                lines = result.stderr.split('\n')
                stdout_lines = result.stdout.strip().split('\n') if result.stdout else []
                response_idx = 0

                for line in lines:
                    if '[batch]' in line and 'Executing:' in line:
                        # Extract prompt from: [batch] [1/3] Executing: prompt text
                        prompt_part = line.split('Executing:', 1)[-1].strip()
                        self._log(f"\n>>> Q: {prompt_part}")
                    elif '[batch]' in line and 'Complete' in line:
                        # Show the response that came before this
                        if response_idx < len(stdout_lines):
                            self._log(f"<<< A: {stdout_lines[response_idx]}")
                            response_idx += 1

                self._log("--- End Execution ---\n")

            return result
        finally:
            os.unlink(temp_path)


@pytest.fixture
def cli(cli_path: str, request) -> CliRunner:
    """CLI runner fixture."""
    # Enable verbose mode with VERBOSE=1 env var or -v pytest flag
    verbose = (
        os.environ.get("VERBOSE", "").lower() in ("1", "true", "yes")
        or request.config.getoption("-v", default=0) > 0
    )
    return CliRunner(cli_path, verbose=verbose)


@pytest.fixture(scope="session")
def eval_model():
    """Create OpenAI evaluation model from settings.toml.

    Requires OPENAI_API_KEY environment variable.
    Configure model in settings.toml:
        [eval]
        model = "gpt-4o-mini"
    """
    return create_eval_model()


def pytest_configure(config):
    """Register custom markers."""
    config.addinivalue_line(
        "markers", "requires_api: mark test as requiring API credentials"
    )


def pytest_collection_modifyitems(config, items):
    """Skip API tests unless explicitly enabled."""
    if os.environ.get("RUN_API_TESTS", "").lower() not in ("1", "true", "yes"):
        skip_api = pytest.mark.skip(reason="Set RUN_API_TESTS=1 to run API tests")
        for item in items:
            if "requires_api" in item.keywords:
                item.add_marker(skip_api)

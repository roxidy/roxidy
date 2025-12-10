"""Event types and result dataclasses for the Qbit client.

This module defines the data structures used to represent:
- SSE events from the server (JsonEvent)
- Single prompt execution results (RunResult)
- Batch execution results (BatchResult)
"""

from dataclasses import dataclass, field


@dataclass
class JsonEvent:
    """Structured representation of an SSE event.

    Each event from the Qbit server has:
    - event: Event type (started, text_delta, tool_call, completed, etc.)
    - timestamp: Unix timestamp in milliseconds
    - data: Event-specific payload

    Example:
        >>> event = JsonEvent(event="text_delta", timestamp=1234567890, data={"delta": "Hello"})
        >>> event.get("delta")
        'Hello'
    """

    event: str
    timestamp: int
    data: dict = field(default_factory=dict)

    @classmethod
    def from_dict(cls, d: dict) -> "JsonEvent":
        """Create a JsonEvent from a parsed JSON dict."""
        event = d.pop("event", "unknown")
        timestamp = d.pop("timestamp", 0)
        return cls(event=event, timestamp=timestamp, data=d)

    def __getitem__(self, key: str):
        """Allow dict-like access to data fields."""
        return self.data.get(key)

    def get(self, key: str, default=None):
        """Get a data field with optional default."""
        return self.data.get(key, default)


def get_response_from_events(events: list[JsonEvent]) -> str:
    """Extract the final response text from events.

    Looks for a 'completed' event and returns its 'response' field.
    Falls back to accumulated text from last text_delta if no completed event.

    Args:
        events: List of parsed SSE events

    Returns:
        The final response string, or empty string if not found.
    """
    for event in reversed(events):
        if event.event == "completed":
            return event.get("response", "")

    for event in reversed(events):
        if event.event == "text_delta":
            return event.get("accumulated", "")

    return ""


def get_tool_calls(events: list[JsonEvent]) -> list[JsonEvent]:
    """Extract tool call events (both tool_call and tool_auto_approved)."""
    return [e for e in events if e.event in ("tool_call", "tool_auto_approved")]


def get_tool_results(events: list[JsonEvent]) -> list[JsonEvent]:
    """Extract only tool_result events."""
    return [e for e in events if e.event == "tool_result"]


@dataclass
class RunResult:
    """Result of running a single prompt.

    Contains parsed events and convenience accessors for common operations.

    Attributes:
        events: List of SSE events received
        response: Final response text
        success: True if completed without error
        stderr: Error messages (if any)
    """

    events: list[JsonEvent]
    response: str
    success: bool
    stderr: str

    @property
    def tool_calls(self) -> list[JsonEvent]:
        """Get all tool call events (tool_call and tool_auto_approved)."""
        return get_tool_calls(self.events)

    @property
    def tool_results(self) -> list[JsonEvent]:
        """Get all tool_result events."""
        return get_tool_results(self.events)

    @property
    def completed_event(self) -> JsonEvent | None:
        """Get the completed event if present."""
        for event in reversed(self.events):
            if event.event == "completed":
                return event
        return None

    @property
    def error_event(self) -> JsonEvent | None:
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


@dataclass
class BatchResult:
    """Result of running multiple prompts in batch.

    Attributes:
        responses: List of response strings, one per prompt
        success: True if all prompts succeeded
        stdout: All responses joined by newlines
        stderr: Progress and status messages
    """

    responses: list[str]
    success: bool
    stdout: str
    stderr: str

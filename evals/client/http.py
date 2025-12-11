"""
Native Python client for Qbit HTTP/SSE server.

Provides async streaming access to Qbit agent events, eliminating
subprocess overhead for evaluation frameworks.
"""

import asyncio
import json
from dataclasses import dataclass, field
from typing import AsyncIterator, Optional

import httpx


@dataclass
class QbitEvent:
    """Structured representation of an agent event."""

    event: str
    timestamp: int
    data: dict = field(default_factory=dict)

    @classmethod
    def from_sse(cls, event_type: str, data: str) -> "QbitEvent":
        """Parse an SSE event into a QbitEvent."""
        parsed = json.loads(data)
        event = parsed.pop("event", event_type)
        timestamp = parsed.pop("timestamp", 0)
        return cls(event=event, timestamp=timestamp, data=parsed)

    @property
    def response(self) -> Optional[str]:
        """Get response from completed event."""
        if self.event == "completed":
            return self.data.get("response")
        return None

    @property
    def is_terminal(self) -> bool:
        """Check if this is a terminal event (completed, error, or stream_end)."""
        # Note: stream_end is a server workaround for SSE stream termination
        # The proper completed/error events should be received before stream_end
        if self.event in ("completed", "error"):
            return True
        # Handle custom stream_end event
        if self.event == "custom" and self.data.get("name") == "stream_end":
            return True
        return False


class QbitClient:
    """Async client for Qbit HTTP/SSE server.

    Usage:
        async with QbitClient("http://localhost:8080") as client:
            # Create a session
            session_id = await client.create_session()

            # Execute prompts
            async for event in client.execute(session_id, "What is 2+2?"):
                print(event)

            # Cleanup
            await client.delete_session(session_id)
    """

    def __init__(
        self,
        base_url: str = "http://localhost:8080",
        timeout: float = 120.0,
        max_retries: int = 3,
        retry_delay: float = 1.0,
    ):
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        self.max_retries = max_retries
        self.retry_delay = retry_delay
        self._client: Optional[httpx.AsyncClient] = None

    async def __aenter__(self) -> "QbitClient":
        # Disable keepalive to ensure connections close immediately after each request.
        # This prevents async cleanup issues when the GC runs in a different task context.
        limits = httpx.Limits(keepalive_expiry=0)
        self._client = httpx.AsyncClient(timeout=self.timeout, limits=limits)
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if self._client:
            await self._client.aclose()

    @property
    def client(self) -> httpx.AsyncClient:
        if self._client is None:
            raise RuntimeError("Client not initialized. Use 'async with QbitClient()' context.")
        return self._client

    async def health(self) -> bool:
        """Check if server is healthy."""
        try:
            resp = await self.client.get(f"{self.base_url}/health")
            return resp.status_code == 200
        except httpx.RequestError:
            return False

    async def wait_for_ready(self, timeout: float = 30.0, poll_interval: float = 0.5) -> bool:
        """Wait for server to become ready.

        Args:
            timeout: Maximum time to wait in seconds
            poll_interval: Time between health checks

        Returns:
            True if server became ready, False if timeout
        """
        start = asyncio.get_event_loop().time()
        while asyncio.get_event_loop().time() - start < timeout:
            if await self.health():
                return True
            await asyncio.sleep(poll_interval)
        return False

    async def create_session(
        self,
        workspace: Optional[str] = None,
        auto_approve: bool = True,
    ) -> str:
        """Create a new session.

        Args:
            workspace: Working directory for the session
            auto_approve: Whether to auto-approve tool calls

        Returns:
            Session ID (server-generated UUID)
        """
        payload = {"auto_approve": auto_approve}
        if workspace:
            payload["workspace"] = workspace

        resp = await self.client.post(
            f"{self.base_url}/sessions",
            json=payload,
        )
        resp.raise_for_status()
        return resp.json()["session_id"]

    async def delete_session(self, session_id: str) -> bool:
        """Delete a session.

        Args:
            session_id: Session to delete

        Returns:
            True if deleted, False if not found
        """
        resp = await self.client.delete(f"{self.base_url}/sessions/{session_id}")
        return resp.status_code == 204

    async def execute(
        self,
        session_id: str,
        prompt: str,
        timeout_secs: Optional[int] = None,
    ) -> AsyncIterator[QbitEvent]:
        """Execute a prompt and stream events.

        Args:
            session_id: Session to execute in
            prompt: The prompt to execute
            timeout_secs: Server-side timeout (default: 300s)

        Yields:
            QbitEvent objects as they arrive

        Raises:
            httpx.HTTPStatusError: On HTTP errors
            httpx.RequestError: On connection errors
        """
        payload = {"prompt": prompt}
        if timeout_secs:
            payload["timeout_secs"] = timeout_secs

        retries = 0
        while retries <= self.max_retries:
            try:
                async with self.client.stream(
                    "POST",
                    f"{self.base_url}/sessions/{session_id}/execute",
                    json=payload,
                ) as response:
                    response.raise_for_status()

                    event_type = "message"
                    async for line in response.aiter_lines():
                        line = line.strip()
                        if not line:
                            continue

                        if line.startswith("event:"):
                            event_type = line[6:].strip()
                        elif line.startswith("data:"):
                            data = line[5:].strip()
                            if data and data != "keep-alive":
                                event = QbitEvent.from_sse(event_type, data)
                                yield event
                                if event.is_terminal:
                                    return
                    return  # Stream ended normally

            except httpx.RequestError as e:
                retries += 1
                if retries > self.max_retries:
                    raise
                await asyncio.sleep(self.retry_delay * retries)

    async def execute_simple(
        self,
        session_id: str,
        prompt: str,
        timeout_secs: Optional[int] = None,
    ) -> str:
        """Execute a prompt and return just the final response.

        Convenience method that collects events and returns the response.

        Args:
            session_id: Session to execute in
            prompt: The prompt to execute
            timeout_secs: Server-side timeout

        Returns:
            The final response text

        Raises:
            RuntimeError: If execution fails
        """
        async for event in self.execute(session_id, prompt, timeout_secs):
            if event.event == "completed":
                return event.data.get("response", "")
            elif event.event == "error":
                raise RuntimeError(f"Execution error: {event.data.get('message')}")
        raise RuntimeError("No completion event received")

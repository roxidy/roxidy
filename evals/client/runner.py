"""Streaming runner for executing prompts via HTTP/SSE.

The StreamingRunner provides the primary interface for running evaluation
prompts against the Qbit agent server.

Example:
    >>> from client import StreamingRunner
    >>> runner = StreamingRunner(client, session_id)
    >>> result = await runner.run("What is 2+2?")
    >>> print(result.response)
    '4'
"""

from config import TIMEOUT_BATCH, TIMEOUT_DEFAULT

from .events import BatchResult, JsonEvent, RunResult, get_response_from_events


class StreamingRunner:
    """Runner for executing prompts via HTTP/SSE server.

    This is the primary interface for evaluation tests. It wraps the QbitClient
    and provides a simple async interface for running prompts and collecting
    structured results.

    Attributes:
        client: QbitClient instance connected to the server
        session_id: Session ID for this runner
        verbose: Whether to print debug output
    """

    def __init__(self, client, session_id: str, verbose: bool = False):
        """Initialize the streaming runner.

        Args:
            client: QbitClient connected to the server
            session_id: Session ID for this runner
            verbose: Whether to print debug output
        """
        self.client = client
        self.session_id = session_id
        self.verbose = verbose

    def _log(self, *args, **kwargs):
        """Print if verbose mode is enabled."""
        if self.verbose:
            print(*args, **kwargs)

    async def run(
        self,
        prompt: str,
        timeout: int = TIMEOUT_DEFAULT,
    ) -> RunResult:
        """Run a prompt and return structured results.

        Args:
            prompt: The prompt to execute
            timeout: Execution timeout in seconds

        Returns:
            RunResult with parsed events and convenience accessors
        """
        self._log(f"\n>>> PROMPT: {prompt}")

        events = []
        async for event in self.client.execute(
            self.session_id,
            prompt,
            timeout_secs=timeout,
        ):
            json_event = JsonEvent(
                event=event.event,
                timestamp=event.timestamp,
                data=event.data,
            )
            events.append(json_event)
            self._log(f"  EVENT: {event.event}")

        response = get_response_from_events(events)
        error_event = next((e for e in reversed(events) if e.event == "error"), None)

        result = RunResult(
            events=events,
            response=response,
            success=error_event is None,
            stderr="",
        )

        if len(response) > 100:
            self._log(f"<<< RESPONSE: {response[:100]}...")
        else:
            self._log(f"<<< RESPONSE: {response}")

        return result

    async def run_batch(
        self,
        prompts: list[str],
        quiet: bool = False,
        timeout: int = TIMEOUT_BATCH,
    ) -> BatchResult:
        """Run multiple prompts sequentially in the same session.

        This maintains conversation context between prompts, useful for
        testing multi-turn memory and state tracking.

        Args:
            prompts: List of prompts to execute sequentially
            quiet: If True, suppress progress output
            timeout: Timeout per prompt in seconds

        Returns:
            BatchResult with all responses and combined output
        """
        responses = []
        stderr_lines = []
        has_error = False

        for i, prompt in enumerate(prompts, 1):
            if not quiet:
                progress = f"[{i}/{len(prompts)}] Executing: {prompt[:50]}..."
                stderr_lines.append(f"[batch] {progress}")
                self._log(f"\n{'='*60}")
                self._log(progress)

            try:
                result = await self.run(prompt, timeout=timeout)
                responses.append(result.response)

                if not result.success:
                    has_error = True

                if not quiet:
                    stderr_lines.append(f"[batch] [{i}/{len(prompts)}] Complete")
                    self._log(f"<<< {result.response[:100]}...")

            except Exception as e:
                has_error = True
                responses.append(f"Error: {e}")
                stderr_lines.append(f"[batch] [{i}/{len(prompts)}] Error: {e}")
                self._log(f"ERROR: {e}")

        if not quiet:
            stderr_lines.append(f"[batch] All {len(prompts)} prompt(s) completed")

        return BatchResult(
            responses=responses,
            success=not has_error,
            stdout="\n".join(responses),
            stderr="\n".join(stderr_lines),
        )

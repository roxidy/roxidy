"""Qbit HTTP/SSE client package.

This package provides the client-side interface for communicating with
the Qbit agent server via HTTP and Server-Sent Events (SSE).

Quick Start:
    >>> from client import QbitClient, StreamingRunner
    >>>
    >>> async with QbitClient("http://localhost:8080") as client:
    ...     session_id = await client.create_session()
    ...     runner = StreamingRunner(client, session_id)
    ...     result = await runner.run("What is 2+2?")
    ...     print(result.response)

Classes:
    QbitClient: Async HTTP/SSE client for the Qbit server
    StreamingRunner: High-level interface for running evaluation prompts
    JsonEvent: Structured representation of an SSE event
    RunResult: Result of running a single prompt
    BatchResult: Result of running multiple prompts in batch
"""

from .events import BatchResult, JsonEvent, RunResult
from .http import QbitClient
from .runner import StreamingRunner

__all__ = [
    "QbitClient",
    "StreamingRunner",
    "JsonEvent",
    "RunResult",
    "BatchResult",
]

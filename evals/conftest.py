"""Pytest configuration and fixtures for Qbit evaluation tests.

This module provides:
- Pytest hooks and markers
- Server lifecycle fixtures
- Session and runner fixtures
- Sidecar database fixtures
- DeepEval model fixtures

Fixtures:
    runner: StreamingRunner for executing prompts (most common)
    qbit_server: QbitClient connected to the server
    streaming_session: (session_id, client) tuple
    sidecar_db: LanceDB connection
    eval_model: DeepEval GPTModel instance
"""

import os
import re
import subprocess
import time

import pytest

from client import QbitClient, StreamingRunner
from config import create_eval_model, get_binary_path, is_verbose


# =============================================================================
# Pytest Hooks
# =============================================================================


def pytest_configure(config):
    """Register custom markers."""
    config.addinivalue_line(
        "markers", "requires_api: mark test as requiring API credentials"
    )
    config.addinivalue_line(
        "markers", "requires_sidecar: mark test as requiring sidecar database"
    )


def pytest_collection_modifyitems(config, items):
    """Skip API tests unless explicitly enabled."""
    if os.environ.get("RUN_API_TESTS", "").lower() not in ("1", "true", "yes"):
        skip_api = pytest.mark.skip(reason="Set RUN_API_TESTS=1 to run API tests")
        for item in items:
            if "requires_api" in item.keywords:
                item.add_marker(skip_api)


# =============================================================================
# DeepEval Model Fixture
# =============================================================================


@pytest.fixture(scope="session")
def eval_model():
    """Create OpenAI evaluation model for DeepEval.

    Loads configuration from ~/.qbit/settings.toml:
        [eval]
        model = "gpt-4o-mini"
        api_key = "sk-..."  # or use OPENAI_API_KEY env var

    Returns:
        GPTModel instance for DeepEval metrics.
    """
    return create_eval_model()


# =============================================================================
# Sidecar Database Fixture
# =============================================================================


@pytest.fixture(scope="function")
def sidecar_db():
    """Connect to the sidecar LanceDB database.

    Returns:
        LanceDB connection, or None if database doesn't exist.
    """
    try:
        from sidecar import connect_db
        return connect_db()
    except (FileNotFoundError, ImportError):
        return None


# =============================================================================
# Server Fixtures
# =============================================================================


@pytest.fixture(scope="session")
def qbit_server_info():
    """Start qbit server and return connection info.

    This is a session-scoped fixture that starts one server for all tests.
    The server runs on a random port to avoid conflicts.

    Yields:
        Base URL string (e.g., "http://127.0.0.1:54321")
    """
    import httpx

    binary_path = get_binary_path()
    if not os.path.exists(binary_path):
        pytest.skip(f"Binary not found at {binary_path}. Run: just build-server")

    # Start server on random port
    proc = subprocess.Popen(
        [str(binary_path), "--server", "--port", "0"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    try:
        # Parse the bound address from stdout
        line = proc.stdout.readline()
        match = re.search(r"http://([^:]+):(\d+)", line)
        if not match:
            proc.terminate()
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
            pytest.fail("Server did not become ready within 15 seconds")

        yield base_url

    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()


@pytest.fixture
async def qbit_server(qbit_server_info: str):
    """Async QbitClient fixture connected to the running server.

    Creates a fresh QbitClient for each test, properly integrated
    with pytest-asyncio's event loop management.

    Args:
        qbit_server_info: Base URL from the server fixture

    Yields:
        QbitClient instance ready for use.
    """
    async with QbitClient(qbit_server_info) as client:
        yield client


@pytest.fixture
async def streaming_session(qbit_server):
    """Create a fresh session for each test.

    Automatically creates and cleans up the session.

    Args:
        qbit_server: QbitClient from the server fixture

    Yields:
        Tuple of (session_id, client) for flexibility.
    """
    session_id = await qbit_server.create_session()
    yield session_id, qbit_server
    await qbit_server.delete_session(session_id)


@pytest.fixture
async def runner(streaming_session, request) -> StreamingRunner:
    """StreamingRunner fixture for running evaluation prompts.

    This is the primary fixture for evaluation tests. It provides
    a high-level interface for running prompts and collecting results.

    Args:
        streaming_session: (session_id, client) tuple
        request: Pytest request for accessing config

    Returns:
        StreamingRunner instance ready for use.

    Example:
        @pytest.mark.asyncio
        async def test_arithmetic(runner):
            result = await runner.run("What is 2+2?")
            assert "4" in result.response
    """
    session_id, client = streaming_session
    verbose = is_verbose() or request.config.getoption("-v", default=0) > 0
    return StreamingRunner(client, session_id, verbose=verbose)


# =============================================================================
# Class-Scoped Fixtures (for sharing LLM calls across tests in same class)
# =============================================================================


@pytest.fixture(scope="class")
async def class_qbit_server(qbit_server_info: str):
    """Class-scoped QbitClient for shared fixtures."""
    async with QbitClient(qbit_server_info) as client:
        yield client


@pytest.fixture(scope="class")
async def class_streaming_session(class_qbit_server):
    """Class-scoped session for shared fixtures."""
    session_id = await class_qbit_server.create_session()
    yield session_id, class_qbit_server
    await class_qbit_server.delete_session(session_id)


@pytest.fixture(scope="class")
async def class_runner(class_streaming_session, request) -> StreamingRunner:
    """Class-scoped StreamingRunner for shared fixtures.

    Use this when you want to share LLM call results across
    multiple tests in the same class.
    """
    session_id, client = class_streaming_session
    verbose = is_verbose() or request.config.getoption("-v", default=0) > 0
    return StreamingRunner(client, session_id, verbose=verbose)

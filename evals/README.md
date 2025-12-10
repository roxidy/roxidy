# Qbit Evaluation Tests

LLM evaluation tests for the Qbit agent using HTTP/SSE streaming and [DeepEval](https://deepeval.com/).

## Quick Start

```bash
cd evals

# Install dependencies
uv venv .venv && source .venv/bin/activate
uv pip install -e .

# Build the server binary
just build-server

# Run all tests
RUN_API_TESTS=1 pytest -v
```

## Architecture

Tests connect to a qbit server via HTTP/SSE:

```
pytest fixture (qbit_server_info)
        |
        v
  qbit-cli --server (background process)
        |
        v (HTTP/SSE)
  QbitClient (async Python client)
        |
        v
  StreamingRunner (test interface)
```

## Project Structure

```
evals/
├── client/              # HTTP/SSE client package
│   ├── __init__.py      # Package exports
│   ├── http.py          # QbitClient async HTTP/SSE client
│   ├── runner.py        # StreamingRunner for test execution
│   └── events.py        # Event types and result dataclasses
├── sidecar/             # Sidecar database package
│   ├── __init__.py      # Package exports
│   ├── db.py            # LanceDB connection and queries
│   └── assertions.py    # Test assertion/validator factories
├── config.py            # Centralized configuration
├── conftest.py          # Pytest fixtures
├── test_agent.py        # Agent behavior evals
├── test_server_api.py   # Server API tests
├── test_sidecar_db.py   # Sidecar database tests
├── pyproject.toml       # Dependencies
└── README.md
```

## Test Files

| File | Description |
|------|-------------|
| `test_agent.py` | Core agent evals: memory, tools, response quality |
| `test_server_api.py` | Server API tests: health, sessions, streaming |
| `test_sidecar_db.py` | Sidecar database tests: events, search, storage |

## Test Categories

### test_agent.py

| Category | Description |
|----------|-------------|
| `TestBehavior` | Basic behavior (streaming events, tool calls) |
| `TestMemoryAndState` | Multi-turn memory recall |
| `TestResponseQuality` | Arithmetic, instruction following |
| `TestCharacterHandling` | Unicode, special chars, multiline |
| `TestLayer1Tables` | Sidecar L1 normalized tables |
| `TestToolUsage` | File reading, directory listing |

### test_server_api.py

| Category | Description |
|----------|-------------|
| `TestServerBasics` | Health, session CRUD |
| `TestExecution` | Prompt execution, streaming |
| `TestErrorHandling` | Invalid sessions, timeouts |
| `TestConcurrency` | Multiple sessions, limits |
| `TestStreamingRunner` | Runner interface tests |

### test_sidecar_db.py

| Category | Description |
|----------|-------------|
| `TestEventCapture` | Event storage verification |
| `TestSessionLifecycle` | Session creation, metadata |
| `TestSearchFunctionality` | Keyword search |
| `TestSynthesisQuality` | Event coherence (DeepEval) |
| `TestStorageIntegrity` | Data persistence, timestamps |

## Configuration

### Environment Variables

```bash
# Required for tests
RUN_API_TESTS=1              # Enable tests that call LLMs

# Agent model (defaults to settings.toml)
QBIT_EVAL_MODEL=claude-sonnet-4-20250514

# DeepEval evaluator (defaults to settings.toml)
OPENAI_API_KEY=sk-...

# Verbose output
VERBOSE=1
```

### settings.toml

```toml
[eval]
model = "gpt-4o-mini"           # DeepEval evaluator model
agent_model = "claude-sonnet-4-20250514"  # Qbit agent model
# api_key = "sk-..."            # Or use OPENAI_API_KEY env var
```

## Running Tests

```bash
# All evals
RUN_API_TESTS=1 pytest test_agent.py -v

# Server API tests
RUN_API_TESTS=1 pytest test_server_api.py -v

# Sidecar tests
RUN_API_TESTS=1 pytest test_sidecar_db.py -v

# Full suite
RUN_API_TESTS=1 pytest -v
```

## Writing Tests

### Using StreamingRunner

```python
from client import StreamingRunner

@pytest.mark.requires_api
class TestExample:
    @pytest.mark.asyncio
    async def test_simple(self, runner: StreamingRunner):
        result = await runner.run("What is 2+2?")
        assert result.success
        assert "4" in result.response

    @pytest.mark.asyncio
    async def test_multi_turn(self, runner: StreamingRunner):
        result = await runner.run_batch([
            "Remember: x=42",
            "What is x?",
        ])
        assert "42" in result.responses[-1]
```

### Using DeepEval

```python
from deepeval.metrics import GEval
from deepeval.test_case import LLMTestCase, LLMTestCaseParams

@pytest.mark.asyncio
async def test_with_eval(self, runner, eval_model):
    result = await runner.run("Explain recursion")

    test_case = LLMTestCase(
        input="Explain recursion",
        actual_output=result.response,
        expected_output="A function that calls itself",
    )

    metric = GEval(
        name="Explanation Quality",
        criteria="Response explains recursion clearly",
        evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT],
        threshold=0.7,
        model=eval_model,
    )

    from deepeval import assert_test
    assert_test(test_case, [metric])
```

### Querying Sidecar Database

```python
from sidecar import connect_db, get_sessions, get_events, search_events

db = connect_db()
sessions = get_sessions(db, limit=5)
events = get_events(db, sessions[0]["id"])
results = search_events(db, "error", limit=10)
```

# Evaluation Setup Guide

This guide explains how to configure and run the LLM-based evaluation tests for qbit-cli.

## Overview

The integration tests use [DeepEval](https://deepeval.com/) to evaluate agent responses using LLM-as-a-judge metrics. This provides more nuanced evaluation than simple string matching, allowing tests to verify semantic correctness.

## Prerequisites

### Python Environment

```bash
cd evals

# Create virtual environment (if not exists)
uv venv .venv
source .venv/bin/activate

# Install dependencies
uv pip install -e .
```

### API Keys

**OpenAI API Key** (for evaluation model):
Get an API key from https://platform.openai.com/api-keys

**Vertex AI / Anthropic** (for qbit-cli agent):
Configure in `~/.qbit/settings.toml` as described in CLAUDE.md

## Configuration

### Evaluator Model (OpenAI)

The evaluator model judges agent responses. Configure in `~/.qbit/settings.toml`:

```toml
[eval]
model = "gpt-4o-mini"    # OpenAI model for evaluation
temperature = 0          # Lower = more deterministic
# api_key = "sk-..."     # Optional, can use OPENAI_API_KEY env var
```

Or use environment variable:
```bash
export OPENAI_API_KEY=sk-...
```

### Agent Model (qbit-cli)

Override the agent model used during tests:

```bash
# Via environment variable (highest priority)
QBIT_EVAL_MODEL="claude-haiku-4-5@20251001" pytest test_cli.py -v

# Or in settings.toml
[eval]
agent_model = "claude-haiku-4-5@20251001"
```

### Available Evaluator Models

| Model | Cost | Notes |
|-------|------|-------|
| `gpt-4o-mini` | Low | Recommended for routine testing |
| `gpt-4o` | Medium | More capable, use for complex evals |

## Test Files

| File | Description | API Required |
|------|-------------|--------------|
| `test_cli.py` | CLI behavior and response quality tests | Yes (most tests) |
| `test_sidecar.py` | Sidecar event capture and storage tests | Yes (most tests) |
| `test_layer1.py` | Layer 1 session state tests | Yes (most tests) |

## Running Tests

### Basic CLI Tests (No API Required)

```bash
cd evals
pytest test_cli.py -v -k "TestCliBasics"
```

These tests verify CLI argument parsing and don't require any API credentials.

### Full CLI Test Suite

```bash
# Run all CLI tests
RUN_API_TESTS=1 pytest test_cli.py -v

# With verbose CLI output
RUN_API_TESTS=1 VERBOSE=1 pytest test_cli.py -v

# With specific agent model
QBIT_EVAL_MODEL="claude-haiku-4-5@20251001" RUN_API_TESTS=1 pytest test_cli.py -v
```

### Sidecar Tests

The sidecar tests verify event capture, session management, and search functionality:

```bash
# Run all sidecar tests
RUN_API_TESTS=1 pytest test_sidecar.py -v

# Storage integrity tests only (no API needed, but requires existing sidecar DB)
pytest test_sidecar.py -v -k "TestStorageIntegrity"
```

### Test Categories

**CLI Tests (`test_cli.py`):**
```bash
# Basic CLI behavior (no API)
pytest test_cli.py -v -k "TestCliBasics"

# CLI behavior with API (no DeepEval)
RUN_API_TESTS=1 pytest test_cli.py -v -k "TestCliBehavior"

# Memory and state recall
RUN_API_TESTS=1 pytest test_cli.py -v -k "TestMemoryAndState"

# Response quality
RUN_API_TESTS=1 pytest test_cli.py -v -k "TestResponseQuality"

# Character handling (unicode, special chars)
RUN_API_TESTS=1 pytest test_cli.py -v -k "TestCharacterHandling"

# Tool usage
RUN_API_TESTS=1 pytest test_cli.py -v -k "TestToolUsage"
```

**Sidecar Tests (`test_sidecar.py`):**
```bash
# Event capture
RUN_API_TESTS=1 pytest test_sidecar.py -v -k "TestEventCapture"

# Session lifecycle
RUN_API_TESTS=1 pytest test_sidecar.py -v -k "TestSessionLifecycle"

# Search functionality
RUN_API_TESTS=1 pytest test_sidecar.py -v -k "TestSearchFunctionality"

# Synthesis quality (DeepEval)
RUN_API_TESTS=1 pytest test_sidecar.py -v -k "TestSynthesisQuality"

# Storage integrity (no API needed)
pytest test_sidecar.py -v -k "TestStorageIntegrity"
```

## How Evaluation Works

### GEval Metrics

Each test uses DeepEval's `GEval` metric, which:

1. Takes the agent's actual output
2. Compares it against evaluation criteria
3. Uses an LLM to score the response (0.0 - 1.0)
4. Passes if the score exceeds the threshold

Example metric:

```python
memory_metric = GEval(
    name="Number Recall",
    criteria="The response must contain the number 42.",
    evaluation_steps=[
        "Check if the response contains the number 42",
        "The response should be exactly or close to '42'",
    ],
    threshold=0.8,
    model=eval_model,  # From settings.toml
)
```

### Test Case Structure

```python
test_case = LLMTestCase(
    input="What is the magic number?",           # The prompt
    actual_output=result.stdout.strip(),         # Agent's response
    expected_output="42",                        # Reference answer
    context=["The magic number is 42."],         # Background facts
)
```

## Customizing Evaluations

### Adding New Tests

1. Create your test method with `cli` and `eval_model` fixtures
2. Run the CLI command to get the agent response
3. Create an `LLMTestCase` with input/output
4. Define a `GEval` metric with criteria
5. Call `evaluate()` and assert results

```python
def test_my_feature(self, cli: CliRunner, eval_model):
    result = cli.run_prompt_json("Your prompt here")

    test_case = LLMTestCase(
        input="Your prompt here",
        actual_output=result.response,
        expected_output="Expected response",
    )

    my_metric = GEval(
        name="My Metric",
        criteria="What makes a good response",
        evaluation_steps=["Step 1", "Step 2"],
        threshold=0.8,
        model=eval_model,
    )

    results = evaluate([test_case], [my_metric])
    assert results.test_results[0].success
```

### Adjusting Thresholds

- `0.9+` - Strict matching (exact recall, arithmetic)
- `0.7-0.8` - Moderate matching (comprehension, summarization)
- `0.5-0.6` - Lenient matching (creative tasks, partial matches)

## Sidecar Testing

### Prerequisites

The sidecar tests require:
1. A running qbit instance that has created the sidecar database
2. The database at `~/.qbit/sidecar/sidecar.lance`

### Sidecar Utilities

The `sidecar_utils.py` module provides helpers for querying the sidecar database:

```python
from sidecar_utils import (
    connect_sidecar_db,
    get_last_session,
    get_session_events,
    search_events_keyword,
    get_storage_stats,
    list_sessions,
)

# Connect to database
db = connect_sidecar_db()

# Get recent session
session = get_last_session(db)

# Get events for a session
events = get_session_events(db, session["id"])

# Search events by keyword
matches = search_events_keyword(db, "my search term", limit=10)
```

### Known Limitations

- **Batch mode event capture**: Batch mode (`run_batch`) may only capture events from the last prompt. Use single prompts (`run_prompt_json`) for reliable event capture testing.
- **Async flush delay**: Events are flushed asynchronously. Tests use `wait_for_sidecar_flush()` to allow time for writes.

## Layer 1 Session State Testing

Layer 1 maintains a continuously-updated session state model that includes:
- **Goal Stack**: What the agent is trying to accomplish
- **Narrative**: Human-readable "what's happening and why"
- **Decision Log**: Choices made, alternatives rejected, rationale
- **File Context Map**: Per-file summary of agent's understanding
- **Error Journal**: What went wrong, how resolved
- **Open Questions**: Unresolved ambiguities

### Running Layer 1 Tests

```bash
# Storage tests (no API needed, requires session_states table)
pytest test_layer1.py -v -k "TestLayer1Storage"

# All Layer 1 tests
RUN_API_TESTS=1 pytest test_layer1.py -v

# Specific test classes
RUN_API_TESTS=1 pytest test_layer1.py -v -k "TestLayer1GoalCapture"
RUN_API_TESTS=1 pytest test_layer1.py -v -k "TestLayer1FileContext"
RUN_API_TESTS=1 pytest test_layer1.py -v -k "TestLayer1Decisions"
RUN_API_TESTS=1 pytest test_layer1.py -v -k "TestLayer1Quality"
```

### Initializing the Layer 1 Table

Before running storage tests, initialize the `session_states` table:

```bash
cd evals

# Create table only
python init_layer1_table.py

# Create table and seed with test data
python init_layer1_table.py --seed

# Force recreate if table exists
python init_layer1_table.py --seed --force
```

### Layer 1 Utilities

The `sidecar_utils.py` module provides helpers for querying Layer 1 state:

```python
from sidecar_utils import (
    get_layer1_state,          # Get state for a session
    get_layer1_latest,         # Get most recent state
    get_layer1_goals,          # Extract goals from state
    get_layer1_decisions,      # Extract decisions
    get_layer1_file_contexts,  # Extract file contexts
    get_layer1_errors,         # Extract error journal
    get_layer1_open_questions, # Extract open questions
    get_layer1_narrative,      # Extract narrative
    list_layer1_states,        # List recent states
    get_layer1_state_count,    # Count snapshots
)

# Example usage
db = connect_sidecar_db()
state = get_layer1_latest(db)
if state:
    goals = get_layer1_goals(state)
    narrative = get_layer1_narrative(state)
    print(f"Goals: {len(goals)}, Narrative: {narrative[:100]}...")
```

### Layer 1 Scorers

The `sidecar_scorers.py` module provides scorer factories for validating Layer 1 state:

```python
from sidecar_scorers import (
    verify_layer1_state_exists,       # Check state exists
    verify_layer1_has_goal,           # Check has goals
    verify_layer1_goal_contains,      # Check goal contains keyword
    verify_layer1_has_decisions,      # Check decision count
    verify_layer1_decision_contains,  # Check decision keyword
    verify_layer1_has_file_context,   # Check file is tracked
    verify_layer1_file_count,         # Check tracked file count
    verify_layer1_has_narrative,      # Check narrative exists
    verify_layer1_narrative_contains, # Check narrative keyword
    verify_layer1_has_errors,         # Check error count
    verify_layer1_has_open_questions, # Check question count
    verify_layer1_snapshots,          # Check snapshot count
)

# Example usage
scorer = verify_layer1_goal_contains("fibonacci")
passed, reason = scorer(session_id)
print(f"Goal check: {passed} - {reason}")
```

### Test Categories

| Class | Description | Tests |
|-------|-------------|-------|
| `TestLayer1GoalCapture` | Verify goals extracted from prompts | 2 |
| `TestLayer1FileContext` | Verify file tracking on read/edit | 2 |
| `TestLayer1Decisions` | Verify decision logging | 1 |
| `TestLayer1Narrative` | Verify narrative updates | 1 |
| `TestLayer1Errors` | Verify error tracking | 1 |
| `TestLayer1OpenQuestions` | Verify question capture | 1 |
| `TestLayer1Storage` | Verify persistence (no API) | 3 |
| `TestLayer1Quality` | GEval quality metrics | 2 |
| `TestLayer1Integration` | End-to-end verification | 2 |

### Known Limitations

- **Processor activation**: Some tests skip if the Layer 1 processor wasn't active during CLI execution. This happens when the CLI runs without full sidecar integration.
- **State capture timing**: Layer 1 state is captured asynchronously. Tests use `wait_for_layer1_flush()` with longer delays.
- **Quality tests require state**: The GEval tests (`TestLayer1Quality`) require actual captured state from CLI runs, not seeded test data.

## Troubleshooting

### "OPENAI_API_KEY not set"

Set the API key:
```bash
export OPENAI_API_KEY=sk-...
```

Or add to settings.toml:
```toml
[eval]
api_key = "sk-..."
```

### Tests Skip with "Set RUN_API_TESTS=1"

The API tests are disabled by default. Enable them:
```bash
RUN_API_TESTS=1 pytest test_cli.py -v
```

### "Sidecar database not found"

Run qbit at least once to initialize the sidecar database:
```bash
./target/debug/qbit-cli -e "hello" --auto-approve
```

### Slow Tests

- Use `gpt-4o-mini` for faster, cheaper evaluations
- Use `claude-haiku-4-5@20251001` as agent model for faster CLI responses
- Run specific test categories instead of full suite

## Cost Considerations

Each test makes 1-2 LLM API calls for evaluation. Approximate costs:

| Model | Cost per 1000 tests |
|-------|---------------------|
| gpt-4o-mini | ~$0.30 |
| gpt-4o | ~$15.00 |

Use `gpt-4o-mini` (the default) for routine testing.

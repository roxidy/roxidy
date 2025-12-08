# DeepEval Setup Guide

This guide explains how to configure and run the LLM-based evaluation tests for qbit-cli.

## Overview

The integration tests use [DeepEval](https://deepeval.com/) to evaluate agent responses using LLM-as-a-judge metrics. This provides more nuanced evaluation than simple string matching, allowing tests to verify semantic correctness.

## Prerequisites

### Python Environment

```bash
cd src-tauri/tests/integration

# Create virtual environment (if not exists)
uv venv .venv

# Install dependencies
uv pip install -e .
# or
uv pip install pytest deepeval
```

### OpenAI API Key

Get an API key from https://platform.openai.com/api-keys

Configure it in one of two ways:

**Option 1: Environment variable**
```bash
export OPENAI_API_KEY=sk-...
```

**Option 2: settings.toml**
```toml
[eval]
model = "gpt-4o-mini"
api_key = "sk-..."
```

## Configuration

Edit `settings.toml` to configure the eval model:

```toml
[eval]
model = "gpt-4o-mini"    # OpenAI model to use
temperature = 0          # Lower = more deterministic
# api_key = "sk-..."     # Optional, can use OPENAI_API_KEY env var
```

### Available Models

| Model | Cost | Notes |
|-------|------|-------|
| `gpt-4o-mini` | Low | Recommended for routine testing |
| `gpt-4o` | Medium | More capable, use for complex evals |
| `gpt-4-turbo` | Medium | Previous generation |

## Running Tests

### Basic CLI Tests (No LLM Required)

```bash
pytest test_cli.py -v -k "TestCliBasics"
```

These tests verify CLI argument parsing and don't require any API credentials.

### Full Test Suite (Requires OpenAI)

```bash
# Run all tests including LLM evaluations
RUN_API_TESTS=1 pytest test_cli.py -v

# Run with verbose output
RUN_API_TESTS=1 VERBOSE=1 pytest test_cli.py -v
```

### Specific Test Categories

```bash
# Session continuity tests
RUN_API_TESTS=1 pytest test_cli.py -v -k "TestSessionContinuity"

# Tool execution tests
RUN_API_TESTS=1 pytest test_cli.py -v -k "TestToolExecution"

# Edge case tests
RUN_API_TESTS=1 pytest test_cli.py -v -k "TestEdgeCases"
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
    result = cli.run_prompt("Your prompt here", quiet=True)

    test_case = LLMTestCase(
        input="Your prompt here",
        actual_output=result.stdout.strip(),
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
    assert all(r.success for r in results.test_results)
```

### Adjusting Thresholds

- `0.9+` - Strict matching (exact recall, arithmetic)
- `0.7-0.8` - Moderate matching (comprehension, summarization)
- `0.5-0.6` - Lenient matching (creative tasks, partial matches)

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

### Slow Tests

- Use `gpt-4o-mini` for faster, cheaper evaluations
- Run specific test categories instead of full suite

## Cost Considerations

Each test makes 1-2 LLM API calls for evaluation. Approximate costs:

| Model | Cost per 1000 tests |
|-------|---------------------|
| gpt-4o-mini | ~$0.30 |
| gpt-4o | ~$15.00 |

Use `gpt-4o-mini` (the default) for routine testing.

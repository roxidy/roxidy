# qbit-cli Integration Tests

Integration tests for the qbit-cli agent using [DeepEval](https://deepeval.com/) for LLM-based evaluation.

## Quick Start

```bash
# Install dependencies
uv pip install -e .

# Run basic tests (no API required)
pytest test_cli.py -v -k "TestCliBasics"

# Run full test suite (requires OpenAI API key)
RUN_API_TESTS=1 pytest test_cli.py -v
```

## Test Categories

| Category | Description | Requires API |
|----------|-------------|--------------|
| `TestCliBasics` | CLI argument parsing, help, version | No |
| `TestSessionContinuity` | Memory across prompts | Yes |
| `TestBatchProcessing` | Multi-prompt batch execution | Yes |
| `TestSinglePrompt` | Single prompt execution | Yes |
| `TestEdgeCases` | Unicode, special chars, multiline | Yes |
| `TestToolExecution` | File reading, directory listing | Yes |

## Configuration

Edit `settings.toml`:

```toml
[eval]
model = "gpt-4o-mini"
# api_key = "sk-..."  # Or use OPENAI_API_KEY env var
```

## Prerequisites

- **CLI Binary**: `cargo build --no-default-features --features cli --bin qbit-cli`
- **OpenAI API Key**: Set `OPENAI_API_KEY` env var or add `api_key` to settings.toml

## Documentation

- [Eval Setup Guide](../docs/eval-setup.md) - Detailed configuration and usage

## Project Structure

```
.
├── conftest.py      # Fixtures: cli, eval_model, temp files
├── test_cli.py      # All test cases
├── settings.toml    # Eval model configuration
├── pyproject.toml   # Python dependencies
├── docs/
│   └── eval-setup.md
└── README.md
```

"""Inspect AI evaluation framework for qbit-cli.

Run with:
    inspect eval inspect_qbit_eval.py --model qbit-cli

View results:
    inspect view

Configure scorer model in settings.toml:
    [inspect]
    scorer_model = "openai/gpt-4o-mini"
"""

import os
import subprocess
import tempfile
import tomllib
from pathlib import Path

from inspect_ai import Task, task
from inspect_ai.dataset import Sample, MemoryDataset
from inspect_ai.model import (
    ChatMessage,
    ChatMessageUser,
    ChatMessageAssistant,
    GenerateConfig,
    Model,
    ModelAPI,
    ModelOutput,
    modelapi,
)
from inspect_ai.scorer import includes, model_graded_fact, scorer, Score, Target
from inspect_ai.solver import generate


def get_cli_path() -> str:
    """Get the path to the qbit-cli binary."""
    if cli_path := os.environ.get("QBIT_CLI_PATH"):
        return cli_path
    repo_root = Path(__file__).parent.parent.parent
    return str(repo_root / "target" / "debug" / "qbit-cli")


def load_settings() -> dict:
    """Load settings from settings.toml."""
    settings_path = Path(__file__).parent / "settings.toml"
    if not settings_path.exists():
        return {}
    with open(settings_path, "rb") as f:
        return tomllib.load(f)


# Register qbit-cli as a custom model provider
@modelapi(name="qbit-cli")
def qbit_cli_api() -> ModelAPI:
    return QbitCliModelAPI()


class QbitCliModelAPI(ModelAPI):
    """Model API that wraps the qbit-cli binary."""

    def __init__(self):
        super().__init__(model_name="qbit-cli")
        self.cli_path = get_cli_path()

    async def generate(
        self,
        input: list[ChatMessage],
        tools: list = [],
        tool_choice: str | None = None,
        config: GenerateConfig = GenerateConfig(),
    ) -> ModelOutput:
        """Execute prompts through qbit-cli and return the response."""

        # Extract user messages to form the conversation
        user_prompts = []
        for msg in input:
            if isinstance(msg, ChatMessageUser):
                user_prompts.append(msg.content)

        if not user_prompts:
            return ModelOutput.from_content(
                model="qbit-cli",
                content="No user prompts provided",
            )

        # Use a temp file for batch execution (like CliRunner.run_batch)
        with tempfile.NamedTemporaryFile(mode="w", suffix=".txt", delete=False) as f:
            for prompt in user_prompts:
                f.write(prompt + "\n")
            temp_path = f.name

        try:
            result = subprocess.run(
                [self.cli_path, "-f", temp_path, "--auto-approve", "--quiet"],
                capture_output=True,
                text=True,
                timeout=300,
                cwd=Path(__file__).parent,  # Run from integration test dir
            )

            if result.returncode != 0:
                content = f"Error (exit {result.returncode}): {result.stderr}"
            else:
                # Get the last response (like get_last_response in conftest.py)
                lines = [line for line in result.stdout.strip().split("\n") if line.strip()]
                content = lines[-1] if lines else ""

            return ModelOutput.from_content(
                model="qbit-cli",
                content=content,
            )
        finally:
            os.unlink(temp_path)


# =============================================================================
# Datasets
# =============================================================================

# Session continuity tests - memory across prompts
memory_dataset = MemoryDataset(
    samples=[
        # Simple number recall
        Sample(
            input=[
                ChatMessageUser(content="Remember: the magic number is 42. Just say 'OK'."),
                ChatMessageAssistant(content="OK"),
                ChatMessageUser(content="What is the magic number? Reply with just the number."),
            ],
            target="42",
            metadata={"category": "memory", "test": "number_recall"},
        ),
        # Word recall
        Sample(
            input=[
                ChatMessageUser(content="The secret word is 'elephant'. Just say 'understood'."),
                ChatMessageAssistant(content="understood"),
                ChatMessageUser(content="What was the secret word? Reply with just that word."),
            ],
            target="elephant",
            metadata={"category": "memory", "test": "word_recall"},
        ),
        # Cumulative calculation
        Sample(
            input=[
                ChatMessageUser(content="I have 3 apples. Say 'noted'."),
                ChatMessageAssistant(content="noted"),
                ChatMessageUser(content="I buy 2 more apples. Say 'noted'."),
                ChatMessageAssistant(content="noted"),
                ChatMessageUser(content="How many apples do I have now? Just the number."),
            ],
            target="5",
            metadata={"category": "memory", "test": "cumulative_calculation"},
        ),
    ],
    name="memory",
)

# Multi-fact recall tests
multi_fact_dataset = MemoryDataset(
    samples=[
        Sample(
            input=[
                ChatMessageUser(content="My name is Alice. Say 'noted'."),
                ChatMessageAssistant(content="noted"),
                ChatMessageUser(content="My favorite color is blue. Say 'noted'."),
                ChatMessageAssistant(content="noted"),
                ChatMessageUser(content="I live in Paris. Say 'noted'."),
                ChatMessageAssistant(content="noted"),
                ChatMessageUser(content="Summarize what you know about me in one sentence."),
            ],
            target=["Alice", "blue", "Paris"],  # All must be included
            metadata={"category": "memory", "test": "multi_fact_recall"},
        ),
        # Long conversation chain
        Sample(
            input=[
                ChatMessageUser(content="Step 1: Remember A=1. Say 'ok'."),
                ChatMessageAssistant(content="ok"),
                ChatMessageUser(content="Step 2: Remember B=2. Say 'ok'."),
                ChatMessageAssistant(content="ok"),
                ChatMessageUser(content="Step 3: Remember C=3. Say 'ok'."),
                ChatMessageAssistant(content="ok"),
                ChatMessageUser(content="Step 4: Remember D=4. Say 'ok'."),
                ChatMessageAssistant(content="ok"),
                ChatMessageUser(content="Step 5: What are the values of A, B, C, and D? List them."),
            ],
            target=["1", "2", "3", "4"],
            metadata={"category": "memory", "test": "long_chain_recall"},
        ),
    ],
    name="multi_fact",
)

# Edge case tests
edge_case_dataset = MemoryDataset(
    samples=[
        # Unicode handling
        Sample(
            input=[
                ChatMessageUser(content="The word is '日本語'. Say 'received'."),
                ChatMessageAssistant(content="received"),
                ChatMessageUser(content="What was the word? Reply with just that word."),
            ],
            target="日本語",
            metadata={"category": "edge_case", "test": "unicode_handling"},
        ),
    ],
    name="edge_cases",
)

# Simple arithmetic tests
arithmetic_dataset = MemoryDataset(
    samples=[
        Sample(
            input=[ChatMessageUser(content="What is 1+1? Just the number.")],
            target="2",
            metadata={"category": "arithmetic", "test": "simple_addition"},
        ),
        Sample(
            input=[ChatMessageUser(content="What is 2+2? Just the number.")],
            target="4",
            metadata={"category": "arithmetic", "test": "simple_addition_2"},
        ),
    ],
    name="arithmetic",
)


# =============================================================================
# Custom Scorers
# =============================================================================

@scorer(metrics=["accuracy"])
def includes_all():
    """Score that checks if all target strings are included in output."""
    async def score(state, target: Target) -> Score:
        output = state.output.completion.lower() if state.output.completion else ""

        if isinstance(target.target, list):
            # Check if all targets are included
            matches = sum(1 for t in target.target if t.lower() in output)
            total = len(target.target)
            value = matches / total if total > 0 else 0.0
            explanation = f"Found {matches}/{total} expected values"
        else:
            # Single target
            value = 1.0 if target.target.lower() in output else 0.0
            explanation = f"Target '{target.target}' {'found' if value else 'not found'}"

        return Score(
            value=value,
            answer=state.output.completion,
            explanation=explanation,
        )

    return score


def get_scorer_model() -> str:
    """Get the scorer model from settings."""
    settings = load_settings()
    inspect_settings = settings.get("inspect", {})
    return inspect_settings.get("scorer_model", "openai/gpt-4o-mini")


# =============================================================================
# Tasks
# =============================================================================

@task
def qbit_memory_eval() -> Task:
    """Evaluate session memory and recall."""
    return Task(
        dataset=memory_dataset,
        solver=[generate()],
        scorer=includes(),
        config=GenerateConfig(temperature=0),
    )


@task
def qbit_multi_fact_eval() -> Task:
    """Evaluate multi-fact recall and summarization."""
    return Task(
        dataset=multi_fact_dataset,
        solver=[generate()],
        scorer=includes_all(),
        config=GenerateConfig(temperature=0),
    )


@task
def qbit_edge_case_eval() -> Task:
    """Evaluate edge cases like Unicode handling."""
    return Task(
        dataset=edge_case_dataset,
        solver=[generate()],
        scorer=includes(),
        config=GenerateConfig(temperature=0),
    )


@task
def qbit_arithmetic_eval() -> Task:
    """Evaluate simple arithmetic."""
    return Task(
        dataset=arithmetic_dataset,
        solver=[generate()],
        scorer=includes(),
        config=GenerateConfig(temperature=0),
    )


@task
def qbit_full_eval() -> Task:
    """Full evaluation suite combining all datasets."""
    # Combine all datasets
    all_samples = (
        list(memory_dataset)
        + list(multi_fact_dataset)
        + list(edge_case_dataset)
        + list(arithmetic_dataset)
    )

    return Task(
        dataset=MemoryDataset(samples=all_samples, name="full"),
        solver=[generate()],
        scorer=includes_all(),
        config=GenerateConfig(temperature=0),
    )


# =============================================================================
# Main entry point
# =============================================================================

if __name__ == "__main__":
    from inspect_ai import eval

    # Run the full eval by default
    results = eval(qbit_full_eval(), model="qbit-cli")
    print(f"\nResults: {results}")

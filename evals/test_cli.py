"""Integration tests for qbit-cli using DeepEval.

Run basic tests (no API needed):
    pytest test_cli.py -v -k "TestCliBasics"

Run all tests including API/eval tests:
    RUN_API_TESTS=1 pytest test_cli.py -v

Run tests in parallel (requires pytest-xdist):
    RUN_API_TESTS=1 pytest test_cli.py -v -n auto

Configure OpenAI model in settings.toml:
    [eval]
    model = "gpt-4o-mini"
    api_key = "sk-..."  # or use OPENAI_API_KEY env var

Test Organization:
- TestCliBasics: Fast CLI tests, no API needed
- TestCliBehavior: CLI behavior tests with API, no DeepEval (fast)
- TestMemoryAndState: Memory recall tests, concurrent execution
- TestResponseQuality: Arithmetic/instruction tests, concurrent execution
- TestCharacterHandling: Unicode/special char tests, concurrent execution
- TestToolUsage: Tool execution tests, concurrent execution
"""

import concurrent.futures
from typing import Any

import pytest
from deepeval import evaluate
from deepeval.metrics import GEval
from deepeval.test_case import LLMTestCase, LLMTestCaseParams

from conftest import CliRunner, JsonRunResult, get_last_response


# =============================================================================
# Helper Functions for Concurrent Test Execution
# =============================================================================


def run_cli_scenarios(cli: CliRunner, scenarios: list[dict]) -> list[dict]:
    """Run multiple CLI scenarios concurrently using ThreadPoolExecutor.

    Args:
        cli: The CLI runner instance
        scenarios: List of scenario dicts with 'prompts' or 'prompt' key

    Returns:
        List of completed scenarios with 'output' and 'success' fields added
    """

    def run_scenario(scenario: dict) -> dict:
        if "prompts" in scenario:
            # Batch mode
            result = cli.run_batch(scenario["prompts"], quiet=True)
            output = get_last_response(result.stdout) if result.returncode == 0 else ""
            return {
                **scenario,
                "result": result,
                "output": output,
                "success": result.returncode == 0,
            }
        elif "prompt" in scenario:
            # Single prompt mode (JSON)
            json_result = cli.run_prompt_json(scenario["prompt"])
            return {
                **scenario,
                "json_result": json_result,
                "output": json_result.response,
                "success": json_result.returncode == 0,
            }
        else:
            raise ValueError("Scenario must have 'prompts' or 'prompt' key")

    max_workers = min(len(scenarios), 5)  # Cap at 5 concurrent CLI processes
    with concurrent.futures.ThreadPoolExecutor(max_workers=max_workers) as executor:
        return list(executor.map(run_scenario, scenarios))


def evaluate_scenarios(
    scenarios: list[dict],
    eval_model: Any,
    max_concurrent: int = 5,
) -> None:
    """Build test cases and metrics from scenarios and evaluate concurrently.

    Args:
        scenarios: Completed scenarios with 'output' field
        eval_model: DeepEval model for evaluation
        max_concurrent: Max concurrent evaluations

    Raises:
        AssertionError: If any evaluations fail
    """
    test_cases = []
    metrics = []

    for scenario in scenarios:
        # Determine eval params based on scenario config
        eval_params = [LLMTestCaseParams.ACTUAL_OUTPUT]
        if scenario.get("use_context"):
            eval_params.append(LLMTestCaseParams.CONTEXT)
        else:
            eval_params.append(LLMTestCaseParams.EXPECTED_OUTPUT)

        test_case = LLMTestCase(
            input=scenario["input"],
            actual_output=scenario["output"],
            expected_output=scenario.get("expected", ""),
            context=scenario.get("context", []),
        )
        test_cases.append(test_case)

        metric = GEval(
            name=scenario["metric_name"],
            criteria=scenario["criteria"],
            evaluation_steps=scenario["steps"],
            evaluation_params=eval_params,
            threshold=scenario.get("threshold", 0.8),
            model=eval_model,
        )
        metrics.append(metric)

    # Run all evaluations concurrently
    results = evaluate(
        test_cases,
        metrics,
        run_async=True,
        max_concurrent=max_concurrent,
    )

    # Check results with detailed failure info
    failed = []
    for i, r in enumerate(results.test_results):
        if not r.success:
            failed.append({
                "name": scenarios[i]["metric_name"],
                "input": scenarios[i]["input"],
                "output": scenarios[i]["output"][:200] if scenarios[i]["output"] else "",
            })

    assert len(failed) == 0, f"DeepEval failed for {len(failed)} test(s): {failed}"


# =============================================================================
# Basic CLI Tests (no API needed) - Fast, parallelizable
# =============================================================================


class TestCliBasics:
    """Tests that don't require API credentials - instant execution."""

    def test_help(self, cli: CliRunner):
        """CLI shows help."""
        result = cli.run("--help")
        assert result.returncode == 0
        assert "--execute" in result.stdout
        assert "--file" in result.stdout
        assert "--auto-approve" in result.stdout
        assert "--json" in result.stdout
        assert "--quiet" in result.stdout

    def test_version(self, cli: CliRunner):
        """CLI shows version."""
        result = cli.run("--version")
        assert result.returncode == 0
        assert "qbit-cli" in result.stdout

    def test_conflicting_args(self, cli: CliRunner, temp_prompt_file):
        """Cannot use -e and -f together."""
        temp_prompt_file.write_text("test")
        result = cli.run("-e", "test", "-f", str(temp_prompt_file))
        assert result.returncode != 0
        assert "cannot be used with" in result.stderr

    def test_missing_file(self, cli: CliRunner):
        """Error on missing prompt file."""
        result = cli.run("-f", "/nonexistent/path.txt", "--auto-approve")
        assert result.returncode != 0

    def test_empty_file(self, cli: CliRunner, temp_prompt_file):
        """Error on empty prompt file."""
        temp_prompt_file.write_text("")
        result = cli.run("-f", str(temp_prompt_file), "--auto-approve")
        assert result.returncode != 0
        assert "No prompts found" in result.stderr

    def test_comments_only_file(self, cli: CliRunner, temp_prompt_file):
        """Error when file has only comments."""
        temp_prompt_file.write_text("# comment 1\n# comment 2\n")
        result = cli.run("-f", str(temp_prompt_file), "--auto-approve")
        assert result.returncode != 0
        assert "No prompts found" in result.stderr


# =============================================================================
# CLI Behavior Tests (API required, no DeepEval) - Fast, parallelizable
# =============================================================================


@pytest.mark.requires_api
class TestCliBehavior:
    """Tests that verify CLI behavior without DeepEval evaluation.

    These tests are fast because they only check CLI output structure,
    not LLM response quality. Can run in parallel with pytest-xdist.
    """

    def test_batch_progress_output(self, cli: CliRunner):
        """Batch mode shows progress."""
        result = cli.run_batch(["Say 'one'", "Say 'two'", "Say 'three'"], quiet=False)
        assert result.returncode == 0
        assert "[1/3]" in result.stderr
        assert "[2/3]" in result.stderr
        assert "[3/3]" in result.stderr
        assert "All 3 prompt(s) completed" in result.stderr

    def test_batch_skips_comments(self, cli: CliRunner, temp_prompt_file):
        """Batch mode skips comment lines."""
        temp_prompt_file.write_text(
            "# This is a comment\nSay 'first'\n# Another comment\n\nSay 'second'\n"
        )
        result = cli.run("-f", str(temp_prompt_file), "--auto-approve")
        assert result.returncode == 0
        assert "[1/2]" in result.stderr
        assert "[2/2]" in result.stderr

    def test_json_output_structure(self, cli: CliRunner):
        """JSON output mode produces valid structured events."""
        result: JsonRunResult = cli.run_prompt_json("Say 'hello'")
        assert result.returncode == 0
        assert len(result.events) > 0, "Expected at least one event"
        event_types = {e.event for e in result.events}
        assert "started" in event_types
        assert "completed" in event_types
        for event in result.events:
            assert event.timestamp > 0
        assert result.response

    def test_json_event_sequence(self, cli: CliRunner):
        """JSON events arrive in correct order."""
        result: JsonRunResult = cli.run_prompt_json("Say 'test'")
        assert result.returncode == 0
        event_types = [e.event for e in result.events]
        started_idx = event_types.index("started") if "started" in event_types else -1
        completed_idx = event_types.index("completed") if "completed" in event_types else -1
        assert started_idx >= 0 and completed_idx >= 0
        assert started_idx < completed_idx
        timestamps = [e.timestamp for e in result.events]
        assert timestamps == sorted(timestamps)

    def test_unicode_in_json(self, cli: CliRunner):
        """Unicode characters are preserved in JSON output."""
        result: JsonRunResult = cli.run_prompt_json("Say the Japanese word '日本語'")
        assert result.returncode == 0
        assert len(result.events) > 0
        assert result.completed_event is not None
        if any(ord(c) > 127 for c in result.response):
            assert "\\u" not in result.response

    def test_newlines_in_json_response(self, cli: CliRunner):
        """Newlines don't break JSON parsing."""
        result: JsonRunResult = cli.run_prompt_json("Print 'line1' then 'line2' on separate lines")
        assert result.returncode == 0
        assert len(result.events) > 0
        event_types = {e.event for e in result.events}
        assert "started" in event_types and "completed" in event_types

    def test_tool_call_has_input(self, cli: CliRunner):
        """Tool calls include input parameters."""
        result: JsonRunResult = cli.run_prompt_json("Read the file conftest.py")
        assert result.returncode == 0
        assert len(result.tool_calls) > 0
        assert result.tool_calls[0].get("input") is not None

    def test_tool_result_has_output(self, cli: CliRunner):
        """Tool results include output."""
        result: JsonRunResult = cli.run_prompt_json("Read the file conftest.py")
        assert result.returncode == 0
        successful = [tr for tr in result.tool_results if tr.get("success")]
        assert len(successful) > 0
        assert successful[0].get("output") is not None

    def test_tool_sequence_correct(self, cli: CliRunner):
        """Tool calls precede results."""
        result: JsonRunResult = cli.run_prompt_json("Read conftest.py and summarize briefly")
        assert result.returncode == 0
        events = result.events
        call_idx = [i for i, e in enumerate(events) if e.event in ("tool_call", "tool_auto_approved")]
        result_idx = [i for i, e in enumerate(events) if e.event == "tool_result"]
        assert len(call_idx) > 0 and len(result_idx) > 0
        assert call_idx[0] < result_idx[0]

    def test_started_event_has_turn_id(self, cli: CliRunner):
        """Started event includes turn_id."""
        result: JsonRunResult = cli.run_prompt_json("Say hello")
        assert result.returncode == 0
        started = [e for e in result.events if e.event == "started"]
        assert len(started) == 1
        assert started[0].get("turn_id") is not None

    def test_text_delta_events_stream(self, cli: CliRunner):
        """Text delta events contain streaming chunks."""
        result: JsonRunResult = cli.run_prompt_json("Write a short sentence about cats")
        assert result.returncode == 0
        deltas = [e for e in result.events if e.event == "text_delta"]
        assert len(deltas) > 0
        for d in deltas:
            assert "delta" in d.data or "accumulated" in d.data

    def test_completed_event_has_duration(self, cli: CliRunner):
        """Completed event includes duration."""
        result: JsonRunResult = cli.run_prompt_json("What is 2+2?")
        assert result.returncode == 0
        assert result.duration_ms is not None and result.duration_ms > 0

    def test_all_event_types_recognized(self, cli: CliRunner):
        """All events have known types."""
        result: JsonRunResult = cli.run_prompt_json("Read conftest.py and summarize briefly")
        assert result.returncode == 0
        known = {
            "started", "text_delta", "tool_call", "tool_result", "tool_approval",
            "tool_auto_approved", "tool_denied", "reasoning", "completed", "error",
            "sub_agent_started", "sub_agent_tool_request", "sub_agent_tool_result",
            "sub_agent_completed", "sub_agent_error", "context_pruned", "context_warning",
            "tool_response_truncated", "loop_warning", "loop_blocked", "max_iterations_reached",
            "workflow_started", "workflow_step_started", "workflow_step_completed",
            "workflow_completed", "workflow_error",
        }
        for e in result.events:
            assert e.event in known, f"Unknown event: {e.event}"

    def test_json_result_convenience_methods(self, cli: CliRunner):
        """JsonRunResult convenience methods work."""
        result: JsonRunResult = cli.run_prompt_json("Read conftest.py and tell me about it")
        assert result.returncode == 0
        assert not result.has_tool("nonexistent_tool_xyz")
        if result.tool_calls:
            first = result.tool_calls[0].get("tool_name")
            assert result.has_tool(first)
        if result.tool_results:
            name = result.tool_results[0].get("tool_name")
            assert result.get_tool_output(name) is not None


# =============================================================================
# DeepEval Tests - Memory & State (runs concurrently)
# =============================================================================


@pytest.mark.requires_api
class TestMemoryAndState:
    """Tests for session memory and state tracking.

    Combines 5 memory/state scenarios, runs CLI concurrently, evaluates together.
    """

    def test_memory_recall(self, cli: CliRunner, eval_model):
        """Agent remembers facts across prompts - all scenarios run concurrently."""
        scenarios = [
            {
                "name": "number_recall",
                "prompts": [
                    "Remember: the magic number is 42. Just say 'OK'.",
                    "What is the magic number? Reply with just the number.",
                ],
                "input": "What is the magic number?",
                "expected": "42",
                "context": ["The magic number is 42."],
                "metric_name": "Number Recall",
                "criteria": "The response must contain the number 42.",
                "steps": ["Check if response contains 42", "Should be exactly or close to '42'"],
                "threshold": 0.8,
            },
            {
                "name": "word_recall",
                "prompts": [
                    "The secret word is 'elephant'. Just say 'understood'.",
                    "What was the secret word? Reply with just that word.",
                ],
                "input": "What was the secret word?",
                "expected": "elephant",
                "context": ["The secret word is 'elephant'."],
                "metric_name": "Word Recall",
                "criteria": "The response must contain 'elephant' (case-insensitive).",
                "steps": ["Check if response contains 'elephant'", "Case should not matter"],
                "threshold": 0.8,
            },
            {
                "name": "multi_fact_recall",
                "prompts": [
                    "My name is Alice. Say 'noted'.",
                    "My favorite color is blue. Say 'noted'.",
                    "I live in Paris. Say 'noted'.",
                    "Summarize what you know about me in one sentence.",
                ],
                "input": "Summarize what you know about me.",
                "expected": "Alice lives in Paris and her favorite color is blue.",
                "context": ["User's name is Alice", "Favorite color is blue", "Lives in Paris"],
                "metric_name": "Multi-Fact Recall",
                "criteria": "Summary must include: name (Alice), color (blue), location (Paris).",
                "steps": ["Check for Alice", "Check for blue", "Check for Paris", "No hallucinations"],
                "threshold": 0.9,
                "use_context": True,
            },
            {
                "name": "cumulative_calculation",
                "prompts": [
                    "I have 3 apples. Say 'noted'.",
                    "I buy 2 more apples. Say 'noted'.",
                    "How many apples do I have now? Just the number.",
                ],
                "input": "How many apples do I have now?",
                "expected": "5",
                "context": ["Had 3 apples", "Bought 2 more", "Total should be 5"],
                "metric_name": "Arithmetic Recall",
                "criteria": "Response must contain 5 (3 + 2 = 5).",
                "steps": ["Check if response contains 5", "Calculation 3 + 2 = 5 is correct"],
                "threshold": 0.9,
            },
            {
                "name": "long_chain_recall",
                "prompts": [
                    "Step 1: Remember A=1. Say 'ok'.",
                    "Step 2: Remember B=2. Say 'ok'.",
                    "Step 3: Remember C=3. Say 'ok'.",
                    "Step 4: Remember D=4. Say 'ok'.",
                    "Step 5: What are the values of A, B, C, and D? List them.",
                ],
                "input": "What are the values of A, B, C, and D?",
                "expected": "A=1, B=2, C=3, D=4",
                "context": ["A=1", "B=2", "C=3", "D=4"],
                "metric_name": "Long Chain Recall",
                "criteria": "Response must contain all four values: A=1, B=2, C=3, D=4.",
                "steps": ["Check for A=1", "Check for B=2", "Check for C=3", "Check for D=4"],
                "threshold": 0.9,
                "use_context": True,
            },
        ]

        # Run all CLI operations concurrently
        completed = run_cli_scenarios(cli, scenarios)

        # Verify all succeeded
        for s in completed:
            assert s["success"], f"CLI failed for {s['name']}: {s['result'].stderr}"

        # Evaluate all concurrently
        evaluate_scenarios(completed, eval_model, max_concurrent=5)


# =============================================================================
# DeepEval Tests - Response Quality (runs concurrently)
# =============================================================================


@pytest.mark.requires_api
class TestResponseQuality:
    """Tests for arithmetic and instruction following.

    Combines 4 response quality scenarios, runs CLI concurrently, evaluates together.
    """

    def test_arithmetic_and_instructions(self, cli: CliRunner, eval_model):
        """Arithmetic and instruction following - all scenarios run concurrently."""
        scenarios = [
            {
                "name": "basic_arithmetic_json",
                "prompt": "What is 1+1? Just the number.",
                "input": "What is 1+1?",
                "expected": "2",
                "metric_name": "Basic Arithmetic (JSON)",
                "criteria": "Response must contain the number 2.",
                "steps": ["Check if response contains '2'"],
                "threshold": 0.9,
            },
            {
                "name": "batch_arithmetic",
                "prompts": ["What is 2+2? Just the number."],
                "input": "What is 2+2?",
                "expected": "4",
                "metric_name": "Batch Arithmetic",
                "criteria": "Response must contain the number 4.",
                "steps": ["Check if response contains '4'"],
                "threshold": 0.9,
            },
            {
                "name": "instruction_following",
                "prompts": ["Say exactly: 'test response'"],
                "input": "Say exactly: 'test response'",
                "expected": "test response",
                "metric_name": "Instruction Following",
                "criteria": "Response should contain or closely match 'test response'.",
                "steps": ["Check if response contains 'test response' (case-insensitive)"],
                "threshold": 0.8,
            },
        ]

        completed = run_cli_scenarios(cli, scenarios)
        for s in completed:
            assert s["success"], f"CLI failed for {s['name']}"
        evaluate_scenarios(completed, eval_model, max_concurrent=3)


# =============================================================================
# DeepEval Tests - Character Handling (runs concurrently)
# =============================================================================


@pytest.mark.requires_api
class TestCharacterHandling:
    """Tests for unicode, special characters, and multiline responses.

    Combines 3 character handling scenarios, runs CLI concurrently, evaluates together.
    """

    def test_character_handling(self, cli: CliRunner, eval_model):
        """Unicode, special chars, multiline - all scenarios run concurrently."""
        scenarios = [
            {
                "name": "unicode_recall",
                "prompts": [
                    "The word is '日本語'. Say 'received'.",
                    "What was the word? Reply with just that word.",
                ],
                "input": "What was the word?",
                "expected": "日本語",
                "context": ["The word is '日本語'"],
                "metric_name": "Unicode Recall",
                "criteria": "Response must contain the Japanese characters '日本語'.",
                "steps": ["Check for '日本語'", "Unicode should be preserved exactly"],
                "threshold": 0.9,
            },
            {
                "name": "special_characters",
                "prompt": "Echo back exactly: @#$%^&*()",
                "input": "Echo back exactly: @#$%^&*()",
                "expected": "@#$%^&*()",
                "metric_name": "Special Character Handling",
                "criteria": "Response should contain some or all of: @#$%^&*()",
                "steps": ["Check for at least some special characters"],
                "threshold": 0.6,
            },
            {
                "name": "multiline_response",
                "prompt": "List the numbers 1, 2, 3 on separate lines.",
                "input": "List the numbers 1, 2, 3 on separate lines.",
                "expected": "1\n2\n3",
                "metric_name": "Multiline Output",
                "criteria": "Response should contain 1, 2, 3 each on separate lines or clearly listed.",
                "steps": ["Check for '1'", "Check for '2'", "Check for '3'", "Should be separated"],
                "threshold": 0.8,
            },
        ]

        completed = run_cli_scenarios(cli, scenarios)
        for s in completed:
            assert s["success"], f"CLI failed for {s['name']}"
        evaluate_scenarios(completed, eval_model, max_concurrent=3)


# =============================================================================
# DeepEval Tests - Tool Usage (runs concurrently)
# =============================================================================


@pytest.mark.requires_api
class TestToolUsage:
    """Tests for tool execution and file operations.

    Combines 2 tool usage scenarios, runs CLI concurrently, evaluates together.
    Also verifies tool calling behavior.
    """

    def test_file_operations(self, cli: CliRunner, eval_model):
        """File reading and directory listing - run concurrently."""
        scenarios = [
            {
                "name": "read_file",
                "prompt": "Read conftest.py and tell me what the CliRunner class does in one sentence.",
                "input": "What does the CliRunner class do?",
                "expected": "CliRunner is a helper class that runs CLI commands for testing.",
                "context": [
                    "conftest.py contains the CliRunner class",
                    "CliRunner wraps subprocess calls to qbit-cli",
                    "Methods include run(), run_prompt(), run_batch()",
                ],
                "metric_name": "File Reading Comprehension",
                "criteria": "Response should accurately describe what CliRunner does.",
                "steps": [
                    "Check if mentions CLI or command execution",
                    "Check if mentions running or testing",
                    "Should demonstrate understanding of file contents",
                ],
                "threshold": 0.7,
                "use_context": True,
                "verify_tool": {
                    "tools": {"read_file", "read", "file_read"},
                    "content_check": "CliRunner",
                },
            },
            {
                "name": "list_directory",
                "prompt": "What files are in the current directory? Just list a few.",
                "input": "What files are in the current directory?",
                "expected": "conftest.py, test_cli.py, pyproject.toml",
                "context": [
                    "Directory contains conftest.py",
                    "Directory contains test_cli.py",
                    "Directory contains pyproject.toml",
                ],
                "metric_name": "Directory Listing",
                "criteria": "Response should list at least one relevant file from the test directory.",
                "steps": [
                    "Check for conftest.py, test_cli.py, or pyproject.toml",
                    "Should indicate files were successfully listed",
                ],
                "threshold": 0.7,
                "use_context": True,
                "verify_tool": {
                    "tools": {"list_directory", "ls", "list_files", "glob", "list_dir"},
                },
            },
        ]

        # Run scenarios concurrently
        completed = run_cli_scenarios(cli, scenarios)

        # Verify CLI success and tool usage
        for s in completed:
            assert s["success"], f"CLI failed for {s['name']}"

            if verify := s.get("verify_tool"):
                json_result = s["json_result"]
                expected_tools = verify["tools"]
                tool_names = {tc.get("tool_name") for tc in json_result.tool_calls}
                assert tool_names & expected_tools, f"Expected tool from {expected_tools}. Got: {tool_names}"

                # Verify at least one tool succeeded
                successful = [tr for tr in json_result.tool_results if tr.get("success")]
                assert len(successful) > 0, "Expected at least one successful tool result"

                # Check content if specified
                if content_check := verify.get("content_check"):
                    tool_results = [
                        tr for tr in json_result.tool_results
                        if tr.get("tool_name") in expected_tools and tr.get("success")
                    ]
                    if tool_results:
                        output = tool_results[0].get("output", {})
                        content = output.get("content", "") if isinstance(output, dict) else str(output)
                        assert content_check in content, f"Tool output should contain '{content_check}'"

        # Evaluate response quality concurrently
        evaluate_scenarios(completed, eval_model, max_concurrent=2)

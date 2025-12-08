"""Integration tests for qbit-cli using DeepEval.

Run basic tests (no API needed):
    pytest tests/integration/test_cli.py -v -k "TestCliBasics"

Run all tests including API/eval tests:
    RUN_API_TESTS=1 pytest tests/integration/test_cli.py -v

Configure OpenAI model in settings.toml:
    [eval]
    model = "gpt-4o-mini"
    api_key = "sk-..."  # or use OPENAI_API_KEY env var
"""

import json

import pytest
from deepeval import evaluate
from deepeval.metrics import GEval
from deepeval.test_case import LLMTestCase, LLMTestCaseParams

from conftest import CliRunner, get_last_response


# =============================================================================
# Basic CLI Tests (no API needed)
# =============================================================================


class TestCliBasics:
    """Tests that don't require API credentials."""

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
# Session Continuity Tests (require API) - DeepEval
# =============================================================================


@pytest.mark.requires_api
class TestSessionContinuity:
    """Tests that verify session state persists across prompts using DeepEval."""

    def test_remembers_number(self, cli: CliRunner, eval_model):
        """Agent remembers a number across prompts."""
        result = cli.run_batch(
            [
                "Remember: the magic number is 42. Just say 'OK'.",
                "What is the magic number? Reply with just the number.",
            ],
            quiet=True,
        )
        assert result.returncode == 0, f"CLI failed: {result.stderr}"

        test_case = LLMTestCase(
            input="What is the magic number? Reply with just the number.",
            actual_output=get_last_response(result.stdout),
            expected_output="42",
            context=["The magic number is 42."],
        )

        memory_metric = GEval(
            name="Number Recall",
            criteria="The response must contain the number 42.",
            evaluation_steps=[
                "Check if the response contains the number 42",
                "The response should be exactly or close to '42'",
            ],
            evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT, LLMTestCaseParams.EXPECTED_OUTPUT],
            threshold=0.8,
            model=eval_model,
        )

        results = evaluate([test_case], [memory_metric])
        assert all(r.success for r in results.test_results), f"DeepEval failed: {results}"

    def test_remembers_word(self, cli: CliRunner, eval_model):
        """Agent remembers a word across prompts."""
        result = cli.run_batch(
            [
                "The secret word is 'elephant'. Just say 'understood'.",
                "What was the secret word? Reply with just that word.",
            ],
            quiet=True,
        )
        assert result.returncode == 0

        test_case = LLMTestCase(
            input="What was the secret word? Reply with just that word.",
            actual_output=get_last_response(result.stdout),
            expected_output="elephant",
            context=["The secret word is 'elephant'."],
        )

        memory_metric = GEval(
            name="Word Recall",
            criteria="The response must contain the word 'elephant' (case-insensitive).",
            evaluation_steps=[
                "Check if the response contains 'elephant'",
                "Case should not matter",
            ],
            evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT, LLMTestCaseParams.EXPECTED_OUTPUT],
            threshold=0.8,
            model=eval_model,
        )

        results = evaluate([test_case], [memory_metric])
        assert all(r.success for r in results.test_results), f"DeepEval failed: {results}"

    def test_remembers_multiple_facts(self, cli: CliRunner, eval_model):
        """Agent remembers multiple facts across prompts."""
        result = cli.run_batch(
            [
                "My name is Alice. Say 'noted'.",
                "My favorite color is blue. Say 'noted'.",
                "I live in Paris. Say 'noted'.",
                "Summarize what you know about me in one sentence.",
            ],
            quiet=True,
        )
        assert result.returncode == 0

        test_case = LLMTestCase(
            input="Summarize what you know about me in one sentence.",
            actual_output=get_last_response(result.stdout),
            expected_output="Alice lives in Paris and her favorite color is blue.",
            context=[
                "User's name is Alice",
                "User's favorite color is blue",
                "User lives in Paris",
            ],
        )

        correctness_metric = GEval(
            name="Multi-Fact Recall",
            criteria="The summary must accurately reflect all persisted user facts: name (Alice), color (blue), location (Paris).",
            evaluation_steps=[
                "Check that the name 'Alice' is mentioned",
                "Check that the color 'blue' is mentioned",
                "Check that the location 'Paris' is mentioned",
                "No hallucinated facts should be present",
            ],
            evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT, LLMTestCaseParams.CONTEXT],
            threshold=0.9,
            model=eval_model,
        )

        results = evaluate([test_case], [correctness_metric])
        assert all(r.success for r in results.test_results), f"DeepEval failed: {results}"

    def test_cumulative_calculation(self, cli: CliRunner, eval_model):
        """Agent tracks cumulative state."""
        result = cli.run_batch(
            [
                "I have 3 apples. Say 'noted'.",
                "I buy 2 more apples. Say 'noted'.",
                "How many apples do I have now? Just the number.",
            ],
            quiet=True,
        )
        assert result.returncode == 0

        test_case = LLMTestCase(
            input="How many apples do I have now? Just the number.",
            actual_output=get_last_response(result.stdout),
            expected_output="5",
            context=["User had 3 apples", "User bought 2 more apples", "Total should be 5"],
        )

        math_metric = GEval(
            name="Arithmetic Recall",
            criteria="The response must contain the number 5 (3 + 2 = 5).",
            evaluation_steps=[
                "Check if response contains the number 5",
                "The calculation 3 + 2 = 5 should be correctly reflected",
            ],
            evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT, LLMTestCaseParams.EXPECTED_OUTPUT],
            threshold=0.9,
            model=eval_model,
        )

        results = evaluate([test_case], [math_metric])
        assert all(r.success for r in results.test_results), f"DeepEval failed: {results}"

    def test_long_conversation_chain(self, cli: CliRunner, eval_model):
        """Agent maintains context through 5+ prompts."""
        result = cli.run_batch(
            [
                "Step 1: Remember A=1. Say 'ok'.",
                "Step 2: Remember B=2. Say 'ok'.",
                "Step 3: Remember C=3. Say 'ok'.",
                "Step 4: Remember D=4. Say 'ok'.",
                "Step 5: What are the values of A, B, C, and D? List them.",
            ],
            quiet=True,
        )
        assert result.returncode == 0

        test_case = LLMTestCase(
            input="What are the values of A, B, C, and D? List them.",
            actual_output=get_last_response(result.stdout),
            expected_output="A=1, B=2, C=3, D=4",
            context=["A=1", "B=2", "C=3", "D=4"],
        )

        long_memory_metric = GEval(
            name="Long Chain Recall",
            criteria="The response must contain all four values: A=1, B=2, C=3, D=4.",
            evaluation_steps=[
                "Check for A=1 or equivalent",
                "Check for B=2 or equivalent",
                "Check for C=3 or equivalent",
                "Check for D=4 or equivalent",
                "All four values must be present",
            ],
            evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT, LLMTestCaseParams.CONTEXT],
            threshold=0.9,
            model=eval_model,
        )

        results = evaluate([test_case], [long_memory_metric])
        assert all(r.success for r in results.test_results), f"DeepEval failed: {results}"


# =============================================================================
# Batch Processing Tests (require API) - DeepEval
# =============================================================================


@pytest.mark.requires_api
class TestBatchProcessing:
    """Tests for batch file execution."""

    def test_batch_progress_output(self, cli: CliRunner):
        """Batch mode shows progress."""
        result = cli.run_batch(
            ["Say 'one'", "Say 'two'", "Say 'three'"],
            quiet=False,
        )
        assert result.returncode == 0
        assert "[1/3]" in result.stderr
        assert "[2/3]" in result.stderr
        assert "[3/3]" in result.stderr
        assert "All 3 prompt(s) completed" in result.stderr

    def test_batch_skips_comments(self, cli: CliRunner, temp_prompt_file):
        """Batch mode skips comment lines."""
        temp_prompt_file.write_text(
            "# This is a comment\n"
            "Say 'first'\n"
            "# Another comment\n"
            "\n"  # Empty line
            "Say 'second'\n"
        )
        result = cli.run("-f", str(temp_prompt_file), "--auto-approve")
        assert result.returncode == 0
        # Should show 2 prompts, not 4
        assert "[1/2]" in result.stderr
        assert "[2/2]" in result.stderr

    def test_batch_quiet_mode(self, cli: CliRunner, eval_model):
        """Quiet mode suppresses streaming but shows final response."""
        result = cli.run_batch(
            ["What is 2+2? Just the number."],
            quiet=True,
        )
        assert result.returncode == 0

        test_case = LLMTestCase(
            input="What is 2+2? Just the number.",
            actual_output=get_last_response(result.stdout),
            expected_output="4",
        )

        arithmetic_metric = GEval(
            name="Simple Arithmetic",
            criteria="The response must contain the number 4.",
            evaluation_steps=["Check if response contains '4'"],
            evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT, LLMTestCaseParams.EXPECTED_OUTPUT],
            threshold=0.9,
            model=eval_model,
        )

        results = evaluate([test_case], [arithmetic_metric])
        assert all(r.success for r in results.test_results), f"DeepEval failed: {results}"


# =============================================================================
# Single Prompt Tests (require API) - DeepEval
# =============================================================================


@pytest.mark.requires_api
class TestSinglePrompt:
    """Tests for single prompt execution."""

    def test_basic_prompt(self, cli: CliRunner, eval_model):
        """Basic single prompt execution."""
        result = cli.run_prompt("What is 1+1? Just the number.", quiet=True)
        assert result.returncode == 0

        test_case = LLMTestCase(
            input="What is 1+1? Just the number.",
            actual_output=result.stdout.strip(),
            expected_output="2",
        )

        arithmetic_metric = GEval(
            name="Basic Arithmetic",
            criteria="The response must contain the number 2.",
            evaluation_steps=["Check if response contains '2'"],
            evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT, LLMTestCaseParams.EXPECTED_OUTPUT],
            threshold=0.9,
            model=eval_model,
        )

        results = evaluate([test_case], [arithmetic_metric])
        assert all(r.success for r in results.test_results), f"DeepEval failed: {results}"

    def test_json_output(self, cli: CliRunner):
        """JSON output mode produces valid JSON."""
        result = cli.run_prompt("Say 'hello'", json_output=True)
        assert result.returncode == 0

        # Each non-empty line should be valid JSON
        for line in result.stdout.strip().split("\n"):
            if line.strip():
                try:
                    data = json.loads(line)
                    assert "type" in data or isinstance(data, dict)
                except json.JSONDecodeError as e:
                    pytest.fail(f"Invalid JSON: {line}\nError: {e}")

    def test_quiet_mode(self, cli: CliRunner, eval_model):
        """Quiet mode only outputs final response."""
        result = cli.run_prompt("Say exactly: 'test response'", quiet=True)
        assert result.returncode == 0

        test_case = LLMTestCase(
            input="Say exactly: 'test response'",
            actual_output=result.stdout.strip(),
            expected_output="test response",
        )

        instruction_following_metric = GEval(
            name="Instruction Following",
            criteria="The response should contain or closely match 'test response'.",
            evaluation_steps=[
                "Check if response contains 'test response' (case-insensitive)",
            ],
            evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT, LLMTestCaseParams.EXPECTED_OUTPUT],
            threshold=0.8,
            model=eval_model,
        )

        results = evaluate([test_case], [instruction_following_metric])
        assert all(r.success for r in results.test_results), f"DeepEval failed: {results}"


# =============================================================================
# Edge Cases (require API) - DeepEval
# =============================================================================


@pytest.mark.requires_api
class TestEdgeCases:
    """Edge case tests."""

    def test_unicode_handling(self, cli: CliRunner, eval_model):
        """Agent handles Unicode correctly."""
        result = cli.run_batch(
            [
                "The word is '日本語'. Say 'received'.",
                "What was the word? Reply with just that word.",
            ],
            quiet=True,
        )
        assert result.returncode == 0

        test_case = LLMTestCase(
            input="What was the word? Reply with just that word.",
            actual_output=get_last_response(result.stdout),
            expected_output="日本語",
            context=["The word is '日本語'"],
        )

        unicode_metric = GEval(
            name="Unicode Recall",
            criteria="The response must contain the Japanese characters '日本語'.",
            evaluation_steps=[
                "Check if response contains '日本語'",
                "Unicode characters should be preserved exactly",
            ],
            evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT, LLMTestCaseParams.EXPECTED_OUTPUT],
            threshold=0.9,
            model=eval_model,
        )

        results = evaluate([test_case], [unicode_metric])
        assert all(r.success for r in results.test_results), f"DeepEval failed: {results}"

    def test_special_characters(self, cli: CliRunner, eval_model):
        """Agent handles special characters."""
        result = cli.run_prompt(
            "Echo back exactly: @#$%^&*()",
            quiet=True,
        )
        assert result.returncode == 0

        test_case = LLMTestCase(
            input="Echo back exactly: @#$%^&*()",
            actual_output=result.stdout.strip(),
            expected_output="@#$%^&*()",
        )

        special_char_metric = GEval(
            name="Special Character Handling",
            criteria="The response should contain some or all of the special characters: @#$%^&*()",
            evaluation_steps=[
                "Check if response contains at least some of the special characters",
                "Ideally should have @#$%^&*()",
            ],
            evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT, LLMTestCaseParams.EXPECTED_OUTPUT],
            threshold=0.6,  # Lower threshold since exact echo is hard
            model=eval_model,
        )

        results = evaluate([test_case], [special_char_metric])
        assert all(r.success for r in results.test_results), f"DeepEval failed: {results}"

    def test_multiline_response(self, cli: CliRunner, eval_model):
        """Agent can produce multiline responses."""
        result = cli.run_prompt(
            "List the numbers 1, 2, 3 on separate lines.",
            quiet=True,
        )
        assert result.returncode == 0

        test_case = LLMTestCase(
            input="List the numbers 1, 2, 3 on separate lines.",
            actual_output=result.stdout.strip(),
            expected_output="1\n2\n3",
        )

        multiline_metric = GEval(
            name="Multiline Output",
            criteria="The response should contain the numbers 1, 2, 3, each on separate lines or clearly listed.",
            evaluation_steps=[
                "Check if response contains '1'",
                "Check if response contains '2'",
                "Check if response contains '3'",
                "Numbers should be on separate lines or clearly separated",
            ],
            evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT, LLMTestCaseParams.EXPECTED_OUTPUT],
            threshold=0.8,
            model=eval_model,
        )

        results = evaluate([test_case], [multiline_metric])
        assert all(r.success for r in results.test_results), f"DeepEval failed: {results}"


# =============================================================================
# Tool Execution Tests (require API) - DeepEval
# =============================================================================


@pytest.mark.requires_api
class TestToolExecution:
    """Tests that verify tool execution works."""

    def test_read_file(self, cli: CliRunner, eval_model):
        """Agent can read files with auto-approve."""
        result = cli.run_prompt(
            "Read conftest.py and tell me what the CliRunner class does in one sentence.",
            quiet=True,
        )
        assert result.returncode == 0

        test_case = LLMTestCase(
            input="Read conftest.py and tell me what the CliRunner class does in one sentence.",
            actual_output=result.stdout.strip(),
            expected_output="CliRunner is a helper class that runs CLI commands for testing.",
            context=[
                "conftest.py contains the CliRunner class",
                "CliRunner wraps subprocess calls to the qbit-cli binary",
                "It has methods like run(), run_prompt(), and run_batch()",
            ],
        )

        tool_use_metric = GEval(
            name="File Reading Comprehension",
            criteria="The response should accurately describe what CliRunner does based on reading the file.",
            evaluation_steps=[
                "Check if response mentions CLI or command execution",
                "Check if response mentions running or testing",
                "Response should demonstrate understanding of the file contents",
            ],
            evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT, LLMTestCaseParams.CONTEXT],
            threshold=0.7,
            model=eval_model,
        )

        results = evaluate([test_case], [tool_use_metric])
        assert all(r.success for r in results.test_results), f"DeepEval failed: {results}"

    def test_list_directory(self, cli: CliRunner, eval_model):
        """Agent can list directories with auto-approve."""
        result = cli.run_prompt(
            "What files are in the current directory? Just list a few.",
            quiet=True,
        )
        assert result.returncode == 0

        test_case = LLMTestCase(
            input="What files are in the current directory? Just list a few.",
            actual_output=result.stdout.strip(),
            expected_output="conftest.py, test_cli.py, pyproject.toml",
            context=[
                "The directory contains conftest.py",
                "The directory contains test_cli.py",
                "The directory contains pyproject.toml",
            ],
        )

        directory_listing_metric = GEval(
            name="Directory Listing",
            criteria="The response should list at least one relevant file from the test directory.",
            evaluation_steps=[
                "Check if response mentions any of: conftest.py, test_cli.py, or pyproject.toml",
                "Response should indicate files were successfully listed",
            ],
            evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT, LLMTestCaseParams.CONTEXT],
            threshold=0.7,
            model=eval_model,
        )

        results = evaluate([test_case], [directory_listing_metric])
        assert all(r.success for r in results.test_results), f"DeepEval failed: {results}"

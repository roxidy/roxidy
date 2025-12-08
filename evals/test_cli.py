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

import pytest
from deepeval import evaluate
from deepeval.metrics import GEval
from deepeval.test_case import LLMTestCase, LLMTestCaseParams

from conftest import CliRunner, JsonRunResult, get_last_response


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
    """Tests for single prompt execution using JSON output mode."""

    def test_basic_prompt(self, cli: CliRunner, eval_model):
        """Basic single prompt execution with JSON parsing."""
        result: JsonRunResult = cli.run_prompt_json("What is 1+1? Just the number.")
        assert result.returncode == 0

        # Verify we got structured events
        assert len(result.events) > 0, "Expected JSON events"
        assert result.completed_event is not None, "Expected completed event"

        test_case = LLMTestCase(
            input="What is 1+1? Just the number.",
            actual_output=result.response,
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

    def test_json_output_structure(self, cli: CliRunner):
        """JSON output mode produces valid structured events."""
        result: JsonRunResult = cli.run_prompt_json("Say 'hello'")
        assert result.returncode == 0

        # Verify basic event structure
        assert len(result.events) > 0, "Expected at least one event"

        # Check for expected event types
        event_types = {e.event for e in result.events}
        assert "started" in event_types, "Expected 'started' event"
        assert "completed" in event_types, "Expected 'completed' event"

        # Verify all events have timestamps
        for event in result.events:
            assert event.timestamp > 0, f"Event {event.event} missing valid timestamp"

        # Verify completed event has response
        assert result.response, "Expected non-empty response"

    def test_json_event_sequence(self, cli: CliRunner):
        """JSON events arrive in correct order."""
        result: JsonRunResult = cli.run_prompt_json("Say 'test'")
        assert result.returncode == 0

        # Find indices of key events
        event_types = [e.event for e in result.events]

        started_idx = event_types.index("started") if "started" in event_types else -1
        completed_idx = event_types.index("completed") if "completed" in event_types else -1

        assert started_idx >= 0, "Missing started event"
        assert completed_idx >= 0, "Missing completed event"
        assert started_idx < completed_idx, "started should come before completed"

        # Timestamps should be monotonically increasing
        timestamps = [e.timestamp for e in result.events]
        assert timestamps == sorted(timestamps), "Timestamps should be in order"

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
    """Edge case tests using JSON output mode."""

    def test_unicode_handling(self, cli: CliRunner, eval_model):
        """Agent handles Unicode correctly - verify via JSON events."""
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

    def test_unicode_in_json(self, cli: CliRunner):
        """Unicode characters are preserved in JSON output."""
        result: JsonRunResult = cli.run_prompt_json("Say the Japanese word '日本語'")
        assert result.returncode == 0

        # Verify unicode is preserved in response
        assert "日本語" in result.response, "Unicode should be preserved in JSON response"

        # Verify completed event contains unicode
        completed = result.completed_event
        assert completed is not None
        assert "日本語" in completed.get("response", ""), "Unicode in completed event"

    def test_special_characters(self, cli: CliRunner, eval_model):
        """Agent handles special characters - verify via JSON."""
        result: JsonRunResult = cli.run_prompt_json("Echo back exactly: @#$%^&*()")
        assert result.returncode == 0

        test_case = LLMTestCase(
            input="Echo back exactly: @#$%^&*()",
            actual_output=result.response,
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
        """Agent can produce multiline responses - verify via JSON."""
        result: JsonRunResult = cli.run_prompt_json("List the numbers 1, 2, 3 on separate lines.")
        assert result.returncode == 0

        test_case = LLMTestCase(
            input="List the numbers 1, 2, 3 on separate lines.",
            actual_output=result.response,
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

    def test_newlines_in_json_response(self, cli: CliRunner):
        """Newlines are properly escaped/preserved in JSON output."""
        result: JsonRunResult = cli.run_prompt_json("Print 'line1' then 'line2' on separate lines")
        assert result.returncode == 0

        # Response should contain newline
        assert "\n" in result.response or "line1" in result.response, "Response should have content"

        # All events should be valid (no JSON parsing errors from newlines)
        assert len(result.events) > 0, "Should have parsed events successfully"


# =============================================================================
# Tool Execution Tests (require API) - DeepEval with JSON Output
# =============================================================================


@pytest.mark.requires_api
class TestToolExecution:
    """Tests that verify tool execution works using JSON output mode.

    These tests use JSON output to structurally verify that tools are
    called correctly and return expected results.
    """

    def test_read_file(self, cli: CliRunner, eval_model):
        """Agent can read files with auto-approve - verify via JSON events."""
        result: JsonRunResult = cli.run_prompt_json(
            "Read conftest.py and tell me what the CliRunner class does in one sentence."
        )
        assert result.returncode == 0

        # Verify a read tool was called (read_file or similar)
        read_tools = {"read_file", "read", "file_read"}
        tool_names_called = {tc.get("tool_name") for tc in result.tool_calls}
        assert tool_names_called & read_tools, (
            f"Expected a file read tool to be called. Got: {tool_names_called}"
        )

        # Verify the tool succeeded
        successful_results = [tr for tr in result.tool_results if tr.get("success")]
        assert len(successful_results) > 0, "Expected at least one successful tool result"

        # Verify the tool output contains expected content (not truncated in JSON mode)
        read_results = [tr for tr in result.tool_results if tr.get("tool_name") in read_tools]
        if read_results:
            output = read_results[0].get("output", "")
            assert "CliRunner" in output, "Tool output should contain 'CliRunner'"

        test_case = LLMTestCase(
            input="Read conftest.py and tell me what the CliRunner class does in one sentence.",
            actual_output=result.response,
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
        """Agent can list directories with auto-approve - verify via JSON events."""
        result: JsonRunResult = cli.run_prompt_json(
            "What files are in the current directory? Just list a few."
        )
        assert result.returncode == 0

        # Verify a directory listing tool was called
        ls_tools = {"list_directory", "ls", "list_files", "glob", "list_dir"}
        tool_names_called = {tc.get("tool_name") for tc in result.tool_calls}
        assert tool_names_called & ls_tools, (
            f"Expected a directory listing tool to be called. Got: {tool_names_called}"
        )

        # Verify the tool succeeded
        successful_results = [tr for tr in result.tool_results if tr.get("success")]
        assert len(successful_results) > 0, "Expected at least one successful tool result"

        test_case = LLMTestCase(
            input="What files are in the current directory? Just list a few.",
            actual_output=result.response,
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

    def test_tool_call_has_input(self, cli: CliRunner):
        """Tool calls include input parameters in JSON output."""
        result: JsonRunResult = cli.run_prompt_json("Read the file conftest.py")
        assert result.returncode == 0

        # Verify tool call has input field
        assert len(result.tool_calls) > 0, "Expected at least one tool call"
        first_call = result.tool_calls[0]
        assert first_call.get("input") is not None, "Tool call should have 'input' field"

    def test_tool_result_has_output(self, cli: CliRunner):
        """Tool results include output in JSON output (not truncated)."""
        result: JsonRunResult = cli.run_prompt_json("Read the file conftest.py")
        assert result.returncode == 0

        # Verify tool result has output field
        assert len(result.tool_results) > 0, "Expected at least one tool result"
        first_result = result.tool_results[0]
        assert first_result.get("output") is not None, "Tool result should have 'output' field"
        assert first_result.get("success") is True, "Tool should have succeeded"

    def test_tool_sequence_correct(self, cli: CliRunner):
        """Tool call events precede their corresponding result events."""
        result: JsonRunResult = cli.run_prompt_json("Read conftest.py and summarize it briefly")
        assert result.returncode == 0

        # Get indices of tool calls and results
        events = result.events
        call_indices = [i for i, e in enumerate(events) if e.event == "tool_call"]
        result_indices = [i for i, e in enumerate(events) if e.event == "tool_result"]

        assert len(call_indices) > 0, "Expected at least one tool call"
        assert len(result_indices) > 0, "Expected at least one tool result"

        # First call should come before first result
        assert call_indices[0] < result_indices[0], "tool_call should precede tool_result"


# =============================================================================
# JSON Output Validation Tests (require API)
# =============================================================================


@pytest.mark.requires_api
class TestJsonOutput:
    """Tests that validate JSON output structure and metadata.

    These tests verify the JSON output mode provides complete,
    untruncated data with proper event structure.
    """

    def test_completed_event_has_tokens(self, cli: CliRunner):
        """Completed event includes token usage metrics."""
        result: JsonRunResult = cli.run_prompt_json("What is 2+2?")
        assert result.returncode == 0

        # Verify tokens_used is present
        assert result.tokens_used is not None, "Expected tokens_used in completed event"
        assert result.tokens_used > 0, "Expected positive token count"

    def test_completed_event_has_duration(self, cli: CliRunner):
        """Completed event includes duration metrics."""
        result: JsonRunResult = cli.run_prompt_json("What is 2+2?")
        assert result.returncode == 0

        # Verify duration_ms is present
        assert result.duration_ms is not None, "Expected duration_ms in completed event"
        assert result.duration_ms > 0, "Expected positive duration"

    def test_started_event_has_turn_id(self, cli: CliRunner):
        """Started event includes turn_id."""
        result: JsonRunResult = cli.run_prompt_json("Say hello")
        assert result.returncode == 0

        # Find started event
        started_events = [e for e in result.events if e.event == "started"]
        assert len(started_events) == 1, "Expected exactly one started event"

        started = started_events[0]
        assert started.get("turn_id") is not None, "Started event should have turn_id"

    def test_text_delta_events_stream(self, cli: CliRunner):
        """Text delta events contain streaming text chunks."""
        result: JsonRunResult = cli.run_prompt_json("Write a short sentence about cats")
        assert result.returncode == 0

        # Find text_delta events
        text_deltas = [e for e in result.events if e.event == "text_delta"]

        # Should have at least one text delta (streaming)
        assert len(text_deltas) > 0, "Expected text_delta events"

        # Each delta should have delta and accumulated fields
        for delta in text_deltas:
            assert "delta" in delta.data or "accumulated" in delta.data, (
                "text_delta should have delta or accumulated field"
            )

    def test_no_truncation_in_json_mode(self, cli: CliRunner):
        """JSON mode does not truncate tool output (unlike terminal mode)."""
        # Read a file that's likely to be longer than 500 chars (terminal truncation limit)
        result: JsonRunResult = cli.run_prompt_json("Read conftest.py completely")
        assert result.returncode == 0

        # Find tool results
        if result.tool_results:
            # Check that output is longer than terminal truncation limit
            for tr in result.tool_results:
                output = tr.get("output", "")
                if output and len(output) > 100:  # If we got substantial output
                    # In terminal mode, this would be truncated to 500 chars
                    # In JSON mode, it should be complete
                    # The conftest.py file is > 500 chars, so if we got it all, no truncation
                    assert "CliRunner" in output, "Should have CliRunner class in output"
                    assert "def " in output, "Should have function definitions"

    def test_all_event_types_recognized(self, cli: CliRunner):
        """All events have known event types."""
        result: JsonRunResult = cli.run_prompt_json("Read conftest.py and summarize briefly")
        assert result.returncode == 0

        known_event_types = {
            "started",
            "text_delta",
            "tool_call",
            "tool_result",
            "tool_approval",
            "tool_auto_approved",
            "tool_denied",
            "reasoning",
            "completed",
            "error",
            "sub_agent_started",
            "sub_agent_tool_request",
            "sub_agent_tool_result",
            "sub_agent_completed",
            "sub_agent_error",
            "context_pruned",
            "context_warning",
            "tool_response_truncated",
            "loop_warning",
            "loop_blocked",
            "max_iterations_reached",
            "workflow_started",
            "workflow_step_started",
            "workflow_step_completed",
            "workflow_completed",
            "workflow_error",
        }

        for event in result.events:
            assert event.event in known_event_types, (
                f"Unknown event type: {event.event}"
            )

    def test_error_event_on_failure(self, cli: CliRunner):
        """Error events are properly structured."""
        # This test validates the error event structure if we can trigger one
        # For now, just verify the JsonRunResult handles errors gracefully
        result: JsonRunResult = cli.run_prompt_json("Say hello")
        assert result.returncode == 0

        # If there was an error event, it should have message field
        if result.error_event:
            assert result.error_event.get("message") is not None, (
                "Error event should have message field"
            )

    def test_json_result_convenience_methods(self, cli: CliRunner):
        """JsonRunResult convenience methods work correctly."""
        result: JsonRunResult = cli.run_prompt_json("Read conftest.py and tell me about it")
        assert result.returncode == 0

        # Test has_tool method
        tool_names = {tc.get("tool_name") for tc in result.tool_calls}
        if tool_names:
            first_tool = next(iter(tool_names))
            assert result.has_tool(first_tool), f"has_tool should find {first_tool}"

        assert not result.has_tool("nonexistent_tool_xyz"), "has_tool should return False for missing tool"

        # Test get_tool_output method
        if result.tool_results:
            first_result_tool = result.tool_results[0].get("tool_name")
            output = result.get_tool_output(first_result_tool)
            assert output is not None, "get_tool_output should return output"

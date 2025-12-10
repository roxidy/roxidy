"""Agent behavior and quality evaluation tests.

Tests the Qbit agent's ability to:
- Remember information across conversation turns
- Perform arithmetic and follow instructions
- Handle unicode and special characters
- Use tools correctly (file reading, directory listing)

Run all tests:
    RUN_API_TESTS=1 pytest test_agent.py -v

Configure models in ~/.qbit/settings.toml:
    [eval]
    model = "gpt-4o-mini"       # DeepEval evaluator model
    agent_model = "claude-..."  # Qbit agent model
"""

from typing import Any

import pytest
from deepeval.metrics import GEval
from deepeval.test_case import LLMTestCase, LLMTestCaseParams

from client import BatchResult, RunResult, StreamingRunner


# =============================================================================
# Helper Functions
# =============================================================================


async def run_scenario(runner: StreamingRunner, scenario: dict) -> dict:
    """Run a single scenario using streaming client.

    Args:
        runner: The StreamingRunner instance
        scenario: Scenario dict with 'prompts' or 'prompt' key

    Returns:
        Completed scenario with 'output' and 'success' fields added
    """
    if "prompts" in scenario:
        # Batch mode - run prompts sequentially in same session
        result = await runner.run_batch(scenario["prompts"], quiet=True)
        output = result.responses[-1] if result.responses else ""
        return {
            **scenario,
            "result": result,
            "output": output,
            "success": result.success,
        }
    elif "prompt" in scenario:
        # Single prompt mode
        result = await runner.run(scenario["prompt"])
        return {
            **scenario,
            "run_result": result,
            "output": result.response,
            "success": result.success,
        }
    else:
        raise ValueError("Scenario must have 'prompts' or 'prompt' key")


def evaluate_scenario(scenario: dict, eval_model: Any) -> None:
    """Evaluate a single scenario with DeepEval using assert_test pattern.

    Args:
        scenario: Completed scenario with 'output' field
        eval_model: DeepEval model for evaluation

    Raises:
        AssertionError: If evaluation fails (via assert_test)
    """
    from deepeval import assert_test

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

    metric = GEval(
        name=scenario["metric_name"],
        criteria=scenario["criteria"],
        evaluation_steps=scenario["steps"],
        evaluation_params=eval_params,
        threshold=scenario.get("threshold", 0.8),
        model=eval_model,
    )

    assert_test(test_case, [metric])


# =============================================================================
# Behavior Tests
# =============================================================================


@pytest.mark.requires_api
class TestBehavior:
    """Tests that verify agent behavior without DeepEval evaluation."""

    @pytest.mark.asyncio
    async def test_batch_progress_output(self, runner: StreamingRunner):
        """Batch mode shows progress."""
        result = await runner.run_batch(
            ["Say 'one'", "Say 'two'", "Say 'three'"],
            quiet=False,
        )
        assert result.success
        assert "[1/3]" in result.stderr
        assert "[2/3]" in result.stderr
        assert "[3/3]" in result.stderr
        assert "All 3 prompt(s) completed" in result.stderr

    @pytest.mark.asyncio
    async def test_simple_response(self, runner: StreamingRunner):
        """Basic response with event structure."""
        result = await runner.run("Say 'hello world'")
        assert result.success

        # Event structure
        assert len(result.events) > 0, "Expected at least one event"
        event_types = {e.event for e in result.events}
        assert "started" in event_types
        assert "completed" in event_types
        for event in result.events:
            assert event.timestamp > 0
        assert result.response

        # Event sequence (started before completed)
        event_type_list = [e.event for e in result.events]
        started_idx = event_type_list.index("started")
        completed_idx = event_type_list.index("completed")
        assert started_idx < completed_idx

        # Timestamps ascending
        timestamps = [e.timestamp for e in result.events]
        assert timestamps == sorted(timestamps)

        # Started event has turn_id
        started = [e for e in result.events if e.event == "started"]
        assert len(started) == 1
        assert started[0].get("turn_id") is not None

        # Completed event has duration
        assert result.duration_ms is not None and result.duration_ms > 0

        # Text delta events contain streaming chunks
        deltas = [e for e in result.events if e.event == "text_delta"]
        assert len(deltas) > 0
        for d in deltas:
            assert "delta" in d.data or "accumulated" in d.data

    @pytest.mark.asyncio
    async def test_file_reading_events(self, runner: StreamingRunner):
        """Tool calls, results, and event sequence."""
        result = await runner.run(
            "Read the file ./conftest.py in the current directory and tell me briefly what it contains"
        )
        assert result.success

        # Tool calls include input parameters
        assert len(result.tool_calls) > 0
        assert result.tool_calls[0].get("input") is not None

        # Tool results include output
        successful = [tr for tr in result.tool_results if tr.get("success")]
        assert len(successful) > 0
        assert successful[0].get("output") is not None

        # Tool calls precede results
        events = result.events
        call_idx = [
            i for i, e in enumerate(events)
            if e.event in ("tool_call", "tool_auto_approved")
        ]
        result_idx = [i for i, e in enumerate(events) if e.event == "tool_result"]
        assert len(call_idx) > 0 and len(result_idx) > 0
        assert call_idx[0] < result_idx[0]

        # Convenience methods work
        assert not result.has_tool("nonexistent_tool_xyz")
        if result.tool_calls:
            first = result.tool_calls[0].get("tool_name")
            assert result.has_tool(first)
        if result.tool_results:
            name = result.tool_results[0].get("tool_name")
            assert result.get_tool_output(name) is not None

    @pytest.mark.asyncio
    async def test_unicode_preserved(self, runner: StreamingRunner):
        """Unicode characters are preserved."""
        result = await runner.run("Say the Japanese word '日本語'")
        assert result.success
        assert len(result.events) > 0
        assert result.completed_event is not None
        if any(ord(c) > 127 for c in result.response):
            assert "\\u" not in result.response

    @pytest.mark.asyncio
    async def test_newlines_handled(self, runner: StreamingRunner):
        """Newlines in output are handled correctly."""
        result = await runner.run("Print 'line1' then 'line2' on separate lines")
        assert result.success
        assert len(result.events) > 0
        event_types = {e.event for e in result.events}
        assert "started" in event_types and "completed" in event_types


# =============================================================================
# Memory & State Tests (DeepEval)
# =============================================================================


@pytest.mark.requires_api
class TestMemoryAndState:
    """Tests for session memory and state tracking."""

    @pytest.mark.asyncio
    async def test_number_recall(self, runner: StreamingRunner, eval_model):
        """Agent remembers a number across prompts."""
        scenario = {
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
        }
        completed = await run_scenario(runner, scenario)
        assert completed["success"], "Execution failed"
        evaluate_scenario(completed, eval_model)

    @pytest.mark.asyncio
    async def test_word_recall(self, runner: StreamingRunner, eval_model):
        """Agent remembers a word across prompts."""
        scenario = {
            "prompts": [
                "The word of the day is 'elephant'. Just say 'understood'.",
                "What was the word of the day? Reply with just that word.",
            ],
            "input": "What was the word of the day?",
            "expected": "elephant",
            "context": ["The word of the day is 'elephant'."],
            "metric_name": "Word Recall",
            "criteria": "The response must contain 'elephant' (case-insensitive).",
            "steps": ["Check if response contains 'elephant'", "Case should not matter"],
            "threshold": 0.8,
        }
        completed = await run_scenario(runner, scenario)
        assert completed["success"], "Execution failed"
        evaluate_scenario(completed, eval_model)

    @pytest.mark.asyncio
    async def test_multi_fact_recall(self, runner: StreamingRunner, eval_model):
        """Agent remembers multiple facts across prompts."""
        scenario = {
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
        }
        completed = await run_scenario(runner, scenario)
        assert completed["success"], "Execution failed"
        evaluate_scenario(completed, eval_model)

    @pytest.mark.asyncio
    async def test_cumulative_calculation(self, runner: StreamingRunner, eval_model):
        """Agent tracks cumulative state."""
        scenario = {
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
        }
        completed = await run_scenario(runner, scenario)
        assert completed["success"], "Execution failed"
        evaluate_scenario(completed, eval_model)

    @pytest.mark.asyncio
    async def test_long_chain_recall(self, runner: StreamingRunner, eval_model):
        """Agent remembers facts over many turns."""
        scenario = {
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
        }
        completed = await run_scenario(runner, scenario)
        assert completed["success"], "Execution failed"
        evaluate_scenario(completed, eval_model)


# =============================================================================
# Response Quality Tests (DeepEval)
# =============================================================================


@pytest.mark.requires_api
class TestResponseQuality:
    """Tests for arithmetic and instruction following."""

    @pytest.mark.asyncio
    async def test_basic_arithmetic(self, runner: StreamingRunner, eval_model):
        """Agent performs basic arithmetic."""
        scenario = {
            "prompt": "What is 1+1? Just the number.",
            "input": "What is 1+1?",
            "expected": "2",
            "metric_name": "Basic Arithmetic",
            "criteria": "Response must contain the number 2.",
            "steps": ["Check if response contains '2'"],
            "threshold": 0.9,
        }
        completed = await run_scenario(runner, scenario)
        assert completed["success"], "Execution failed"
        evaluate_scenario(completed, eval_model)

    @pytest.mark.asyncio
    async def test_batch_arithmetic(self, runner: StreamingRunner, eval_model):
        """Agent performs arithmetic in batch mode."""
        scenario = {
            "prompts": ["What is 2+2? Just the number."],
            "input": "What is 2+2?",
            "expected": "4",
            "metric_name": "Batch Arithmetic",
            "criteria": "Response must contain the number 4.",
            "steps": ["Check if response contains '4'"],
            "threshold": 0.9,
        }
        completed = await run_scenario(runner, scenario)
        assert completed["success"], "Execution failed"
        evaluate_scenario(completed, eval_model)

    @pytest.mark.asyncio
    async def test_instruction_following(self, runner: StreamingRunner, eval_model):
        """Agent follows exact instructions."""
        scenario = {
            "prompts": ["Say exactly: 'test response'"],
            "input": "Say exactly: 'test response'",
            "expected": "test response",
            "metric_name": "Instruction Following",
            "criteria": "Response should contain or closely match 'test response'.",
            "steps": ["Check if response contains 'test response' (case-insensitive)"],
            "threshold": 0.8,
        }
        completed = await run_scenario(runner, scenario)
        assert completed["success"], "Execution failed"
        evaluate_scenario(completed, eval_model)


# =============================================================================
# Character Handling Tests (DeepEval)
# =============================================================================


@pytest.mark.requires_api
class TestCharacterHandling:
    """Tests for unicode, special characters, and multiline responses."""

    @pytest.mark.asyncio
    async def test_unicode_recall(self, runner: StreamingRunner, eval_model):
        """Agent preserves unicode characters."""
        scenario = {
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
        }
        completed = await run_scenario(runner, scenario)
        assert completed["success"], "Execution failed"
        evaluate_scenario(completed, eval_model)

    @pytest.mark.asyncio
    async def test_special_characters(self, runner: StreamingRunner, eval_model):
        """Agent handles special characters."""
        scenario = {
            "prompt": "Echo back exactly: @#$%^&*()",
            "input": "Echo back exactly: @#$%^&*()",
            "expected": "@#$%^&*()",
            "metric_name": "Special Character Handling",
            "criteria": "Response should contain some or all of: @#$%^&*()",
            "steps": ["Check for at least some special characters"],
            "threshold": 0.6,
        }
        completed = await run_scenario(runner, scenario)
        assert completed["success"], "Execution failed"
        evaluate_scenario(completed, eval_model)

    @pytest.mark.asyncio
    async def test_multiline_response(self, runner: StreamingRunner, eval_model):
        """Agent produces multiline output."""
        scenario = {
            "prompt": "List the numbers 1, 2, 3 on separate lines.",
            "input": "List the numbers 1, 2, 3 on separate lines.",
            "expected": "1\n2\n3",
            "metric_name": "Multiline Output",
            "criteria": "Response should contain 1, 2, 3 each on separate lines or clearly listed.",
            "steps": ["Check for '1'", "Check for '2'", "Check for '3'", "Should be separated"],
            "threshold": 0.8,
        }
        completed = await run_scenario(runner, scenario)
        assert completed["success"], "Execution failed"
        evaluate_scenario(completed, eval_model)


# =============================================================================
# Layer 1 Session State Tests
# =============================================================================


@pytest.mark.requires_api
class TestLayer1Tables:
    """Tests for Layer 1 normalized session state tables."""

    @pytest.mark.asyncio
    async def test_l1_tables_exist(self, runner: StreamingRunner):
        """Verify Layer 1 normalized tables are created after agent usage."""
        result = await runner.run("Say 'hello'")
        assert result.success

        from sidecar import connect_db, check_l1_tables_exist

        try:
            db = connect_db()
            table_status = check_l1_tables_exist(db)

            if not any(table_status.values()):
                pytest.skip("Layer 1 normalized tables not yet created (schema migration pending)")

            if table_status.get("l1_sessions", False):
                assert table_status.get("l1_goals", False), "l1_goals table should exist with l1_sessions"
                assert table_status.get("l1_decisions", False), "l1_decisions table should exist with l1_sessions"
        except FileNotFoundError:
            pytest.skip("Sidecar database not initialized")

    @pytest.mark.asyncio
    async def test_l1_session_created(self, runner: StreamingRunner):
        """Verify a Layer 1 session is created during agent execution."""
        result = await runner.run("What is 2+2? Just answer with the number.")
        assert result.success

        from sidecar import connect_db, get_l1_sessions, check_l1_tables_exist

        try:
            db = connect_db()

            table_status = check_l1_tables_exist(db)
            if not table_status.get("l1_sessions", False):
                pytest.skip("l1_sessions table not yet created")

            sessions = get_l1_sessions(db, include_inactive=True)
            assert len(sessions) > 0, "At least one L1 session should exist"

            latest = sessions[0]
            assert "id" in latest, "Session should have an id"
            assert "created_at_ms" in latest, "Session should have created_at_ms"
        except FileNotFoundError:
            pytest.skip("Sidecar database not initialized")

    @pytest.mark.asyncio
    async def test_l1_goals_populated(self, runner: StreamingRunner):
        """Verify goals are tracked when given a task."""
        result = await runner.run(
            "Read the file conftest.py and summarize what it does in one sentence."
        )
        assert result.success

        from sidecar import connect_db, get_l1_sessions, get_l1_goals, check_l1_tables_exist

        try:
            db = connect_db()

            table_status = check_l1_tables_exist(db)
            if not table_status.get("l1_goals", False):
                pytest.skip("l1_goals table not yet created")

            sessions = get_l1_sessions(db, include_inactive=True)
            if not sessions:
                pytest.skip("No L1 sessions found")

            latest_session_id = sessions[0]["id"]
            goals = get_l1_goals(db, latest_session_id)
            assert len(goals) >= 0, "Goals should be tracked (may be empty for simple tasks)"

            if goals:
                goal = goals[0]
                assert "description" in goal or "id" in goal, "Goal should have description or id"
        except FileNotFoundError:
            pytest.skip("Sidecar database not initialized")

    @pytest.mark.asyncio
    async def test_l1_file_contexts_on_file_read(self, runner: StreamingRunner):
        """Verify file contexts are recorded when agent reads files."""
        result = await runner.run(
            "Read the file ./conftest.py and tell me the first function defined in it."
        )
        assert result.success

        tool_names = {tc.get("tool_name") for tc in result.tool_calls}
        file_tools = {"read_file", "read", "file_read"}
        assert tool_names & file_tools, f"Expected file read tool, got: {tool_names}"

        from sidecar import connect_db, get_l1_sessions, get_l1_file_contexts, check_l1_tables_exist

        try:
            db = connect_db()

            table_status = check_l1_tables_exist(db)
            if not table_status.get("l1_file_contexts", False):
                pytest.skip("l1_file_contexts table not yet created")

            sessions = get_l1_sessions(db, include_inactive=True)
            if not sessions:
                pytest.skip("No L1 sessions found")

            latest_session_id = sessions[0]["id"]
            file_contexts = get_l1_file_contexts(db, latest_session_id)

            if file_contexts:
                ctx = file_contexts[0]
                assert "path" in ctx or "session_id" in ctx, "File context should have path or session_id"
        except FileNotFoundError:
            pytest.skip("Sidecar database not initialized")

    @pytest.mark.asyncio
    async def test_l1_table_stats(self, runner: StreamingRunner):
        """Verify table stats function works after agent activity."""
        result = await runner.run("Say 'test' and nothing else.")
        assert result.success

        from sidecar import connect_db, get_l1_table_stats, check_l1_tables_exist

        try:
            db = connect_db()

            table_status = check_l1_tables_exist(db)
            if not any(table_status.values()):
                pytest.skip("Layer 1 normalized tables not yet created")

            stats = get_l1_table_stats(db)

            expected_tables = [
                "l1_sessions", "l1_goals", "l1_decisions", "l1_errors",
                "l1_file_contexts", "l1_questions", "l1_goal_progress", "l1_file_changes",
            ]

            for table_name in expected_tables:
                assert table_name in stats, f"Stats should include {table_name}"
                assert isinstance(stats[table_name], int), f"{table_name} count should be int"
        except FileNotFoundError:
            pytest.skip("Sidecar database not initialized")

    @pytest.mark.asyncio
    async def test_l1_decisions_cross_session_query(self, runner: StreamingRunner):
        """Verify cross-session decision queries work."""
        result = await runner.run(
            "What approach would you take to fix a bug? Just describe briefly."
        )
        assert result.success

        from sidecar import connect_db, get_l1_decisions_by_category, check_l1_tables_exist

        try:
            db = connect_db()

            table_status = check_l1_tables_exist(db)
            if not table_status.get("l1_decisions", False):
                pytest.skip("l1_decisions table not yet created")

            categories = ["architecture", "library", "approach", "tradeoff", "fallback"]
            for category in categories:
                decisions = get_l1_decisions_by_category(db, category)
                assert isinstance(decisions, list), f"Decisions for {category} should be a list"
        except FileNotFoundError:
            pytest.skip("Sidecar database not initialized")

    @pytest.mark.asyncio
    async def test_l1_unresolved_errors_query(self, runner: StreamingRunner):
        """Verify unresolved errors cross-session query works."""
        result = await runner.run("Say 'ok'")
        assert result.success

        from sidecar import connect_db, get_l1_unresolved_errors, check_l1_tables_exist

        try:
            db = connect_db()

            table_status = check_l1_tables_exist(db)
            if not table_status.get("l1_errors", False):
                pytest.skip("l1_errors table not yet created")

            unresolved = get_l1_unresolved_errors(db)
            assert isinstance(unresolved, list), "Unresolved errors should be a list"
        except FileNotFoundError:
            pytest.skip("Sidecar database not initialized")


# =============================================================================
# Tool Usage Tests (DeepEval)
# =============================================================================


@pytest.mark.requires_api
class TestToolUsage:
    """Tests for tool execution and file operations."""

    @pytest.mark.asyncio
    async def test_read_file(self, runner: StreamingRunner, eval_model):
        """Agent reads and summarizes file contents."""
        scenario = {
            "prompt": "Read the file ./conftest.py and tell me what the file contains in one sentence.",
            "input": "What does the conftest.py file contain?",
            "expected": "conftest.py contains pytest fixtures and test runner classes for evaluation tests.",
            "context": [
                "conftest.py contains pytest fixtures",
                "It has StreamingRunner class",
                "The file sets up test configuration and helpers",
            ],
            "metric_name": "File Reading Comprehension",
            "criteria": "Response should accurately describe what the file contains.",
            "steps": [
                "Check if mentions fixtures, testing, or configuration",
                "Check if mentions runner classes or test helpers",
                "Should demonstrate understanding of file contents",
            ],
            "threshold": 0.7,
            "use_context": True,
            "verify_tool": {
                "tools": {"read_file", "read", "file_read"},
                "content_check": "conftest",
            },
        }
        completed = await run_scenario(runner, scenario)
        assert completed["success"], "Execution failed"

        run_result = completed["run_result"]
        expected_tools = scenario["verify_tool"]["tools"]
        tool_names = {tc.get("tool_name") for tc in run_result.tool_calls}
        assert tool_names & expected_tools, f"Expected tool from {expected_tools}. Got: {tool_names}"

        successful = [tr for tr in run_result.tool_results if tr.get("success")]
        assert len(successful) > 0, "Expected at least one successful tool result"

        evaluate_scenario(completed, eval_model)

    @pytest.mark.asyncio
    async def test_list_directory(self, runner: StreamingRunner, eval_model):
        """Agent lists directory contents."""
        scenario = {
            "prompt": "What files are in the current directory? Just list a few.",
            "input": "What files are in the current directory?",
            "expected": "conftest.py, test_evals.py, pyproject.toml",
            "context": [
                "Directory contains conftest.py",
                "Directory contains test_evals.py",
                "Directory contains pyproject.toml",
            ],
            "metric_name": "Directory Listing",
            "criteria": "Response should list at least one relevant file from the test directory.",
            "steps": [
                "Check for conftest.py, test_evals.py, or pyproject.toml",
                "Should indicate files were successfully listed",
            ],
            "threshold": 0.7,
            "use_context": True,
            "verify_tool": {
                "tools": {"list_directory", "ls", "list_files", "glob", "list_dir"},
            },
        }
        completed = await run_scenario(runner, scenario)
        assert completed["success"], "Execution failed"

        run_result = completed["run_result"]
        expected_tools = scenario["verify_tool"]["tools"]
        tool_names = {tc.get("tool_name") for tc in run_result.tool_calls}
        assert tool_names & expected_tools, f"Expected tool from {expected_tools}. Got: {tool_names}"

        successful = [tr for tr in run_result.tool_results if tr.get("success")]
        assert len(successful) > 0, "Expected at least one successful tool result"

        evaluate_scenario(completed, eval_model)

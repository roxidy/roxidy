"""Integration tests for qbit-cli.

Run basic tests (no API needed):
    pytest tests/integration/test_cli.py -v

Run all tests including API tests:
    RUN_API_TESTS=1 pytest tests/integration/test_cli.py -v
"""

import json

import pytest

from conftest import CliRunner


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
# Session Continuity Tests (require API)
# =============================================================================


@pytest.mark.requires_api
class TestSessionContinuity:
    """Tests that verify session state persists across prompts."""

    def test_remembers_number(self, cli: CliRunner):
        """Agent remembers a number across prompts."""
        result = cli.run_batch(
            [
                "Remember: the magic number is 42. Just say 'OK'.",
                "What is the magic number? Reply with just the number.",
            ],
            quiet=True,
        )
        assert result.returncode == 0, f"CLI failed: {result.stderr}"
        assert "42" in result.stdout, f"Expected '42' in: {result.stdout}"

    def test_remembers_word(self, cli: CliRunner):
        """Agent remembers a word across prompts."""
        result = cli.run_batch(
            [
                "The secret word is 'elephant'. Just say 'understood'.",
                "What was the secret word? Reply with just that word.",
            ],
            quiet=True,
        )
        assert result.returncode == 0
        assert "elephant" in result.stdout.lower()

    def test_remembers_multiple_facts(self, cli: CliRunner):
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
        stdout_lower = result.stdout.lower()
        assert "alice" in stdout_lower, f"Missing 'alice' in: {result.stdout}"
        assert "blue" in stdout_lower, f"Missing 'blue' in: {result.stdout}"
        assert "paris" in stdout_lower, f"Missing 'paris' in: {result.stdout}"

    def test_cumulative_calculation(self, cli: CliRunner):
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
        assert "5" in result.stdout

    def test_long_conversation_chain(self, cli: CliRunner):
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
        for val in ["1", "2", "3", "4"]:
            assert val in result.stdout, f"Missing '{val}' in: {result.stdout}"


# =============================================================================
# Batch Processing Tests (require API)
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

    def test_batch_quiet_mode(self, cli: CliRunner):
        """Quiet mode suppresses streaming but shows final response."""
        result = cli.run_batch(
            ["What is 2+2? Just the number."],
            quiet=True,
        )
        assert result.returncode == 0
        # Response should be in stdout
        assert "4" in result.stdout


# =============================================================================
# Single Prompt Tests (require API)
# =============================================================================


@pytest.mark.requires_api
class TestSinglePrompt:
    """Tests for single prompt execution."""

    def test_basic_prompt(self, cli: CliRunner):
        """Basic single prompt execution."""
        result = cli.run_prompt("What is 1+1? Just the number.", quiet=True)
        assert result.returncode == 0
        assert "2" in result.stdout

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

    def test_quiet_mode(self, cli: CliRunner):
        """Quiet mode only outputs final response."""
        result = cli.run_prompt("Say exactly: 'test response'", quiet=True)
        assert result.returncode == 0
        # Output should be minimal
        assert "test response" in result.stdout.lower()


# =============================================================================
# Edge Cases (require API)
# =============================================================================


@pytest.mark.requires_api
class TestEdgeCases:
    """Edge case tests."""

    def test_unicode_handling(self, cli: CliRunner):
        """Agent handles Unicode correctly."""
        result = cli.run_batch(
            [
                "The word is '日本語'. Say 'received'.",
                "What was the word? Reply with just that word.",
            ],
            quiet=True,
        )
        assert result.returncode == 0
        assert "日本語" in result.stdout

    def test_special_characters(self, cli: CliRunner):
        """Agent handles special characters."""
        result = cli.run_prompt(
            "Echo back exactly: @#$%^&*()",
            quiet=True,
        )
        assert result.returncode == 0
        # At least some special chars should be preserved
        assert "@" in result.stdout or "#" in result.stdout

    def test_multiline_response(self, cli: CliRunner):
        """Agent can produce multiline responses."""
        result = cli.run_prompt(
            "List the numbers 1, 2, 3 on separate lines.",
            quiet=True,
        )
        assert result.returncode == 0
        assert "1" in result.stdout
        assert "2" in result.stdout
        assert "3" in result.stdout


# =============================================================================
# Tool Execution Tests (require API)
# =============================================================================


@pytest.mark.requires_api
class TestToolExecution:
    """Tests that verify tool execution works."""

    def test_read_file(self, cli: CliRunner):
        """Agent can read files with auto-approve."""
        # Use conftest.py which is in the workspace
        result = cli.run_prompt(
            "Read conftest.py and tell me what the CliRunner class does in one sentence.",
            quiet=True,
        )
        assert result.returncode == 0
        # Should mention something about CLI or running commands
        stdout_lower = result.stdout.lower()
        assert (
            "cli" in stdout_lower
            or "command" in stdout_lower
            or "run" in stdout_lower
        ), f"Unexpected response: {result.stdout}"

    def test_list_directory(self, cli: CliRunner):
        """Agent can list directories with auto-approve."""
        result = cli.run_prompt(
            "What files are in the current directory? Just list a few.",
            quiet=True,
        )
        assert result.returncode == 0
        # Should see the test files we created
        assert (
            "conftest" in result.stdout
            or "test_cli" in result.stdout
            or "pyproject" in result.stdout
        )

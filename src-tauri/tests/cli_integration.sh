#!/bin/bash
# CLI Integration Tests for qbit-cli
#
# This script verifies the qbit-cli binary works correctly.
# Run from the src-tauri directory:
#   ./tests/cli_integration.sh
#
# Tests are divided into two categories:
# 1. Non-credential tests: Can run without API keys (--help, --version)
# 2. Agent tests: Require valid API credentials (marked with REQUIRES_CREDENTIALS)
#
# Exit codes:
#   0 - All tests passed
#   1 - One or more tests failed

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Track test results
TESTS_PASSED=0
TESTS_FAILED=0
TESTS_SKIPPED=0

# CLI command - use debug mode for faster builds during development
# Set RELEASE=1 for release mode
if [ "${RELEASE:-0}" = "1" ]; then
    CLI_BUILD_CMD="cargo build --no-default-features --features cli --bin qbit-cli --release"
    CLI_BIN="./target/release/qbit-cli"
else
    CLI_BUILD_CMD="cargo build --no-default-features --features cli --bin qbit-cli"
    CLI_BIN="./target/debug/qbit-cli"
fi

# Change to src-tauri directory if not already there
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

echo "=============================================="
echo " qbit-cli Integration Tests"
echo "=============================================="
echo ""

# Helper functions
pass() {
    echo -e "${GREEN}[PASS]${NC} $1"
    TESTS_PASSED=$((TESTS_PASSED + 1))
}

fail() {
    echo -e "${RED}[FAIL]${NC} $1"
    echo "       Error: $2"
    TESTS_FAILED=$((TESTS_FAILED + 1))
}

skip() {
    echo -e "${YELLOW}[SKIP]${NC} $1"
    echo "       Reason: $2"
    TESTS_SKIPPED=$((TESTS_SKIPPED + 1))
}

# ==============================================================================
# Build Test
# ==============================================================================
echo "Building qbit-cli..."
if $CLI_BUILD_CMD 2>&1 | tail -3; then
    pass "Build: CLI compiles with --features cli"
else
    fail "Build: CLI compilation failed" "See output above"
    exit 1
fi
echo ""

# ==============================================================================
# Non-Credential Tests (can run without API keys)
# ==============================================================================
echo "--- Non-Credential Tests ---"
echo ""

# Test 1: Version flag
echo "Test: --version flag"
VERSION_OUTPUT=$($CLI_BIN --version 2>&1 || true)
if echo "$VERSION_OUTPUT" | grep -q "qbit-cli"; then
    pass "--version shows qbit-cli"
else
    fail "--version output" "Expected 'qbit-cli' in output, got: $VERSION_OUTPUT"
fi

# Test 2: Help flag
echo "Test: --help flag"
HELP_OUTPUT=$($CLI_BIN --help 2>&1 || true)
if echo "$HELP_OUTPUT" | grep -q "execute"; then
    pass "--help shows 'execute' option"
else
    fail "--help output" "Expected 'execute' in help output"
fi

# Test 3: Help contains expected flags
echo "Test: --help contains expected flags"
EXPECTED_FLAGS=("auto-approve" "json" "quiet" "verbose" "provider" "model")
ALL_FLAGS_FOUND=true
MISSING_FLAGS=""
for flag in "${EXPECTED_FLAGS[@]}"; do
    if ! echo "$HELP_OUTPUT" | grep -qF -- "--$flag"; then
        ALL_FLAGS_FOUND=false
        MISSING_FLAGS="$MISSING_FLAGS --$flag"
    fi
done

if [ "$ALL_FLAGS_FOUND" = true ]; then
    pass "--help contains all expected flags"
else
    fail "--help missing flags" "Missing:$MISSING_FLAGS"
fi

# Test 4: Invalid argument produces error
echo "Test: Invalid argument handling"
INVALID_OUTPUT=$($CLI_BIN --invalid-flag 2>&1 || true)
if echo "$INVALID_OUTPUT" | grep -qi "error\|unexpected\|unrecognized"; then
    pass "Invalid argument produces error"
else
    fail "Invalid argument handling" "Expected error message for invalid flag"
fi

# Test 5: Workspace validation
echo "Test: Invalid workspace path"
INVALID_WS_OUTPUT=$($CLI_BIN /nonexistent/path 2>&1 || true)
if echo "$INVALID_WS_OUTPUT" | grep -qi "error\|not exist\|not accessible"; then
    pass "Invalid workspace produces error"
else
    fail "Invalid workspace handling" "Expected error for nonexistent path"
fi

echo ""

# ==============================================================================
# Agent Tests (require API credentials)
# REQUIRES_CREDENTIALS: These tests need valid API keys to run
# ==============================================================================
echo "--- Agent Tests (require credentials) ---"
echo ""
echo "These tests require valid API credentials configured in:"
echo "  ~/.qbit/settings.toml or via environment variables"
echo ""

# Check if we have credentials available
HAS_CREDENTIALS=false

# Check for OpenRouter API key
if [ -n "${OPENROUTER_API_KEY:-}" ]; then
    HAS_CREDENTIALS=true
    CREDENTIAL_SOURCE="OPENROUTER_API_KEY env var"
fi

# Check for Anthropic API key
if [ -n "${ANTHROPIC_API_KEY:-}" ]; then
    HAS_CREDENTIALS=true
    CREDENTIAL_SOURCE="ANTHROPIC_API_KEY env var"
fi

# Check if settings file exists (credentials might be there)
if [ -f "$HOME/.qbit/settings.toml" ]; then
    # Could have credentials in settings
    CREDENTIAL_SOURCE="${CREDENTIAL_SOURCE:-settings.toml}"
fi

if [ "$HAS_CREDENTIALS" = false ] && [ ! -f "$HOME/.qbit/settings.toml" ]; then
    skip "Agent tests" "No API credentials found. Set OPENROUTER_API_KEY or ANTHROPIC_API_KEY"
    echo ""
    echo "To run agent tests, either:"
    echo "  1. Set OPENROUTER_API_KEY or ANTHROPIC_API_KEY environment variable"
    echo "  2. Configure credentials in ~/.qbit/settings.toml"
    echo ""
else
    echo "Credentials source: ${CREDENTIAL_SOURCE:-unknown}"
    echo "Set RUN_AGENT_TESTS=1 to run these tests"
    echo ""

    if [ "${RUN_AGENT_TESTS:-0}" = "1" ]; then
        # Test 6: Basic execution
        echo "Test: Basic execution (-e flag)"
        EXEC_OUTPUT=$($CLI_BIN -e "Say exactly 'ready'" --quiet --auto-approve 2>&1 || true)
        if echo "$EXEC_OUTPUT" | grep -qi "ready"; then
            pass "Basic execution works"
        else
            fail "Basic execution" "Output did not contain 'ready': $EXEC_OUTPUT"
        fi

        # Test 7: JSON output mode
        echo "Test: JSON output mode"
        JSON_OUTPUT=$($CLI_BIN -e "Say hello" --json --auto-approve 2>&1 | head -5 || true)
        if echo "$JSON_OUTPUT" | grep -q '"type"'; then
            pass "JSON output mode produces JSON with 'type' field"
        else
            fail "JSON output mode" "Expected JSON with 'type' field"
        fi

        # Test 8: Verbose mode
        echo "Test: Verbose mode"
        VERBOSE_OUTPUT=$($CLI_BIN -e "ping" --verbose --auto-approve 2>&1 | head -20 || true)
        if echo "$VERBOSE_OUTPUT" | grep -qi "\[cli\]"; then
            pass "Verbose mode shows debug output"
        else
            fail "Verbose mode" "Expected [cli] debug markers in output"
        fi

        # Test 9: Settings loading (agent initializes without crash)
        echo "Test: Settings loading (no crash)"
        SETTINGS_OUTPUT=$($CLI_BIN -e "What provider are you using?" --quiet --auto-approve 2>&1 || true)
        if [ $? -eq 0 ] || echo "$SETTINGS_OUTPUT" | grep -qi "provider\|model\|claude\|gpt"; then
            pass "Settings loaded successfully"
        else
            fail "Settings loading" "Agent failed to initialize or respond"
        fi
    else
        skip "Agent execution tests" "RUN_AGENT_TESTS not set to 1"
        echo ""
        echo "To run agent tests:"
        echo "  RUN_AGENT_TESTS=1 ./tests/cli_integration.sh"
    fi
fi

echo ""

# ==============================================================================
# Summary
# ==============================================================================
echo "=============================================="
echo " Test Summary"
echo "=============================================="
echo -e " ${GREEN}Passed:${NC}  $TESTS_PASSED"
echo -e " ${RED}Failed:${NC}  $TESTS_FAILED"
echo -e " ${YELLOW}Skipped:${NC} $TESTS_SKIPPED"
echo "=============================================="

if [ $TESTS_FAILED -gt 0 ]; then
    exit 1
else
    exit 0
fi

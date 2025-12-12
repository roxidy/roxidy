# Future Evaluation Tests

This document outlines evaluation tests to be implemented to increase confidence in the Qbit application behavior.

## Current Coverage Summary

| Area | Test Count | Status |
|------|------------|--------|
| Server API | 14 | Complete |
| Agent Behavior | 29 | Complete |
| Session Management | 9 | Complete |
| Sidecar | 15 | Complete |
| File Operations | 12 | Complete |

**Total: 84 tests passing, 2 skipped**

---

## Priority 1: Git Operations

Git is a core workflow for developers. These tests ensure the agent can safely interact with version control.

### Tests to Implement

- `test_git_status` - Agent can report working tree status
- `test_git_diff` - Agent can show staged/unstaged changes
- `test_git_add` - Agent can stage files (with approval)
- `test_git_commit` - Agent can create commits (with approval)
- `test_git_log` - Agent can read commit history
- `test_git_branch` - Agent can list/create branches
- `test_git_checkout` - Agent can switch branches (with approval)
- `test_git_stash` - Agent can stash/pop changes

### Safety Considerations

- Destructive operations (reset, force push) should require explicit approval
- Tests should use isolated git repos to avoid affecting real projects

---

## Priority 2: Tool Approval Rejection Flow (HITL)

Currently we test auto-approval but not the manual rejection path.

### Tests to Implement

- `test_tool_rejection_stops_execution` - Rejecting a tool stops the current action
- `test_tool_rejection_agent_recovers` - Agent can continue after rejection with alternative approach
- `test_multiple_rejections_graceful` - Agent handles repeated rejections gracefully
- `test_rejection_reason_captured` - Rejection reasons are logged in sidecar
- `test_partial_approval_workflow` - Approve some tools, reject others in sequence

---

## Priority 3: Context Window Management

Test behavior as conversations grow toward context limits.

### Tests to Implement

- `test_long_conversation_memory` - Agent maintains coherence over 20+ turns
- `test_large_file_read` - Agent handles reading files >100KB
- `test_many_tool_calls_in_session` - Session remains stable after 50+ tool calls
- `test_context_warning_emitted` - Agent warns when approaching limits
- `test_context_summarization` - Context is properly summarized when needed

---

## Priority 4: Extended Thinking / Reasoning Events

Verify reasoning events are properly emitted and captured.

### Tests to Implement

- `test_reasoning_event_emitted` - Complex prompts emit reasoning events
- `test_reasoning_content_captured` - Reasoning content is meaningful
- `test_reasoning_in_sidecar_log` - Reasoning appears in session log
- `test_reasoning_before_tool_call` - Reasoning precedes tool decisions

---

## Priority 5: Error Scenarios

Test resilience to various failure modes.

### Tests to Implement

- `test_network_timeout_recovery` - Agent handles API timeouts gracefully
- `test_malformed_tool_response` - Agent handles unexpected tool output
- `test_permission_denied_file` - Agent reports permission errors clearly
- `test_disk_full_scenario` - Agent handles write failures
- `test_concurrent_file_modification` - Agent detects external file changes
- `test_circular_dependency_detection` - Agent doesn't loop infinitely

---

## Priority 6: Edge Cases

Test unusual but valid inputs.

### Tests to Implement

- `test_unicode_file_paths` - Paths with unicode characters work
- `test_paths_with_spaces` - Paths with spaces are properly quoted
- `test_symlink_handling` - Symlinks are followed/reported correctly
- `test_empty_directory` - Agent handles empty directories
- `test_hidden_files` - Agent can see/ignore hidden files as appropriate
- `test_very_long_file_names` - Near-limit filename lengths work
- `test_binary_file_detection` - Agent identifies and handles binary files

---

## Priority 7: Performance / Load Testing

Ensure stability under load (may require separate test infrastructure).

### Tests to Implement

- `test_rapid_prompt_submission` - 10 prompts in quick succession
- `test_concurrent_sessions_stress` - Max sessions all executing
- `test_large_directory_listing` - Directory with 1000+ files
- `test_memory_stability` - No memory leaks over extended session
- `test_response_time_baseline` - Simple prompts respond within threshold

---

## Priority 8: Security Testing

Verify security boundaries are maintained.

### Tests to Implement

- `test_path_traversal_blocked` - Cannot access files outside workspace
- `test_command_injection_blocked` - Shell metacharacters are escaped
- `test_env_var_not_leaked` - Sensitive env vars not exposed in responses
- `test_credentials_not_logged` - API keys/tokens not in sidecar logs

---

## Implementation Notes

### Test Infrastructure Needs

1. **Isolated git repos** - Create temporary repos for git tests
2. **Mock rejection handler** - Simulate user rejecting tool calls
3. **Large file fixtures** - Pre-generated large files for context tests
4. **Timing utilities** - Measure response times for performance tests

### Running Subsets

```bash
# Run only git tests
just eval -k "git"

# Run only security tests
just eval -k "security"

# Run performance tests (slower)
just eval -k "performance" --timeout 600
```

### Maintenance

- Review and update this plan quarterly
- Archive completed items to a changelog
- Add new categories as features are added

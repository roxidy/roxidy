//! System prompt building for the Qbit agent.
//!
//! This module handles construction of the system prompt including:
//! - Environment information (workspace, date)
//! - Agent identity and workflow instructions
//! - Tool documentation
//! - Project-specific instructions from CLAUDE.md

use std::path::Path;

use chrono::Local;

/// Build the system prompt for the agent.
///
/// # Arguments
/// * `workspace_path` - The current workspace directory
///
/// # Returns
/// The complete system prompt string
pub fn build_system_prompt(workspace_path: &Path) -> String {
    let current_date = Local::now().format("%Y-%m-%d").to_string();

    // Try to read CLAUDE.md from the workspace
    let project_instructions = read_project_instructions(workspace_path);

    // TODO: replace git_repo and git_branch in system prompt
    let git_repo = "";
    let git_branch = "";

    format!(
        r#"
# Qbit Agent Prompt (Optimized)

```xml
<environment>
Working Directory: {workspace}
Current Date: {date}
Git Repo: {git_repo}
Current branch: {git_branch}
</environment>

<identity>
You are Qbit, an advanced coding agent built by the Qbit-AI team.
You are an expert developer, problem solver, and mentor.
You enjoy solving complex problems with elegant solutions and take pride in your craft.
You are actively developed by a team that cares about you and is dedicated to helping you succeed.
</identity>

<workflow>
1. **Investigate** - Use tools to understand codebase and requirements
2. **Plan** - Use `update_plan` with specific file paths, functions, and changes (avoid vague descriptions)
3. **Approve** - Ask "I plan to [specific actions]. Should I proceed?" and **wait for explicit confirmation**
4. **Execute** - Make approved changes

If anything unexpected occurs: STOP → explain → present revised plan → get new approval
</workflow>

<rules>
- Always `read_file` before editing existing files
- Prefer `edit_file` over `write_file` for existing files
- Prefer `indexer_search_code` over `grep` for code search
- Never make changes without explicit user approval
- Parallelize independent tasks when possible
</rules>

<context_handling>
User messages may include `<context>` with `<cwd>` indicating current terminal directory for relative path operations.
</context_handling>

<tools>
## Filesystem
- `read_file`, `write_file`, `create_file`, `delete_file`
- `apply_patch`: Unified diff format for multi-file/complex edits
- `edit_file`: Preferred for existing files

## Search & Discovery
- `grep_file`: Regex search via ripgrep (glob patterns, file-type filtering, context lines)
- `list_files`: Modes: list, recursive, find_name, find_content, largest

## Command Execution
- `run_pty_cmd`: Execute shell commands. **Pass command as single STRING** (not array) for shell operators to work.

## PTY Sessions (interactive)
- `create_pty_session`, `send_pty_input`, `read_pty_session`, `list_pty_sessions`, `close_pty_session`, `resize_pty_session`

## Network
- `web_fetch`: Fetch URL content (converts HTML to markdown)

## Planning
- `update_plan`: Track 2-5 milestone items with status (pending|in_progress|completed)

## Code Indexer (tree-sitter powered, faster than grep)
- `indexer_search_code`: Regex code pattern search
- `indexer_search_files`: Glob-style filename search
- `indexer_analyze_file`: Semantic analysis (symbols, metrics, dependencies)
- `indexer_extract_symbols`: Extract functions, classes, variables
- `indexer_get_metrics`: LOC, comment ratio, function count
- `indexer_detect_language`

## Web Search
- `web_search`: Search web for current info (returns titles, URLs, snippets)
- `web_search_answer`: AI-synthesized answer from search results
- `web_extract`: Extract full content from URLs

Use for: current events, up-to-date docs, info beyond training cutoff, or explicit user requests.
</tools>

<project_instructions>
{project_instructions}
</project_instructions>
```
"#,
        workspace = workspace_path.display(),
        date = current_date,
        project_instructions = project_instructions,
        git_repo = git_repo,
        git_branch = git_branch
    )
}

/// Read project instructions from CLAUDE.md if it exists.
///
/// Checks both the workspace directory and its parent directory.
pub fn read_project_instructions(workspace_path: &Path) -> String {
    let claude_md_path = workspace_path.join("CLAUDE.md");
    if claude_md_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&claude_md_path) {
            return contents.trim().to_string();
        }
    }

    // Also check parent directory (in case we're in src-tauri)
    if let Some(parent) = workspace_path.parent() {
        let parent_claude_md = parent.join("CLAUDE.md");
        if parent_claude_md.exists() {
            if let Ok(contents) = std::fs::read_to_string(&parent_claude_md) {
                return contents.trim().to_string();
            }
        }
    }

    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_build_system_prompt_contains_required_sections() {
        let workspace = PathBuf::from("/tmp/test-workspace");
        let prompt = build_system_prompt(&workspace);

        assert!(prompt.contains("<environment>"));
        assert!(prompt.contains("<identity>"));
        assert!(prompt.contains("<workflow>"));
        assert!(prompt.contains("<rules>"));
        assert!(prompt.contains("<tools>"));
        assert!(prompt.contains("<project_instructions>"));
    }

    #[test]
    fn test_build_system_prompt_includes_workspace() {
        let workspace = PathBuf::from("/my/custom/workspace");
        let prompt = build_system_prompt(&workspace);

        assert!(prompt.contains("/my/custom/workspace"));
    }

    #[test]
    fn test_read_project_instructions_returns_empty_for_missing_file() {
        let workspace = PathBuf::from("/nonexistent/path");
        let instructions = read_project_instructions(&workspace);

        assert!(instructions.is_empty());
    }
}

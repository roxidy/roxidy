//! Sub-agent system for running specialized agents as tools.
//!
//! This module provides the infrastructure for:
//! - Defining specialized sub-agents with custom system prompts and tool restrictions
//! - Executing sub-agents as tools from a parent agent
//! - Managing state and context between agents

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Context passed to a sub-agent during execution
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SubAgentContext {
    /// The original user request that triggered the workflow
    pub original_request: String,

    /// Summary of conversation history for context awareness
    pub conversation_summary: Option<String>,

    /// Variables passed from parent agent's state
    pub variables: HashMap<String, serde_json::Value>,

    /// Current depth in the agent hierarchy (to prevent infinite recursion)
    pub depth: usize,
}

/// Result returned by a sub-agent after execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentResult {
    /// ID of the sub-agent that produced this result
    pub agent_id: String,

    /// The agent's response text
    pub response: String,

    /// Updated context (may include new variables)
    pub context: SubAgentContext,

    /// Whether the sub-agent completed successfully
    pub success: bool,

    /// Execution duration in milliseconds
    pub duration_ms: u64,
}

/// Definition of a specialized sub-agent
#[derive(Clone, Debug)]
pub struct SubAgentDefinition {
    /// Unique identifier for this sub-agent
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Description for the parent agent to understand when to invoke this sub-agent
    pub description: String,

    /// System prompt that defines this sub-agent's role and capabilities
    pub system_prompt: String,

    /// List of tool names this sub-agent is allowed to use (empty = all tools)
    pub allowed_tools: Vec<String>,

    /// Maximum iterations for this sub-agent's tool loop
    pub max_iterations: usize,
}

impl SubAgentDefinition {
    /// Create a new sub-agent definition
    #[allow(dead_code)]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        system_prompt: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            system_prompt: system_prompt.into(),
            allowed_tools: Vec::new(),
            max_iterations: 50,
        }
    }

    /// Set allowed tools for this sub-agent
    #[allow(dead_code)]
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_tools = tools;
        self
    }

    /// Set maximum iterations
    #[allow(dead_code)]
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }
}

/// Registry of available sub-agents
#[derive(Default)]
pub struct SubAgentRegistry {
    agents: HashMap<String, SubAgentDefinition>,
}

impl SubAgentRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    /// Get a sub-agent by ID
    pub fn get(&self, id: &str) -> Option<&SubAgentDefinition> {
        self.agents.get(id)
    }

    /// Get all registered sub-agents
    pub fn all(&self) -> impl Iterator<Item = &SubAgentDefinition> {
        self.agents.values()
    }

    /// Get count of registered sub-agents
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    /// Check if registry is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }

    /// Register a sub-agent in the registry
    pub fn register(&mut self, agent: SubAgentDefinition) {
        self.agents.insert(agent.id.clone(), agent);
    }

    /// Register multiple sub-agents at once
    pub fn register_multiple(&mut self, agents: Vec<SubAgentDefinition>) {
        for agent in agents {
            self.register(agent);
        }
    }
}

/// Maximum recursion depth to prevent infinite sub-agent loops
pub const MAX_AGENT_DEPTH: usize = 5;

/// Create default sub-agents for common tasks
#[allow(dead_code)]
pub fn create_default_sub_agents() -> Vec<SubAgentDefinition> {
    vec![
        SubAgentDefinition::new(
            "code_analyzer",
            "Code Analyzer",
            "Analyzes code structure, identifies patterns, and provides insights about codebases. Use this agent when you need deep analysis of code without making changes.",
            r#"You are a specialized code analysis agent. Your role is to provide CONCISE, ACTIONABLE analysis.

## Key Rules
- **Do NOT show your thinking process** - only output the final analysis
- **Skip intermediate tool calls** - don't mention "Now let me look at...", "Let me search for..."
- **Be brief** - get straight to the point with key findings
- **Focus on what matters** - only include insights relevant to the question
- **No verbose explanations** - avoid lengthy descriptions unless specifically requested

## Analysis Tools
Use these semantic tools for deep insights (don't mention their use in your response):
- `indexer_analyze_file`, `indexer_extract_symbols`, `indexer_get_metrics`
- `indexer_search_code`, `indexer_search_files`, `indexer_detect_language`
- `read_file`, `grep_file`, `list_directory` for specific content

## Output Format
Start directly with findings. Use bullet points and concise explanations.
Example BAD response: "Now let me look at the streaming module... Now let me check the client... Here's what I found..."
Example GOOD response: "The streaming module handles SSE parsing in three key functions: parse_event(), accumulate_chunks(), and finalize_response()."

Do NOT modify any files. Provide clear, structured analysis with file paths and line numbers only when relevant."#,
        )
        .with_tools(vec![
            "read_file".to_string(),
            "grep_file".to_string(),
            "list_directory".to_string(),
            "find_files".to_string(),
            "indexer_search_code".to_string(),
            "indexer_search_files".to_string(),
            "indexer_analyze_file".to_string(),
            "indexer_extract_symbols".to_string(),
            "indexer_get_metrics".to_string(),
            "indexer_detect_language".to_string(),
        ])
        .with_max_iterations(30),

        SubAgentDefinition::new(
            "code_explorer",
            "Code Explorer",
            "Explores and maps a codebase to build context for a task. Use this agent when you need to understand how components relate, find integration points, trace dependencies, or navigate unfamiliar code before making decisions.",
            r#"You are a specialized code exploration agent. Your role is to EFFICIENTLY navigate and understand codebases to build context.

## Key Rules
- **Be systematic** - Start broad, then narrow down to specifics
- **Be concise** - Report only relevant findings, no fluff
- **Be thorough** - Follow the trail of dependencies and integrations
- **No modifications** - Only read and search, never modify files
- **No thinking out loud** - Don't narrate your process

## Exploration Strategy
1. **Start with the target** - Read the file(s) or module(s) in question
2. **Map connections** - Search for imports, usages, and references
3. **Understand structure** - List directories to see project organization
4. **Trace dependencies** - Follow imports and check configurations
5. **Verify state** - Run quick checks if needed (e.g., cargo check, tsc --noEmit)

## Output Format
Provide a structured summary with these sections as relevant:

**Key Files**
- `path/to/file.rs` - Brief description of purpose

**Integration Points**
- How components connect to each other

**Dependencies**
- External crates/packages and internal module dependencies

**Current State**
- Compilation status, any issues observed

**Summary**
- Direct answer to the exploration question

Do NOT output your thought process. Start directly with findings."#,
        )
        .with_tools(vec![
            "read_file".to_string(),
            "list_files".to_string(),
            "list_directory".to_string(),
            "grep_file".to_string(),
            "find_files".to_string(),
            "run_pty_cmd".to_string(),
        ])
        .with_max_iterations(40),

        SubAgentDefinition::new(
            "code_writer",
            "Code Writer",
            "Writes and modifies code based on specifications. Use this agent when you need to implement new features or make code changes.",
            r#"You are a specialized code writing agent.

## Response Style
- Be concise by default - output results, not process
- Explain when:
  - Something unexpected happens (errors, edge cases, failures)
  - A non-obvious decision was made
  - The result differs from what was requested
- No preambles ("I'll help you...") or postambles ("Let me know if...")

## Your Role
- Implement new features based on specifications
- Write clean, well-documented code
- Follow existing code patterns and conventions
- Create or modify files as needed

Before writing, analyze the existing codebase to understand patterns.
Report what was changed, not the process of changing it.

## apply_patch Format (CRITICAL)

Use `apply_patch` for multi-hunk edits. **Malformed patches corrupt files.**

```
*** Begin Patch
*** Update File: path/to/file.rs
@@ context line near the change
 context line (SPACE prefix required)
-line to remove (- prefix)
+line to add (+ prefix)
 more context (SPACE prefix required)
*** End Patch
```

### Rules
1. **Context lines MUST start with a space** (` `) - NOT raw text
2. **Additions start with `+`**, removals with `-`
3. **Use `@@` marker** with text to anchor the change location
4. **Include 3+ context lines** to uniquely identify location
5. Use `*** Add File: path` for new files, `*** Delete File: path` to remove

### Common Mistakes (AVOID)
- Context lines without space prefix
- Non-unique context that matches multiple locations
- Missing `*** End Patch` marker"#,
        )
        .with_tools(vec![
            "read_file".to_string(),
            "write_file".to_string(),
            "edit_file".to_string(),
            "create_file".to_string(),
            "grep_file".to_string(),
            "list_directory".to_string(),
            "apply_patch".to_string(),
            "indexer_search_code".to_string(),
            "indexer_search_files".to_string(),
            "indexer_analyze_file".to_string(),
            "indexer_extract_symbols".to_string(),
            "indexer_get_metrics".to_string(),
           "indexer_detect_language".to_string(),
       ])
       .with_max_iterations(50),

        SubAgentDefinition::new(
           "researcher",
            "Research Agent",
            "Researches topics by reading documentation, searching the web, and gathering information. Use this agent when you need to understand APIs, libraries, or gather external information.",
            r#"You are a specialized research agent.

## Response Style
- Be concise by default - output results, not process
- Explain when:
  - Information is conflicting or ambiguous
  - Sources are outdated or unreliable
  - The answer differs from common assumptions
- No preambles ("I'll help you...") or postambles ("Let me know if...")

## Your Role
- Search for documentation and examples
- Read and summarize technical documentation
- Find solutions to technical problems
- Gather information from multiple sources

Output format: Direct answer first, then supporting details with source references.
Focus on practical, actionable information."#,
        )
        .with_tools(vec![
            "web_search".to_string(),
            "web_fetch".to_string(),
            "read_file".to_string(),
        ])
        .with_max_iterations(25),

        SubAgentDefinition::new(
            "shell_executor",
            "Shell Command Executor",
            "Executes shell commands and manages system operations. Use this agent when you need to run commands, install packages, or perform system tasks.",
            r#"You are a specialized shell execution agent.

## Response Style
- Be concise by default - output results, not process
- Explain when:
  - Commands fail or produce unexpected output
  - A destructive operation is about to run (ask confirmation)
  - Environment issues are detected
- No preambles ("I'll help you...") or postambles ("Let me know if...")

## Your Role
- Execute shell commands safely
- Install packages and manage dependencies
- Run build processes
- Manage git operations

When using run_pty_cmd, pass the command as a STRING (not an array).
Example: {"command": "cd /path && npm install"}

Output format: Command result summary. Include full output only on failure."#,
        )
        .with_tools(vec![
            "run_pty_cmd".to_string(),
            "read_file".to_string(),
            "list_directory".to_string(),
        ])
        .with_max_iterations(30),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // SubAgentDefinition Tests
    // ===========================================

    #[test]
    fn test_sub_agent_definition_new() {
        let agent = SubAgentDefinition::new(
            "test_agent",
            "Test Agent",
            "A test agent for unit tests",
            "You are a test agent.",
        );

        assert_eq!(agent.id, "test_agent");
        assert_eq!(agent.name, "Test Agent");
        assert_eq!(agent.description, "A test agent for unit tests");
        assert_eq!(agent.system_prompt, "You are a test agent.");
        assert!(agent.allowed_tools.is_empty());
        assert_eq!(agent.max_iterations, 50); // default
    }

    #[test]
    fn test_sub_agent_definition_with_tools() {
        let agent = SubAgentDefinition::new("test", "Test", "desc", "prompt")
            .with_tools(vec!["read_file".to_string(), "write_file".to_string()]);

        assert_eq!(agent.allowed_tools.len(), 2);
        assert!(agent.allowed_tools.contains(&"read_file".to_string()));
        assert!(agent.allowed_tools.contains(&"write_file".to_string()));
    }

    #[test]
    fn test_sub_agent_definition_with_max_iterations() {
        let agent =
            SubAgentDefinition::new("test", "Test", "desc", "prompt").with_max_iterations(100);

        assert_eq!(agent.max_iterations, 100);
    }

    #[test]
    fn test_sub_agent_definition_builder_chain() {
        let agent = SubAgentDefinition::new("chained", "Chained Agent", "desc", "prompt")
            .with_tools(vec!["tool1".to_string()])
            .with_max_iterations(25);

        assert_eq!(agent.id, "chained");
        assert_eq!(agent.allowed_tools, vec!["tool1".to_string()]);
        assert_eq!(agent.max_iterations, 25);
    }

    // ===========================================
    // SubAgentRegistry Tests
    // ===========================================

    #[test]
    fn test_registry_new_is_empty() {
        let registry = SubAgentRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_default_is_empty() {
        let registry = SubAgentRegistry::default();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_registry_get_nonexistent() {
        let registry = SubAgentRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    // ===========================================
    // SubAgentContext Tests
    // ===========================================

    #[test]
    fn test_context_default() {
        let context = SubAgentContext::default();
        assert_eq!(context.original_request, "");
        assert!(context.conversation_summary.is_none());
        assert!(context.variables.is_empty());
        assert_eq!(context.depth, 0);
    }

    #[test]
    fn test_context_with_values() {
        let mut variables = HashMap::new();
        variables.insert("key".to_string(), serde_json::json!("value"));

        let context = SubAgentContext {
            original_request: "Do something".to_string(),
            conversation_summary: Some("Previous context".to_string()),
            variables,
            depth: 2,
        };

        assert_eq!(context.original_request, "Do something");
        assert_eq!(
            context.conversation_summary,
            Some("Previous context".to_string())
        );
        assert_eq!(
            context.variables.get("key").unwrap(),
            &serde_json::json!("value")
        );
        assert_eq!(context.depth, 2);
    }

    // ===========================================
    // SubAgentResult Tests
    // ===========================================

    #[test]
    fn test_result_construction() {
        let result = SubAgentResult {
            agent_id: "test_agent".to_string(),
            response: "Task completed".to_string(),
            context: SubAgentContext::default(),
            success: true,
            duration_ms: 1500,
        };

        assert_eq!(result.agent_id, "test_agent");
        assert_eq!(result.response, "Task completed");
        assert!(result.success);
        assert_eq!(result.duration_ms, 1500);
    }

    // ===========================================
    // create_default_sub_agents Tests
    // ===========================================

    #[test]
    fn test_create_default_sub_agents_count() {
        let agents = create_default_sub_agents();
        assert_eq!(agents.len(), 5);
    }

    #[test]
    fn test_create_default_sub_agents_ids() {
        let agents = create_default_sub_agents();
        let ids: Vec<&str> = agents.iter().map(|a| a.id.as_str()).collect();

        assert!(ids.contains(&"code_analyzer"));
        assert!(ids.contains(&"code_explorer"));
        assert!(ids.contains(&"code_writer"));
        assert!(ids.contains(&"researcher"));
        assert!(ids.contains(&"shell_executor"));
    }

    #[test]
    fn test_code_analyzer_has_read_only_tools() {
        let agents = create_default_sub_agents();
        let analyzer = agents.iter().find(|a| a.id == "code_analyzer").unwrap();

        assert!(analyzer.allowed_tools.contains(&"read_file".to_string()));
        assert!(!analyzer.allowed_tools.contains(&"write_file".to_string()));
        assert!(!analyzer.allowed_tools.contains(&"edit_file".to_string()));
    }

    #[test]
    fn test_code_explorer_has_navigation_tools() {
        let agents = create_default_sub_agents();
        let explorer = agents.iter().find(|a| a.id == "code_explorer").unwrap();

        // Should have navigation and search tools
        assert!(explorer.allowed_tools.contains(&"read_file".to_string()));
        assert!(explorer.allowed_tools.contains(&"list_files".to_string()));
        assert!(explorer
            .allowed_tools
            .contains(&"list_directory".to_string()));
        assert!(explorer.allowed_tools.contains(&"grep_file".to_string()));
        assert!(explorer.allowed_tools.contains(&"find_files".to_string()));
        assert!(explorer.allowed_tools.contains(&"run_pty_cmd".to_string()));

        // Should NOT have write tools
        assert!(!explorer.allowed_tools.contains(&"write_file".to_string()));
        assert!(!explorer.allowed_tools.contains(&"edit_file".to_string()));

        // Should NOT have indexer tools (those are for code_analyzer)
        assert!(!explorer
            .allowed_tools
            .contains(&"indexer_analyze_file".to_string()));
    }

    #[test]
    fn test_code_writer_has_write_tools() {
        let agents = create_default_sub_agents();
        let writer = agents.iter().find(|a| a.id == "code_writer").unwrap();

        assert!(writer.allowed_tools.contains(&"read_file".to_string()));
        assert!(writer.allowed_tools.contains(&"write_file".to_string()));
        assert!(writer.allowed_tools.contains(&"edit_file".to_string()));
    }

    #[test]
    fn test_researcher_has_web_tools() {
        let agents = create_default_sub_agents();
        let researcher = agents.iter().find(|a| a.id == "researcher").unwrap();

        assert!(researcher.allowed_tools.contains(&"web_search".to_string()));
        assert!(researcher.allowed_tools.contains(&"web_fetch".to_string()));
    }

    #[test]
    fn test_default_agents_have_reasonable_iterations() {
        let agents = create_default_sub_agents();

        for agent in &agents {
            assert!(
                agent.max_iterations >= 20,
                "{} has too few iterations",
                agent.id
            );
            assert!(
                agent.max_iterations <= 50,
                "{} has too many iterations",
                agent.id
            );
        }
    }

    // ===========================================
    // Constants Tests
    // ===========================================

    #[test]
    fn test_max_agent_depth() {
        assert_eq!(MAX_AGENT_DEPTH, 5);
    }
}

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
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_tools = tools;
        self
    }

    /// Set maximum iterations
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

    /// Register a sub-agent definition
    pub fn register(&mut self, agent: SubAgentDefinition) {
        self.agents.insert(agent.id.clone(), agent);
    }

    /// Get a sub-agent by ID
    pub fn get(&self, id: &str) -> Option<&SubAgentDefinition> {
        self.agents.get(id)
    }

    /// Get all registered sub-agents
    pub fn all(&self) -> impl Iterator<Item = &SubAgentDefinition> {
        self.agents.values()
    }

    /// Check if a sub-agent exists
    pub fn contains(&self, id: &str) -> bool {
        self.agents.contains_key(id)
    }

    /// Remove a sub-agent
    pub fn remove(&mut self, id: &str) -> Option<SubAgentDefinition> {
        self.agents.remove(id)
    }

    /// Get count of registered sub-agents
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }
}

/// Maximum recursion depth to prevent infinite sub-agent loops
pub const MAX_AGENT_DEPTH: usize = 5;

/// Create default sub-agents for common tasks
pub fn create_default_sub_agents() -> Vec<SubAgentDefinition> {
    vec![
        SubAgentDefinition::new(
            "code_analyzer",
            "Code Analyzer",
            "Analyzes code structure, identifies patterns, and provides insights about codebases. Use this agent when you need deep analysis of code without making changes.",
            r#"You are a specialized code analysis agent. Your role is to:
- Analyze code structure and architecture
- Identify patterns, anti-patterns, and potential issues
- Explain how code works
- Find dependencies and relationships between components

You have access to file reading and search tools. Do NOT modify any files.
Always provide clear, structured analysis with specific line references."#,
        )
        .with_tools(vec![
            "read_file".to_string(),
            "grep_file".to_string(),
            "list_directory".to_string(),
            "find_files".to_string(),
        ])
        .with_max_iterations(30),

        SubAgentDefinition::new(
            "code_writer",
            "Code Writer",
            "Writes and modifies code based on specifications. Use this agent when you need to implement new features or make code changes.",
            r#"You are a specialized code writing agent. Your role is to:
- Implement new features based on specifications
- Write clean, well-documented code
- Follow existing code patterns and conventions
- Create or modify files as needed

Before writing, analyze the existing codebase to understand patterns.
Always explain what changes you're making and why."#,
        )
        .with_tools(vec![
            "read_file".to_string(),
            "write_file".to_string(),
            "edit_file".to_string(),
            "create_file".to_string(),
            "grep_file".to_string(),
            "list_directory".to_string(),
        ])
        .with_max_iterations(50),

        SubAgentDefinition::new(
            "test_runner",
            "Test Runner",
            "Runs tests and analyzes results. Use this agent when you need to execute tests and understand failures.",
            r#"You are a specialized test execution agent. Your role is to:
- Run test suites and individual tests
- Analyze test failures and provide diagnostic information
- Suggest fixes for failing tests
- Report test coverage when available

Use shell commands to run tests appropriate for the project type.
Provide clear summaries of test results."#,
        )
        .with_tools(vec![
            "run_command".to_string(),
            "read_file".to_string(),
            "grep_file".to_string(),
        ])
        .with_max_iterations(20),

        SubAgentDefinition::new(
            "researcher",
            "Research Agent",
            "Researches topics by reading documentation, searching the web, and gathering information. Use this agent when you need to understand APIs, libraries, or gather external information.",
            r#"You are a specialized research agent. Your role is to:
- Search for documentation and examples
- Read and summarize technical documentation
- Find solutions to technical problems
- Gather information from multiple sources

Provide well-organized summaries with source references.
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
            r#"You are a specialized shell execution agent. Your role is to:
- Execute shell commands safely
- Install packages and manage dependencies
- Run build processes
- Manage git operations

Always explain what commands you're running and why.
Be cautious with destructive operations and ask for confirmation when appropriate."#,
        )
        .with_tools(vec![
            "run_command".to_string(),
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
        let agent = SubAgentDefinition::new("test", "Test", "desc", "prompt")
            .with_max_iterations(100);

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
    fn test_registry_register_and_get() {
        let mut registry = SubAgentRegistry::new();
        let agent = SubAgentDefinition::new("my_agent", "My Agent", "desc", "prompt");

        registry.register(agent);

        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);

        let retrieved = registry.get("my_agent");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "My Agent");
    }

    #[test]
    fn test_registry_get_nonexistent() {
        let registry = SubAgentRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_registry_contains() {
        let mut registry = SubAgentRegistry::new();
        registry.register(SubAgentDefinition::new("exists", "Exists", "d", "p"));

        assert!(registry.contains("exists"));
        assert!(!registry.contains("does_not_exist"));
    }

    #[test]
    fn test_registry_remove() {
        let mut registry = SubAgentRegistry::new();
        registry.register(SubAgentDefinition::new("to_remove", "Remove Me", "d", "p"));

        assert!(registry.contains("to_remove"));

        let removed = registry.remove("to_remove");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, "to_remove");
        assert!(!registry.contains("to_remove"));
        assert!(registry.is_empty());
    }

    #[test]
    fn test_registry_remove_nonexistent() {
        let mut registry = SubAgentRegistry::new();
        let removed = registry.remove("nonexistent");
        assert!(removed.is_none());
    }

    #[test]
    fn test_registry_all_iterates_agents() {
        let mut registry = SubAgentRegistry::new();
        registry.register(SubAgentDefinition::new("agent1", "Agent 1", "d", "p"));
        registry.register(SubAgentDefinition::new("agent2", "Agent 2", "d", "p"));
        registry.register(SubAgentDefinition::new("agent3", "Agent 3", "d", "p"));

        let all: Vec<_> = registry.all().collect();
        assert_eq!(all.len(), 3);

        let ids: Vec<&str> = all.iter().map(|a| a.id.as_str()).collect();
        assert!(ids.contains(&"agent1"));
        assert!(ids.contains(&"agent2"));
        assert!(ids.contains(&"agent3"));
    }

    #[test]
    fn test_registry_overwrite_existing() {
        let mut registry = SubAgentRegistry::new();
        registry.register(SubAgentDefinition::new("same_id", "Original", "d", "p"));
        registry.register(SubAgentDefinition::new("same_id", "Replaced", "d", "p"));

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get("same_id").unwrap().name, "Replaced");
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
        assert_eq!(context.conversation_summary, Some("Previous context".to_string()));
        assert_eq!(context.variables.get("key").unwrap(), &serde_json::json!("value"));
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
        assert!(ids.contains(&"code_writer"));
        assert!(ids.contains(&"test_runner"));
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
            assert!(agent.max_iterations >= 20, "{} has too few iterations", agent.id);
            assert!(agent.max_iterations <= 50, "{} has too many iterations", agent.id);
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

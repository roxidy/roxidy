//! Core traits and types for the workflow system.
//!
//! This module provides the generic abstractions that all workflows implement.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use graph_flow::Graph;
use serde::{Deserialize, Serialize};

/// Configuration for LLM calls within workflow tasks.
///
/// All fields are optional - unset fields use executor defaults.
///
/// # Example
///
/// ```rust,ignore
/// let config = WorkflowLlmConfig::default()
///     .with_temperature(0.3)
///     .with_max_tokens(4096)
///     .with_tools(vec!["read_file", "grep_file"]);
/// ```
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowLlmConfig {
    /// Model identifier override (e.g., "claude-3-haiku", "claude-3-sonnet").
    /// If None, uses the executor's default model.
    pub model: Option<String>,

    /// Temperature for generation (0.0-1.0).
    /// Lower = more deterministic, higher = more creative.
    /// If None, uses executor default (typically 0.7).
    pub temperature: Option<f32>,

    /// Maximum tokens for the response.
    /// If None, uses executor default.
    pub max_tokens: Option<u32>,

    /// Tools available to this task.
    /// - None: No tools (simple completion)
    /// - Some(vec![]): All available tools
    /// - Some(vec!["tool1", "tool2"]): Only specified tools
    pub tools: Option<Vec<String>>,

    /// Whether to enable extended thinking/reasoning.
    /// If true, the model can use chain-of-thought before responding.
    pub extended_thinking: Option<bool>,
}

#[allow(dead_code)]
impl WorkflowLlmConfig {
    /// Create a new config with a specific model.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the temperature.
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set max tokens.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Enable specific tools for this task.
    pub fn with_tools(mut self, tools: Vec<impl Into<String>>) -> Self {
        self.tools = Some(tools.into_iter().map(|t| t.into()).collect());
        self
    }

    /// Enable all available tools for this task.
    pub fn with_all_tools(mut self) -> Self {
        self.tools = Some(vec![]);
        self
    }

    /// Enable extended thinking.
    pub fn with_extended_thinking(mut self, enabled: bool) -> Self {
        self.extended_thinking = Some(enabled);
        self
    }
}

/// Result from an LLM completion that may include tool calls.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct WorkflowLlmResult {
    /// The text response from the LLM.
    pub text: String,

    /// Tool calls made during the completion (if tools were enabled).
    pub tool_calls: Vec<WorkflowToolCall>,

    /// Tool results from executed tools.
    pub tool_results: Vec<WorkflowToolResult>,
}

/// A tool call made by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowToolCall {
    /// Unique ID for this tool call.
    pub id: String,
    /// Name of the tool being called.
    pub name: String,
    /// Arguments passed to the tool.
    pub arguments: serde_json::Value,
}

/// Result from a tool execution.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowToolResult {
    /// ID of the tool call this result is for.
    pub tool_call_id: String,
    /// The result content.
    pub content: serde_json::Value,
    /// Whether the tool execution succeeded.
    pub success: bool,
}

/// Configuration for spawning a mini agent within a workflow task.
///
/// This allows workflow tasks to run a full agent loop with tool access,
/// rather than just a single LLM completion.
///
/// # Example
///
/// ```rust,ignore
/// let config = WorkflowAgentConfig::new(
///     "You are a code analyzer.",
///     "Analyze src/main.rs for bugs",
/// )
/// .with_tools(vec!["read_file", "grep_file"])
/// .with_max_iterations(15);
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowAgentConfig {
    /// System prompt for this agent.
    pub system_prompt: String,

    /// Initial task/message for the agent.
    pub task: String,

    /// Tools available to this agent.
    /// - None: No tools (simple completion, same as `complete()`)
    /// - Some(vec![]): All available tools
    /// - Some(vec!["tool1", "tool2"]): Only specified tools
    pub tools: Option<Vec<String>>,

    /// Maximum iterations before forcing stop.
    /// Default: 25
    pub max_iterations: Option<usize>,

    /// Model override (e.g., "claude-3-5-haiku" for faster execution).
    pub model: Option<String>,

    /// Temperature for generation.
    pub temperature: Option<f32>,

    /// Whether to emit events for tool calls (for UI visibility).
    /// Default: true
    pub emit_events: Option<bool>,

    /// Current workflow step name (for source tracking in events).
    pub step_name: Option<String>,

    /// Current workflow step index (0-based, for source tracking).
    pub step_index: Option<usize>,
}

impl WorkflowAgentConfig {
    /// Create a new agent config with required fields.
    pub fn new(system_prompt: impl Into<String>, task: impl Into<String>) -> Self {
        Self {
            system_prompt: system_prompt.into(),
            task: task.into(),
            tools: None,
            max_iterations: None,
            model: None,
            temperature: None,
            emit_events: None,
            step_name: None,
            step_index: None,
        }
    }

    /// Enable specific tools for this agent.
    pub fn with_tools(mut self, tools: Vec<impl Into<String>>) -> Self {
        self.tools = Some(tools.into_iter().map(|t| t.into()).collect());
        self
    }

    /// Enable all available tools.
    #[allow(dead_code)]
    pub fn with_all_tools(mut self) -> Self {
        self.tools = Some(vec![]);
        self
    }

    /// Set max iterations.
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = Some(max);
        self
    }

    /// Set model override.
    #[allow(dead_code)]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set temperature.
    #[allow(dead_code)]
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Set whether to emit events.
    pub fn with_emit_events(mut self, emit: bool) -> Self {
        self.emit_events = Some(emit);
        self
    }

    /// Set the workflow step context for source tracking.
    pub fn with_step(mut self, name: impl Into<String>, index: usize) -> Self {
        self.step_name = Some(name.into());
        self.step_index = Some(index);
        self
    }
}

/// Result from a mini agent execution within a workflow task.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowAgentResult {
    /// Final text response from the agent.
    pub response: String,

    /// All tool calls made during execution.
    pub tool_history: Vec<WorkflowToolCall>,

    /// Number of LLM iterations taken.
    pub iterations: usize,

    /// Total tokens used (if available).
    pub tokens_used: Option<u64>,

    /// Whether the agent completed successfully or hit max iterations.
    pub completed: bool,

    /// Error message if the agent failed.
    pub error: Option<String>,
}

/// A record of a tool call made during agent execution.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowToolHistory {
    /// Name of the tool called.
    pub tool_name: String,
    /// Arguments passed to the tool.
    pub arguments: serde_json::Value,
    /// Result from the tool.
    pub result: serde_json::Value,
    /// Whether the tool call succeeded.
    pub success: bool,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// Trait for executing LLM completions within workflow tasks.
///
/// This trait abstracts the LLM interaction, allowing workflow tasks
/// to request completions without directly depending on the agent bridge.
///
/// # Basic Usage
///
/// ```rust,ignore
/// // Simple completion (no tools)
/// let response = executor.complete(SYSTEM_PROMPT, &user_prompt, HashMap::new()).await?;
/// ```
///
/// # With Configuration
///
/// ```rust,ignore
/// // Completion with custom config and tools
/// let config = WorkflowLlmConfig::default()
///     .with_temperature(0.3)
///     .with_tools(vec!["read_file", "edit_file"]);
///
/// let result = executor.complete_with_config(
///     SYSTEM_PROMPT,
///     &user_prompt,
///     HashMap::new(),
///     config,
/// ).await?;
/// ```
#[async_trait]
pub trait WorkflowLlmExecutor: Send + Sync {
    /// Execute a simple LLM completion without tools.
    ///
    /// This is the basic completion method for tasks that don't need tools.
    ///
    /// # Arguments
    /// * `system_prompt` - System-level instructions for the LLM
    /// * `user_prompt` - The user's request/input
    /// * `context` - Additional context variables (currently unused, reserved for future)
    ///
    /// # Returns
    /// The LLM's response text
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        context: HashMap<String, serde_json::Value>,
    ) -> anyhow::Result<String>;

    /// Execute an LLM completion with configuration options.
    ///
    /// This method allows tasks to customize the LLM behavior including:
    /// - Model selection (use faster/cheaper models for simple tasks)
    /// - Temperature control
    /// - Tool access
    /// - Extended thinking
    ///
    /// The default implementation ignores configuration and delegates to `complete()`.
    /// Executors should override this to support full configuration.
    ///
    /// # Arguments
    /// * `system_prompt` - System-level instructions for the LLM
    /// * `user_prompt` - The user's request/input
    /// * `context` - Additional context variables
    /// * `config` - LLM configuration options
    ///
    /// # Returns
    /// A `WorkflowLlmResult` containing the response and any tool interactions
    #[allow(unused)]
    async fn complete_with_config(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        context: HashMap<String, serde_json::Value>,
        _config: WorkflowLlmConfig,
    ) -> anyhow::Result<WorkflowLlmResult> {
        // Default implementation: ignore config, call basic complete
        let text = self.complete(system_prompt, user_prompt, context).await?;
        Ok(WorkflowLlmResult {
            text,
            tool_calls: vec![],
            tool_results: vec![],
        })
    }

    /// Run a full agent loop for this task.
    ///
    /// This spawns a mini Qbit agent with its own system prompt, tools, and
    /// iteration loop. The agent runs until it produces a final response or
    /// hits the max iteration limit.
    ///
    /// Use this when a workflow task needs to:
    /// - Read/write files
    /// - Search code
    /// - Execute commands
    /// - Make multiple LLM calls with tool use
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let config = WorkflowAgentConfig::new(
    ///     "You are a code reviewer. Analyze the code and report issues.",
    ///     "Review the changes in src/lib.rs",
    /// )
    /// .with_tools(vec!["read_file", "grep_file"])
    /// .with_max_iterations(20);
    ///
    /// let result = executor.run_agent(config).await?;
    /// println!("Agent found: {}", result.response);
    /// println!("Made {} tool calls", result.tool_history.len());
    /// ```
    ///
    /// # Default Implementation
    ///
    /// The default implementation falls back to a simple completion without tools.
    /// Executors should override this to provide full agent capabilities.
    async fn run_agent(&self, config: WorkflowAgentConfig) -> anyhow::Result<WorkflowAgentResult> {
        // Default: fall back to simple completion (no tools)
        let text = self
            .complete(&config.system_prompt, &config.task, HashMap::new())
            .await?;

        Ok(WorkflowAgentResult {
            response: text,
            tool_history: vec![],
            iterations: 1,
            tokens_used: None,
            completed: true,
            error: None,
        })
    }

    /// Emit a workflow step started event for UI visibility.
    ///
    /// Tasks should call this at the beginning of their `run()` method.
    /// For tasks using `run_agent()`, this is called automatically.
    ///
    /// # Arguments
    /// * `step_name` - Human-readable name of the step (e.g., "analyzer")
    /// * `step_index` - Zero-based index of this step in the workflow
    /// * `total_steps` - Total number of steps in the workflow
    fn emit_step_started(&self, _step_name: &str, _step_index: usize, _total_steps: usize) {
        // Default: no-op. Executors should override to emit events.
    }

    /// Emit a workflow step completed event for UI visibility.
    ///
    /// Tasks should call this at the end of their `run()` method.
    /// For tasks using `run_agent()`, this is called automatically.
    ///
    /// # Arguments
    /// * `step_name` - Human-readable name of the step (e.g., "analyzer")
    /// * `output` - Optional output/summary from the step
    /// * `duration_ms` - How long the step took in milliseconds
    fn emit_step_completed(&self, _step_name: &str, _output: Option<&str>, _duration_ms: u64) {
        // Default: no-op. Executors should override to emit events.
    }
}

/// Trait that each workflow must implement.
///
/// A workflow definition describes how to build and initialize a workflow.
/// Each workflow type (git_commit, code_review, etc.) implements this trait.
///
/// # Example
///
/// ```rust,ignore
/// struct MyWorkflow;
///
/// impl WorkflowDefinition for MyWorkflow {
///     fn name(&self) -> &str { "my_workflow" }
///
///     fn build_graph(&self, executor: Arc<dyn WorkflowLlmExecutor>) -> Arc<Graph> {
///         // Build your graph here
///     }
///
///     fn init_state(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
///         // Initialize state from input
///     }
///
///     fn start_task(&self) -> &str { "initialize" }
///
///     fn state_key(&self) -> &str { "my_workflow_state" }
/// }
/// ```
#[allow(dead_code)]
pub trait WorkflowDefinition: Send + Sync {
    /// Unique name for this workflow (e.g., "git_commit", "code_review")
    fn name(&self) -> &str;

    /// Human-readable description of what this workflow does
    fn description(&self) -> &str {
        ""
    }

    /// Build the workflow graph with the given LLM executor
    fn build_graph(&self, executor: Arc<dyn WorkflowLlmExecutor>) -> Arc<Graph>;

    /// Initialize workflow state from input JSON.
    ///
    /// The input is workflow-specific (e.g., git status/diff for git_commit).
    /// Returns the initial state as JSON to be stored in the session context.
    fn init_state(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value>;

    /// The ID of the first task to run
    fn start_task(&self) -> &str;

    /// The context key used to store workflow state
    fn state_key(&self) -> &str;

    /// Number of tasks in this workflow (for progress tracking)
    #[allow(dead_code)]
    fn task_count(&self) -> usize {
        1 // Default to 1 if not implemented
    }
}

/// Information about a registered workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInfo {
    /// Unique name of the workflow
    pub name: String,
    /// Human-readable description
    pub description: String,
}

/// Response from starting a workflow.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartWorkflowResponse {
    /// Unique session ID for this workflow execution
    pub session_id: String,
    /// Name of the workflow that was started
    pub workflow_name: String,
}

/// Response from a workflow step.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStepResponse {
    /// Output from the step (if any)
    pub output: Option<String>,
    /// Current status: "running", "paused", "waiting_for_input", "completed", "error"
    pub status: String,
    /// ID of the next task to run (if paused)
    pub next_task_id: Option<String>,
    /// Error message (if status is "error")
    pub error: Option<String>,
}

/// Response from getting workflow state.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStateResponse {
    /// The workflow's current state as JSON
    pub state: serde_json::Value,
    /// Current status
    pub status: String,
    /// Current task ID
    pub current_task: String,
}

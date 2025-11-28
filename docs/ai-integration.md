# AI Integration with Rig

This document covers the AI agent architecture using [rig](https://github.com/0xPlaygrounds/rig) for multi-provider LLM support, including the graph-flow based multi-agent orchestration system.

> **API Verified:** 2025-11-27 against rig crate (latest). Key APIs confirmed:
> - `Client::new()` / `Client::from_env()` for providers
> - `client.completion_model()` returns `CompletionModel`
> - `AgentBuilder::new(model).preamble().tool().build()`
> - `Tool` trait with async `definition(_prompt: String)` and `call()`
> - `agent.stream_prompt()` yields `MultiTurnStreamItem::StreamItem(StreamedAssistantContent::*)` and `FinalResponse`

## Multi-Agent System with graph-flow

Roxidy uses [graph-flow](https://crates.io/crates/graph-flow) for multi-agent orchestration. This enables:

- **Sub-agents as tools**: Specialized agents can be invoked as tools from a parent agent
- **Graph-based routing**: Define workflows with conditional transitions between agents
- **Session persistence**: Workflow state persists across steps
- **Recursion protection**: Maximum depth of 5 to prevent infinite loops

### Sub-Agent System

Sub-agents are specialized agents with restricted tool access and focused system prompts:

```rust
// src-tauri/src/ai/sub_agent.rs

pub struct SubAgentDefinition {
    pub id: String,                    // Unique identifier
    pub name: String,                  // Human-readable name
    pub description: String,           // Description for parent agent
    pub system_prompt: String,         // Specialized role instructions
    pub allowed_tools: Vec<String>,    // Tool whitelist (empty = all)
    pub max_iterations: usize,         // Tool loop limit
}

// Default sub-agents included:
// - code_analyzer: Deep code analysis without modifications
// - code_writer: Implements features and makes changes
// - test_runner: Executes tests and analyzes failures
// - researcher: Web search and documentation lookup
// - shell_executor: Shell commands and system operations
```

### Workflow Graph with graph-flow

The workflow system uses the `graph-flow` crate for type-safe, session-based execution:

```rust
// src-tauri/src/ai/workflow.rs

use graph_flow::{Task, Context, TaskResult, NextAction};

// Sub-agent task implements graph-flow's Task trait
pub struct SubAgentTask {
    agent: SubAgentDefinition,
    executor: Arc<dyn SubAgentExecutor + Send + Sync>,
}

#[async_trait]
impl Task for SubAgentTask {
    fn id(&self) -> &str {
        &self.agent.id
    }

    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let prompt: String = context.get("prompt").await.unwrap_or_default();

        match self.executor.execute_agent(&self.agent, &prompt, HashMap::new()).await {
            Ok(response) => {
                context.set("response", response.clone()).await;
                Ok(TaskResult::new(Some(response), NextAction::Continue))
            }
            Err(e) => Ok(TaskResult::new(Some(e.to_string()), NextAction::End))
        }
    }
}

// Build workflows with the builder pattern
let graph = AgentWorkflowBuilder::new("code_review")
    .add_agent_task(analyzer_task)
    .add_agent_task(writer_task)
    .add_router_task(router)
    .edge("analyzer", "router")
    .conditional_edge("router", |ctx| {...}, "writer", "done")
    .build()?;

// Execute with session management
let runner = WorkflowRunner::new_in_memory(graph);
let session_id = runner.start_session("Review this code", "analyzer").await?;
let result = runner.run_to_completion(&session_id).await?;
```

### Sub-Agents as Tools

When the parent agent needs specialized help, it can invoke sub-agents as tools:

```rust
// AgentBridge automatically exposes sub-agents as tools
// Tool name format: sub_agent_{agent_id}

// Example tool call from LLM:
{
    "name": "sub_agent_code_analyzer",
    "arguments": {
        "task": "Analyze the authentication module for security issues",
        "context": "Focus on SQL injection and XSS vulnerabilities"
    }
}

// The parent agent receives a structured result:
{
    "agent_id": "code_analyzer",
    "response": "Found 3 potential issues...",
    "success": true,
    "duration_ms": 2340
}
```

### Sub-Agent Events

The event system tracks sub-agent execution for UI visibility:

```rust
// src-tauri/src/ai/events.rs

pub enum AiEvent {
    // ... existing events ...

    /// Sub-agent started executing a task
    SubAgentStarted {
        agent_id: String,
        agent_name: String,
        task: String,
        depth: usize,  // Recursion depth (max 5)
    },

    /// Sub-agent tool usage (for nested visibility)
    SubAgentToolRequest {
        agent_id: String,
        tool_name: String,
        args: serde_json::Value,
    },
    SubAgentToolResult {
        agent_id: String,
        tool_name: String,
        success: bool,
    },

    /// Sub-agent completed
    SubAgentCompleted {
        agent_id: String,
        response: String,
        duration_ms: u64,
    },

    /// Sub-agent error
    SubAgentError {
        agent_id: String,
        error: String,
    },
}
```

### Workflow Patterns

Pre-built patterns for common use cases:

```rust
// Sequential: task1 → task2 → task3
let graph = patterns::sequential("pipeline", vec![
    analyzer_task,
    writer_task,
    reviewer_task,
])?;

// Router dispatch: router decides which agent handles the task
let graph = patterns::router_dispatch("dispatch", router_task, vec![
    code_analyzer_task,
    test_runner_task,
    researcher_task,
])?;
```

## Provider Configuration

Roxidy supports multiple AI providers, configured via settings:

```rust
// src-tauri/src/ai/providers.rs

use rig::providers::{anthropic, openai, gemini};
use rig::completion::CompletionModel;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIConfig {
    pub provider: AIProvider,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,  // For OpenRouter, local models
    pub temperature: f32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AIProvider {
    Anthropic,
    OpenAI,
    Google,       // Vertex AI
    OpenRouter,
    Ollama,
    Custom,       // Any OpenAI-compatible endpoint
}

impl AIConfig {
    /// Create a rig completion model from config
    pub fn create_model(&self) -> anyhow::Result<Box<dyn CompletionModel>> {
        match self.provider {
            AIProvider::Anthropic => {
                let api_key = self.api_key.as_ref()
                    .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok().as_ref())
                    .ok_or_else(|| anyhow::anyhow!("Anthropic API key required"))?;

                let client = anthropic::Client::new(api_key);
                Ok(Box::new(client.completion_model(&self.model)))
            }

            AIProvider::OpenAI => {
                let api_key = self.api_key.as_ref()
                    .or_else(|| std::env::var("OPENAI_API_KEY").ok().as_ref())
                    .ok_or_else(|| anyhow::anyhow!("OpenAI API key required"))?;

                let client = openai::Client::new(api_key);
                Ok(Box::new(client.completion_model(&self.model)))
            }

            AIProvider::Google => {
                // Google Gemini API
                let api_key = self.api_key.as_ref()
                    .or_else(|| std::env::var("GEMINI_API_KEY").ok().as_ref())
                    .ok_or_else(|| anyhow::anyhow!("Google/Gemini API key required"))?;

                let client = gemini::Client::new(api_key);
                Ok(Box::new(client.completion_model(&self.model)))
            }

            AIProvider::OpenRouter => {
                // OpenRouter uses OpenAI-compatible API
                let api_key = self.api_key.as_ref()
                    .or_else(|| std::env::var("OPENROUTER_API_KEY").ok().as_ref())
                    .ok_or_else(|| anyhow::anyhow!("OpenRouter API key required"))?;

                let client = openai::Client::from_url("https://openrouter.ai/api/v1", api_key);
                Ok(Box::new(client.completion_model(&self.model)))
            }

            AIProvider::Ollama => {
                // Ollama uses OpenAI-compatible API locally
                let base_url = self.base_url.as_deref()
                    .unwrap_or("http://localhost:11434/v1");

                let client = openai::Client::from_url(base_url, "ollama");
                Ok(Box::new(client.completion_model(&self.model)))
            }

            AIProvider::Custom => {
                let base_url = self.base_url.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Custom provider requires base_url"))?;
                let api_key = self.api_key.as_deref().unwrap_or("none");

                let client = openai::Client::from_url(base_url, api_key);
                Ok(Box::new(client.completion_model(&self.model)))
            }
        }
    }
}

/// Common model presets
pub fn get_model_presets(provider: &AIProvider) -> Vec<ModelPreset> {
    match provider {
        AIProvider::Anthropic => vec![
            ModelPreset { id: "claude-sonnet-4-20250514", name: "Claude Sonnet 4", recommended: true },
            ModelPreset { id: "claude-opus-4-20250514", name: "Claude Opus 4", recommended: false },
            ModelPreset { id: "claude-3-5-haiku-20241022", name: "Claude 3.5 Haiku", recommended: false },
        ],
        AIProvider::OpenAI => vec![
            ModelPreset { id: "gpt-4o", name: "GPT-4o", recommended: true },
            ModelPreset { id: "gpt-4o-mini", name: "GPT-4o Mini", recommended: false },
            ModelPreset { id: "o1", name: "o1", recommended: false },
        ],
        AIProvider::Google => vec![
            ModelPreset { id: "gemini-2.0-flash", name: "Gemini 2.0 Flash", recommended: true },
            ModelPreset { id: "gemini-1.5-pro", name: "Gemini 1.5 Pro", recommended: false },
        ],
        AIProvider::OpenRouter => vec![
            // OpenRouter has many models, just show popular ones
            ModelPreset { id: "anthropic/claude-sonnet-4", name: "Claude Sonnet 4", recommended: true },
            ModelPreset { id: "openai/gpt-4o", name: "GPT-4o", recommended: false },
            ModelPreset { id: "google/gemini-2.0-flash", name: "Gemini 2.0 Flash", recommended: false },
        ],
        AIProvider::Ollama => vec![
            ModelPreset { id: "llama3.3", name: "Llama 3.3", recommended: true },
            ModelPreset { id: "qwen2.5-coder", name: "Qwen 2.5 Coder", recommended: false },
            ModelPreset { id: "deepseek-coder-v2", name: "DeepSeek Coder V2", recommended: false },
        ],
        AIProvider::Custom => vec![],
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub recommended: bool,
}
```

## Agent System Prompt

```rust
// src-tauri/src/ai/agent.rs

pub const SYSTEM_PROMPT: &str = r#"You are Roxidy AI, an intelligent assistant integrated into a terminal application. You help users with:

- Explaining command output and errors
- Suggesting fixes for failed commands
- Writing and editing files
- Running shell commands
- Answering programming questions

## Context
You have access to:
- The user's recent command history with outputs and exit codes
- The current working directory
- File system tools to read and write files
- The ability to execute shell commands

## Guidelines
1. Be concise. Terminal users prefer brief, actionable responses.
2. When suggesting commands, show them in code blocks.
3. Before running destructive commands (rm, overwriting files), explain what will happen.
4. If a command failed, analyze the error and suggest a fix.
5. Use the available tools rather than asking the user to run commands manually.

## Tool Usage
- Use `run_command` to execute shell commands
- Use `read_file` to examine file contents before editing
- Use `write_file` or `edit_file` to modify files
- Use `get_command_history` to see recent commands and their outputs
- Ask for confirmation before making significant changes

## Response Format
- Use markdown formatting
- Code blocks with language hints: ```bash, ```python, etc.
- Keep explanations brief unless the user asks for details
"#;
```

## Tool Definitions

```rust
// src-tauri/src/ai/tools.rs

use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

// ============ Run Command ============

#[derive(Debug)]
pub struct RunCommandTool {
    pub session_id: Uuid,
    pub pty_manager: Arc<RwLock<PtyManager>>,
}

#[derive(Debug, Deserialize)]
pub struct RunCommandArgs {
    /// The shell command to execute
    pub command: String,
    /// Whether to wait for completion (default: true)
    #[serde(default = "default_true")]
    pub wait: bool,
}

fn default_true() -> bool { true }

#[derive(Debug, Serialize)]
pub struct RunCommandResult {
    pub exit_code: Option<i32>,
    pub output: String,
    pub truncated: bool,
}

impl Tool for RunCommandTool {
    const NAME: &'static str = "run_command";
    type Error = anyhow::Error;
    type Args = RunCommandArgs;
    type Output = RunCommandResult;

    async fn definition(&self, _prompt: String) -> rig::tool::ToolDefinition {
        rig::tool::ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Execute a shell command in the user's terminal".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    },
                    "wait": {
                        "type": "boolean",
                        "description": "Wait for command to complete (default: true)",
                        "default": true
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let manager = self.pty_manager.read().await;
        let result = manager.execute_command(
            self.session_id,
            &args.command,
            args.wait,
        ).await?;

        // Truncate very long output
        let (output, truncated) = if result.output.len() > 10000 {
            (format!("{}...\n[output truncated]", &result.output[..10000]), true)
        } else {
            (result.output, false)
        };

        Ok(RunCommandResult {
            exit_code: result.exit_code,
            output,
            truncated,
        })
    }
}

// ============ Read File ============

#[derive(Debug)]
pub struct ReadFileTool {
    pub working_directory: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct ReadFileArgs {
    /// Path to the file (relative to working directory or absolute)
    pub path: String,
    /// Maximum lines to read (default: 500)
    #[serde(default = "default_max_lines")]
    pub max_lines: usize,
}

fn default_max_lines() -> usize { 500 }

#[derive(Debug, Serialize)]
pub struct ReadFileResult {
    pub content: String,
    pub total_lines: usize,
    pub truncated: bool,
}

impl Tool for ReadFileTool {
    const NAME: &'static str = "read_file";
    type Error = anyhow::Error;
    type Args = ReadFileArgs;
    type Output = ReadFileResult;

    async fn definition(&self, _prompt: String) -> rig::tool::ToolDefinition {
        rig::tool::ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Read the contents of a file".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path (relative or absolute)"
                    },
                    "max_lines": {
                        "type": "integer",
                        "description": "Maximum lines to read (default: 500)",
                        "default": 500
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = if PathBuf::from(&args.path).is_absolute() {
            PathBuf::from(&args.path)
        } else {
            self.working_directory.join(&args.path)
        };

        let content = tokio::fs::read_to_string(&path).await?;
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let (content, truncated) = if lines.len() > args.max_lines {
            (lines[..args.max_lines].join("\n"), true)
        } else {
            (content, false)
        };

        Ok(ReadFileResult {
            content,
            total_lines,
            truncated,
        })
    }
}

// ============ Write File ============

#[derive(Debug)]
pub struct WriteFileTool {
    pub working_directory: PathBuf,
    pub requires_approval: bool,
}

#[derive(Debug, Deserialize)]
pub struct WriteFileArgs {
    /// Path to the file
    pub path: String,
    /// Content to write
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct WriteFileResult {
    pub success: bool,
    pub path: String,
    pub bytes_written: usize,
}

impl Tool for WriteFileTool {
    const NAME: &'static str = "write_file";
    type Error = anyhow::Error;
    type Args = WriteFileArgs;
    type Output = WriteFileResult;

    async fn definition(&self, _prompt: String) -> rig::tool::ToolDefinition {
        rig::tool::ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Write content to a file (creates or overwrites)".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to write to"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = if PathBuf::from(&args.path).is_absolute() {
            PathBuf::from(&args.path)
        } else {
            self.working_directory.join(&args.path)
        };

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let bytes = args.content.as_bytes().len();
        tokio::fs::write(&path, &args.content).await?;

        Ok(WriteFileResult {
            success: true,
            path: path.to_string_lossy().to_string(),
            bytes_written: bytes,
        })
    }
}

// ============ Get Command History ============

#[derive(Debug)]
pub struct GetCommandHistoryTool {
    pub session_id: Uuid,
    pub db: Arc<RwLock<Connection>>,
}

#[derive(Debug, Deserialize)]
pub struct GetCommandHistoryArgs {
    /// Number of recent commands to retrieve (default: 10)
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Only include failed commands
    #[serde(default)]
    pub failed_only: bool,
}

fn default_limit() -> u32 { 10 }

#[derive(Debug, Serialize)]
pub struct CommandHistoryEntry {
    pub command: String,
    pub exit_code: Option<i32>,
    pub output_preview: String,
    pub working_directory: String,
}

impl Tool for GetCommandHistoryTool {
    const NAME: &'static str = "get_command_history";
    type Error = anyhow::Error;
    type Args = GetCommandHistoryArgs;
    type Output = Vec<CommandHistoryEntry>;

    async fn definition(&self, _prompt: String) -> rig::tool::ToolDefinition {
        rig::tool::ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Get recent command history with outputs".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Number of commands to retrieve (default: 10)",
                        "default": 10
                    },
                    "failed_only": {
                        "type": "boolean",
                        "description": "Only include failed commands (exit code != 0)",
                        "default": false
                    }
                },
                "required": []
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let db = self.db.read().await;

        let query = if args.failed_only {
            "SELECT command, exit_code, output, working_directory
             FROM command_blocks
             WHERE session_id = ?1 AND exit_code != 0
             ORDER BY start_time DESC
             LIMIT ?2"
        } else {
            "SELECT command, exit_code, output, working_directory
             FROM command_blocks
             WHERE session_id = ?1
             ORDER BY start_time DESC
             LIMIT ?2"
        };

        let mut stmt = db.prepare(query)?;
        let rows = stmt.query_map(
            rusqlite::params![self.session_id.to_string(), args.limit],
            |row| {
                let output: String = row.get(2)?;
                let output_preview = if output.len() > 500 {
                    format!("{}...", &output[..500])
                } else {
                    output
                };

                Ok(CommandHistoryEntry {
                    command: row.get(0)?,
                    exit_code: row.get(1)?,
                    output_preview,
                    working_directory: row.get(3)?,
                })
            },
        )?;

        let entries: Result<Vec<_>, _> = rows.collect();
        Ok(entries?)
    }
}

// ============ List Directory ============

#[derive(Debug)]
pub struct ListDirectoryTool {
    pub working_directory: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct ListDirectoryArgs {
    /// Directory path (default: current directory)
    #[serde(default)]
    pub path: Option<String>,
    /// Include hidden files
    #[serde(default)]
    pub show_hidden: bool,
}

#[derive(Debug, Serialize)]
pub struct DirectoryEntry {
    pub name: String,
    pub is_directory: bool,
    pub size: Option<u64>,
}

impl Tool for ListDirectoryTool {
    const NAME: &'static str = "list_directory";
    type Error = anyhow::Error;
    type Args = ListDirectoryArgs;
    type Output = Vec<DirectoryEntry>;

    async fn definition(&self, _prompt: String) -> rig::tool::ToolDefinition {
        rig::tool::ToolDefinition {
            name: Self::NAME.to_string(),
            description: "List contents of a directory".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path (default: current directory)"
                    },
                    "show_hidden": {
                        "type": "boolean",
                        "description": "Include hidden files (starting with .)",
                        "default": false
                    }
                },
                "required": []
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = match &args.path {
            Some(p) if PathBuf::from(p).is_absolute() => PathBuf::from(p),
            Some(p) => self.working_directory.join(p),
            None => self.working_directory.clone(),
        };

        let mut entries = Vec::new();
        let mut read_dir = tokio::fs::read_dir(&path).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            let name = entry.file_name().to_string_lossy().to_string();

            if !args.show_hidden && name.starts_with('.') {
                continue;
            }

            let metadata = entry.metadata().await?;
            entries.push(DirectoryEntry {
                name,
                is_directory: metadata.is_dir(),
                size: if metadata.is_file() { Some(metadata.len()) } else { None },
            });
        }

        entries.sort_by(|a, b| {
            // Directories first, then alphabetically
            match (a.is_directory, b.is_directory) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });

        Ok(entries)
    }
}
```

## Building the Agent

```rust
// src-tauri/src/ai/agent.rs

use rig::agent::AgentBuilder;
use rig::completion::{Prompt, StreamedAssistantContent, MultiTurnStreamItem};

pub struct RoxidyAgent {
    agent: Agent<impl CompletionModel>,
    session_id: Uuid,
}

impl RoxidyAgent {
    pub fn new(
        config: &AIConfig,
        session_id: Uuid,
        pty_manager: Arc<RwLock<PtyManager>>,
        db: Arc<RwLock<Connection>>,
        working_directory: PathBuf,
    ) -> anyhow::Result<Self> {
        let model = config.create_model()?;

        let agent = AgentBuilder::new(model)
            .preamble(SYSTEM_PROMPT)
            .temperature(config.temperature)
            .max_tokens(config.max_tokens as usize)
            // Register tools
            .tool(RunCommandTool {
                session_id,
                pty_manager: pty_manager.clone(),
            })
            .tool(ReadFileTool {
                working_directory: working_directory.clone(),
            })
            .tool(WriteFileTool {
                working_directory: working_directory.clone(),
                requires_approval: true,
            })
            .tool(GetCommandHistoryTool {
                session_id,
                db: db.clone(),
            })
            .tool(ListDirectoryTool {
                working_directory: working_directory.clone(),
            })
            .build();

        Ok(Self { agent, session_id })
    }

    /// Send a prompt and stream the response
    pub async fn prompt_stream(
        &self,
        message: &str,
        app_handle: tauri::AppHandle,
    ) -> anyhow::Result<()> {
        let stream = self.agent.stream_prompt(message).await?;

        // Process stream items
        tokio::pin!(stream);

        while let Some(item) = stream.next().await {
            match item {
                Ok(MultiTurnStreamItem::StreamItem(content)) => {
                    match content {
                        StreamedAssistantContent::Text(delta) => {
                            app_handle.emit("ai_stream", AIStreamEvent {
                                session_id: self.session_id,
                                delta: Some(delta),
                                tool_call: None,
                                done: false,
                                error: None,
                            })?;
                        }
                        StreamedAssistantContent::ToolCall(call) => {
                            // Emit tool start
                            app_handle.emit("tool_start", ToolStartEvent {
                                session_id: self.session_id,
                                tool_call_id: call.id.clone(),
                                tool_name: call.name.clone(),
                                arguments: serde_json::to_string(&call.arguments).unwrap_or_default(),
                                requires_approval: call.name == "write_file",
                            })?;
                        }
                        StreamedAssistantContent::ToolCallDelta(delta) => {
                            // Incremental tool call arguments (for streaming tools)
                            app_handle.emit("tool_delta", ToolDeltaEvent {
                                session_id: self.session_id,
                                tool_call_id: delta.id.clone(),
                                arguments_delta: delta.arguments_delta,
                            })?;
                        }
                        StreamedAssistantContent::Reasoning(reasoning) => {
                            // Some models emit reasoning tokens
                            app_handle.emit("ai_reasoning", AIReasoningEvent {
                                session_id: self.session_id,
                                reasoning,
                            })?;
                        }
                    }
                }
                Ok(MultiTurnStreamItem::FinalResponse(response)) => {
                    // The full response after all streaming is complete
                    app_handle.emit("ai_stream", AIStreamEvent {
                        session_id: self.session_id,
                        delta: None,
                        tool_call: None,
                        done: true,
                        error: None,
                    })?;
                }
                Err(e) => {
                    app_handle.emit("ai_stream", AIStreamEvent {
                        session_id: self.session_id,
                        delta: None,
                        tool_call: None,
                        done: true,
                        error: Some(e.to_string()),
                    })?;
                    return Err(e.into());
                }
            }
        }

        Ok(())
    }
}
```

## Conversation History

```rust
// src-tauri/src/ai/conversation.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_results: Option<Vec<ToolResult>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    Tool,
}

pub struct Conversation {
    messages: Vec<Message>,
    session_id: Uuid,
    db: Arc<RwLock<Connection>>,
}

impl Conversation {
    pub fn new(session_id: Uuid, db: Arc<RwLock<Connection>>) -> Self {
        Self {
            messages: Vec::new(),
            session_id,
            db,
        }
    }

    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(Message {
            role: Role::User,
            content,
            tool_calls: None,
            tool_results: None,
        });
        self.persist();
    }

    pub fn add_assistant_message(&mut self, content: String, tool_calls: Option<Vec<ToolCall>>) {
        self.messages.push(Message {
            role: Role::Assistant,
            content,
            tool_calls,
            tool_results: None,
        });
        self.persist();
    }

    pub fn get_context_for_prompt(&self) -> Vec<&Message> {
        // Return last N messages for context window management
        let max_messages = 20;
        let start = self.messages.len().saturating_sub(max_messages);
        self.messages[start..].iter().collect()
    }

    fn persist(&self) {
        // Save to SQLite in background
        let messages = self.messages.clone();
        let session_id = self.session_id;
        let db = self.db.clone();

        tokio::spawn(async move {
            if let Ok(db) = db.write().await {
                let json = serde_json::to_string(&messages).unwrap_or_default();
                let _ = db.execute(
                    "INSERT OR REPLACE INTO ai_conversations (id, session_id, messages, created_at)
                     VALUES (?1, ?2, ?3, datetime('now'))",
                    rusqlite::params![
                        Uuid::new_v4().to_string(),
                        session_id.to_string(),
                        json,
                    ],
                );
            }
        });
    }
}
```

## Frontend AI Settings

```typescript
// src/components/Settings/AISettings.tsx

import { useState, useEffect } from "react";
import { settingsGetAll, settingsSet, getModelPresets } from "../../lib/tauri";

const PROVIDERS = [
  { id: "anthropic", name: "Anthropic", requiresKey: true },
  { id: "openai", name: "OpenAI", requiresKey: true },
  { id: "google", name: "Google (Vertex)", requiresKey: true },
  { id: "openrouter", name: "OpenRouter", requiresKey: true },
  { id: "ollama", name: "Ollama (Local)", requiresKey: false },
  { id: "custom", name: "Custom Endpoint", requiresKey: false },
];

export function AISettings() {
  const [provider, setProvider] = useState("anthropic");
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [presets, setPresets] = useState<ModelPreset[]>([]);

  useEffect(() => {
    loadSettings();
  }, []);

  useEffect(() => {
    loadPresets(provider);
  }, [provider]);

  async function loadSettings() {
    const settings = await settingsGetAll();
    setProvider(settings.aiProvider || "anthropic");
    setModel(settings.aiModel || "");
    setApiKey(settings.aiApiKey || "");
    setBaseUrl(settings.aiBaseUrl || "");
  }

  async function loadPresets(provider: string) {
    const models = await getModelPresets(provider);
    setPresets(models);
    // Auto-select recommended model
    const recommended = models.find((m) => m.recommended);
    if (recommended && !model) {
      setModel(recommended.id);
    }
  }

  async function handleSave() {
    await settingsSet("aiProvider", provider);
    await settingsSet("aiModel", model);
    if (apiKey) await settingsSet("aiApiKey", apiKey);
    if (baseUrl) await settingsSet("aiBaseUrl", baseUrl);
  }

  const selectedProvider = PROVIDERS.find((p) => p.id === provider);

  return (
    <div className="ai-settings">
      <h3>AI Configuration</h3>

      <label>
        Provider
        <select value={provider} onChange={(e) => setProvider(e.target.value)}>
          {PROVIDERS.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </label>

      <label>
        Model
        <select value={model} onChange={(e) => setModel(e.target.value)}>
          {presets.map((m) => (
            <option key={m.id} value={m.id}>
              {m.name} {m.recommended && "(recommended)"}
            </option>
          ))}
        </select>
        <input
          type="text"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          placeholder="Or enter custom model ID"
        />
      </label>

      {selectedProvider?.requiresKey && (
        <label>
          API Key
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="Enter API key"
          />
          <small>
            Stored locally. You can also set via environment variable.
          </small>
        </label>
      )}

      {(provider === "ollama" || provider === "custom") && (
        <label>
          Base URL
          <input
            type="text"
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
            placeholder={
              provider === "ollama"
                ? "http://localhost:11434/v1"
                : "https://api.example.com/v1"
            }
          />
        </label>
      )}

      <button onClick={handleSave} className="primary">
        Save Settings
      </button>
    </div>
  );
}
```

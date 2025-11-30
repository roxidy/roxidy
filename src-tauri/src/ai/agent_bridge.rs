use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use chrono::Local;
use rig::completion::{
    AssistantContent, CompletionModel as RigCompletionModel, Message, ToolDefinition,
};
use rig::message::{Text, ToolCall, ToolResult, ToolResultContent, UserContent};
use rig::one_or_many::OneOrMany;
use serde_json::json;
use tokio::sync::{mpsc, oneshot, RwLock};
use vtcode_core::llm::{make_client, AnyClient};
use vtcode_core::tools::registry::build_function_declarations;
use vtcode_core::tools::ToolRegistry;

use super::context_manager::{ContextEvent, ContextManager, ContextSummary, ContextTrimConfig};
use super::events::AiEvent;
use super::hitl::{
    ApprovalDecision, ApprovalPattern, ApprovalRecorder, RiskLevel, ToolApprovalConfig,
};
use super::session::QbitSessionManager;
use super::token_budget::{TokenAlertLevel, TokenBudgetConfig, TokenUsageStats};
use super::token_trunc::aggregate_tool_output;
use super::tool_policy::{PolicyConstraintResult, ToolPolicy, ToolPolicyConfig, ToolPolicyManager};
use super::sub_agent::{
    create_default_sub_agents, SubAgentContext, SubAgentDefinition, SubAgentRegistry,
    SubAgentResult, MAX_AGENT_DEPTH,
};
use crate::indexer::IndexerState;
use crate::pty::PtyManager;

/// Maximum number of tool call iterations before stopping
const MAX_TOOL_ITERATIONS: usize = 100;

/// LLM client abstraction that supports both vtcode and rig-based providers
enum LlmClient {
    /// vtcode-core client (OpenRouter, OpenAI, etc.)
    Vtcode(AnyClient),
    /// Anthropic on Vertex AI via rig-anthropic-vertex
    VertexAnthropic(rig_anthropic_vertex::CompletionModel),
}

/// Bridge between Qbit and LLM providers.
/// Handles LLM streaming and tool execution.
pub struct AgentBridge {
    /// Current workspace/working directory - can be updated dynamically
    workspace: Arc<RwLock<PathBuf>>,
    provider_name: String,
    model_name: String,
    /// ToolRegistry requires &mut self for execute_tool, so we need RwLock
    tool_registry: Arc<RwLock<ToolRegistry>>,
    /// LLM client (either vtcode or rig-based)
    client: Arc<RwLock<LlmClient>>,
    event_tx: mpsc::UnboundedSender<AiEvent>,
    /// Registry of available sub-agents
    sub_agent_registry: Arc<RwLock<SubAgentRegistry>>,
    /// Reference to PtyManager for executing commands in user's terminal
    pty_manager: Option<Arc<PtyManager>>,
    /// Current session ID for terminal execution (set per-request)
    current_session_id: Arc<RwLock<Option<String>>>,
    /// Persisted conversation history for multi-turn conversations
    conversation_history: Arc<RwLock<Vec<Message>>>,
    /// Reference to IndexerState for code analysis tools
    indexer_state: Option<Arc<IndexerState>>,
    /// Session manager for persisting conversations (optional, initialized lazily)
    session_manager: Arc<RwLock<Option<QbitSessionManager>>>,
    /// Whether session persistence is enabled
    session_persistence_enabled: Arc<RwLock<bool>>,
    /// HITL approval recorder for tracking and learning approval patterns
    approval_recorder: Arc<ApprovalRecorder>,
    /// Pending approval responses (request_id -> oneshot sender)
    pending_approvals: Arc<RwLock<HashMap<String, oneshot::Sender<ApprovalDecision>>>>,
    /// Tool policy manager for allow/prompt/deny rules and constraints
    tool_policy_manager: Arc<ToolPolicyManager>,
    /// Context manager for token budgeting and conversation trimming
    context_manager: Arc<ContextManager>,
    /// Channel for context events
    context_event_rx: Arc<RwLock<Option<mpsc::Receiver<ContextEvent>>>>,
}

impl AgentBridge {
    /// Create a new AgentBridge with vtcode-core (for OpenRouter, OpenAI, etc.)
    ///
    /// # Arguments
    /// * `workspace` - The workspace directory for tool operations
    /// * `provider` - Provider name (e.g., "openrouter", "anthropic", "openai")
    /// * `model` - Model identifier (e.g., "anthropic/claude-3.5-sonnet")
    /// * `api_key` - API key for the provider
    /// * `event_tx` - Channel to send AI events to the frontend
    pub async fn new(
        workspace: PathBuf,
        provider: &str,
        model: &str,
        api_key: &str,
        event_tx: mpsc::UnboundedSender<AiEvent>,
    ) -> Result<Self> {
        // Create the model ID using FromStr trait
        let model_id = vtcode_core::config::models::ModelId::from_str(model)
            .map_err(|e| anyhow::anyhow!("Invalid model ID '{}': {}", model, e))?;

        // Create LLM client (wrapped in RwLock since generate requires &mut self)
        let client = Arc::new(RwLock::new(LlmClient::Vtcode(make_client(
            api_key.to_string(),
            model_id,
        ))));

        // Create tool registry (async)
        let tool_registry = Arc::new(RwLock::new(ToolRegistry::new(workspace.clone()).await));

        // Create sub-agent registry with defaults
        let mut sub_agent_registry = SubAgentRegistry::new();
        for agent in create_default_sub_agents() {
            sub_agent_registry.register(agent);
        }

        // Create HITL approval recorder (stores in workspace/.qbit/hitl/)
        let hitl_storage = workspace.join(".qbit").join("hitl");
        let approval_recorder = Arc::new(ApprovalRecorder::new(hitl_storage).await);

        // Create tool policy manager (loads from workspace/.qbit/tool-policy.json)
        let tool_policy_manager = Arc::new(ToolPolicyManager::new(&workspace).await);

        // Create context manager for token budgeting
        let mut context_manager = ContextManager::for_model(model);
        let (context_tx, context_rx) = mpsc::channel::<ContextEvent>(100);
        context_manager.set_event_channel(context_tx);

        Ok(Self {
            workspace: Arc::new(RwLock::new(workspace)),
            provider_name: provider.to_string(),
            model_name: model.to_string(),
            tool_registry,
            client,
            event_tx,
            sub_agent_registry: Arc::new(RwLock::new(sub_agent_registry)),
            pty_manager: None,
            current_session_id: Arc::new(RwLock::new(None)),
            conversation_history: Arc::new(RwLock::new(Vec::new())),
            indexer_state: None,
            session_manager: Arc::new(RwLock::new(None)),
            session_persistence_enabled: Arc::new(RwLock::new(true)), // Enabled by default
            approval_recorder,
            pending_approvals: Arc::new(RwLock::new(HashMap::new())),
            tool_policy_manager,
            context_manager: Arc::new(context_manager),
            context_event_rx: Arc::new(RwLock::new(Some(context_rx))),
        })
    }

    /// Create a new AgentBridge for Anthropic on Google Cloud Vertex AI.
    ///
    /// # Arguments
    /// * `workspace` - The workspace directory for tool operations
    /// * `credentials_path` - Path to the service account JSON file
    /// * `project_id` - Google Cloud project ID
    /// * `location` - Vertex AI location (e.g., "us-east5")
    /// * `model` - Model identifier (e.g., "claude-opus-4-5@20251101")
    /// * `event_tx` - Channel to send AI events to the frontend
    pub async fn new_vertex_anthropic(
        workspace: PathBuf,
        credentials_path: &str,
        project_id: &str,
        location: &str,
        model: &str,
        event_tx: mpsc::UnboundedSender<AiEvent>,
    ) -> Result<Self> {
        // Create Vertex AI client
        let vertex_client = rig_anthropic_vertex::Client::from_service_account(
            credentials_path,
            project_id,
            location,
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create Vertex AI client: {}", e))?;

        // Create completion model
        let completion_model = vertex_client.completion_model(model);

        // Create tool registry (async)
        let tool_registry = Arc::new(RwLock::new(ToolRegistry::new(workspace.clone()).await));

        // Create sub-agent registry with defaults
        let mut sub_agent_registry = SubAgentRegistry::new();
        for agent in create_default_sub_agents() {
            sub_agent_registry.register(agent);
        }

        // Create HITL approval recorder (stores in workspace/.qbit/hitl/)
        let hitl_storage = workspace.join(".qbit").join("hitl");
        let approval_recorder = Arc::new(ApprovalRecorder::new(hitl_storage).await);

        // Create tool policy manager (loads from workspace/.qbit/tool-policy.json)
        let tool_policy_manager = Arc::new(ToolPolicyManager::new(&workspace).await);

        // Create context manager for token budgeting
        let mut context_manager = ContextManager::for_model(model);
        let (context_tx, context_rx) = mpsc::channel::<ContextEvent>(100);
        context_manager.set_event_channel(context_tx);

        Ok(Self {
            workspace: Arc::new(RwLock::new(workspace)),
            provider_name: "anthropic_vertex".to_string(),
            model_name: model.to_string(),
            tool_registry,
            client: Arc::new(RwLock::new(LlmClient::VertexAnthropic(completion_model))),
            event_tx,
            sub_agent_registry: Arc::new(RwLock::new(sub_agent_registry)),
            pty_manager: None,
            current_session_id: Arc::new(RwLock::new(None)),
            conversation_history: Arc::new(RwLock::new(Vec::new())),
            indexer_state: None,
            session_manager: Arc::new(RwLock::new(None)),
            session_persistence_enabled: Arc::new(RwLock::new(true)), // Enabled by default
            approval_recorder,
            pending_approvals: Arc::new(RwLock::new(HashMap::new())),
            tool_policy_manager,
            context_manager: Arc::new(context_manager),
            context_event_rx: Arc::new(RwLock::new(Some(context_rx))),
        })
    }

    /// Set the PtyManager for executing commands in user's terminal
    pub fn set_pty_manager(&mut self, pty_manager: Arc<PtyManager>) {
        self.pty_manager = Some(pty_manager);
    }

    /// Set the IndexerState for code analysis tools
    pub fn set_indexer_state(&mut self, indexer_state: Arc<IndexerState>) {
        self.indexer_state = Some(indexer_state);
    }

    /// Execute an indexer tool
    async fn execute_indexer_tool(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> (serde_json::Value, bool) {
        let indexer = match &self.indexer_state {
            Some(state) => state,
            None => {
                return (json!({"error": "Indexer not initialized"}), false);
            }
        };

        match tool_name {
            "indexer_search_code" => {
                let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
                let path_filter = args.get("path_filter").and_then(|v| v.as_str());

                match indexer.with_indexer(|idx| idx.search(pattern, path_filter)) {
                    Ok(results) => (
                        json!({
                            "matches": results.iter().map(|r| json!({
                                "file": r.file_path,
                                "line": r.line_number,
                                "content": r.line_content,
                                "matches": r.matches
                            })).collect::<Vec<_>>(),
                            "count": results.len()
                        }),
                        true,
                    ),
                    Err(e) => (json!({"error": e.to_string()}), false),
                }
            }
            "indexer_search_files" => {
                let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");

                match indexer.with_indexer(|idx| idx.find_files(pattern)) {
                    Ok(files) => (
                        json!({
                            "files": files,
                            "count": files.len()
                        }),
                        true,
                    ),
                    Err(e) => (json!({"error": e.to_string()}), false),
                }
            }
            "indexer_analyze_file" => {
                let file_path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or("");

                match indexer.get_analyzer() {
                    Ok(mut analyzer) => {
                        use vtcode_core::tools::tree_sitter::analysis::CodeAnalyzer;

                        let path = PathBuf::from(file_path);
                        match analyzer.parse_file(&path).await {
                            Ok(tree) => {
                                let code_analyzer = CodeAnalyzer::new(&tree.language);
                                let analysis = code_analyzer.analyze(&tree, file_path);

                                (
                                    json!({
                                        "symbols": analysis.symbols.iter().map(|s| json!({
                                            "name": s.name,
                                            "kind": format!("{:?}", s.kind),
                                            "line": s.position.row,
                                            "column": s.position.column
                                        })).collect::<Vec<_>>(),
                                        "metrics": {
                                            "lines_of_code": analysis.metrics.lines_of_code,
                                            "lines_of_comments": analysis.metrics.lines_of_comments,
                                            "blank_lines": analysis.metrics.blank_lines,
                                            "functions_count": analysis.metrics.functions_count,
                                            "classes_count": analysis.metrics.classes_count,
                                            "comment_ratio": analysis.metrics.comment_ratio
                                        },
                                        "dependencies": analysis.dependencies.iter().map(|d| json!({
                                            "name": d.name,
                                            "kind": format!("{:?}", d.kind)
                                        })).collect::<Vec<_>>()
                                    }),
                                    true,
                                )
                            }
                            Err(e) => (
                                json!({"error": format!("Failed to parse file: {}", e)}),
                                false,
                            ),
                        }
                    }
                    Err(e) => (json!({"error": e.to_string()}), false),
                }
            }
            "indexer_extract_symbols" => {
                let file_path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or("");

                match indexer.get_analyzer() {
                    Ok(mut analyzer) => {
                        use vtcode_core::tools::tree_sitter::languages::LanguageAnalyzer;

                        let path = PathBuf::from(file_path);
                        match analyzer.parse_file(&path).await {
                            Ok(tree) => {
                                let lang_analyzer = LanguageAnalyzer::new(&tree.language);
                                let symbols = lang_analyzer.extract_symbols(&tree);

                                (
                                    json!({
                                        "symbols": symbols.iter().map(|s| json!({
                                            "name": s.name,
                                            "kind": format!("{:?}", s.kind),
                                            "line": s.position.row,
                                            "column": s.position.column,
                                            "scope": s.scope,
                                            "signature": s.signature,
                                            "documentation": s.documentation
                                        })).collect::<Vec<_>>(),
                                        "count": symbols.len()
                                    }),
                                    true,
                                )
                            }
                            Err(e) => (
                                json!({"error": format!("Failed to parse file: {}", e)}),
                                false,
                            ),
                        }
                    }
                    Err(e) => (json!({"error": e.to_string()}), false),
                }
            }
            "indexer_get_metrics" => {
                let file_path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or("");

                match indexer.get_analyzer() {
                    Ok(mut analyzer) => {
                        use vtcode_core::tools::tree_sitter::analysis::CodeAnalyzer;

                        let path = PathBuf::from(file_path);
                        match analyzer.parse_file(&path).await {
                            Ok(tree) => {
                                let code_analyzer = CodeAnalyzer::new(&tree.language);
                                let analysis = code_analyzer.analyze(&tree, file_path);

                                (
                                    json!({
                                        "lines_of_code": analysis.metrics.lines_of_code,
                                        "lines_of_comments": analysis.metrics.lines_of_comments,
                                        "blank_lines": analysis.metrics.blank_lines,
                                        "functions_count": analysis.metrics.functions_count,
                                        "classes_count": analysis.metrics.classes_count,
                                        "variables_count": analysis.metrics.variables_count,
                                        "imports_count": analysis.metrics.imports_count,
                                        "comment_ratio": analysis.metrics.comment_ratio
                                    }),
                                    true,
                                )
                            }
                            Err(e) => (
                                json!({"error": format!("Failed to parse file: {}", e)}),
                                false,
                            ),
                        }
                    }
                    Err(e) => (json!({"error": e.to_string()}), false),
                }
            }
            "indexer_detect_language" => {
                let file_path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or("");

                match indexer.get_analyzer() {
                    Ok(analyzer) => {
                        let path = PathBuf::from(file_path);
                        match analyzer.detect_language_from_path(&path) {
                            Ok(lang) => (json!({"language": format!("{:?}", lang)}), true),
                            Err(e) => (json!({"error": e.to_string()}), false),
                        }
                    }
                    Err(e) => (json!({"error": e.to_string()}), false),
                }
            }
            _ => (
                json!({"error": format!("Unknown indexer tool: {}", tool_name)}),
                false,
            ),
        }
    }

    /// Set the current session ID for terminal execution
    pub async fn set_session_id(&self, session_id: Option<String>) {
        *self.current_session_id.write().await = session_id;
    }

    /// Execute a command in the user's terminal by writing to their PTY
    async fn execute_in_terminal(&self, command: &str) -> Result<serde_json::Value> {
        let session_id = self.current_session_id.read().await.clone();
        let pty_manager = self.pty_manager.as_ref();

        match (session_id, pty_manager) {
            (Some(sid), Some(pm)) => {
                // Write command + newline to the user's terminal
                let cmd_with_newline = format!("{}\n", command);
                pm.write(&sid, cmd_with_newline.as_bytes())
                    .map_err(|e| anyhow::anyhow!("Failed to write to terminal: {}", e))?;

                Ok(json!({
                    "success": true,
                    "message": format!("Command '{}' sent to terminal", command),
                    "session_id": sid,
                    "note": "Command output will appear in the terminal. Use terminal output events to capture results."
                }))
            }
            (None, _) => Err(anyhow::anyhow!(
                "No session ID available - cannot execute in terminal"
            )),
            (_, None) => Err(anyhow::anyhow!(
                "PtyManager not available - cannot execute in terminal"
            )),
        }
    }

    /// Get tool definitions in rig format from vtcode's function declarations.
    /// Sanitizes schemas to remove anyOf/allOf/oneOf which Anthropic doesn't support.
    /// Also overrides descriptions for specific tools (e.g., run_pty_cmd).
    fn get_tool_definitions() -> Vec<ToolDefinition> {
        let mut tools: Vec<ToolDefinition> = build_function_declarations()
            .into_iter()
            .map(|fd| {
                // Override description for run_pty_cmd to instruct agent not to repeat output
                let description = if fd.name == "run_pty_cmd" {
                    format!(
                        "{}. IMPORTANT: The command output is displayed directly in the user's terminal. \
                         Do NOT repeat or summarize the command output in your response - the user can already see it. \
                         Only mention significant errors or ask clarifying questions if needed.",
                        fd.description
                    )
                } else {
                    fd.description
                };

                ToolDefinition {
                    name: fd.name,
                    description,
                    parameters: Self::sanitize_schema(fd.parameters),
                }
            })
            .collect();

        // Add code indexer tools
        tools.extend(Self::get_indexer_tool_definitions());

        tools
    }

    /// Get tool definitions for the code indexer.
    fn get_indexer_tool_definitions() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "indexer_search_code".to_string(),
                description: "Search for code patterns using regex in the indexed workspace. Returns matching lines with file paths and line numbers.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Regex pattern to search for"
                        },
                        "path_filter": {
                            "type": "string",
                            "description": "Optional file path filter (glob pattern)"
                        }
                    },
                    "required": ["pattern"]
                }),
            },
            ToolDefinition {
                name: "indexer_search_files".to_string(),
                description: "Find files by name pattern (glob-style) in the indexed workspace.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Glob pattern to match file names (e.g., '*.rs', 'src/**/*.ts')"
                        }
                    },
                    "required": ["pattern"]
                }),
            },
            ToolDefinition {
                name: "indexer_analyze_file".to_string(),
                description: "Get semantic analysis of a file using tree-sitter. Returns symbols, code metrics, and dependencies.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the file to analyze"
                        }
                    },
                    "required": ["file_path"]
                }),
            },
            ToolDefinition {
                name: "indexer_extract_symbols".to_string(),
                description: "Extract all symbols (functions, classes, structs, variables, imports) from a file.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the file to extract symbols from"
                        }
                    },
                    "required": ["file_path"]
                }),
            },
            ToolDefinition {
                name: "indexer_get_metrics".to_string(),
                description: "Get code metrics for a file: lines of code, comment lines, blank lines, function count, class count, etc.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the file to get metrics for"
                        }
                    },
                    "required": ["file_path"]
                }),
            },
            ToolDefinition {
                name: "indexer_detect_language".to_string(),
                description: "Detect the programming language of a file based on its extension and content.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the file"
                        }
                    },
                    "required": ["file_path"]
                }),
            },
        ]
    }

    /// Remove anyOf, allOf, oneOf from JSON schema as Anthropic doesn't support them.
    /// Also simplifies nested oneOf in properties to just use the first option.
    fn sanitize_schema(mut schema: serde_json::Value) -> serde_json::Value {
        if let Some(obj) = schema.as_object_mut() {
            // Remove top-level anyOf/allOf/oneOf
            obj.remove("anyOf");
            obj.remove("allOf");
            obj.remove("oneOf");

            // Recursively sanitize properties
            if let Some(props) = obj.get_mut("properties") {
                if let Some(props_obj) = props.as_object_mut() {
                    for (_key, prop_value) in props_obj.iter_mut() {
                        if let Some(prop_obj) = prop_value.as_object_mut() {
                            // If property has oneOf, replace with first option or simplify to string
                            if prop_obj.contains_key("oneOf") {
                                if let Some(one_of) = prop_obj.remove("oneOf") {
                                    if let Some(arr) = one_of.as_array() {
                                        if let Some(first) = arr.first() {
                                            // Merge the first oneOf option into this property
                                            if let Some(first_obj) = first.as_object() {
                                                for (k, v) in first_obj {
                                                    prop_obj.insert(k.clone(), v.clone());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            // Remove anyOf/allOf from properties too
                            prop_obj.remove("anyOf");
                            prop_obj.remove("allOf");
                        }
                    }
                }
            }
        }
        schema
    }

    /// Normalize tool arguments for run_pty_cmd.
    /// If the command is passed as an array, convert it to a space-joined string.
    /// This prevents shell_words::join() from quoting metacharacters like &&, ||, |, etc.
    fn normalize_run_pty_cmd_args(mut args: serde_json::Value) -> serde_json::Value {
        if let Some(obj) = args.as_object_mut() {
            if let Some(command) = obj.get_mut("command") {
                if let Some(arr) = command.as_array() {
                    // Convert array to space-joined string
                    let cmd_str: String = arr
                        .iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    *command = serde_json::Value::String(cmd_str);
                }
            }
        }
        args
    }

    /// Get sub-agent tool definitions from the registry.
    async fn get_sub_agent_tool_definitions(&self) -> Vec<ToolDefinition> {
        let registry = self.sub_agent_registry.read().await;
        registry
            .all()
            .map(|agent| ToolDefinition {
                name: format!("sub_agent_{}", agent.id),
                description: format!(
                    "[SUB-AGENT: {}] {}",
                    agent.name, agent.description
                ),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "task": {
                            "type": "string",
                            "description": "The specific task or question for this sub-agent to handle"
                        },
                        "context": {
                            "type": "string",
                            "description": "Optional additional context to help the sub-agent understand the task"
                        }
                    },
                    "required": ["task"]
                }),
            })
            .collect()
    }

    /// Execute a prompt with agentic tool loop.
    /// The agent will call tools as needed until it produces a final response.
    pub async fn execute(&self, prompt: &str) -> Result<String> {
        self.execute_with_context(prompt, SubAgentContext::default())
            .await
    }

    /// Execute a prompt with context (for sub-agent calls).
    pub async fn execute_with_context(
        &self,
        prompt: &str,
        context: SubAgentContext,
    ) -> Result<String> {
        // Check recursion depth
        if context.depth >= MAX_AGENT_DEPTH {
            return Err(anyhow::anyhow!(
                "Maximum agent recursion depth ({}) exceeded",
                MAX_AGENT_DEPTH
            ));
        }
        // Generate a unique turn ID
        let turn_id = uuid::Uuid::new_v4().to_string();

        // Emit turn started event
        let _ = self.event_tx.send(AiEvent::Started {
            turn_id: turn_id.clone(),
        });

        let start_time = std::time::Instant::now();
        let client = self.client.read().await;

        match &*client {
            LlmClient::Vtcode(_vtcode_client) => {
                // vtcode handles its own tool loop, just use generate
                drop(client); // Release read lock before write
                let mut client = self.client.write().await;
                if let LlmClient::Vtcode(vtcode_client) = &mut *client {
                    let result = vtcode_client
                        .generate(prompt)
                        .await
                        .map(|r| r.content)
                        .map_err(|e| anyhow::anyhow!("{}", e));

                    match result {
                        Ok(content) => {
                            let duration_ms = start_time.elapsed().as_millis() as u64;
                            let _ = self.event_tx.send(AiEvent::TextDelta {
                                delta: content.clone(),
                                accumulated: content.clone(),
                            });
                            let _ = self.event_tx.send(AiEvent::Completed {
                                response: content.clone(),
                                tokens_used: None,
                                duration_ms: Some(duration_ms),
                            });
                            Ok(content)
                        }
                        Err(e) => {
                            let _ = self.event_tx.send(AiEvent::Error {
                                message: e.to_string(),
                                error_type: "llm_error".to_string(),
                            });
                            Err(e)
                        }
                    }
                } else {
                    Err(anyhow::anyhow!("Client type mismatch"))
                }
            }
            LlmClient::VertexAnthropic(vertex_model) => {
                // Implement agentic tool loop for Vertex AI
                let vertex_model = vertex_model.clone();
                drop(client); // Release lock

                self.execute_with_tools(&vertex_model, prompt, start_time)
                    .await
            }
        }
    }

    /// Execute prompt with tool calling loop for rig-based models.
    async fn execute_with_tools(
        &self,
        model: &rig_anthropic_vertex::CompletionModel,
        initial_prompt: &str,
        start_time: std::time::Instant,
    ) -> Result<String> {
        self.execute_with_tools_and_context(
            model,
            initial_prompt,
            start_time,
            SubAgentContext::default(),
        )
        .await
    }

    /// Execute prompt with tool calling loop, supporting sub-agent context.
    async fn execute_with_tools_and_context(
        &self,
        model: &rig_anthropic_vertex::CompletionModel,
        initial_prompt: &str,
        start_time: std::time::Instant,
        context: SubAgentContext,
    ) -> Result<String> {
        // Get all available tools (standard + sub-agents)
        let mut tools = Self::get_tool_definitions();

        // Only add sub-agent tools if we're not at max depth
        if context.depth < MAX_AGENT_DEPTH - 1 {
            tools.extend(self.get_sub_agent_tool_definitions().await);
        }

        // System prompt for the agent
        let workspace_path = self.workspace.read().await;

        // Get current date
        let current_date = Local::now().format("%Y-%m-%d").to_string();

        // Try to read CLAUDE.md from the workspace
        let claude_md_path = workspace_path.join("CLAUDE.md");
        let project_instructions = if claude_md_path.exists() {
            match std::fs::read_to_string(&claude_md_path) {
                Ok(contents) => format!(
                    "\n<project_instructions>\n{}\n</project_instructions>\n",
                    contents.trim()
                ),
                Err(_) => String::new(),
            }
        } else {
            // Also check parent directory (in case we're in src-tauri)
            let parent_claude_md = workspace_path
                .parent()
                .map(|p| p.join("CLAUDE.md"))
                .filter(|p| p.exists());

            match parent_claude_md {
                Some(path) => match std::fs::read_to_string(&path) {
                    Ok(contents) => format!(
                        "\n<project_instructions>\n{}\n</project_instructions>\n",
                        contents.trim()
                    ),
                    Err(_) => String::new(),
                },
                None => String::new(),
            }
        };

        let system_prompt = format!(
            r#"<environment>
Working Directory: {workspace}
Current Date: {date}
</environment>{project_instructions}

<workflow>
ALWAYS follow this workflow:

1. **Investigate** - Use available tools to understand the codebase and requirements.

2. **Create a Plan** - Explain clearly:
   - What you found
   - Changes you'll make
   - Specific files you'll modify
   - Exact functions/classes affected
   Be specific - include file paths, function names, and actual changes. Avoid vague descriptions.

3. **Get Approval** - Ask: "I plan to [specific actions]. Should I proceed?"
   **Wait for explicit "yes" or confirmation. Never proceed without approval.**

4. **Execute** - Make the approved changes.

**If anything unexpected happens or the plan needs to change:**
- STOP immediately
- Explain what changed
- Present revised plan
- Get new approval before continuing
</workflow>

<important>
- Always use `read_file` before using `edit_file` or `write_file` on existing files
- Never make changes without explicit user approval
- If the plan changes mid-execution, stop and get new approval
- Prefer `edit_file` over `write_file` for existing files
</important>

<context_handling>
User messages may include a <context> block with metadata like <cwd> (current working directory).
When present, <cwd> indicates the user's current terminal directory - use this for relative path
operations and understand that the user is working from that location.
</context_handling>

## Filesystem Tools

- `read_file`: Read file contents. Auto-chunks large files (>2000 lines). Supports max_bytes/max_tokens limits.
- `write_file`: Create or overwrite a file. Modes: overwrite (default), append, skip_if_exists. Use for full-file rewrites.
- `edit_file`: Replace text in a file by exact string match. Best for surgical updates to preserve surrounding code.
- `create_file`: Create a new file. Fails if file already exists to prevent accidental overwrites.
- `delete_file`: Delete a file or directory (with recursive flag).
- `apply_patch`: Apply structured diffs using unified diff format. Use for multi-file or complex edits.

## Search & Discovery Tools

- `grep_file`: Fast regex-based code search using ripgrep. Supports glob patterns, file-type filtering, context lines.
- `list_files`: Explore workspace. Modes: list (directory contents), recursive (full tree), find_name (by filename), find_content (by content), largest (by file size).

## Command Execution: `run_pty_cmd`

Execute shell commands (git, cargo, npm, shell scripts, etc). Full terminal emulation with PTY support.

**IMPORTANT**: Always pass the command as a single STRING, not an array.
This is critical for shell operators (&&, ||, |, >, <, etc.) to work correctly.

Examples:
- CORRECT: {{"command": "cd /path && npm install"}}
- WRONG: {{"command": ["cd", "/path", "&&", "npm", "install"]}}

## PTY Session Management (for interactive commands)

- `create_pty_session`: Create persistent PTY session for reuse across calls.
- `send_pty_input`: Send input to PTY session.
- `read_pty_session`: Read PTY session state (screen + scrollback).
- `list_pty_sessions`: List active PTY sessions.
- `close_pty_session`: Terminate PTY session.
- `resize_pty_session`: Resize PTY session terminal dimensions.

## Network Tools

- `web_fetch`: Fetch content from a URL and process it. Converts HTML to markdown.

## Planning & Diagnostics

- `update_plan`: Track multi-step plan with status (pending|in_progress|completed). Use 2-5 milestone items.
- `get_errors`: Aggregate recent error traces from session archives and tool outputs.
- `debug_agent`: Return diagnostic information about the agent environment.
- `analyze_agent`: Return analysis of agent behavior and performance metrics.

## Skills (Reusable Code Functions)

- `save_skill`: Save a reusable skill (code function) to .vtcode/skills/ for later use.
- `load_skill`: Load and retrieve a saved skill by name.
- `list_skills`: List all available saved skills in the workspace.
- `search_skills`: Search for skills by keyword, name, description, or tag.

## Code Execution

- `execute_code`: Execute Python or JavaScript code with access to MCP tools as library functions.
- `search_tools`: Search available MCP tools by keyword with progressive disclosure.

## Code Indexer Tools

The workspace has a semantic code indexer powered by tree-sitter. Use these tools for intelligent code navigation:

- `indexer_search_code`: Search for code patterns using regex. Faster than grep for indexed workspaces.
- `indexer_search_files`: Find files by name pattern (glob-style).
- `indexer_analyze_file`: Get semantic analysis of a file including symbols, metrics, and dependencies.
- `indexer_extract_symbols`: Extract all symbols (functions, classes, variables) from a file.
- `indexer_get_metrics`: Get code metrics (lines of code, comment ratio, function count, etc.) for a file.
- `indexer_detect_language`: Detect the programming language of a file.

These tools provide faster, more accurate results than grep/find for code exploration.

## Sub-Agents: `sub_agent_*`

You can delegate tasks to specialized sub-agents:

- `sub_agent_code_analyzer`: Analyzes code structure without making changes. Use for deep code analysis.
- `sub_agent_code_writer`: Writes and modifies code. Use for implementing features.
- `sub_agent_test_runner`: Runs tests and analyzes results. Use for test execution.
- `sub_agent_researcher`: Searches documentation and gathers information. Use for research tasks.
- `sub_agent_shell_executor`: Executes shell commands safely. Use for system operations.

When to use sub-agents:
- Complex tasks that can be fully delegated in isolation
- Tasks that are independent and can run in parallel
- When focused reasoning or heavy context usage would bloat the main thread

## Important Notes

- Whenever possible, parallelize independent tasks by making multiple tool calls or spawning sub-agents in parallel
- Keep the user informed of your progress
- Be cautious with destructive operations and ask for confirmation when appropriate"#,
            workspace = workspace_path.display(),
            date = current_date,
            project_instructions = project_instructions
        );
        drop(workspace_path);

        // Start session for persistence (if enabled and not already started)
        self.start_session().await;

        // Record user message in session
        self.record_user_message(initial_prompt).await;

        // Load persisted conversation history and add the new user message
        let mut history_guard = self.conversation_history.write().await;

        // Add the new user message to history
        history_guard.push(Message::User {
            content: OneOrMany::one(UserContent::Text(Text {
                text: initial_prompt.to_string(),
            })),
        });

        // Clone history for use in the loop (we'll update the persisted version at the end)
        let original_history_len = history_guard.len();
        let mut chat_history: Vec<Message> = history_guard.clone();
        drop(history_guard);

        // Update context manager with current history
        self.context_manager.update_from_messages(&chat_history).await;

        // Enforce context window limits if needed
        let alert_level = self.context_manager.alert_level().await;
        if matches!(alert_level, TokenAlertLevel::Alert | TokenAlertLevel::Critical) {
            let utilization_before = self.context_manager.utilization().await;
            tracing::info!(
                "Context alert level {:?} ({:.1}% utilization), enforcing context window",
                alert_level,
                utilization_before * 100.0
            );
            chat_history = self.context_manager.enforce_context_window(&chat_history).await;

            // Update stats after pruning
            self.context_manager.update_from_messages(&chat_history).await;
            let utilization_after = self.context_manager.utilization().await;

            // Emit context event to frontend
            let _ = self.event_tx.send(AiEvent::ContextPruned {
                messages_removed: original_history_len.saturating_sub(chat_history.len()),
                utilization_before,
                utilization_after,
            });
        }

        let mut accumulated_response = String::new();
        let mut iteration = 0;

        loop {
            iteration += 1;
            if iteration > MAX_TOOL_ITERATIONS {
                let _ = self.event_tx.send(AiEvent::Error {
                    message: "Maximum tool iterations reached".to_string(),
                    error_type: "max_iterations".to_string(),
                });
                break;
            }

            // Build request
            let request = rig::completion::CompletionRequest {
                preamble: Some(system_prompt.clone()),
                chat_history: OneOrMany::many(chat_history.clone())
                    .unwrap_or_else(|_| OneOrMany::one(chat_history[0].clone())),
                documents: vec![],
                tools: tools.clone(),
                temperature: Some(0.7),
                max_tokens: Some(8192),
                tool_choice: None,
                additional_params: None,
            };

            // Make completion request
            let response = model
                .completion(request)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            // Process response
            let mut has_tool_calls = false;
            let mut tool_calls_to_execute: Vec<ToolCall> = vec![];
            let mut text_content = String::new();

            for content in response.choice.iter() {
                match content {
                    AssistantContent::Text(text) => {
                        text_content.push_str(&text.text);
                    }
                    AssistantContent::ToolCall(tool_call) => {
                        has_tool_calls = true;
                        tool_calls_to_execute.push(tool_call.clone());
                    }
                    _ => {}
                }
            }

            // Emit text delta if we have text
            if !text_content.is_empty() {
                accumulated_response.push_str(&text_content);
                let _ = self.event_tx.send(AiEvent::TextDelta {
                    delta: text_content.clone(),
                    accumulated: accumulated_response.clone(),
                });
            }

            // If no tool calls, we're done
            if !has_tool_calls {
                break;
            }

            // Add assistant response to history
            let assistant_content: Vec<AssistantContent> =
                response.choice.iter().cloned().collect();
            chat_history.push(Message::Assistant {
                id: None,
                content: OneOrMany::many(assistant_content).unwrap_or_else(|_| {
                    OneOrMany::one(AssistantContent::Text(Text {
                        text: String::new(),
                    }))
                }),
            });

            // Execute tool calls and collect results
            let mut tool_results: Vec<UserContent> = vec![];

            for tool_call in tool_calls_to_execute {
                let tool_name = &tool_call.function.name;
                // Normalize run_pty_cmd args to convert array commands to strings
                let tool_args = if tool_name == "run_pty_cmd" {
                    Self::normalize_run_pty_cmd_args(tool_call.function.arguments.clone())
                } else {
                    tool_call.function.arguments.clone()
                };
                let tool_id = tool_call.id.clone();

                // ================================================================
                // HITL: Check if tool needs approval before execution
                // ================================================================
                let (result_value, success) =
                    match self.execute_with_hitl(tool_name, &tool_args, &tool_id, &context, model).await {
                        Ok((result, success)) => (result, success),
                        Err(e) => (serde_json::json!({ "error": e.to_string() }), false),
                    };

                // Emit tool result event
                let _ = self.event_tx.send(AiEvent::ToolResult {
                    tool_name: tool_name.clone(),
                    result: result_value.clone(),
                    success,
                    request_id: tool_id.clone(),
                });

                // Record tool use in session
                let result_text = serde_json::to_string(&result_value).unwrap_or_default();
                self.record_tool_use(tool_name, &result_text).await;

                // Add to tool results for LLM
                tool_results.push(UserContent::ToolResult(ToolResult {
                    id: tool_id.clone(),
                    call_id: Some(tool_id),
                    content: OneOrMany::one(ToolResultContent::Text(Text { text: result_text })),
                }));
            }

            // Add tool results as user message
            chat_history.push(Message::User {
                content: OneOrMany::many(tool_results).unwrap_or_else(|_| {
                    OneOrMany::one(UserContent::Text(Text {
                        text: "Tool executed".to_string(),
                    }))
                }),
            });
        }

        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Persist the updated conversation history (includes all messages from this turn)
        // We need to add the final assistant response if there was text content
        {
            let mut history_guard = self.conversation_history.write().await;
            // The chat_history now contains all messages including tool calls and results
            // We want to persist a clean version: user message + final assistant text
            // For simplicity, we'll store the final assistant text response
            if !accumulated_response.is_empty() {
                history_guard.push(Message::Assistant {
                    id: None,
                    content: OneOrMany::one(AssistantContent::Text(Text {
                        text: accumulated_response.clone(),
                    })),
                });
            }
        }

        // Record assistant response in session and save
        if !accumulated_response.is_empty() {
            self.record_assistant_message(&accumulated_response).await;
            // Save session after each complete turn (user message + assistant response)
            self.save_session().await;
        }

        // Emit completion event
        let _ = self.event_tx.send(AiEvent::Completed {
            response: accumulated_response.clone(),
            tokens_used: None,
            duration_ms: Some(duration_ms),
        });

        Ok(accumulated_response)
    }

    /// Execute a sub-agent with the given task and context.
    async fn execute_sub_agent(
        &self,
        agent_id: &str,
        args: &serde_json::Value,
        parent_context: &SubAgentContext,
        model: &rig_anthropic_vertex::CompletionModel,
    ) -> Result<SubAgentResult> {
        let start_time = std::time::Instant::now();

        // Get the sub-agent definition
        let registry = self.sub_agent_registry.read().await;
        let agent_def = registry
            .get(agent_id)
            .ok_or_else(|| anyhow::anyhow!("Sub-agent '{}' not found", agent_id))?
            .clone();
        drop(registry);

        // Extract task and additional context from args
        let task = args
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Sub-agent call missing 'task' parameter"))?;
        let additional_context = args.get("context").and_then(|v| v.as_str()).unwrap_or("");

        // Build the sub-agent context with incremented depth
        let sub_context = SubAgentContext {
            original_request: parent_context.original_request.clone(),
            conversation_summary: parent_context.conversation_summary.clone(),
            variables: parent_context.variables.clone(),
            depth: parent_context.depth + 1,
        };

        // Build the prompt for the sub-agent
        let sub_prompt = if additional_context.is_empty() {
            task.to_string()
        } else {
            format!("{}\n\nAdditional context: {}", task, additional_context)
        };

        // Emit sub-agent start event
        let _ = self.event_tx.send(AiEvent::SubAgentStarted {
            agent_id: agent_id.to_string(),
            agent_name: agent_def.name.clone(),
            task: task.to_string(),
            depth: sub_context.depth,
        });

        // Build filtered tools based on agent's allowed tools
        let all_tools = Self::get_tool_definitions();
        let tools: Vec<ToolDefinition> = if agent_def.allowed_tools.is_empty() {
            all_tools
        } else {
            let allowed_set: HashSet<&str> =
                agent_def.allowed_tools.iter().map(|s| s.as_str()).collect();
            all_tools
                .into_iter()
                .filter(|t| allowed_set.contains(t.name.as_str()))
                .collect()
        };

        // Build chat history for sub-agent
        let mut chat_history: Vec<Message> = vec![Message::User {
            content: OneOrMany::one(UserContent::Text(Text {
                text: sub_prompt.clone(),
            })),
        }];

        let mut accumulated_response = String::new();
        let mut iteration = 0;

        loop {
            iteration += 1;
            if iteration > agent_def.max_iterations {
                let _ = self.event_tx.send(AiEvent::SubAgentError {
                    agent_id: agent_id.to_string(),
                    error: "Maximum iterations reached".to_string(),
                });
                break;
            }

            // Build request with sub-agent's system prompt
            let request = rig::completion::CompletionRequest {
                preamble: Some(agent_def.system_prompt.clone()),
                chat_history: OneOrMany::many(chat_history.clone())
                    .unwrap_or_else(|_| OneOrMany::one(chat_history[0].clone())),
                documents: vec![],
                tools: tools.clone(),
                temperature: Some(0.7),
                max_tokens: Some(8192),
                tool_choice: None,
                additional_params: None,
            };

            // Make completion request
            let response = match model.completion(request).await {
                Ok(r) => r,
                Err(e) => {
                    let _ = self.event_tx.send(AiEvent::SubAgentError {
                        agent_id: agent_id.to_string(),
                        error: e.to_string(),
                    });
                    return Ok(SubAgentResult {
                        agent_id: agent_id.to_string(),
                        response: format!("Error: {}", e),
                        context: sub_context,
                        success: false,
                        duration_ms: start_time.elapsed().as_millis() as u64,
                    });
                }
            };

            // Process response
            let mut has_tool_calls = false;
            let mut tool_calls_to_execute: Vec<ToolCall> = vec![];
            let mut text_content = String::new();

            for content in response.choice.iter() {
                match content {
                    AssistantContent::Text(text) => {
                        text_content.push_str(&text.text);
                    }
                    AssistantContent::ToolCall(tool_call) => {
                        has_tool_calls = true;
                        tool_calls_to_execute.push(tool_call.clone());
                    }
                    _ => {}
                }
            }

            // Accumulate text
            if !text_content.is_empty() {
                accumulated_response.push_str(&text_content);
            }

            // If no tool calls, we're done
            if !has_tool_calls {
                break;
            }

            // Add assistant response to history
            let assistant_content: Vec<AssistantContent> =
                response.choice.iter().cloned().collect();
            chat_history.push(Message::Assistant {
                id: None,
                content: OneOrMany::many(assistant_content).unwrap_or_else(|_| {
                    OneOrMany::one(AssistantContent::Text(Text {
                        text: String::new(),
                    }))
                }),
            });

            // Execute tool calls
            let mut tool_results: Vec<UserContent> = vec![];

            for tool_call in tool_calls_to_execute {
                let tool_name = &tool_call.function.name;
                // Normalize run_pty_cmd args to convert array commands to strings
                let tool_args = if tool_name == "run_pty_cmd" {
                    Self::normalize_run_pty_cmd_args(tool_call.function.arguments.clone())
                } else {
                    tool_call.function.arguments.clone()
                };
                let tool_id = tool_call.id.clone();

                // Execute the tool
                let (result_value, _success) = if tool_name == "run_pty_cmd"
                    && self.pty_manager.is_some()
                    && self.current_session_id.read().await.is_some()
                {
                    // Intercept run_pty_cmd and execute in user's terminal instead
                    let command = tool_args
                        .get("command")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    match self.execute_in_terminal(command).await {
                        Ok(v) => (v, true),
                        Err(e) => (serde_json::json!({ "error": e.to_string() }), false),
                    }
                } else {
                    let mut registry = self.tool_registry.write().await;
                    let result = registry.execute_tool(tool_name, tool_args).await;

                    match &result {
                        Ok(v) => (v.clone(), true),
                        Err(e) => (serde_json::json!({ "error": e.to_string() }), false),
                    }
                };

                // Add to tool results
                let result_text = serde_json::to_string(&result_value).unwrap_or_default();
                tool_results.push(UserContent::ToolResult(ToolResult {
                    id: tool_id.clone(),
                    call_id: Some(tool_id),
                    content: OneOrMany::one(ToolResultContent::Text(Text { text: result_text })),
                }));
            }

            // Add tool results as user message
            chat_history.push(Message::User {
                content: OneOrMany::many(tool_results).unwrap_or_else(|_| {
                    OneOrMany::one(UserContent::Text(Text {
                        text: "Tool executed".to_string(),
                    }))
                }),
            });
        }

        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Emit sub-agent completed event
        let _ = self.event_tx.send(AiEvent::SubAgentCompleted {
            agent_id: agent_id.to_string(),
            response: accumulated_response.clone(),
            duration_ms,
        });

        Ok(SubAgentResult {
            agent_id: agent_id.to_string(),
            response: accumulated_response,
            context: sub_context,
            success: true,
            duration_ms,
        })
    }

    /// Execute a tool by name.
    /// Note: execute_tool requires &mut self, hence RwLock.
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        // Normalize run_pty_cmd args to convert array commands to strings
        let normalized_args = if tool_name == "run_pty_cmd" {
            Self::normalize_run_pty_cmd_args(args)
        } else {
            args
        };

        // Intercept run_pty_cmd if we have terminal access
        if tool_name == "run_pty_cmd"
            && self.pty_manager.is_some()
            && self.current_session_id.read().await.is_some()
        {
            let command = normalized_args
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            return self.execute_in_terminal(command).await;
        }

        // Execute the tool (requires write lock due to &mut self)
        let mut registry = self.tool_registry.write().await;
        let result = registry.execute_tool(tool_name, normalized_args).await;

        result.map_err(|e| anyhow::anyhow!(e))
    }

    /// Get available tools for the LLM.
    /// Returns tool names as JSON.
    pub async fn available_tools(&self) -> Vec<serde_json::Value> {
        let registry = self.tool_registry.read().await;
        // available_tools() returns Vec<String> (tool names)
        let tool_names = registry.available_tools().await;

        // Convert tool names to JSON objects
        tool_names
            .into_iter()
            .map(|name| {
                serde_json::json!({
                    "name": name,
                })
            })
            .collect()
    }

    /// Get the workspace path (async since it's behind a lock).
    #[allow(dead_code)]
    pub async fn workspace(&self) -> PathBuf {
        self.workspace.read().await.clone()
    }

    /// Update the workspace/working directory.
    /// This updates the system prompt for future requests.
    pub async fn set_workspace(&self, new_workspace: PathBuf) {
        let mut workspace = self.workspace.write().await;
        *workspace = new_workspace;
    }

    /// Get provider name.
    #[allow(dead_code)]
    pub fn provider(&self) -> &str {
        &self.provider_name
    }

    /// Get model name.
    #[allow(dead_code)]
    pub fn model(&self) -> &str {
        &self.model_name
    }

    /// Register a new sub-agent.
    #[allow(dead_code)]
    pub async fn register_sub_agent(&self, agent: SubAgentDefinition) {
        let mut registry = self.sub_agent_registry.write().await;
        registry.register(agent);
    }

    /// Remove a sub-agent by ID.
    #[allow(dead_code)]
    pub async fn unregister_sub_agent(&self, agent_id: &str) -> Option<SubAgentDefinition> {
        let mut registry = self.sub_agent_registry.write().await;
        registry.remove(agent_id)
    }

    /// Get list of registered sub-agents.
    #[allow(dead_code)]
    pub async fn list_sub_agents(&self) -> Vec<serde_json::Value> {
        let registry = self.sub_agent_registry.read().await;
        registry
            .all()
            .map(|agent| {
                serde_json::json!({
                    "id": agent.id,
                    "name": agent.name,
                    "description": agent.description,
                    "allowed_tools": agent.allowed_tools,
                    "max_iterations": agent.max_iterations,
                })
            })
            .collect()
    }

    /// Check if a sub-agent exists.
    #[allow(dead_code)]
    pub async fn has_sub_agent(&self, agent_id: &str) -> bool {
        let registry = self.sub_agent_registry.read().await;
        registry.contains(agent_id)
    }

    /// Clear the conversation history.
    /// Call this when starting a new conversation or when the user wants to reset context.
    pub async fn clear_conversation_history(&self) {
        // Finalize current session before clearing
        self.finalize_session().await;

        let mut history = self.conversation_history.write().await;
        history.clear();
        tracing::debug!("Conversation history cleared");
    }

    /// Get the current conversation history length (for debugging/UI).
    pub async fn conversation_history_len(&self) -> usize {
        self.conversation_history.read().await.len()
    }

    /// Restore conversation history from a previous session.
    /// This clears the current history and replaces it with the restored messages.
    pub async fn restore_session(&self, messages: Vec<super::session::QbitSessionMessage>) {
        // Finalize any current session first
        self.finalize_session().await;

        // Convert QbitSessionMessages to rig Messages
        let rig_messages: Vec<Message> = messages
            .iter()
            .filter_map(|m| m.to_rig_message())
            .collect();

        // Replace conversation history
        let mut history = self.conversation_history.write().await;
        *history = rig_messages;

        tracing::info!(
            "Restored session with {} messages ({} in history)",
            messages.len(),
            history.len()
        );
    }

    // ========================================================================
    // Session Persistence Methods
    // ========================================================================

    /// Enable or disable session persistence.
    pub async fn set_session_persistence_enabled(&self, enabled: bool) {
        *self.session_persistence_enabled.write().await = enabled;
        tracing::debug!("Session persistence enabled: {}", enabled);
    }

    /// Check if session persistence is enabled.
    pub async fn is_session_persistence_enabled(&self) -> bool {
        *self.session_persistence_enabled.read().await
    }

    /// Start a new session for persistence.
    /// Called automatically when first message is sent (if persistence is enabled).
    async fn start_session(&self) {
        if !*self.session_persistence_enabled.read().await {
            return;
        }

        // Only start if no active session
        let mut manager_guard = self.session_manager.write().await;
        if manager_guard.is_some() {
            return;
        }

        let workspace = self.workspace.read().await.clone();
        match QbitSessionManager::new(workspace, &self.model_name, &self.provider_name).await {
            Ok(manager) => {
                *manager_guard = Some(manager);
                tracing::debug!("Session started for persistence");
            }
            Err(e) => {
                tracing::warn!("Failed to start session for persistence: {}", e);
            }
        }
    }

    /// Record a user message in the current session.
    async fn record_user_message(&self, content: &str) {
        let mut manager_guard = self.session_manager.write().await;
        if let Some(ref mut manager) = *manager_guard {
            manager.add_user_message(content);
        }
    }

    /// Record an assistant message in the current session.
    async fn record_assistant_message(&self, content: &str) {
        let mut manager_guard = self.session_manager.write().await;
        if let Some(ref mut manager) = *manager_guard {
            manager.add_assistant_message(content);
        }
    }

    /// Record a tool use in the current session.
    async fn record_tool_use(&self, tool_name: &str, result: &str) {
        let mut manager_guard = self.session_manager.write().await;
        if let Some(ref mut manager) = *manager_guard {
            manager.add_tool_use(tool_name, result);
        }
    }

    /// Save the current session to disk (incremental save).
    /// This saves the current state without finalizing the session.
    async fn save_session(&self) {
        let manager_guard = self.session_manager.read().await;
        if let Some(ref manager) = *manager_guard {
            match manager.save() {
                Ok(path) => {
                    tracing::debug!("Session saved to: {}", path.display());
                }
                Err(e) => {
                    tracing::warn!("Failed to save session: {}", e);
                }
            }
        }
    }

    /// Finalize and save the current session.
    /// Returns the path to the saved session file, if any.
    pub async fn finalize_session(&self) -> Option<PathBuf> {
        let mut manager_guard = self.session_manager.write().await;
        if let Some(ref mut manager) = manager_guard.take() {
            match manager.finalize() {
                Ok(path) => {
                    tracing::info!("Session finalized: {}", path.display());
                    return Some(path);
                }
                Err(e) => {
                    tracing::warn!("Failed to finalize session: {}", e);
                }
            }
        }
        None
    }

    // ========================================================================
    // HITL (Human-in-the-Loop) Methods
    // ========================================================================

    /// Execute a tool with HITL approval check.
    /// This is the main entry point for tool execution during agent loops.
    ///
    /// Policy flow:
    /// 1. Check if tool is denied by policy → return error immediately
    /// 2. Apply constraints → return error if violated, possibly modify args
    /// 3. Check if tool is allowed by policy → execute directly
    /// 4. Otherwise, proceed to HITL approval flow
    async fn execute_with_hitl(
        &self,
        tool_name: &str,
        tool_args: &serde_json::Value,
        tool_id: &str,
        context: &SubAgentContext,
        model: &rig_anthropic_vertex::CompletionModel,
    ) -> Result<(serde_json::Value, bool)> {
        // ================================================================
        // Step 1: Check if tool is denied by policy
        // ================================================================
        if self.tool_policy_manager.is_denied(tool_name).await {
            let _ = self.event_tx.send(AiEvent::ToolDenied {
                request_id: tool_id.to_string(),
                tool_name: tool_name.to_string(),
                args: tool_args.clone(),
                reason: "Tool is denied by policy".to_string(),
            });
            return Ok((
                json!({
                    "error": format!("Tool '{}' is denied by policy", tool_name),
                    "denied_by_policy": true
                }),
                false,
            ));
        }

        // ================================================================
        // Step 2: Apply constraints and check for violations
        // ================================================================
        let (effective_args, constraint_note) = match self
            .tool_policy_manager
            .apply_constraints(tool_name, tool_args)
            .await
        {
            PolicyConstraintResult::Allowed => (tool_args.clone(), None),
            PolicyConstraintResult::Violated(reason) => {
                let _ = self.event_tx.send(AiEvent::ToolDenied {
                    request_id: tool_id.to_string(),
                    tool_name: tool_name.to_string(),
                    args: tool_args.clone(),
                    reason: reason.clone(),
                });
                return Ok((
                    json!({
                        "error": format!("Tool constraint violated: {}", reason),
                        "constraint_violated": true
                    }),
                    false,
                ));
            }
            PolicyConstraintResult::Modified(modified_args, note) => {
                tracing::info!(
                    "Tool '{}' args modified by constraint: {}",
                    tool_name,
                    note
                );
                (modified_args, Some(note))
            }
        };

        // ================================================================
        // Step 3: Check if tool is allowed by policy (bypasses HITL)
        // ================================================================
        let policy = self.tool_policy_manager.get_policy(tool_name).await;
        if policy == ToolPolicy::Allow {
            // Emit auto-approval event for UI notification
            let reason = if let Some(note) = constraint_note {
                format!("Allowed by policy ({})", note)
            } else {
                "Allowed by tool policy".to_string()
            };
            let _ = self.event_tx.send(AiEvent::ToolAutoApproved {
                request_id: tool_id.to_string(),
                tool_name: tool_name.to_string(),
                args: effective_args.clone(),
                reason,
            });

            // Execute directly without approval
            return self
                .execute_tool_direct(tool_name, &effective_args, context, model)
                .await;
        }

        // ================================================================
        // Step 4: Fall through to HITL approval system
        // ================================================================
        // Check if tool should be auto-approved based on learned patterns
        if self.approval_recorder.should_auto_approve(tool_name).await {
            // Emit auto-approval event for UI notification
            let _ = self.event_tx.send(AiEvent::ToolAutoApproved {
                request_id: tool_id.to_string(),
                tool_name: tool_name.to_string(),
                args: effective_args.clone(),
                reason: "Auto-approved based on learned patterns or always-allow list".to_string(),
            });

            // Execute directly without approval
            return self
                .execute_tool_direct(tool_name, &effective_args, context, model)
                .await;
        }

        // Need approval - create request with stats
        let stats = self.approval_recorder.get_pattern(tool_name).await;
        let risk_level = RiskLevel::for_tool(tool_name);
        let config = self.approval_recorder.get_config().await;
        let can_learn = !config
            .always_require_approval
            .contains(&tool_name.to_string());
        let suggestion = self.approval_recorder.get_suggestion(tool_name).await;

        // Create oneshot channel for response
        let (tx, rx) = oneshot::channel::<ApprovalDecision>();

        // Store the sender
        {
            let mut pending = self.pending_approvals.write().await;
            pending.insert(tool_id.to_string(), tx);
        }

        // Emit approval request event with HITL metadata
        let _ = self.event_tx.send(AiEvent::ToolApprovalRequest {
            request_id: tool_id.to_string(),
            tool_name: tool_name.to_string(),
            args: effective_args.clone(),
            stats,
            risk_level,
            can_learn,
            suggestion,
        });

        // Wait for approval response (with timeout of 5 minutes)
        match tokio::time::timeout(std::time::Duration::from_secs(300), rx).await {
            Ok(Ok(decision)) => {
                if decision.approved {
                    // Record approval
                    let _ = self
                        .approval_recorder
                        .record_approval(
                            tool_name,
                            true,
                            decision.reason,
                            decision.always_allow,
                        )
                        .await;

                    // Execute the tool with effective (possibly constrained) args
                    self.execute_tool_direct(tool_name, &effective_args, context, model)
                        .await
                } else {
                    // Record denial
                    let _ = self
                        .approval_recorder
                        .record_approval(tool_name, false, decision.reason, false)
                        .await;

                    Ok((
                        json!({"error": "Tool execution denied by user", "denied": true}),
                        false,
                    ))
                }
            }
            Ok(Err(_)) => {
                // Channel closed (sender dropped)
                Ok((
                    json!({"error": "Approval request cancelled", "cancelled": true}),
                    false,
                ))
            }
            Err(_) => {
                // Timeout - clean up pending approval
                let mut pending = self.pending_approvals.write().await;
                pending.remove(tool_id);

                Ok((
                    json!({"error": "Approval request timed out after 5 minutes", "timeout": true}),
                    false,
                ))
            }
        }
    }

    /// Execute a tool directly (after approval or auto-approved).
    async fn execute_tool_direct(
        &self,
        tool_name: &str,
        tool_args: &serde_json::Value,
        context: &SubAgentContext,
        model: &rig_anthropic_vertex::CompletionModel,
    ) -> Result<(serde_json::Value, bool)> {
        // Check if this is an indexer tool call
        if tool_name.starts_with("indexer_") {
            return Ok(self.execute_indexer_tool(tool_name, tool_args).await);
        }

        // Check if this is a sub-agent call
        if tool_name.starts_with("sub_agent_") {
            let agent_id = tool_name.strip_prefix("sub_agent_").unwrap_or("");
            match self
                .execute_sub_agent(agent_id, tool_args, context, model)
                .await
            {
                Ok(result) => {
                    return Ok((
                        json!({
                            "agent_id": result.agent_id,
                            "response": result.response,
                            "success": result.success,
                            "duration_ms": result.duration_ms
                        }),
                        result.success,
                    ));
                }
                Err(e) => return Ok((json!({ "error": e.to_string() }), false)),
            }
        }

        // Check if this is a terminal command that should be intercepted
        if tool_name == "run_pty_cmd"
            && self.pty_manager.is_some()
            && self.current_session_id.read().await.is_some()
        {
            let command = tool_args
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            match self.execute_in_terminal(command).await {
                Ok(v) => return Ok((v, true)),
                Err(e) => return Ok((json!({"error": e.to_string()}), false)),
            }
        }

        // Execute regular tool via registry
        let mut registry = self.tool_registry.write().await;
        let result = registry.execute_tool(tool_name, tool_args.clone()).await;

        match &result {
            Ok(v) => {
                let is_success = v
                    .get("exit_code")
                    .and_then(|ec| ec.as_i64())
                    .map(|ec| ec == 0)
                    .unwrap_or(true);
                Ok((v.clone(), is_success))
            }
            Err(e) => Ok((json!({"error": e.to_string()}), false)),
        }
    }

    /// Get all approval patterns.
    pub async fn get_approval_patterns(&self) -> Vec<ApprovalPattern> {
        self.approval_recorder.get_all_patterns().await
    }

    /// Get the approval pattern for a specific tool.
    pub async fn get_tool_approval_pattern(&self, tool_name: &str) -> Option<ApprovalPattern> {
        self.approval_recorder.get_pattern(tool_name).await
    }

    /// Get the HITL configuration.
    pub async fn get_hitl_config(&self) -> ToolApprovalConfig {
        self.approval_recorder.get_config().await
    }

    /// Set the HITL configuration.
    pub async fn set_hitl_config(&self, config: ToolApprovalConfig) -> Result<()> {
        self.approval_recorder.set_config(config).await
    }

    /// Add a tool to the always-allow list.
    pub async fn add_tool_always_allow(&self, tool_name: &str) -> Result<()> {
        self.approval_recorder.add_always_allow(tool_name).await
    }

    /// Remove a tool from the always-allow list.
    pub async fn remove_tool_always_allow(&self, tool_name: &str) -> Result<()> {
        self.approval_recorder.remove_always_allow(tool_name).await
    }

    /// Reset all approval patterns.
    pub async fn reset_approval_patterns(&self) -> Result<()> {
        self.approval_recorder.reset_patterns().await
    }

    /// Respond to a pending approval request.
    pub async fn respond_to_approval(&self, decision: ApprovalDecision) -> Result<()> {
        // Get the pending sender
        let sender = {
            let mut pending = self.pending_approvals.write().await;
            pending.remove(&decision.request_id)
        };

        // Record the decision (for pattern learning)
        self.approval_recorder
            .record_approval(
                &decision.request_id.split('_').last().unwrap_or("unknown"), // Extract tool name from request_id
                decision.approved,
                decision.reason.clone(),
                decision.always_allow,
            )
            .await?;

        // Send the response to unblock the waiting tool execution
        if let Some(sender) = sender {
            let _ = sender.send(decision);
        } else {
            tracing::warn!(
                "No pending approval found for request_id: {}",
                decision.request_id
            );
        }

        Ok(())
    }

    // ========================================================================
    // Tool Policy Methods
    // ========================================================================

    /// Get the tool policy configuration.
    pub async fn get_tool_policy_config(&self) -> ToolPolicyConfig {
        self.tool_policy_manager.get_config().await
    }

    /// Set the tool policy configuration.
    pub async fn set_tool_policy_config(&self, config: ToolPolicyConfig) -> Result<()> {
        self.tool_policy_manager.set_config(config).await
    }

    /// Get the policy for a specific tool.
    pub async fn get_tool_policy(&self, tool_name: &str) -> ToolPolicy {
        self.tool_policy_manager.get_policy(tool_name).await
    }

    /// Set the policy for a specific tool.
    pub async fn set_tool_policy(&self, tool_name: &str, policy: ToolPolicy) -> Result<()> {
        self.tool_policy_manager.set_policy(tool_name, policy).await
    }

    /// Reset tool policies to defaults.
    pub async fn reset_tool_policies(&self) -> Result<()> {
        self.tool_policy_manager.reset_to_defaults().await
    }

    /// Enable full-auto mode with the given allowed tools.
    pub async fn enable_full_auto_mode(&self, allowed_tools: Vec<String>) {
        self.tool_policy_manager.enable_full_auto(allowed_tools).await;
    }

    /// Disable full-auto mode.
    pub async fn disable_full_auto_mode(&self) {
        self.tool_policy_manager.disable_full_auto().await;
    }

    /// Check if full-auto mode is enabled.
    pub async fn is_full_auto_mode_enabled(&self) -> bool {
        self.tool_policy_manager.is_full_auto_enabled().await
    }

    // ========================================================================
    // Context Management Methods
    // ========================================================================

    /// Get the context manager reference.
    pub fn context_manager(&self) -> Arc<ContextManager> {
        Arc::clone(&self.context_manager)
    }

    /// Get current context summary (token usage, alert level, etc.).
    pub async fn get_context_summary(&self) -> ContextSummary {
        self.context_manager.get_summary().await
    }

    /// Get current token usage statistics.
    pub async fn get_token_usage_stats(&self) -> TokenUsageStats {
        self.context_manager.stats().await
    }

    /// Get current token alert level.
    pub async fn get_token_alert_level(&self) -> TokenAlertLevel {
        self.context_manager.alert_level().await
    }

    /// Get context utilization percentage (0.0 - 1.0+).
    pub async fn get_context_utilization(&self) -> f64 {
        self.context_manager.utilization().await
    }

    /// Get remaining available tokens.
    pub async fn get_remaining_tokens(&self) -> usize {
        self.context_manager.remaining_tokens().await
    }

    /// Update token budget from current conversation history.
    pub async fn update_context_from_history(&self) {
        let history = self.conversation_history.read().await;
        self.context_manager.update_from_messages(&history).await;
    }

    /// Enforce context window limits by pruning old messages if needed.
    /// Returns the number of messages pruned.
    pub async fn enforce_context_window(&self) -> usize {
        let mut history = self.conversation_history.write().await;
        let original_len = history.len();
        let pruned = self.context_manager.enforce_context_window(&history).await;
        let pruned_count = original_len.saturating_sub(pruned.len());
        *history = pruned;
        pruned_count
    }

    /// Reset the context manager (clear all token tracking).
    pub async fn reset_context_manager(&self) {
        self.context_manager.reset().await;
    }

    /// Get the context trim configuration.
    pub fn get_context_trim_config(&self) -> ContextTrimConfig {
        self.context_manager.trim_config().clone()
    }

    /// Check if context management is enabled.
    pub fn is_context_management_enabled(&self) -> bool {
        self.context_manager.is_enabled()
    }

    /// Truncate a tool response if it exceeds limits.
    pub async fn truncate_tool_response(&self, content: &str, tool_name: &str) -> String {
        let result = self
            .context_manager
            .truncate_tool_response(content, tool_name)
            .await;
        result.content
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_normalize_run_pty_cmd_array_to_string() {
        // Command as array with shell operators
        let args = json!({
            "command": ["cd", "/path", "&&", "pwd"],
            "cwd": "."
        });

        let normalized = AgentBridge::normalize_run_pty_cmd_args(args);

        assert_eq!(normalized["command"].as_str().unwrap(), "cd /path && pwd");
        // Other fields should be preserved
        assert_eq!(normalized["cwd"].as_str().unwrap(), ".");
    }

    #[test]
    fn test_normalize_run_pty_cmd_string_unchanged() {
        // Command already as string - should be unchanged
        let args = json!({
            "command": "cd /path && pwd",
            "cwd": "."
        });

        let normalized = AgentBridge::normalize_run_pty_cmd_args(args);

        assert_eq!(normalized["command"].as_str().unwrap(), "cd /path && pwd");
    }

    #[test]
    fn test_normalize_run_pty_cmd_pipe_operator() {
        let args = json!({
            "command": ["ls", "-la", "|", "grep", "foo"]
        });

        let normalized = AgentBridge::normalize_run_pty_cmd_args(args);

        assert_eq!(normalized["command"].as_str().unwrap(), "ls -la | grep foo");
    }

    #[test]
    fn test_normalize_run_pty_cmd_redirect() {
        let args = json!({
            "command": ["echo", "hello", ">", "output.txt"]
        });

        let normalized = AgentBridge::normalize_run_pty_cmd_args(args);

        assert_eq!(
            normalized["command"].as_str().unwrap(),
            "echo hello > output.txt"
        );
    }

    #[test]
    fn test_normalize_run_pty_cmd_empty_array() {
        let args = json!({
            "command": []
        });

        let normalized = AgentBridge::normalize_run_pty_cmd_args(args);

        assert_eq!(normalized["command"].as_str().unwrap(), "");
    }

    #[test]
    fn test_normalize_run_pty_cmd_no_command_field() {
        // Args without command field should pass through unchanged
        let args = json!({
            "cwd": "/some/path"
        });

        let normalized = AgentBridge::normalize_run_pty_cmd_args(args.clone());

        assert_eq!(normalized, args);
    }
}

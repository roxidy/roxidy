use std::collections::HashSet;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use rig::completion::{CompletionModel as RigCompletionModel, AssistantContent, Message, ToolDefinition};
use rig::message::{Text, ToolCall, ToolResult, ToolResultContent, UserContent};
use rig::one_or_many::OneOrMany;
use serde_json::json;
use tokio::sync::{mpsc, RwLock};
use vtcode_core::llm::{make_client, AnyClient};
use vtcode_core::tools::ToolRegistry;
use vtcode_core::tools::registry::build_function_declarations;

use super::events::AiEvent;
use super::sub_agent::{
    create_default_sub_agents, SubAgentContext, SubAgentDefinition, SubAgentRegistry,
    SubAgentResult, MAX_AGENT_DEPTH,
};

/// Maximum number of tool call iterations before stopping
const MAX_TOOL_ITERATIONS: usize = 100;

/// LLM client abstraction that supports both vtcode and rig-based providers
enum LlmClient {
    /// vtcode-core client (OpenRouter, OpenAI, etc.)
    Vtcode(AnyClient),
    /// Anthropic on Vertex AI via rig-anthropic-vertex
    VertexAnthropic(rig_anthropic_vertex::CompletionModel),
}

/// Bridge between Roxidy and LLM providers.
/// Handles LLM streaming and tool execution.
pub struct AgentBridge {
    workspace: PathBuf,
    provider_name: String,
    model_name: String,
    /// ToolRegistry requires &mut self for execute_tool, so we need RwLock
    tool_registry: Arc<RwLock<ToolRegistry>>,
    /// LLM client (either vtcode or rig-based)
    client: Arc<RwLock<LlmClient>>,
    event_tx: mpsc::UnboundedSender<AiEvent>,
    /// Registry of available sub-agents
    sub_agent_registry: Arc<RwLock<SubAgentRegistry>>,
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

        Ok(Self {
            workspace,
            provider_name: provider.to_string(),
            model_name: model.to_string(),
            tool_registry,
            client,
            event_tx,
            sub_agent_registry: Arc::new(RwLock::new(sub_agent_registry)),
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
        let vertex_client =
            rig_anthropic_vertex::Client::from_service_account(credentials_path, project_id, location)
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

        Ok(Self {
            workspace,
            provider_name: "anthropic_vertex".to_string(),
            model_name: model.to_string(),
            tool_registry,
            client: Arc::new(RwLock::new(LlmClient::VertexAnthropic(completion_model))),
            event_tx,
            sub_agent_registry: Arc::new(RwLock::new(sub_agent_registry)),
        })
    }

    /// Get tool definitions in rig format from vtcode's function declarations.
    /// Sanitizes schemas to remove anyOf/allOf/oneOf which Anthropic doesn't support.
    fn get_tool_definitions() -> Vec<ToolDefinition> {
        build_function_declarations()
            .into_iter()
            .map(|fd| ToolDefinition {
                name: fd.name,
                description: fd.description,
                parameters: Self::sanitize_schema(fd.parameters),
            })
            .collect()
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
        self.execute_with_context(prompt, SubAgentContext::default()).await
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
            LlmClient::Vtcode(vtcode_client) => {
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

                self.execute_with_tools(&vertex_model, prompt, start_time).await
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
        self.execute_with_tools_and_context(model, initial_prompt, start_time, SubAgentContext::default())
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
        let system_prompt = format!(
            "You are a helpful coding assistant with access to tools for file operations, \
             code search, and command execution. The workspace is: {}. \
             Use tools when needed to help the user. Always explain what you're doing.",
            self.workspace.display()
        );

        // Build initial chat history
        let mut chat_history: Vec<Message> = vec![
            Message::User {
                content: OneOrMany::one(UserContent::Text(Text {
                    text: initial_prompt.to_string(),
                })),
            },
        ];

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
            let assistant_content: Vec<AssistantContent> = response.choice.iter().cloned().collect();
            chat_history.push(Message::Assistant {
                id: None,
                content: OneOrMany::many(assistant_content)
                    .unwrap_or_else(|_| OneOrMany::one(AssistantContent::Text(Text { text: String::new() }))),
            });

            // Execute tool calls and collect results
            let mut tool_results: Vec<UserContent> = vec![];

            for tool_call in tool_calls_to_execute {
                let tool_name = &tool_call.function.name;
                let tool_args = tool_call.function.arguments.clone();
                let tool_id = tool_call.id.clone();

                // Emit tool request event
                let _ = self.event_tx.send(AiEvent::ToolRequest {
                    tool_name: tool_name.clone(),
                    args: tool_args.clone(),
                    request_id: tool_id.clone(),
                });

                // Check if this is a sub-agent tool call
                let (result_value, success) = if tool_name.starts_with("sub_agent_") {
                    // Extract sub-agent ID from tool name
                    let agent_id = tool_name.strip_prefix("sub_agent_").unwrap_or("");

                    // Execute sub-agent
                    match self.execute_sub_agent(agent_id, &tool_args, &context, model).await {
                        Ok(result) => (serde_json::json!({
                            "agent_id": result.agent_id,
                            "response": result.response,
                            "success": result.success,
                            "duration_ms": result.duration_ms
                        }), result.success),
                        Err(e) => (serde_json::json!({ "error": e.to_string() }), false),
                    }
                } else {
                    // Execute regular tool
                    let mut registry = self.tool_registry.write().await;
                    let result = registry.execute_tool(tool_name, tool_args).await;

                    match &result {
                        Ok(v) => (v.clone(), true),
                        Err(e) => (serde_json::json!({ "error": e.to_string() }), false),
                    }
                };

                // Emit tool result event
                let _ = self.event_tx.send(AiEvent::ToolResult {
                    tool_name: tool_name.clone(),
                    result: result_value.clone(),
                    success,
                    request_id: tool_id.clone(),
                });

                // Add to tool results for LLM
                let result_text = serde_json::to_string(&result_value).unwrap_or_default();
                tool_results.push(UserContent::ToolResult(ToolResult {
                    id: tool_id.clone(),
                    call_id: Some(tool_id),
                    content: OneOrMany::one(ToolResultContent::Text(Text { text: result_text })),
                }));
            }

            // Add tool results as user message
            chat_history.push(Message::User {
                content: OneOrMany::many(tool_results)
                    .unwrap_or_else(|_| OneOrMany::one(UserContent::Text(Text { text: "Tool executed".to_string() }))),
            });
        }

        let duration_ms = start_time.elapsed().as_millis() as u64;

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
        let additional_context = args
            .get("context")
            .and_then(|v| v.as_str())
            .unwrap_or("");

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
            let allowed_set: HashSet<&str> = agent_def.allowed_tools.iter().map(|s| s.as_str()).collect();
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
                let tool_args = tool_call.function.arguments.clone();
                let tool_id = tool_call.id.clone();

                // Emit tool request event for sub-agent
                let _ = self.event_tx.send(AiEvent::SubAgentToolRequest {
                    agent_id: agent_id.to_string(),
                    tool_name: tool_name.clone(),
                    args: tool_args.clone(),
                });

                // Execute the tool
                let mut registry = self.tool_registry.write().await;
                let result = registry.execute_tool(tool_name, tool_args).await;

                let (result_value, success) = match &result {
                    Ok(v) => (v.clone(), true),
                    Err(e) => (serde_json::json!({ "error": e.to_string() }), false),
                };

                // Emit tool result event for sub-agent
                let _ = self.event_tx.send(AiEvent::SubAgentToolResult {
                    agent_id: agent_id.to_string(),
                    tool_name: tool_name.clone(),
                    success,
                });

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
        let request_id = uuid::Uuid::new_v4().to_string();

        // Emit tool request event
        let _ = self.event_tx.send(AiEvent::ToolRequest {
            tool_name: tool_name.to_string(),
            args: args.clone(),
            request_id: request_id.clone(),
        });

        // Execute the tool (requires write lock due to &mut self)
        let mut registry = self.tool_registry.write().await;
        let result = registry.execute_tool(tool_name, args).await;

        // Emit tool result event
        let (result_value, success) = match &result {
            Ok(v) => (v.clone(), true),
            Err(e) => (serde_json::json!({ "error": e.to_string() }), false),
        };

        let _ = self.event_tx.send(AiEvent::ToolResult {
            tool_name: tool_name.to_string(),
            result: result_value.clone(),
            success,
            request_id,
        });

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

    /// Get the workspace path.
    pub fn workspace(&self) -> &std::path::Path {
        &self.workspace
    }

    /// Get provider name.
    pub fn provider(&self) -> &str {
        &self.provider_name
    }

    /// Get model name.
    pub fn model(&self) -> &str {
        &self.model_name
    }

    /// Register a new sub-agent.
    pub async fn register_sub_agent(&self, agent: SubAgentDefinition) {
        let mut registry = self.sub_agent_registry.write().await;
        registry.register(agent);
    }

    /// Remove a sub-agent by ID.
    pub async fn unregister_sub_agent(&self, agent_id: &str) -> Option<SubAgentDefinition> {
        let mut registry = self.sub_agent_registry.write().await;
        registry.remove(agent_id)
    }

    /// Get list of registered sub-agents.
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
    pub async fn has_sub_agent(&self, agent_id: &str) -> bool {
        let registry = self.sub_agent_registry.read().await;
        registry.contains(agent_id)
    }
}

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use rig::completion::CompletionModel as RigCompletionModel;
use tokio::sync::{mpsc, RwLock};
use vtcode_core::llm::{make_client, AnyClient};
use vtcode_core::tools::ToolRegistry;

use super::events::AiEvent;

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

        Ok(Self {
            workspace,
            provider_name: provider.to_string(),
            model_name: model.to_string(),
            tool_registry,
            client,
            event_tx,
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

        Ok(Self {
            workspace,
            provider_name: "anthropic_vertex".to_string(),
            model_name: model.to_string(),
            tool_registry,
            client: Arc::new(RwLock::new(LlmClient::VertexAnthropic(completion_model))),
            event_tx,
        })
    }

    /// Execute a prompt and stream events back to the frontend.
    pub async fn execute(&self, prompt: &str) -> Result<String> {
        // Generate a unique turn ID
        let turn_id = uuid::Uuid::new_v4().to_string();

        // Emit turn started event
        let _ = self.event_tx.send(AiEvent::Started {
            turn_id: turn_id.clone(),
        });

        let start_time = std::time::Instant::now();
        let mut client = self.client.write().await;

        let result: Result<String> = match &mut *client {
            LlmClient::Vtcode(vtcode_client) => {
                // Use vtcode-core's generate method
                vtcode_client
                    .generate(prompt)
                    .await
                    .map(|r| r.content)
                    .map_err(|e| anyhow::anyhow!("{}", e))
            }
            LlmClient::VertexAnthropic(vertex_model) => {
                // Use rig's completion method
                use rig::completion::Message;
                use rig::one_or_many::OneOrMany;

                let request = rig::completion::CompletionRequest {
                    preamble: None,
                    chat_history: OneOrMany::one(Message::User {
                        content: OneOrMany::one(rig::message::UserContent::Text(
                            rig::message::Text {
                                text: prompt.to_string(),
                            },
                        )),
                    }),
                    documents: vec![],
                    tools: vec![],
                    temperature: None,
                    max_tokens: None,
                    tool_choice: None,
                    additional_params: None,
                };

                vertex_model
                    .completion(request)
                    .await
                    .map(|response| {
                        // Extract text from the response
                        response
                            .choice
                            .iter()
                            .filter_map(|c| {
                                if let rig::completion::AssistantContent::Text(t) = c {
                                    Some(t.text.clone())
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("")
                    })
                    .map_err(|e| anyhow::anyhow!("{}", e))
            }
        };

        match result {
            Ok(content) => {
                let duration_ms = start_time.elapsed().as_millis() as u64;

                // Emit the full response as a single text delta (non-streaming for now)
                let _ = self.event_tx.send(AiEvent::TextDelta {
                    delta: content.clone(),
                    accumulated: content.clone(),
                });

                // Emit completion event
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
}

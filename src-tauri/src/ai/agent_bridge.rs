//! Agent bridge for LLM interaction.
//!
//! This module provides the main AgentBridge struct that orchestrates:
//! - LLM communication (vtcode-core and Vertex AI Anthropic)
//! - Tool execution with HITL approval
//! - Conversation history management
//! - Session persistence
//! - Context window management
//! - Loop detection
//!
//! The implementation is split across multiple extension modules:
//! - `bridge_session` - Session persistence and conversation history
//! - `bridge_hitl` - HITL approval handling
//! - `bridge_policy` - Tool policies and loop protection
//! - `bridge_context` - Context window management
//!
//! Core execution logic is in:
//! - `agentic_loop` - Main tool execution loop
//! - `system_prompt` - System prompt building
//! - `sub_agent_executor` - Sub-agent execution

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use rig::completion::{AssistantContent, Message};
use rig::message::{Text, UserContent};
use rig::one_or_many::OneOrMany;
use tokio::sync::{mpsc, oneshot, RwLock};
use vtcode_core::tools::ToolRegistry;

use super::agentic_loop::{run_agentic_loop, AgenticLoopContext};
use super::context_manager::ContextEvent;
use super::context_manager::ContextManager;
use super::events::AiEvent;
use super::hitl::{ApprovalDecision, ApprovalRecorder};
use super::llm_client::{
    create_vertex_components, create_vtcode_components, AgentBridgeComponents, LlmClient,
    VertexAnthropicClientConfig, VtcodeClientConfig,
};
use super::loop_detection::LoopDetector;
use super::session::QbitSessionManager;
use super::sub_agent::{SubAgentContext, SubAgentDefinition, SubAgentRegistry, MAX_AGENT_DEPTH};
use super::system_prompt::build_system_prompt;
use super::tool_executors::normalize_run_pty_cmd_args;
use super::tool_policy::ToolPolicyManager;
use crate::indexer::IndexerState;
use crate::pty::PtyManager;
use crate::tavily::TavilyState;

/// Bridge between Qbit and LLM providers.
/// Handles LLM streaming and tool execution.
pub struct AgentBridge {
    // Core fields
    pub(crate) workspace: Arc<RwLock<PathBuf>>,
    pub(crate) provider_name: String,
    pub(crate) model_name: String,
    pub(crate) tool_registry: Arc<RwLock<ToolRegistry>>,
    pub(crate) client: Arc<RwLock<LlmClient>>,
    pub(crate) event_tx: mpsc::UnboundedSender<AiEvent>,

    // Sub-agents
    pub(crate) sub_agent_registry: Arc<RwLock<SubAgentRegistry>>,

    // Terminal integration
    pub(crate) pty_manager: Option<Arc<PtyManager>>,
    pub(crate) current_session_id: Arc<RwLock<Option<String>>>,

    // Conversation state
    pub(crate) conversation_history: Arc<RwLock<Vec<Message>>>,

    // External services
    pub(crate) indexer_state: Option<Arc<IndexerState>>,
    pub(crate) tavily_state: Option<Arc<TavilyState>>,

    // Session persistence
    pub(crate) session_manager: Arc<RwLock<Option<QbitSessionManager>>>,
    pub(crate) session_persistence_enabled: Arc<RwLock<bool>>,

    // HITL approval
    pub(crate) approval_recorder: Arc<ApprovalRecorder>,
    pub(crate) pending_approvals: Arc<RwLock<HashMap<String, oneshot::Sender<ApprovalDecision>>>>,

    // Tool policy
    pub(crate) tool_policy_manager: Arc<ToolPolicyManager>,

    // Context management
    pub(crate) context_manager: Arc<ContextManager>,
    #[allow(dead_code)]
    pub(crate) context_event_rx: Arc<RwLock<Option<mpsc::Receiver<ContextEvent>>>>,

    // Loop detection
    pub(crate) loop_detector: Arc<RwLock<LoopDetector>>,
}

impl AgentBridge {
    // ========================================================================
    // Constructor Methods
    // ========================================================================

    /// Create a new AgentBridge with vtcode-core (for OpenRouter, OpenAI, etc.)
    pub async fn new(
        workspace: PathBuf,
        provider: &str,
        model: &str,
        api_key: &str,
        event_tx: mpsc::UnboundedSender<AiEvent>,
    ) -> Result<Self> {
        let config = VtcodeClientConfig {
            workspace,
            provider,
            model,
            api_key,
        };

        let components = create_vtcode_components(config).await?;
        let (_context_tx, context_rx) = mpsc::channel::<ContextEvent>(100);

        Ok(Self::from_components(components, event_tx, context_rx))
    }

    /// Create a new AgentBridge for Anthropic on Google Cloud Vertex AI.
    pub async fn new_vertex_anthropic(
        workspace: PathBuf,
        credentials_path: &str,
        project_id: &str,
        location: &str,
        model: &str,
        event_tx: mpsc::UnboundedSender<AiEvent>,
    ) -> Result<Self> {
        let config = VertexAnthropicClientConfig {
            workspace,
            credentials_path,
            project_id,
            location,
            model,
        };

        let components = create_vertex_components(config).await?;
        let (_context_tx, context_rx) = mpsc::channel::<ContextEvent>(100);

        Ok(Self::from_components(components, event_tx, context_rx))
    }

    /// Create an AgentBridge from pre-built components.
    fn from_components(
        components: AgentBridgeComponents,
        event_tx: mpsc::UnboundedSender<AiEvent>,
        context_rx: mpsc::Receiver<ContextEvent>,
    ) -> Self {
        Self {
            workspace: components.workspace,
            provider_name: components.provider_name,
            model_name: components.model_name,
            tool_registry: components.tool_registry,
            client: components.client,
            event_tx,
            sub_agent_registry: components.sub_agent_registry,
            pty_manager: None,
            current_session_id: Arc::new(RwLock::new(None)),
            conversation_history: Arc::new(RwLock::new(Vec::new())),
            indexer_state: None,
            tavily_state: None,
            session_manager: Arc::new(RwLock::new(None)),
            session_persistence_enabled: Arc::new(RwLock::new(true)),
            approval_recorder: components.approval_recorder,
            pending_approvals: Arc::new(RwLock::new(HashMap::new())),
            tool_policy_manager: components.tool_policy_manager,
            context_manager: components.context_manager,
            context_event_rx: Arc::new(RwLock::new(Some(context_rx))),
            loop_detector: components.loop_detector,
        }
    }

    // ========================================================================
    // Configuration Methods
    // ========================================================================

    /// Set the PtyManager for executing commands in user's terminal
    pub fn set_pty_manager(&mut self, pty_manager: Arc<PtyManager>) {
        self.pty_manager = Some(pty_manager);
    }

    /// Set the IndexerState for code analysis tools
    pub fn set_indexer_state(&mut self, indexer_state: Arc<IndexerState>) {
        self.indexer_state = Some(indexer_state);
    }

    /// Set the TavilyState for web search tools
    pub fn set_tavily_state(&mut self, tavily_state: Arc<TavilyState>) {
        self.tavily_state = Some(tavily_state);
    }

    /// Set the current session ID for terminal execution
    pub async fn set_session_id(&self, session_id: Option<String>) {
        *self.current_session_id.write().await = session_id;
    }

    /// Update the workspace/working directory.
    pub async fn set_workspace(&self, new_workspace: PathBuf) {
        let mut workspace = self.workspace.write().await;
        *workspace = new_workspace;
    }

    /// Get the workspace path.
    #[allow(dead_code)]
    pub async fn workspace(&self) -> PathBuf {
        self.workspace.read().await.clone()
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

    // ========================================================================
    // Main Execution Methods
    // ========================================================================

    /// Execute a prompt with agentic tool loop.
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
                drop(client);
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
                let vertex_model = vertex_model.clone();
                drop(client);

                self.execute_with_vertex_model(&vertex_model, prompt, start_time, context)
                    .await
            }
        }
    }

    /// Execute with Vertex AI model using the agentic loop.
    async fn execute_with_vertex_model(
        &self,
        model: &rig_anthropic_vertex::CompletionModel,
        initial_prompt: &str,
        start_time: std::time::Instant,
        context: SubAgentContext,
    ) -> Result<String> {
        // Build system prompt
        let workspace_path = self.workspace.read().await;
        let system_prompt = build_system_prompt(&workspace_path);
        drop(workspace_path);

        // Start session for persistence
        self.start_session().await;
        self.record_user_message(initial_prompt).await;

        // Prepare initial history with user message
        let mut history_guard = self.conversation_history.write().await;
        history_guard.push(Message::User {
            content: OneOrMany::one(UserContent::Text(Text {
                text: initial_prompt.to_string(),
            })),
        });
        let initial_history = history_guard.clone();
        drop(history_guard);

        // Build agentic loop context
        let loop_ctx = AgenticLoopContext {
            event_tx: &self.event_tx,
            tool_registry: &self.tool_registry,
            sub_agent_registry: &self.sub_agent_registry,
            pty_manager: self.pty_manager.as_ref(),
            current_session_id: &self.current_session_id,
            indexer_state: self.indexer_state.as_ref(),
            tavily_state: self.tavily_state.as_ref(),
            approval_recorder: &self.approval_recorder,
            pending_approvals: &self.pending_approvals,
            tool_policy_manager: &self.tool_policy_manager,
            context_manager: &self.context_manager,
            loop_detector: &self.loop_detector,
        };

        // Run the agentic loop
        let (accumulated_response, _final_history) =
            run_agentic_loop(model, &system_prompt, initial_history, context, &loop_ctx).await?;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Persist the assistant response
        if !accumulated_response.is_empty() {
            let mut history_guard = self.conversation_history.write().await;
            history_guard.push(Message::Assistant {
                id: None,
                content: OneOrMany::one(AssistantContent::Text(Text {
                    text: accumulated_response.clone(),
                })),
            });
        }

        // Record and save session
        if !accumulated_response.is_empty() {
            self.record_assistant_message(&accumulated_response).await;
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

    /// Execute a tool by name (public API).
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use super::tool_executors::execute_in_terminal;

        let normalized_args = if tool_name == "run_pty_cmd" {
            normalize_run_pty_cmd_args(args)
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

            return execute_in_terminal(
                self.pty_manager.as_ref(),
                &self.current_session_id,
                command,
            )
            .await;
        }

        let mut registry = self.tool_registry.write().await;
        let result = registry.execute_tool(tool_name, normalized_args).await;

        result.map_err(|e| anyhow::anyhow!(e))
    }

    /// Get available tools for the LLM.
    pub async fn available_tools(&self) -> Vec<serde_json::Value> {
        let registry = self.tool_registry.read().await;
        let tool_names = registry.available_tools().await;

        tool_names
            .into_iter()
            .map(|name| serde_json::json!({ "name": name }))
            .collect()
    }

    // ========================================================================
    // Sub-Agent Methods
    // ========================================================================

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
}

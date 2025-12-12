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
#![allow(dead_code)]
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
use rig::providers::openrouter as rig_openrouter;
use tokio::sync::{mpsc, oneshot, RwLock};
use vtcode_core::tools::ToolRegistry;

use super::agentic_loop::{run_agentic_loop, run_agentic_loop_generic, AgenticLoopContext};
use super::context_manager::ContextManager;
use super::events::AiEvent;
use super::hitl::{ApprovalDecision, ApprovalRecorder};
use super::llm_client::{
    create_vertex_components, create_vtcode_components, AgentBridgeComponents, LlmClient,
    VertexAnthropicClientConfig, VtcodeClientConfig,
};
use super::loop_detection::LoopDetector;
use super::session::QbitSessionManager;
use super::sub_agent::{SubAgentContext, SubAgentRegistry, MAX_AGENT_DEPTH};
use super::system_prompt::build_system_prompt;
use super::tool_definitions::ToolConfig;
use super::tool_policy::ToolPolicyManager;
use crate::indexer::IndexerState;
use crate::pty::PtyManager;
use crate::runtime::{QbitRuntime, RuntimeEvent};
use crate::sidecar::SidecarState;
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

    // Event emission - dual mode during transition
    // The event_tx channel is the legacy path, runtime is the new abstraction.
    // During transition, emit_event() sends through BOTH to verify parity.
    pub(crate) event_tx: Option<mpsc::UnboundedSender<AiEvent>>,
    pub(crate) runtime: Option<Arc<dyn QbitRuntime>>,

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
    #[cfg(feature = "tauri")]
    pub(crate) workflow_state: Option<Arc<super::commands::workflow::WorkflowState>>,

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

    // Loop detection
    pub(crate) loop_detector: Arc<RwLock<LoopDetector>>,

    // Tool configuration
    pub(crate) tool_config: ToolConfig,

    // Sidecar context capture
    pub(crate) sidecar_state: Option<Arc<SidecarState>>,
}

impl AgentBridge {
    // ========================================================================
    // Constructor Methods
    // ========================================================================

    /// Create a new AgentBridge with vtcode-core (for OpenRouter, OpenAI, etc.)
    ///
    /// This is the legacy constructor using event_tx channel.
    /// For new code, prefer `new_with_runtime()`.
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

        Ok(Self::from_components_with_event_tx(components, event_tx))
    }

    /// Create a new AgentBridge with runtime abstraction.
    ///
    /// This is the preferred constructor for CLI and future code.
    /// Uses the `QbitRuntime` trait for event emission and approval handling.
    pub async fn new_with_runtime(
        workspace: PathBuf,
        provider: &str,
        model: &str,
        api_key: &str,
        runtime: Arc<dyn QbitRuntime>,
    ) -> Result<Self> {
        let config = VtcodeClientConfig {
            workspace,
            provider,
            model,
            api_key,
        };

        let components = create_vtcode_components(config).await?;

        Ok(Self::from_components_with_runtime(components, runtime))
    }

    /// Create a new AgentBridge for Anthropic on Google Cloud Vertex AI.
    ///
    /// This is the legacy constructor using event_tx channel.
    /// For new code, prefer `new_vertex_anthropic_with_runtime()`.
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

        Ok(Self::from_components_with_event_tx(components, event_tx))
    }

    /// Create a new AgentBridge for Anthropic on Google Cloud Vertex AI with runtime.
    ///
    /// This is the preferred constructor for CLI and future code.
    pub async fn new_vertex_anthropic_with_runtime(
        workspace: PathBuf,
        credentials_path: &str,
        project_id: &str,
        location: &str,
        model: &str,
        runtime: Arc<dyn QbitRuntime>,
    ) -> Result<Self> {
        let config = VertexAnthropicClientConfig {
            workspace,
            credentials_path,
            project_id,
            location,
            model,
        };

        let components = create_vertex_components(config).await?;

        Ok(Self::from_components_with_runtime(components, runtime))
    }

    /// Create an AgentBridge from pre-built components (legacy event_tx path).
    fn from_components_with_event_tx(
        components: AgentBridgeComponents,
        event_tx: mpsc::UnboundedSender<AiEvent>,
    ) -> Self {
        let AgentBridgeComponents {
            workspace,
            provider_name,
            model_name,
            tool_registry,
            client,
            sub_agent_registry,
            approval_recorder,
            tool_policy_manager,
            context_manager,
            loop_detector,
        } = components;

        Self {
            workspace,
            provider_name,
            model_name,
            tool_registry,
            client,
            event_tx: Some(event_tx),
            runtime: None,
            sub_agent_registry,
            pty_manager: None,
            current_session_id: Default::default(),
            conversation_history: Default::default(),
            indexer_state: None,
            tavily_state: None,
            #[cfg(feature = "tauri")]
            workflow_state: None,
            session_manager: Default::default(),
            session_persistence_enabled: Arc::new(RwLock::new(true)),
            approval_recorder,
            pending_approvals: Default::default(),
            tool_policy_manager,
            context_manager,
            loop_detector,
            tool_config: ToolConfig::main_agent(),
            sidecar_state: None,
        }
    }

    /// Create an AgentBridge from pre-built components with runtime abstraction.
    fn from_components_with_runtime(
        components: AgentBridgeComponents,
        runtime: Arc<dyn QbitRuntime>,
    ) -> Self {
        let AgentBridgeComponents {
            workspace,
            provider_name,
            model_name,
            tool_registry,
            client,
            sub_agent_registry,
            approval_recorder,
            tool_policy_manager,
            context_manager,
            loop_detector,
        } = components;

        Self {
            workspace,
            provider_name,
            model_name,
            tool_registry,
            client,
            event_tx: None,
            runtime: Some(runtime),
            sub_agent_registry,
            pty_manager: None,
            current_session_id: Default::default(),
            conversation_history: Default::default(),
            indexer_state: None,
            tavily_state: None,
            #[cfg(feature = "tauri")]
            workflow_state: None,
            session_manager: Default::default(),
            session_persistence_enabled: Arc::new(RwLock::new(true)),
            approval_recorder,
            pending_approvals: Default::default(),
            tool_policy_manager,
            context_manager,
            loop_detector,
            tool_config: ToolConfig::main_agent(),
            sidecar_state: None,
        }
    }

    // ========================================================================
    // Event Emission Helpers
    // ========================================================================

    /// Helper to emit events through available channels.
    ///
    /// During the transition period, this emits through BOTH `event_tx` and `runtime`
    /// if both are available. This ensures no events are lost during migration.
    ///
    /// After migration is complete, only `runtime` will be used.
    pub(crate) fn emit_event(&self, event: AiEvent) {
        // Emit through legacy event_tx channel if available
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(event.clone());
        }

        // Emit through runtime abstraction if available
        if let Some(ref rt) = self.runtime {
            if let Err(e) = rt.emit(RuntimeEvent::Ai(Box::new(event))) {
                tracing::warn!("Failed to emit event through runtime: {}", e);
            }
        }
    }

    /// Get or create an event channel for the agentic loop.
    ///
    /// If `event_tx` is available, returns a clone of that sender.
    /// If only `runtime` is available, creates a forwarding channel that sends to runtime.
    ///
    /// This is a transition helper - once we update AgenticLoopContext to use runtime
    /// directly, this method will be removed.
    pub(crate) fn get_or_create_event_tx(&self) -> mpsc::UnboundedSender<AiEvent> {
        // If we have an event_tx, use it
        if let Some(ref tx) = self.event_tx {
            return tx.clone();
        }

        // Otherwise, create a forwarding channel to runtime
        let runtime = self.runtime.clone().expect(
            "AgentBridge must have either event_tx or runtime - this is a bug in construction",
        );

        let (tx, mut rx) = mpsc::unbounded_channel::<AiEvent>();

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                if let Err(e) = runtime.emit(RuntimeEvent::Ai(Box::new(event))) {
                    tracing::warn!("Failed to forward event to runtime: {}", e);
                }
            }
            tracing::debug!("Agentic loop event forwarder shut down");
        });

        tx
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

    /// Set the WorkflowState for workflow tools
    #[cfg(feature = "tauri")]
    pub fn set_workflow_state(
        &mut self,
        workflow_state: Arc<super::commands::workflow::WorkflowState>,
    ) {
        self.workflow_state = Some(workflow_state);
    }

    /// Set the SidecarState for context capture
    pub fn set_sidecar_state(&mut self, sidecar_state: Arc<SidecarState>) {
        self.sidecar_state = Some(sidecar_state);
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

    // ========================================================================
    // Main Execution Methods
    // ========================================================================

    /// Execute a prompt with agentic tool loop.
    pub async fn execute(&self, prompt: &str) -> Result<String> {
        self.execute_with_context(prompt, SubAgentContext::default())
            .await
    }

    // ========================================================================
    // Cancellation-Enabled Execution Methods (server feature only)
    // ========================================================================

    /// Execute a prompt with cancellation support.
    ///
    /// The cancellation token allows external cancellation of the execution,
    /// which is essential for HTTP server timeouts and client disconnections.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The user prompt to execute
    /// * `cancel_token` - Token that can be used to cancel the execution
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - The accumulated response from the agent
    /// * `Err` - If execution was cancelled or failed
    ///
    /// # Cancellation Behavior
    ///
    /// - If the token is already cancelled when called, returns early with an error
    /// - If cancelled during execution, emits an error event and returns an error
    /// - Child tokens created from this token will also be cancelled (for sub-agents)
    #[cfg(feature = "server")]
    pub async fn execute_with_cancellation(
        &self,
        prompt: &str,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Result<String> {
        self.execute_with_context_and_cancellation(prompt, SubAgentContext::default(), cancel_token)
            .await
    }

    /// Execute a prompt with context and cancellation support.
    ///
    /// This is the full-featured execution method that supports:
    /// - Sub-agent context for nested agent calls
    /// - Cancellation token for graceful shutdown
    ///
    /// # Arguments
    ///
    /// * `prompt` - The user prompt to execute
    /// * `context` - Sub-agent context with recursion depth tracking
    /// * `cancel_token` - Token that can be used to cancel the execution
    #[cfg(feature = "server")]
    pub async fn execute_with_context_and_cancellation(
        &self,
        prompt: &str,
        context: SubAgentContext,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Result<String> {
        // Check for early cancellation before any work
        if cancel_token.is_cancelled() {
            self.emit_event(AiEvent::Error {
                message: "Execution cancelled before start".to_string(),
                error_type: "cancelled".to_string(),
            });
            return Err(anyhow::anyhow!("Execution cancelled before start"));
        }

        // Wrap the main execution in a select! to handle cancellation during execution
        tokio::select! {
            biased;

            // Check cancellation first (biased towards cancellation)
            _ = cancel_token.cancelled() => {
                self.emit_event(AiEvent::Error {
                    message: "Execution cancelled".to_string(),
                    error_type: "cancelled".to_string(),
                });
                Err(anyhow::anyhow!("Execution cancelled"))
            }

            // Run the actual execution
            result = self.execute_with_context(prompt, context) => {
                result
            }
        }
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
        self.emit_event(AiEvent::Started {
            turn_id: turn_id.clone(),
        });

        let start_time = std::time::Instant::now();
        let client = self.client.read().await;

        match &*client {
            LlmClient::Vtcode(_) => {
                drop(client);
                self.execute_with_vtcode(prompt, start_time).await
            }
            LlmClient::VertexAnthropic(vertex_model) => {
                let vertex_model = vertex_model.clone();
                drop(client);

                self.execute_with_vertex_model(&vertex_model, prompt, start_time, context)
                    .await
            }
            LlmClient::RigOpenRouter(openrouter_model) => {
                let openrouter_model = openrouter_model.clone();
                drop(client);

                self.execute_with_openrouter_model(&openrouter_model, prompt, start_time, context)
                    .await
            }
        }
    }

    /// Execute with vtcode-core client.
    async fn execute_with_vtcode(
        &self,
        prompt: &str,
        start_time: std::time::Instant,
    ) -> Result<String> {
        let mut client = self.client.write().await;
        let LlmClient::Vtcode(vtcode_client) = &mut *client else {
            unreachable!("execute_with_vtcode called with non-vtcode client");
        };

        let result = vtcode_client
            .generate(prompt)
            .await
            .map(|r| r.content)
            .map_err(|e| anyhow::anyhow!("{}", e));

        match result {
            Ok(content) => {
                let duration_ms = start_time.elapsed().as_millis() as u64;
                self.emit_event(AiEvent::TextDelta {
                    delta: content.clone(),
                    accumulated: content.clone(),
                });
                self.emit_event(AiEvent::Completed {
                    response: content.clone(),
                    tokens_used: None,
                    duration_ms: Some(duration_ms),
                });
                Ok(content)
            }
            Err(e) => {
                self.emit_event(AiEvent::Error {
                    message: e.to_string(),
                    error_type: "llm_error".to_string(),
                });
                Err(e)
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
        let mut system_prompt = build_system_prompt(&workspace_path);
        drop(workspace_path);

        // Inject Layer 1 session context if available
        if let Some(session_context) = self.get_session_context().await {
            if !session_context.is_empty() {
                tracing::debug!(
                    "[agent] Injecting Layer 1 session context ({} chars)",
                    session_context.len()
                );
                system_prompt.push_str("\n\n");
                system_prompt.push_str(&session_context);
            }
        }

        // Start session for persistence
        self.start_session().await;
        self.record_user_message(initial_prompt).await;

        // Capture user prompt in sidecar session
        // Only start a new session if one doesn't already exist (sessions span conversations)
        if let Some(ref sidecar) = self.sidecar_state {
            use crate::sidecar::events::SessionEvent;

            let session_id = if let Some(existing_id) = sidecar.current_session_id() {
                // Reuse existing session
                tracing::debug!("Reusing existing sidecar session: {}", existing_id);
                Some(existing_id)
            } else {
                // Start a new session
                match sidecar.start_session(initial_prompt) {
                    Ok(new_id) => {
                        tracing::info!("Started new sidecar session: {}", new_id);
                        Some(new_id)
                    }
                    Err(e) => {
                        tracing::warn!("Failed to start sidecar session: {}", e);
                        None
                    }
                }
            };

            // Capture the user prompt as an event (if we have a session)
            if let Some(sid) = session_id {
                let prompt_event = SessionEvent::user_prompt(sid, initial_prompt);
                sidecar.capture(prompt_event);
            }
        }

        // Prepare initial history with user message
        let mut history_guard = self.conversation_history.write().await;
        history_guard.push(Message::User {
            content: OneOrMany::one(UserContent::Text(Text {
                text: initial_prompt.to_string(),
            })),
        });
        let initial_history = history_guard.clone();
        drop(history_guard);

        // Get or create event channel for the agentic loop
        // This handles both legacy (event_tx) and new (runtime) paths
        let loop_event_tx = self.get_or_create_event_tx();

        // Build agentic loop context
        let loop_ctx = AgenticLoopContext {
            event_tx: &loop_event_tx,
            tool_registry: &self.tool_registry,
            sub_agent_registry: &self.sub_agent_registry,
            indexer_state: self.indexer_state.as_ref(),
            tavily_state: self.tavily_state.as_ref(),
            #[cfg(feature = "tauri")]
            workflow_state: self.workflow_state.as_ref(),
            workspace: &self.workspace,
            client: &self.client,
            approval_recorder: &self.approval_recorder,
            pending_approvals: &self.pending_approvals,
            tool_policy_manager: &self.tool_policy_manager,
            context_manager: &self.context_manager,
            loop_detector: &self.loop_detector,
            tool_config: &self.tool_config,
            sidecar_state: self.sidecar_state.as_ref(),
            runtime: self.runtime.as_ref(),
            // No cancellation token for non-server execute paths
            // (cancellation is handled at the execute_with_cancellation level)
            #[cfg(feature = "server")]
            cancel_token: None,
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

        // Capture AI response in sidecar session
        if let Some(ref sidecar) = self.sidecar_state {
            use crate::sidecar::events::SessionEvent;

            if let Some(session_id) = sidecar.current_session_id() {
                if !accumulated_response.is_empty() {
                    let response_event =
                        SessionEvent::ai_response(session_id, &accumulated_response);
                    sidecar.capture(response_event);
                    tracing::debug!(
                        "[agent] Captured AI response in sidecar ({} chars)",
                        accumulated_response.len()
                    );
                }
            }
        }

        // Note: Sidecar session is NOT ended here - it persists across prompts in the
        // same conversation. The session is only ended when:
        // 1. The AgentBridge is dropped (see Drop impl)
        // 2. The conversation is explicitly cleared
        // 3. A new conversation/session is started

        // Emit completion event
        self.emit_event(AiEvent::Completed {
            response: accumulated_response.clone(),
            tokens_used: None,
            duration_ms: Some(duration_ms),
        });

        Ok(accumulated_response)
    }

    /// Execute with OpenRouter model using the generic agentic loop.
    async fn execute_with_openrouter_model(
        &self,
        model: &rig_openrouter::CompletionModel,
        initial_prompt: &str,
        start_time: std::time::Instant,
        context: SubAgentContext,
    ) -> Result<String> {
        // Build system prompt
        let workspace_path = self.workspace.read().await;
        let mut system_prompt = build_system_prompt(&workspace_path);
        drop(workspace_path);

        // Inject Layer 1 session context if available
        if let Some(session_context) = self.get_session_context().await {
            if !session_context.is_empty() {
                tracing::debug!(
                    "[agent] Injecting Layer 1 session context ({} chars)",
                    session_context.len()
                );
                system_prompt.push_str("\n\n");
                system_prompt.push_str(&session_context);
            }
        }

        // Start session for persistence
        self.start_session().await;
        self.record_user_message(initial_prompt).await;

        // Capture user prompt in sidecar session
        // Only start a new session if one doesn't already exist (sessions span conversations)
        if let Some(ref sidecar) = self.sidecar_state {
            use crate::sidecar::events::SessionEvent;

            let session_id = if let Some(existing_id) = sidecar.current_session_id() {
                // Reuse existing session
                tracing::debug!("Reusing existing sidecar session: {}", existing_id);
                Some(existing_id)
            } else {
                // Start a new session
                match sidecar.start_session(initial_prompt) {
                    Ok(new_id) => {
                        tracing::info!("Started new sidecar session: {}", new_id);
                        Some(new_id)
                    }
                    Err(e) => {
                        tracing::warn!("Failed to start sidecar session: {}", e);
                        None
                    }
                }
            };

            // Capture the user prompt as an event (if we have a session)
            if let Some(sid) = session_id {
                let prompt_event = SessionEvent::user_prompt(sid, initial_prompt);
                sidecar.capture(prompt_event);
            }
        }

        // Prepare initial history with user message
        let mut history_guard = self.conversation_history.write().await;
        history_guard.push(Message::User {
            content: OneOrMany::one(UserContent::Text(Text {
                text: initial_prompt.to_string(),
            })),
        });
        let initial_history = history_guard.clone();
        drop(history_guard);

        // Get or create event channel for the agentic loop
        // This handles both legacy (event_tx) and new (runtime) paths
        let loop_event_tx = self.get_or_create_event_tx();

        // Build agentic loop context
        let loop_ctx = AgenticLoopContext {
            event_tx: &loop_event_tx,
            tool_registry: &self.tool_registry,
            sub_agent_registry: &self.sub_agent_registry,
            indexer_state: self.indexer_state.as_ref(),
            tavily_state: self.tavily_state.as_ref(),
            #[cfg(feature = "tauri")]
            workflow_state: self.workflow_state.as_ref(),
            workspace: &self.workspace,
            client: &self.client,
            approval_recorder: &self.approval_recorder,
            pending_approvals: &self.pending_approvals,
            tool_policy_manager: &self.tool_policy_manager,
            context_manager: &self.context_manager,
            loop_detector: &self.loop_detector,
            tool_config: &self.tool_config,
            sidecar_state: self.sidecar_state.as_ref(),
            runtime: self.runtime.as_ref(),
            // No cancellation token for non-server execute paths
            #[cfg(feature = "server")]
            cancel_token: None,
        };

        // Run the generic agentic loop (works with any rig CompletionModel)
        let (accumulated_response, _final_history) =
            run_agentic_loop_generic(model, &system_prompt, initial_history, context, &loop_ctx)
                .await?;

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

        // End sidecar capture session
        if let Some(ref sidecar) = self.sidecar_state {
            match sidecar.end_session() {
                Ok(Some(session)) => {
                    tracing::info!("Sidecar session {} ended", session.session_id);
                }
                Ok(None) => {
                    tracing::debug!("No active sidecar session to end");
                }
                Err(e) => {
                    tracing::warn!("Failed to end sidecar session: {}", e);
                }
            }
        }

        // Emit completion event
        self.emit_event(AiEvent::Completed {
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
        let mut registry = self.tool_registry.write().await;
        let result = registry.execute_tool(tool_name, args).await;

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

    /// Get session context for injection into agent prompt
    pub async fn get_session_context(&self) -> Option<String> {
        let sidecar = self.sidecar_state.as_ref()?;

        // Use the simplified sidecar API to get injectable context (state.md content)
        match sidecar.get_injectable_context().await {
            Ok(context) => context,
            Err(e) => {
                tracing::warn!("Failed to get session context: {}", e);
                None
            }
        }
    }
}

// ============================================================================
// Drop Implementation for Session Cleanup
// ============================================================================

impl Drop for AgentBridge {
    fn drop(&mut self) {
        // Best-effort session finalization on drop.
        // This ensures sessions are saved even if the bridge is replaced without
        // explicit finalization (e.g., during model switching).
        //
        // We use try_write() because:
        // 1. Drop cannot be async, so we can't use .await
        // 2. If the lock is held, another operation is in progress and will handle cleanup
        // 3. At drop time, we should typically be the only owner
        if let Ok(mut guard) = self.session_manager.try_write() {
            if let Some(ref mut manager) = guard.take() {
                match manager.finalize() {
                    Ok(path) => {
                        tracing::debug!(
                            "AgentBridge::drop - session finalized: {}",
                            path.display()
                        );
                    }
                    Err(e) => {
                        tracing::warn!("AgentBridge::drop - failed to finalize session: {}", e);
                    }
                }
            }
        } else {
            tracing::debug!(
                "AgentBridge::drop - could not acquire session_manager lock, skipping finalization"
            );
        }

        // End sidecar session on bridge drop.
        // This ensures the sidecar session is properly finalized when:
        // - The conversation is cleared
        // - The AgentBridge is replaced (e.g., model switching)
        // - The application shuts down
        if let Some(ref sidecar) = self.sidecar_state {
            match sidecar.end_session() {
                Ok(Some(session)) => {
                    tracing::debug!(
                        "AgentBridge::drop - sidecar session {} ended",
                        session.session_id
                    );
                }
                Ok(None) => {
                    tracing::debug!("AgentBridge::drop - no active sidecar session to end");
                }
                Err(e) => {
                    tracing::warn!("AgentBridge::drop - failed to end sidecar session: {}", e);
                }
            }
        }
    }
}

// ============================================================================
// Tests for CancellationToken support
// ============================================================================

#[cfg(test)]
mod tests {
    // Note: CancellationToken is only available with the server feature
    #[cfg(feature = "server")]
    mod cancellation_tests {
        use tokio_util::sync::CancellationToken;

        /// Test CN-1: Child token cancels when parent cancels
        #[tokio::test]
        async fn child_token_cancels_when_parent_cancels() {
            let parent = CancellationToken::new();
            let child = parent.child_token();

            assert!(!parent.is_cancelled());
            assert!(!child.is_cancelled());

            // Cancel parent
            parent.cancel();

            // Both should be cancelled
            assert!(parent.is_cancelled());
            assert!(child.is_cancelled());
        }

        /// Test CN-2: Child cancel doesn't affect parent
        #[tokio::test]
        async fn child_cancel_does_not_affect_parent() {
            let parent = CancellationToken::new();
            let child = parent.child_token();

            assert!(!parent.is_cancelled());
            assert!(!child.is_cancelled());

            // Cancel child only
            child.cancel();

            // Only child should be cancelled
            assert!(!parent.is_cancelled());
            assert!(child.is_cancelled());
        }

        /// Test: Early cancellation check works
        #[tokio::test]
        async fn early_cancellation_returns_error() {
            let token = CancellationToken::new();

            // Cancel before calling check
            token.cancel();

            // The check should detect cancellation
            assert!(token.is_cancelled());

            // Simulate what execute_with_cancellation would do
            let result: Result<String, anyhow::Error> = if token.is_cancelled() {
                Err(anyhow::anyhow!("Execution cancelled before start"))
            } else {
                Ok("would execute".to_string())
            };

            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("cancelled before start"));
        }

        /// Test: Uncancelled token allows execution
        #[tokio::test]
        async fn uncancelled_token_allows_execution() {
            let token = CancellationToken::new();

            // Not cancelled
            assert!(!token.is_cancelled());

            // The check should allow execution
            let result: Result<String, anyhow::Error> = if token.is_cancelled() {
                Err(anyhow::anyhow!("Execution cancelled before start"))
            } else {
                Ok("executed".to_string())
            };

            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "executed");
        }

        /// Test: tokio::select! properly handles cancellation during async work
        #[tokio::test]
        async fn select_handles_cancellation_during_async_work() {
            use std::time::Duration;

            let token = CancellationToken::new();
            let token_clone = token.clone();

            // Spawn a task that will cancel the token after a short delay
            let cancel_task = tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                token_clone.cancel();
            });

            // Simulate long-running work that should be cancelled
            let result: Result<String, String> = tokio::select! {
                _ = async {
                    // Simulate work that takes longer than the cancel delay
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    Ok::<_, String>("completed".to_string())
                } => Ok("completed".to_string()),
                _ = token.cancelled() => {
                    Err("cancelled".to_string())
                }
            };

            // Wait for cancel task to complete
            cancel_task.await.unwrap();

            // Should have been cancelled
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), "cancelled");
        }

        /// Test: Multiple child tokens from same parent all cancel together
        #[tokio::test]
        async fn multiple_child_tokens_cancel_together() {
            let parent = CancellationToken::new();
            let child1 = parent.child_token();
            let child2 = parent.child_token();
            let grandchild = child1.child_token();

            assert!(!parent.is_cancelled());
            assert!(!child1.is_cancelled());
            assert!(!child2.is_cancelled());
            assert!(!grandchild.is_cancelled());

            // Cancel parent
            parent.cancel();

            // All descendants should be cancelled
            assert!(parent.is_cancelled());
            assert!(child1.is_cancelled());
            assert!(child2.is_cancelled());
            assert!(grandchild.is_cancelled());
        }
    }
}

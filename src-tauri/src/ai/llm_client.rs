//! LLM client abstraction for the agent system.
//!
//! This module provides a unified interface for interacting with different LLM providers:
//! - rig-core OpenRouter (preferred for OpenRouter models - supports tools and system prompts)
//! - vtcode-core (OpenAI, etc. - legacy, no tool support)
//! - Anthropic on Vertex AI

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use rig::client::CompletionClient;
use rig::providers::openrouter as rig_openrouter;
use tokio::sync::RwLock;
use vtcode_core::llm::{make_client, AnyClient};
use vtcode_core::tools::ToolRegistry;

use super::context_manager::ContextManager;
use super::hitl::ApprovalRecorder;
use super::loop_detection::LoopDetector;
use super::sub_agent::{create_default_sub_agents, SubAgentRegistry};
use super::tool_policy::ToolPolicyManager;

/// LLM client abstraction that supports both vtcode and rig-based providers
pub enum LlmClient {
    /// vtcode-core client (OpenAI, etc.) - legacy, no tool/system prompt support
    Vtcode(AnyClient),
    /// Anthropic on Vertex AI via rig-anthropic-vertex
    VertexAnthropic(rig_anthropic_vertex::CompletionModel),
    /// OpenRouter via rig-core (supports tools and system prompts)
    RigOpenRouter(rig_openrouter::CompletionModel),
}

/// Configuration for creating an AgentBridge with vtcode-core
pub struct VtcodeClientConfig<'a> {
    pub workspace: PathBuf,
    pub provider: &'a str,
    pub model: &'a str,
    pub api_key: &'a str,
}

/// Configuration for creating an AgentBridge with Vertex AI Anthropic
pub struct VertexAnthropicClientConfig<'a> {
    pub workspace: PathBuf,
    pub credentials_path: &'a str,
    pub project_id: &'a str,
    pub location: &'a str,
    pub model: &'a str,
}

/// Common initialization result containing shared components
pub struct AgentBridgeComponents {
    pub workspace: Arc<RwLock<PathBuf>>,
    pub provider_name: String,
    pub model_name: String,
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
    pub client: Arc<RwLock<LlmClient>>,
    pub sub_agent_registry: Arc<RwLock<SubAgentRegistry>>,
    pub approval_recorder: Arc<ApprovalRecorder>,
    pub tool_policy_manager: Arc<ToolPolicyManager>,
    pub context_manager: Arc<ContextManager>,
    pub loop_detector: Arc<RwLock<LoopDetector>>,
}

/// Shared components that are common to all LLM providers.
struct SharedComponents {
    tool_registry: Arc<RwLock<ToolRegistry>>,
    sub_agent_registry: Arc<RwLock<SubAgentRegistry>>,
    approval_recorder: Arc<ApprovalRecorder>,
    tool_policy_manager: Arc<ToolPolicyManager>,
    context_manager: Arc<ContextManager>,
    loop_detector: Arc<RwLock<LoopDetector>>,
}

/// Initialize shared components from a workspace path and model name.
async fn create_shared_components(workspace: &Path, model: &str) -> SharedComponents {
    // Create and populate the sub-agent registry
    let mut sub_agent_registry = SubAgentRegistry::new();
    sub_agent_registry.register_multiple(create_default_sub_agents());

    SharedComponents {
        tool_registry: Arc::new(RwLock::new(
            ToolRegistry::new(workspace.to_path_buf()).await,
        )),
        sub_agent_registry: Arc::new(RwLock::new(sub_agent_registry)),
        approval_recorder: Arc::new(
            ApprovalRecorder::new(workspace.join(".qbit").join("hitl")).await,
        ),
        tool_policy_manager: Arc::new(ToolPolicyManager::new(workspace).await),
        context_manager: Arc::new(ContextManager::for_model(model)),
        loop_detector: Arc::new(RwLock::new(LoopDetector::with_defaults())),
    }
}

/// Create components for a vtcode-core based client.
pub async fn create_vtcode_components(
    config: VtcodeClientConfig<'_>,
) -> Result<AgentBridgeComponents> {
    // For OpenRouter, use rig's native OpenRouter provider which supports:
    // - Tool calling (function calling)
    // - System prompts
    // - Streaming with the agentic loop
    // This replaces vtcode-core's OpenRouterProvider which only supports simple text generation.
    let client = if config.provider == "openrouter" {
        let openrouter_client = rig_openrouter::Client::new(config.api_key);
        let completion_model = openrouter_client.completion_model(config.model);
        LlmClient::RigOpenRouter(completion_model)
    } else {
        // For other providers, use the standard vtcode-core ModelId parsing path
        let model_id = vtcode_core::config::models::ModelId::from_str(config.model)
            .map_err(|e| anyhow::anyhow!("Invalid model ID '{}': {}", config.model, e))?;
        let vtcode_client = make_client(config.api_key.to_string(), model_id);
        LlmClient::Vtcode(vtcode_client)
    };

    let client = Arc::new(RwLock::new(client));

    let shared = create_shared_components(&config.workspace, config.model).await;

    Ok(AgentBridgeComponents {
        workspace: Arc::new(RwLock::new(config.workspace)),
        provider_name: config.provider.to_string(),
        model_name: config.model.to_string(),
        tool_registry: shared.tool_registry,
        client,
        sub_agent_registry: shared.sub_agent_registry,
        approval_recorder: shared.approval_recorder,
        tool_policy_manager: shared.tool_policy_manager,
        context_manager: shared.context_manager,
        loop_detector: shared.loop_detector,
    })
}

/// Create components for a Vertex AI Anthropic based client.
pub async fn create_vertex_components(
    config: VertexAnthropicClientConfig<'_>,
) -> Result<AgentBridgeComponents> {
    let vertex_client = rig_anthropic_vertex::Client::from_service_account(
        config.credentials_path,
        config.project_id,
        config.location,
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to create Vertex AI client: {}", e))?;

    // Extended thinking disabled for now due to sub-agent executor compatibility
    // TODO: Re-enable with proper thinking block handling in sub_agent_executor
    // For now, sub-agents are focused task executors that don't need extended thinking
    let completion_model = vertex_client.completion_model(config.model);

    let shared = create_shared_components(&config.workspace, config.model).await;

    Ok(AgentBridgeComponents {
        workspace: Arc::new(RwLock::new(config.workspace)),
        provider_name: "anthropic_vertex".to_string(),
        model_name: config.model.to_string(),
        tool_registry: shared.tool_registry,
        client: Arc::new(RwLock::new(LlmClient::VertexAnthropic(completion_model))),
        sub_agent_registry: shared.sub_agent_registry,
        approval_recorder: shared.approval_recorder,
        tool_policy_manager: shared.tool_policy_manager,
        context_manager: shared.context_manager,
        loop_detector: shared.loop_detector,
    })
}

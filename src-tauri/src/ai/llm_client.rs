//! LLM client abstraction for the agent system.
//!
//! This module provides a unified interface for interacting with different LLM providers:
//! - vtcode-core (OpenRouter, OpenAI, etc.)
//! - Anthropic on Vertex AI

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
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
    /// vtcode-core client (OpenRouter, OpenAI, etc.)
    Vtcode(AnyClient),
    /// Anthropic on Vertex AI via rig-anthropic-vertex
    VertexAnthropic(rig_anthropic_vertex::CompletionModel),
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

/// Create components for a vtcode-core based client.
pub async fn create_vtcode_components(
    config: VtcodeClientConfig<'_>,
) -> Result<AgentBridgeComponents> {
    // Create the model ID using FromStr trait
    let model_id = vtcode_core::config::models::ModelId::from_str(config.model)
        .map_err(|e| anyhow::anyhow!("Invalid model ID '{}': {}", config.model, e))?;

    // Create LLM client
    let client = Arc::new(RwLock::new(LlmClient::Vtcode(make_client(
        config.api_key.to_string(),
        model_id,
    ))));

    // Create tool registry (async)
    let tool_registry = Arc::new(RwLock::new(
        ToolRegistry::new(config.workspace.clone()).await,
    ));

    // Create sub-agent registry with defaults
    let mut sub_agent_registry = SubAgentRegistry::new();
    for agent in create_default_sub_agents() {
        sub_agent_registry.register(agent);
    }

    // Create HITL approval recorder (stores in workspace/.qbit/hitl/)
    let hitl_storage = config.workspace.join(".qbit").join("hitl");
    let approval_recorder = Arc::new(ApprovalRecorder::new(hitl_storage).await);

    // Create tool policy manager (loads from workspace/.qbit/tool-policy.json)
    let tool_policy_manager = Arc::new(ToolPolicyManager::new(&config.workspace).await);

    // Create context manager for token budgeting
    let context_manager = ContextManager::for_model(config.model);

    Ok(AgentBridgeComponents {
        workspace: Arc::new(RwLock::new(config.workspace)),
        provider_name: config.provider.to_string(),
        model_name: config.model.to_string(),
        tool_registry,
        client,
        sub_agent_registry: Arc::new(RwLock::new(sub_agent_registry)),
        approval_recorder,
        tool_policy_manager,
        context_manager: Arc::new(context_manager),
        loop_detector: Arc::new(RwLock::new(LoopDetector::with_defaults())),
    })
}

/// Create components for a Vertex AI Anthropic based client.
pub async fn create_vertex_components(
    config: VertexAnthropicClientConfig<'_>,
) -> Result<AgentBridgeComponents> {
    // Create Vertex AI client
    let vertex_client = rig_anthropic_vertex::Client::from_service_account(
        config.credentials_path,
        config.project_id,
        config.location,
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to create Vertex AI client: {}", e))?;

    // Create completion model
    let completion_model = vertex_client.completion_model(config.model);

    // Create tool registry (async)
    let tool_registry = Arc::new(RwLock::new(
        ToolRegistry::new(config.workspace.clone()).await,
    ));

    // Create sub-agent registry with defaults
    let mut sub_agent_registry = SubAgentRegistry::new();
    for agent in create_default_sub_agents() {
        sub_agent_registry.register(agent);
    }

    // Create HITL approval recorder (stores in workspace/.qbit/hitl/)
    let hitl_storage = config.workspace.join(".qbit").join("hitl");
    let approval_recorder = Arc::new(ApprovalRecorder::new(hitl_storage).await);

    // Create tool policy manager (loads from workspace/.qbit/tool-policy.json)
    let tool_policy_manager = Arc::new(ToolPolicyManager::new(&config.workspace).await);

    // Create context manager for token budgeting
    let context_manager = ContextManager::for_model(config.model);

    Ok(AgentBridgeComponents {
        workspace: Arc::new(RwLock::new(config.workspace)),
        provider_name: "anthropic_vertex".to_string(),
        model_name: config.model.to_string(),
        tool_registry,
        client: Arc::new(RwLock::new(LlmClient::VertexAnthropic(completion_model))),
        sub_agent_registry: Arc::new(RwLock::new(sub_agent_registry)),
        approval_recorder,
        tool_policy_manager,
        context_manager: Arc::new(context_manager),
        loop_detector: Arc::new(RwLock::new(LoopDetector::with_defaults())),
    })
}

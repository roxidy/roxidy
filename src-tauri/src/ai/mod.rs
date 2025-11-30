pub mod agent_bridge;
pub mod agentic_loop;
mod bridge_context;
mod bridge_hitl;
mod bridge_policy;
mod bridge_session;
pub mod commands;
pub mod context_manager;
pub mod context_pruner;
pub mod events;
pub mod hitl;
pub mod llm_client;
pub mod loop_detection;
pub mod session;
pub mod sub_agent;
pub mod sub_agent_executor;
pub mod system_prompt;
pub mod token_budget;
pub mod token_trunc;
pub mod tool_definitions;
pub mod tool_executors;
pub mod tool_policy;
pub mod workflow;

pub use commands::{
    add_tool_always_allow, clear_ai_conversation, disable_full_auto_mode, disable_loop_detection,
    enable_full_auto_mode, enable_loop_detection, enforce_context_window, execute_ai_tool,
    export_ai_session_transcript, finalize_ai_session, find_ai_session, get_ai_conversation_length,
    get_approval_patterns, get_available_tools, get_context_summary, get_context_trim_config,
    get_context_utilization, get_hitl_config, get_loop_detector_stats, get_loop_protection_config,
    get_openrouter_api_key, get_remaining_tokens, get_token_alert_level, get_token_usage_stats,
    get_tool_approval_pattern, get_tool_policy, get_tool_policy_config, get_vertex_ai_config,
    init_ai_agent, init_ai_agent_vertex, is_ai_initialized, is_ai_session_persistence_enabled,
    is_context_management_enabled, is_full_auto_mode_enabled, is_loop_detection_enabled,
    list_ai_sessions, load_ai_session, load_env_file, remove_tool_always_allow,
    reset_approval_patterns, reset_context_manager, reset_loop_detector, reset_tool_policies,
    respond_to_tool_approval, restore_ai_session, send_ai_prompt, set_ai_session_persistence,
    set_hitl_config, set_loop_protection_config, set_tool_policy, set_tool_policy_config,
    shutdown_ai_agent, update_ai_workspace, AiState,
};
// Re-export HITL types for external use
#[allow(unused_imports)]
pub use hitl::{
    ApprovalDecision, ApprovalPattern, ApprovalRecorder, ApprovalRequest, RiskLevel,
    ToolApprovalConfig,
};
// Re-export tool policy types for external use
#[allow(unused_imports)]
pub use tool_policy::{
    PolicyConstraintResult, ToolConstraints, ToolPolicy, ToolPolicyConfig, ToolPolicyManager,
};
// Re-export context management types for external use
#[allow(unused_imports)]
pub use context_manager::{ContextEvent, ContextManager, ContextSummary, ContextTrimConfig};
#[allow(unused_imports)]
pub use context_pruner::{ContextPruner, ContextPrunerConfig, PruneResult, SemanticScore};
#[allow(unused_imports)]
pub use token_budget::{
    TokenAlertLevel, TokenBudgetConfig, TokenBudgetManager, TokenUsageStats,
    DEFAULT_MAX_CONTEXT_TOKENS, MAX_TOOL_RESPONSE_TOKENS,
};
#[allow(unused_imports)]
pub use token_trunc::{
    aggregate_tool_output, truncate_by_chars, truncate_by_tokens, ContentType, TruncationResult,
};
// Re-export loop detection types for external use
#[allow(unused_imports)]
pub use loop_detection::{
    LoopDetectionResult, LoopDetector, LoopDetectorStats, LoopProtectionConfig,
};
// Re-export session types for external use
#[allow(unused_imports)]
pub use session::{QbitMessageRole, QbitSessionMessage, QbitSessionSnapshot, SessionListingInfo};
// Re-exports for sub_agent and workflow modules - currently unused but kept for future use
#[allow(unused_imports)]
pub use sub_agent::{SubAgentContext, SubAgentDefinition, SubAgentRegistry, SubAgentResult};
#[allow(unused_imports)]
pub use workflow::{
    AgentWorkflowBuilder, RouterTask, SubAgentExecutor, SubAgentTask, WorkflowRunner,
    WorkflowStatus, WorkflowStepResult, WorkflowStorage,
};
// Re-export tool configuration types for external use
#[allow(unused_imports)]
pub use tool_definitions::{
    ToolConfig, ToolPreset, get_all_tool_definitions_with_config, get_tool_definitions_for_preset,
    get_tool_definitions_with_config,
};

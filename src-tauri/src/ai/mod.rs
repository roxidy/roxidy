pub mod agent_bridge;
pub mod commands;
pub mod events;
pub mod hitl;
pub mod session;
pub mod sub_agent;
pub mod tool_policy;
pub mod workflow;

pub use commands::{
    add_tool_always_allow, clear_ai_conversation, disable_full_auto_mode, enable_full_auto_mode,
    execute_ai_tool, export_ai_session_transcript, finalize_ai_session, find_ai_session,
    get_ai_conversation_length, get_approval_patterns, get_available_tools, get_hitl_config,
    get_openrouter_api_key, get_tool_approval_pattern, get_tool_policy, get_tool_policy_config,
    get_vertex_ai_config, init_ai_agent, init_ai_agent_vertex, is_ai_initialized,
    is_ai_session_persistence_enabled, is_full_auto_mode_enabled, list_ai_sessions, load_ai_session,
    load_env_file, remove_tool_always_allow, reset_approval_patterns, reset_tool_policies,
    respond_to_tool_approval, restore_ai_session, send_ai_prompt, set_ai_session_persistence,
    set_hitl_config, set_tool_policy, set_tool_policy_config, shutdown_ai_agent, update_ai_workspace,
    AiState,
};
// Re-export HITL types for external use
pub use hitl::{
    ApprovalDecision, ApprovalPattern, ApprovalRecorder, ApprovalRequest, RiskLevel,
    ToolApprovalConfig,
};
// Re-export tool policy types for external use
pub use tool_policy::{
    PolicyConstraintResult, ToolConstraints, ToolPolicy, ToolPolicyConfig, ToolPolicyManager,
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

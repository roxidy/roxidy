use serde::{Deserialize, Serialize};

/// Simplified AI events for the frontend.
/// We emit these directly from AgentBridge instead of converting from vtcode's ThreadEvent,
/// since ThreadEvent uses tuple structs that are harder to work with.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AiEvent {
    /// Agent started processing a turn
    Started { turn_id: String },

    /// Streaming text chunk from the LLM
    TextDelta { delta: String, accumulated: String },

    /// Tool execution requested (for approval UI / HITL)
    ToolRequest {
        tool_name: String,
        args: serde_json::Value,
        request_id: String,
    },

    /// Tool execution completed
    ToolResult {
        tool_name: String,
        result: serde_json::Value,
        success: bool,
        request_id: String,
    },

    /// Agent reasoning/thinking (for models that support extended thinking)
    Reasoning { content: String },

    /// Turn completed successfully
    Completed {
        response: String,
        tokens_used: Option<u32>,
        duration_ms: Option<u64>,
    },

    /// Error occurred during processing
    Error { message: String, error_type: String },

    // Sub-agent events

    /// Sub-agent started executing a task
    SubAgentStarted {
        agent_id: String,
        agent_name: String,
        task: String,
        depth: usize,
    },

    /// Sub-agent tool request (for visibility into sub-agent's tool usage)
    SubAgentToolRequest {
        agent_id: String,
        tool_name: String,
        args: serde_json::Value,
    },

    /// Sub-agent tool result
    SubAgentToolResult {
        agent_id: String,
        tool_name: String,
        success: bool,
    },

    /// Sub-agent completed its task
    SubAgentCompleted {
        agent_id: String,
        response: String,
        duration_ms: u64,
    },

    /// Sub-agent encountered an error
    SubAgentError { agent_id: String, error: String },
}

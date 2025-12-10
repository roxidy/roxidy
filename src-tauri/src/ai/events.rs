use serde::{Deserialize, Serialize};

use super::hitl::{ApprovalPattern, RiskLevel};

/// Source of a tool call - indicates where the tool request originated.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolSource {
    /// Tool called by the main agent
    #[default]
    Main,
    /// Tool called by a sub-agent
    SubAgent {
        agent_id: String,
        agent_name: String,
    },
    /// Tool called by a workflow
    Workflow {
        workflow_id: String,
        workflow_name: String,
        /// Current step name (if within a step)
        #[serde(skip_serializing_if = "Option::is_none")]
        step_name: Option<String>,
        /// Current step index (0-based)
        #[serde(skip_serializing_if = "Option::is_none")]
        step_index: Option<usize>,
    },
}

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
    /// This is the legacy event - kept for backward compatibility
    ToolRequest {
        tool_name: String,
        args: serde_json::Value,
        request_id: String,
        /// Source of this tool call (main agent, sub-agent, or workflow)
        #[serde(default)]
        source: ToolSource,
    },

    /// Tool approval request with HITL metadata
    /// The frontend should show an approval dialog and respond with ToolApprovalResponse
    ToolApprovalRequest {
        request_id: String,
        tool_name: String,
        args: serde_json::Value,
        /// Current approval stats for this tool (if any)
        stats: Option<ApprovalPattern>,
        /// Risk level of this operation
        risk_level: RiskLevel,
        /// Whether this tool can be auto-approved in the future
        can_learn: bool,
        /// Suggestion message (e.g., "2 more approvals needed for auto-approve")
        suggestion: Option<String>,
        /// Source of this tool call (main agent, sub-agent, or workflow)
        #[serde(default)]
        source: ToolSource,
    },

    /// Tool was auto-approved based on learned patterns
    ToolAutoApproved {
        request_id: String,
        tool_name: String,
        args: serde_json::Value,
        /// Reason for auto-approval
        reason: String,
        /// Source of this tool call (main agent, sub-agent, or workflow)
        #[serde(default)]
        source: ToolSource,
    },

    /// Tool was denied by policy or constraint
    ToolDenied {
        request_id: String,
        tool_name: String,
        args: serde_json::Value,
        /// Reason for denial
        reason: String,
        /// Source of this tool call (main agent, sub-agent, or workflow)
        #[serde(default)]
        source: ToolSource,
    },

    /// Tool execution completed
    ToolResult {
        tool_name: String,
        result: serde_json::Value,
        success: bool,
        request_id: String,
        /// Source of this tool call (main agent, sub-agent, or workflow)
        #[serde(default)]
        source: ToolSource,
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

    // Context management events
    /// Context was pruned due to token limits
    ContextPruned {
        messages_removed: usize,
        utilization_before: f64,
        utilization_after: f64,
    },

    /// Context warning threshold exceeded
    ContextWarning {
        utilization: f64,
        total_tokens: usize,
        max_tokens: usize,
    },

    /// Tool response was truncated due to size limits
    ToolResponseTruncated {
        tool_name: String,
        original_tokens: usize,
        truncated_tokens: usize,
    },

    // Loop protection events
    /// Warning: approaching loop detection threshold
    LoopWarning {
        tool_name: String,
        current_count: usize,
        max_count: usize,
        message: String,
    },

    /// Tool call blocked due to loop detection
    LoopBlocked {
        tool_name: String,
        repeat_count: usize,
        max_count: usize,
        message: String,
    },

    /// Maximum tool iterations reached for this turn
    MaxIterationsReached {
        iterations: usize,
        max_iterations: usize,
        message: String,
    },

    // Workflow events
    /// Workflow started
    WorkflowStarted {
        workflow_id: String,
        workflow_name: String,
        session_id: String,
    },

    /// Workflow step started
    WorkflowStepStarted {
        workflow_id: String,
        step_name: String,
        step_index: usize,
        total_steps: usize,
    },

    /// Workflow step completed
    WorkflowStepCompleted {
        workflow_id: String,
        step_name: String,
        output: Option<String>,
        duration_ms: u64,
    },

    /// Workflow completed
    WorkflowCompleted {
        workflow_id: String,
        final_output: String,
        total_duration_ms: u64,
    },

    /// Workflow error
    WorkflowError {
        workflow_id: String,
        step_name: Option<String>,
        error: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Baseline snapshot tests for AiEvent JSON serialization.
    ///
    /// These tests capture the exact JSON format that the frontend expects.
    /// They MUST pass before AND after any migration (e.g., HTTP/SSE server).
    ///
    /// If a test fails after a change, it means the frontend contract has been broken
    /// and the frontend code will need to be updated as well.
    mod json_serialization {
        use super::*;

        #[test]
        fn started_event_json_format() {
            let event = AiEvent::Started {
                turn_id: "turn-123".to_string(),
            };
            let json = serde_json::to_value(&event).unwrap();

            // Verify exact structure
            assert_eq!(json["type"], "started");
            assert_eq!(json["turn_id"], "turn-123");

            // Snapshot: exact JSON representation
            let expected = json!({
                "type": "started",
                "turn_id": "turn-123"
            });
            assert_eq!(json, expected);
        }

        #[test]
        fn text_delta_event_json_format() {
            let event = AiEvent::TextDelta {
                delta: "Hello".to_string(),
                accumulated: "Hello world".to_string(),
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "text_delta");
            assert_eq!(json["delta"], "Hello");
            assert_eq!(json["accumulated"], "Hello world");

            let expected = json!({
                "type": "text_delta",
                "delta": "Hello",
                "accumulated": "Hello world"
            });
            assert_eq!(json, expected);
        }

        #[test]
        fn tool_request_event_json_format() {
            let event = AiEvent::ToolRequest {
                tool_name: "read_file".to_string(),
                args: json!({"path": "/src/main.rs"}),
                request_id: "req-456".to_string(),
                source: ToolSource::Main,
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "tool_request");
            assert_eq!(json["tool_name"], "read_file");
            assert_eq!(json["args"]["path"], "/src/main.rs");
            assert_eq!(json["request_id"], "req-456");
            assert_eq!(json["source"]["type"], "main");
        }

        #[test]
        fn tool_approval_request_event_json_format() {
            use chrono::{DateTime, Utc};

            let event = AiEvent::ToolApprovalRequest {
                request_id: "req-789".to_string(),
                tool_name: "write_file".to_string(),
                args: json!({"path": "/src/lib.rs", "content": "// code"}),
                stats: Some(ApprovalPattern {
                    tool_name: "write_file".to_string(),
                    total_requests: 5,
                    approvals: 4,
                    denials: 1,
                    always_allow: false,
                    last_updated: DateTime::<Utc>::from_timestamp(1700000000, 0).unwrap(),
                    justifications: vec!["User approved".to_string()],
                }),
                risk_level: RiskLevel::Medium,
                can_learn: true,
                suggestion: Some("1 more approval for auto-approve".to_string()),
                source: ToolSource::Main,
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "tool_approval_request");
            assert_eq!(json["request_id"], "req-789");
            assert_eq!(json["tool_name"], "write_file");
            assert_eq!(json["risk_level"], "medium");
            assert_eq!(json["can_learn"], true);
            assert_eq!(json["stats"]["total_requests"], 5);
            assert_eq!(json["stats"]["approvals"], 4);
        }

        #[test]
        fn tool_auto_approved_event_json_format() {
            let event = AiEvent::ToolAutoApproved {
                request_id: "req-auto-1".to_string(),
                tool_name: "read_file".to_string(),
                args: json!({"path": "/readme.md"}),
                reason: "Always allowed by user".to_string(),
                source: ToolSource::Main,
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "tool_auto_approved");
            assert_eq!(json["request_id"], "req-auto-1");
            assert_eq!(json["tool_name"], "read_file");
            assert_eq!(json["reason"], "Always allowed by user");
        }

        #[test]
        fn tool_denied_event_json_format() {
            let event = AiEvent::ToolDenied {
                request_id: "req-denied-1".to_string(),
                tool_name: "shell_exec".to_string(),
                args: json!({"command": "rm -rf /"}),
                reason: "Dangerous command blocked".to_string(),
                source: ToolSource::Main,
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "tool_denied");
            assert_eq!(json["request_id"], "req-denied-1");
            assert_eq!(json["tool_name"], "shell_exec");
            assert_eq!(json["reason"], "Dangerous command blocked");
        }

        #[test]
        fn tool_result_event_json_format() {
            let event = AiEvent::ToolResult {
                tool_name: "read_file".to_string(),
                result: json!({"content": "file contents here"}),
                success: true,
                request_id: "req-result-1".to_string(),
                source: ToolSource::Main,
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "tool_result");
            assert_eq!(json["tool_name"], "read_file");
            assert_eq!(json["success"], true);
            assert_eq!(json["request_id"], "req-result-1");
            assert_eq!(json["result"]["content"], "file contents here");
        }

        #[test]
        fn reasoning_event_json_format() {
            let event = AiEvent::Reasoning {
                content: "Let me think about this...".to_string(),
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "reasoning");
            assert_eq!(json["content"], "Let me think about this...");

            let expected = json!({
                "type": "reasoning",
                "content": "Let me think about this..."
            });
            assert_eq!(json, expected);
        }

        #[test]
        fn completed_event_json_format() {
            let event = AiEvent::Completed {
                response: "Task completed successfully.".to_string(),
                tokens_used: Some(1500),
                duration_ms: Some(2500),
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "completed");
            assert_eq!(json["response"], "Task completed successfully.");
            assert_eq!(json["tokens_used"], 1500);
            assert_eq!(json["duration_ms"], 2500);
        }

        #[test]
        fn completed_event_with_null_fields() {
            let event = AiEvent::Completed {
                response: "Done".to_string(),
                tokens_used: None,
                duration_ms: None,
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "completed");
            assert_eq!(json["response"], "Done");
            assert!(json["tokens_used"].is_null());
            assert!(json["duration_ms"].is_null());
        }

        #[test]
        fn error_event_json_format() {
            let event = AiEvent::Error {
                message: "Connection timeout".to_string(),
                error_type: "network".to_string(),
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "error");
            assert_eq!(json["message"], "Connection timeout");
            assert_eq!(json["error_type"], "network");

            let expected = json!({
                "type": "error",
                "message": "Connection timeout",
                "error_type": "network"
            });
            assert_eq!(json, expected);
        }

        #[test]
        fn sub_agent_started_event_json_format() {
            let event = AiEvent::SubAgentStarted {
                agent_id: "agent-001".to_string(),
                agent_name: "code_analyzer".to_string(),
                task: "Analyze the codebase structure".to_string(),
                depth: 1,
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "sub_agent_started");
            assert_eq!(json["agent_id"], "agent-001");
            assert_eq!(json["agent_name"], "code_analyzer");
            assert_eq!(json["task"], "Analyze the codebase structure");
            assert_eq!(json["depth"], 1);
        }

        #[test]
        fn sub_agent_completed_event_json_format() {
            let event = AiEvent::SubAgentCompleted {
                agent_id: "agent-001".to_string(),
                response: "Analysis complete".to_string(),
                duration_ms: 5000,
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "sub_agent_completed");
            assert_eq!(json["agent_id"], "agent-001");
            assert_eq!(json["response"], "Analysis complete");
            assert_eq!(json["duration_ms"], 5000);
        }

        #[test]
        fn context_pruned_event_json_format() {
            let event = AiEvent::ContextPruned {
                messages_removed: 5,
                utilization_before: 0.95,
                utilization_after: 0.75,
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "context_pruned");
            assert_eq!(json["messages_removed"], 5);
            assert_eq!(json["utilization_before"], 0.95);
            assert_eq!(json["utilization_after"], 0.75);
        }

        #[test]
        fn context_warning_event_json_format() {
            let event = AiEvent::ContextWarning {
                utilization: 0.85,
                total_tokens: 170000,
                max_tokens: 200000,
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "context_warning");
            assert_eq!(json["utilization"], 0.85);
            assert_eq!(json["total_tokens"], 170000);
            assert_eq!(json["max_tokens"], 200000);
        }

        #[test]
        fn loop_warning_event_json_format() {
            let event = AiEvent::LoopWarning {
                tool_name: "list_files".to_string(),
                current_count: 8,
                max_count: 10,
                message: "Approaching loop limit".to_string(),
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "loop_warning");
            assert_eq!(json["tool_name"], "list_files");
            assert_eq!(json["current_count"], 8);
            assert_eq!(json["max_count"], 10);
        }

        #[test]
        fn loop_blocked_event_json_format() {
            let event = AiEvent::LoopBlocked {
                tool_name: "list_files".to_string(),
                repeat_count: 10,
                max_count: 10,
                message: "Loop detected, blocking further calls".to_string(),
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "loop_blocked");
            assert_eq!(json["tool_name"], "list_files");
            assert_eq!(json["repeat_count"], 10);
            assert_eq!(json["max_count"], 10);
        }

        #[test]
        fn max_iterations_reached_event_json_format() {
            let event = AiEvent::MaxIterationsReached {
                iterations: 50,
                max_iterations: 50,
                message: "Maximum tool iterations reached".to_string(),
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "max_iterations_reached");
            assert_eq!(json["iterations"], 50);
            assert_eq!(json["max_iterations"], 50);
        }

        #[test]
        fn workflow_started_event_json_format() {
            let event = AiEvent::WorkflowStarted {
                workflow_id: "wf-001".to_string(),
                workflow_name: "git_commit".to_string(),
                session_id: "session-123".to_string(),
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "workflow_started");
            assert_eq!(json["workflow_id"], "wf-001");
            assert_eq!(json["workflow_name"], "git_commit");
            assert_eq!(json["session_id"], "session-123");
        }

        #[test]
        fn workflow_step_started_event_json_format() {
            let event = AiEvent::WorkflowStepStarted {
                workflow_id: "wf-001".to_string(),
                step_name: "analyze".to_string(),
                step_index: 0,
                total_steps: 4,
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "workflow_step_started");
            assert_eq!(json["workflow_id"], "wf-001");
            assert_eq!(json["step_name"], "analyze");
            assert_eq!(json["step_index"], 0);
            assert_eq!(json["total_steps"], 4);
        }

        #[test]
        fn workflow_step_completed_event_json_format() {
            let event = AiEvent::WorkflowStepCompleted {
                workflow_id: "wf-001".to_string(),
                step_name: "analyze".to_string(),
                output: Some("Analysis complete".to_string()),
                duration_ms: 1500,
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "workflow_step_completed");
            assert_eq!(json["workflow_id"], "wf-001");
            assert_eq!(json["step_name"], "analyze");
            assert_eq!(json["output"], "Analysis complete");
            assert_eq!(json["duration_ms"], 1500);
        }

        #[test]
        fn workflow_completed_event_json_format() {
            let event = AiEvent::WorkflowCompleted {
                workflow_id: "wf-001".to_string(),
                final_output: "Commit created successfully".to_string(),
                total_duration_ms: 8500,
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "workflow_completed");
            assert_eq!(json["workflow_id"], "wf-001");
            assert_eq!(json["final_output"], "Commit created successfully");
            assert_eq!(json["total_duration_ms"], 8500);
        }

        #[test]
        fn workflow_error_event_json_format() {
            let event = AiEvent::WorkflowError {
                workflow_id: "wf-001".to_string(),
                step_name: Some("commit".to_string()),
                error: "Git commit failed".to_string(),
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "workflow_error");
            assert_eq!(json["workflow_id"], "wf-001");
            assert_eq!(json["step_name"], "commit");
            assert_eq!(json["error"], "Git commit failed");
        }

        #[test]
        fn tool_response_truncated_event_json_format() {
            let event = AiEvent::ToolResponseTruncated {
                tool_name: "read_file".to_string(),
                original_tokens: 50000,
                truncated_tokens: 10000,
            };
            let json = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "tool_response_truncated");
            assert_eq!(json["tool_name"], "read_file");
            assert_eq!(json["original_tokens"], 50000);
            assert_eq!(json["truncated_tokens"], 10000);
        }
    }

    /// Tests for ToolSource JSON serialization
    mod tool_source_serialization {
        use super::*;

        #[test]
        fn main_source_json_format() {
            let source = ToolSource::Main;
            let json = serde_json::to_value(&source).unwrap();

            assert_eq!(json["type"], "main");
        }

        #[test]
        fn sub_agent_source_json_format() {
            let source = ToolSource::SubAgent {
                agent_id: "agent-001".to_string(),
                agent_name: "code_analyzer".to_string(),
            };
            let json = serde_json::to_value(&source).unwrap();

            assert_eq!(json["type"], "sub_agent");
            assert_eq!(json["agent_id"], "agent-001");
            assert_eq!(json["agent_name"], "code_analyzer");
        }

        #[test]
        fn workflow_source_json_format() {
            let source = ToolSource::Workflow {
                workflow_id: "wf-001".to_string(),
                workflow_name: "git_commit".to_string(),
                step_name: Some("analyze".to_string()),
                step_index: Some(0),
            };
            let json = serde_json::to_value(&source).unwrap();

            assert_eq!(json["type"], "workflow");
            assert_eq!(json["workflow_id"], "wf-001");
            assert_eq!(json["workflow_name"], "git_commit");
            assert_eq!(json["step_name"], "analyze");
            assert_eq!(json["step_index"], 0);
        }

        #[test]
        fn workflow_source_without_step_json_format() {
            let source = ToolSource::Workflow {
                workflow_id: "wf-001".to_string(),
                workflow_name: "git_commit".to_string(),
                step_name: None,
                step_index: None,
            };
            let json = serde_json::to_value(&source).unwrap();

            assert_eq!(json["type"], "workflow");
            assert_eq!(json["workflow_id"], "wf-001");
            // step_name and step_index should be absent (skip_serializing_if)
            assert!(json.get("step_name").is_none());
            assert!(json.get("step_index").is_none());
        }
    }

    /// Tests for complete roundtrip (serialize -> deserialize)
    mod roundtrip {
        use super::*;

        #[test]
        fn all_event_types_roundtrip() {
            let events = vec![
                AiEvent::Started {
                    turn_id: "turn-1".to_string(),
                },
                AiEvent::TextDelta {
                    delta: "Hello".to_string(),
                    accumulated: "Hello world".to_string(),
                },
                AiEvent::ToolRequest {
                    tool_name: "read_file".to_string(),
                    args: json!({"path": "/test"}),
                    request_id: "req-1".to_string(),
                    source: ToolSource::Main,
                },
                AiEvent::ToolApprovalRequest {
                    request_id: "req-2".to_string(),
                    tool_name: "write_file".to_string(),
                    args: json!({}),
                    stats: None,
                    risk_level: RiskLevel::High,
                    can_learn: false,
                    suggestion: None,
                    source: ToolSource::Main,
                },
                AiEvent::ToolAutoApproved {
                    request_id: "req-3".to_string(),
                    tool_name: "read_file".to_string(),
                    args: json!({}),
                    reason: "Always allowed".to_string(),
                    source: ToolSource::Main,
                },
                AiEvent::ToolDenied {
                    request_id: "req-4".to_string(),
                    tool_name: "shell".to_string(),
                    args: json!({}),
                    reason: "Blocked".to_string(),
                    source: ToolSource::Main,
                },
                AiEvent::ToolResult {
                    tool_name: "read_file".to_string(),
                    result: json!("content"),
                    success: true,
                    request_id: "req-5".to_string(),
                    source: ToolSource::Main,
                },
                AiEvent::Reasoning {
                    content: "Thinking...".to_string(),
                },
                AiEvent::Completed {
                    response: "Done".to_string(),
                    tokens_used: Some(100),
                    duration_ms: Some(500),
                },
                AiEvent::Error {
                    message: "Failed".to_string(),
                    error_type: "api".to_string(),
                },
                AiEvent::SubAgentStarted {
                    agent_id: "a1".to_string(),
                    agent_name: "analyzer".to_string(),
                    task: "analyze".to_string(),
                    depth: 1,
                },
                AiEvent::SubAgentToolRequest {
                    agent_id: "a1".to_string(),
                    tool_name: "read_file".to_string(),
                    args: json!({}),
                },
                AiEvent::SubAgentToolResult {
                    agent_id: "a1".to_string(),
                    tool_name: "read_file".to_string(),
                    success: true,
                },
                AiEvent::SubAgentCompleted {
                    agent_id: "a1".to_string(),
                    response: "Done".to_string(),
                    duration_ms: 1000,
                },
                AiEvent::SubAgentError {
                    agent_id: "a1".to_string(),
                    error: "Failed".to_string(),
                },
                AiEvent::ContextPruned {
                    messages_removed: 5,
                    utilization_before: 0.95,
                    utilization_after: 0.75,
                },
                AiEvent::ContextWarning {
                    utilization: 0.85,
                    total_tokens: 170000,
                    max_tokens: 200000,
                },
                AiEvent::ToolResponseTruncated {
                    tool_name: "read_file".to_string(),
                    original_tokens: 50000,
                    truncated_tokens: 10000,
                },
                AiEvent::LoopWarning {
                    tool_name: "list".to_string(),
                    current_count: 8,
                    max_count: 10,
                    message: "Warning".to_string(),
                },
                AiEvent::LoopBlocked {
                    tool_name: "list".to_string(),
                    repeat_count: 10,
                    max_count: 10,
                    message: "Blocked".to_string(),
                },
                AiEvent::MaxIterationsReached {
                    iterations: 50,
                    max_iterations: 50,
                    message: "Max reached".to_string(),
                },
                AiEvent::WorkflowStarted {
                    workflow_id: "wf1".to_string(),
                    workflow_name: "git_commit".to_string(),
                    session_id: "s1".to_string(),
                },
                AiEvent::WorkflowStepStarted {
                    workflow_id: "wf1".to_string(),
                    step_name: "analyze".to_string(),
                    step_index: 0,
                    total_steps: 4,
                },
                AiEvent::WorkflowStepCompleted {
                    workflow_id: "wf1".to_string(),
                    step_name: "analyze".to_string(),
                    output: Some("Done".to_string()),
                    duration_ms: 1000,
                },
                AiEvent::WorkflowCompleted {
                    workflow_id: "wf1".to_string(),
                    final_output: "Complete".to_string(),
                    total_duration_ms: 5000,
                },
                AiEvent::WorkflowError {
                    workflow_id: "wf1".to_string(),
                    step_name: Some("commit".to_string()),
                    error: "Failed".to_string(),
                },
            ];

            for event in events {
                let json_str = serde_json::to_string(&event).expect("serialize failed");
                let roundtrip: AiEvent =
                    serde_json::from_str(&json_str).expect("deserialize failed");

                // Verify roundtrip produces identical JSON
                let original_json = serde_json::to_value(&event).unwrap();
                let roundtrip_json = serde_json::to_value(&roundtrip).unwrap();
                assert_eq!(
                    original_json, roundtrip_json,
                    "Roundtrip failed for event type"
                );
            }
        }
    }
}

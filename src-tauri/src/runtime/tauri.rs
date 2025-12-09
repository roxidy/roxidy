use super::{ApprovalResult, QbitRuntime, RuntimeError, RuntimeEvent};
use crate::ai::events::AiEvent;
use crate::ai::hitl::RiskLevel;
use async_trait::async_trait;
use parking_lot::RwLock;
use serde::Serialize;
use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

#[derive(Debug, Clone, Serialize)]
struct TerminalOutputEvent {
    session_id: String,
    data: String,
}

pub struct TauriRuntime {
    app_handle: AppHandle,
    pending_approvals: Arc<RwLock<HashMap<String, oneshot::Sender<ApprovalResult>>>>,
}

impl TauriRuntime {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle,
            pending_approvals: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Called by frontend when user responds to approval dialog
    ///
    /// This is exposed as a Tauri command:
    /// ```rust,ignore
    /// #[tauri::command]
    /// pub async fn respond_to_tool_approval(
    ///     app_state: tauri::State<'_, AppState>,
    ///     request_id: String,
    ///     approved: bool,
    /// ) -> Result<(), String> {
    ///     let decision = if approved {
    ///         ApprovalResult::Approved
    ///     } else {
    ///         ApprovalResult::Denied
    ///     };
    ///     app_state.runtime.respond_to_approval(&request_id, decision);
    ///     Ok(())
    /// }
    /// ```
    pub fn respond_to_approval(&self, request_id: &str, decision: ApprovalResult) {
        if let Some(tx) = self.pending_approvals.write().remove(request_id) {
            // Ignore send errors - if receiver dropped, they already timed out
            let _ = tx.send(decision);
        }
    }
}

#[async_trait]
impl QbitRuntime for TauriRuntime {
    fn emit(&self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        // Emit with appropriate event name based on the RuntimeEvent variant
        match &event {
            RuntimeEvent::Ai(ai_event) => {
                // AI events go to ai-event channel
                self.app_handle
                    .emit("ai-event", ai_event)
                    .map_err(|e| RuntimeError::EmitFailed(e.to_string()))?;
            }
            RuntimeEvent::TerminalOutput { session_id, data } => {
                // Terminal output goes to terminal_output channel
                let output_str = String::from_utf8_lossy(data).to_string();
                self.app_handle
                    .emit(
                        "terminal_output",
                        TerminalOutputEvent {
                            session_id: session_id.clone(),
                            data: output_str,
                        },
                    )
                    .map_err(|e| RuntimeError::EmitFailed(e.to_string()))?;
            }
            RuntimeEvent::TerminalExit { session_id, .. } => {
                // Session ended goes to session_ended channel
                self.app_handle
                    .emit(
                        "session_ended",
                        serde_json::json!({
                            "sessionId": session_id
                        }),
                    )
                    .map_err(|e| RuntimeError::EmitFailed(e.to_string()))?;
            }
            RuntimeEvent::Custom { name, payload } => {
                // Custom events use the specified name
                self.app_handle
                    .emit(name, payload)
                    .map_err(|e| RuntimeError::EmitFailed(e.to_string()))?;
            }
        }
        Ok(())
    }

    async fn request_approval(
        &self,
        request_id: String,
        tool_name: String,
        args: serde_json::Value,
        risk_level: String,
    ) -> Result<ApprovalResult, RuntimeError> {
        // Create response channel
        let (tx, rx) = oneshot::channel();

        // Insert into map (lock dropped immediately)
        {
            self.pending_approvals
                .write()
                .insert(request_id.clone(), tx);
        }

        // Parse risk level from string (default to High if unknown)
        let risk = match risk_level.to_lowercase().as_str() {
            "low" => RiskLevel::Low,
            "medium" => RiskLevel::Medium,
            "high" => RiskLevel::High,
            "critical" => RiskLevel::Critical,
            _ => RiskLevel::High,
        };

        // Emit approval request to frontend
        self.emit(RuntimeEvent::Ai(Box::new(AiEvent::ToolApprovalRequest {
            request_id: request_id.clone(),
            tool_name,
            args,
            stats: None,
            risk_level: risk,
            can_learn: true,
            suggestion: None,
            source: Default::default(),
        })))?;

        // Wait for response with 5-minute timeout
        match tokio::time::timeout(Duration::from_secs(300), rx).await {
            Ok(Ok(decision)) => Ok(decision),
            Ok(Err(_)) => {
                // Sender dropped without sending - shouldn't happen
                self.pending_approvals.write().remove(&request_id);
                Err(RuntimeError::ApprovalTimeout(300))
            }
            Err(_) => {
                // Timeout - clean up pending approval
                self.pending_approvals.write().remove(&request_id);
                Err(RuntimeError::ApprovalTimeout(300))
            }
        }
    }

    fn is_interactive(&self) -> bool {
        true // Tauri always has UI
    }

    fn auto_approve(&self) -> bool {
        false // Tauri uses UI-based approval
    }

    async fn shutdown(&self) -> Result<(), RuntimeError> {
        // Cancel all pending approvals
        let pending = {
            let mut approvals = self.pending_approvals.write();
            std::mem::take(&mut *approvals)
        };

        for (_, tx) in pending {
            let _ = tx.send(ApprovalResult::Timeout);
        }

        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

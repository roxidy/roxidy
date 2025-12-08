use super::{ApprovalResult, QbitRuntime, RuntimeError, RuntimeEvent};
use async_trait::async_trait;
use parking_lot::RwLock;
use std::any::Any;
use std::io::{self, Write};
use tokio::sync::mpsc;

pub struct CliRuntime {
    event_tx: RwLock<mpsc::UnboundedSender<RuntimeEvent>>,
    auto_approve: bool,
    json_mode: bool,
    quiet_mode: bool,
}

impl CliRuntime {
    pub fn new(
        event_tx: mpsc::UnboundedSender<RuntimeEvent>,
        auto_approve: bool,
        json_mode: bool,
        quiet_mode: bool,
    ) -> Self {
        Self {
            event_tx: RwLock::new(event_tx),
            auto_approve,
            json_mode,
            quiet_mode,
        }
    }

    /// Replace the event sender (used for batch mode where each prompt needs a fresh channel)
    pub fn replace_event_tx(&self, new_tx: mpsc::UnboundedSender<RuntimeEvent>) {
        *self.event_tx.write() = new_tx;
    }
}

#[async_trait]
impl QbitRuntime for CliRuntime {
    fn emit(&self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        // Send to channel for CLI event handler to process
        self.event_tx
            .read()
            .send(event)
            .map_err(|_| RuntimeError::ReceiverClosed)?;
        Ok(())
    }

    async fn request_approval(
        &self,
        _request_id: String,
        tool_name: String,
        args: serde_json::Value,
        risk_level: String,
    ) -> Result<ApprovalResult, RuntimeError> {
        // Auto-approve if flag set
        if self.auto_approve {
            if !self.json_mode {
                eprintln!("[auto-approved] {}", tool_name);
            }
            return Ok(ApprovalResult::Approved);
        }

        // Check if stdin is a TTY
        if !atty::is(atty::Stream::Stdin) {
            return Err(RuntimeError::NotInteractive);
        }

        // Prompt user
        eprint!(
            "\n[{}] {} {}\n(a)pprove / (d)eny / (A)lways / (D)never: ",
            risk_level, tool_name, args
        );
        io::stderr().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.trim().to_lowercase().as_str() {
            "a" | "y" | "yes" => Ok(ApprovalResult::Approved),
            "d" | "n" | "no" => Ok(ApprovalResult::Denied),
            "always" | "aa" => Ok(ApprovalResult::AlwaysAllow),
            "never" | "dd" => Ok(ApprovalResult::AlwaysDeny),
            _ => Ok(ApprovalResult::Denied), // Default to deny on invalid input
        }
    }

    fn is_interactive(&self) -> bool {
        atty::is(atty::Stream::Stdin)
    }

    fn auto_approve(&self) -> bool {
        self.auto_approve
    }

    async fn shutdown(&self) -> Result<(), RuntimeError> {
        // No cleanup needed - channel drop handles it
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

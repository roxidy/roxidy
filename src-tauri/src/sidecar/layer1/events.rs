//! Layer 1 events for frontend communication
//!
//! These events are emitted when the Layer 1 session state changes,
//! allowing the frontend to reactively update its UI.

use serde::Serialize;
use std::path::PathBuf;
use uuid::Uuid;

use super::state::{Decision, ErrorEntry, FileContext, Goal, OpenQuestion};

/// Events emitted when Layer 1 session state changes
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Layer1Event {
    /// Session state was updated
    StateUpdated {
        session_id: Uuid,
        changes: Vec<String>,
    },

    /// A goal was added
    GoalAdded { session_id: Uuid, goal: Goal },

    /// A goal was completed
    GoalCompleted { session_id: Uuid, goal_id: Uuid },

    /// Narrative was updated
    NarrativeUpdated { session_id: Uuid, narrative: String },

    /// A decision was recorded
    DecisionRecorded {
        session_id: Uuid,
        decision: Decision,
    },

    /// An error was added or resolved
    ErrorUpdated { session_id: Uuid, error: ErrorEntry },

    /// An open question was added
    QuestionAdded {
        session_id: Uuid,
        question: OpenQuestion,
    },

    /// A question was answered
    QuestionAnswered {
        session_id: Uuid,
        question_id: Uuid,
        answer: String,
    },

    /// File context was updated
    FileContextUpdated {
        session_id: Uuid,
        path: PathBuf,
        context: FileContext,
    },
}

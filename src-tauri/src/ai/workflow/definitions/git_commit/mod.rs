//! Git commit workflow definition.
//!
//! A multi-agent workflow that:
//! 1. Gathers git status and diff by running commands (if not provided)
//! 2. Analyzes git status and diff output
//! 3. Organizes changes into logical commits
//! 4. Generates git commands for each commit

#![allow(unused)]

mod analyzer;
mod gatherer;
mod organizer;
mod planner;
pub mod state;

pub use analyzer::AnalyzerTask;
pub use gatherer::GathererTask;
pub use organizer::OrganizerTask;
pub use planner::PlannerTask;
pub use state::{GitCommitResult, GitCommitState, WorkflowStage};

use std::sync::Arc;

use async_trait::async_trait;
use graph_flow::{Context, GraphBuilder, NextAction, Task, TaskResult};

use crate::ai::workflow::models::{WorkflowDefinition, WorkflowLlmExecutor};

/// State key for storing GitCommitState in graph-flow Context.
pub const STATE_KEY: &str = "git_commit_state";

/// Git commit workflow definition.
pub struct GitCommitWorkflow;

impl WorkflowDefinition for GitCommitWorkflow {
    fn name(&self) -> &str {
        "git_commit"
    }

    fn description(&self) -> &str {
        "Analyzes git changes and organizes them into logical commits with generated commands"
    }

    fn build_graph(&self, executor: Arc<dyn WorkflowLlmExecutor>) -> Arc<graph_flow::Graph> {
        let initialize = Arc::new(InitializeTask);
        let gatherer = Arc::new(GathererTask::new(executor.clone()));
        let analyzer = Arc::new(AnalyzerTask::new(executor.clone()));
        let organizer = Arc::new(OrganizerTask::new(executor.clone()));
        let planner = Arc::new(PlannerTask::new(executor));
        let formatter = Arc::new(FormatterTask);

        let graph = GraphBuilder::new("git_commit")
            .add_task(initialize.clone())
            .add_task(gatherer.clone())
            .add_task(analyzer.clone())
            .add_task(organizer.clone())
            .add_task(planner.clone())
            .add_task(formatter.clone())
            .add_edge(initialize.id(), gatherer.id())
            .add_edge(gatherer.id(), analyzer.id())
            .add_edge(analyzer.id(), organizer.id())
            .add_edge(organizer.id(), planner.id())
            .add_edge(planner.id(), formatter.id())
            .build();

        Arc::new(graph)
    }

    fn init_state(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        // Input is now optional - the gatherer task will run git commands if needed
        // If input is provided with git_status and git_diff, use them directly
        let state = if input.is_null() || input == serde_json::json!({}) {
            // No input provided - gatherer will collect data
            GitCommitState::default()
        } else {
            // Try to parse optional input
            #[derive(serde::Deserialize, Default)]
            struct OptionalInput {
                #[serde(default)]
                git_status: Option<String>,
                #[serde(default)]
                git_diff: Option<String>,
            }

            let parsed: OptionalInput = serde_json::from_value(input).unwrap_or_default();

            GitCommitState {
                git_status: parsed.git_status,
                git_diff: parsed.git_diff,
                file_changes: vec![],
                commit_plans: vec![],
                git_commands: None,
                errors: vec![],
                stage: WorkflowStage::Initialized,
            }
        };

        Ok(serde_json::to_value(state)?)
    }

    fn start_task(&self) -> &str {
        "initialize"
    }

    fn state_key(&self) -> &str {
        STATE_KEY
    }

    fn task_count(&self) -> usize {
        6 // initialize, gatherer, analyzer, organizer, planner, formatter
    }
}

/// Initialize task - sets up the workflow state from context input.
struct InitializeTask;

#[async_trait]
impl Task for InitializeTask {
    fn id(&self) -> &str {
        "initialize"
    }

    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        // State should already be set by the runner from init_state()
        // Just verify it exists
        let state: Option<GitCommitState> = context.get(STATE_KEY).await;

        if state.is_none() {
            return Ok(TaskResult::new(
                Some("Error: Workflow state not initialized".to_string()),
                NextAction::End,
            ));
        }

        Ok(TaskResult::new(
            Some("Workflow initialized".to_string()),
            NextAction::ContinueAndExecute,
        ))
    }
}

/// Formatter task - formats the final output.
struct FormatterTask;

#[async_trait]
impl Task for FormatterTask {
    fn id(&self) -> &str {
        "formatter"
    }

    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let state: GitCommitState = context.get(STATE_KEY).await.unwrap_or_default();

        // Format the output
        let output = if state.errors.is_empty() {
            if let Some(ref commands) = state.git_commands {
                format!(
                    "## Git Commit Plan\n\n{} commits planned:\n\n{}\n\n### Commands\n```bash\n{}\n```",
                    state.commit_plans.len(),
                    state
                        .commit_plans
                        .iter()
                        .enumerate()
                        .map(|(i, plan)| format!(
                            "{}. **{}**\n   Files: {}",
                            i + 1,
                            plan.message,
                            plan.files.join(", ")
                        ))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    commands
                )
            } else {
                "No git commands generated".to_string()
            }
        } else {
            format!("## Errors\n\n{}", state.errors.join("\n"))
        };

        // Update state to completed
        let mut final_state = state;
        final_state.stage = WorkflowStage::Completed;
        context.set(STATE_KEY, final_state).await;

        Ok(TaskResult::new(Some(output), NextAction::End))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MockExecutor;

    #[async_trait]
    impl WorkflowLlmExecutor for MockExecutor {
        async fn complete(
            &self,
            _system_prompt: &str,
            _user_prompt: &str,
            _context: HashMap<String, serde_json::Value>,
        ) -> anyhow::Result<String> {
            Ok("Mock response".to_string())
        }
    }

    #[test]
    fn test_workflow_definition() {
        let workflow = GitCommitWorkflow;

        assert_eq!(workflow.name(), "git_commit");
        assert_eq!(workflow.start_task(), "initialize");
        assert_eq!(workflow.state_key(), STATE_KEY);
    }

    #[test]
    fn test_init_state_with_input() {
        let workflow = GitCommitWorkflow;

        let input = serde_json::json!({
            "git_status": "M  file.txt",
            "git_diff": "diff content"
        });

        let state = workflow.init_state(input).unwrap();
        let parsed: GitCommitState = serde_json::from_value(state).unwrap();

        assert_eq!(parsed.git_status, Some("M  file.txt".to_string()));
        assert_eq!(parsed.git_diff, Some("diff content".to_string()));
        assert_eq!(parsed.stage, WorkflowStage::Initialized);
    }

    #[test]
    fn test_init_state_empty_input() {
        let workflow = GitCommitWorkflow;

        // Empty input should work - gatherer will collect data
        let state = workflow.init_state(serde_json::json!({})).unwrap();
        let parsed: GitCommitState = serde_json::from_value(state).unwrap();

        assert_eq!(parsed.git_status, None);
        assert_eq!(parsed.git_diff, None);
        assert_eq!(parsed.stage, WorkflowStage::Initialized);
    }

    #[test]
    fn test_build_graph() {
        let executor = Arc::new(MockExecutor);
        let workflow = GitCommitWorkflow;

        // Just verify the graph can be built without panicking
        let _graph = workflow.build_graph(executor);
        // The graph structure is verified by integration tests
    }
}

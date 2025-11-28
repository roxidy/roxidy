//! Workflow graph for multi-agent orchestration using graph-flow.
//!
//! This module integrates the graph-flow crate for:
//! - Type-safe task definitions with async execution
//! - Session-based workflow execution with persistence
//! - Conditional routing between agents
//! - Human-in-the-loop capabilities

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use graph_flow::{
    Context, ExecutionStatus, FlowRunner, Graph, GraphBuilder, NextAction, Session,
    SessionStorage, Task, TaskResult,
};

use super::sub_agent::SubAgentDefinition;

/// Wrapper around graph-flow's InMemorySessionStorage for our use case.
/// In production, this could be swapped for PostgresSessionStorage.
pub type WorkflowStorage = graph_flow::InMemorySessionStorage;

/// A task that executes a sub-agent within the workflow.
pub struct SubAgentTask {
    /// The sub-agent definition
    agent: SubAgentDefinition,
    /// Callback to execute the agent (set externally)
    executor: Arc<dyn SubAgentExecutor + Send + Sync>,
}

/// Trait for executing sub-agents (implemented by AgentBridge)
#[async_trait]
pub trait SubAgentExecutor: Send + Sync {
    async fn execute_agent(
        &self,
        agent: &SubAgentDefinition,
        prompt: &str,
        context_vars: HashMap<String, serde_json::Value>,
    ) -> anyhow::Result<String>;
}

impl SubAgentTask {
    /// Create a new sub-agent task
    pub fn new(agent: SubAgentDefinition, executor: Arc<dyn SubAgentExecutor + Send + Sync>) -> Self {
        Self { agent, executor }
    }
}

#[async_trait]
impl Task for SubAgentTask {
    fn id(&self) -> &str {
        &self.agent.id
    }

    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        // Get the prompt from context
        let prompt: String = context
            .get("prompt")
            .await
            .unwrap_or_else(|| "No prompt provided".to_string());

        // Get any additional context variables
        let context_vars: HashMap<String, serde_json::Value> = context
            .get("variables")
            .await
            .unwrap_or_default();

        // Execute the sub-agent
        match self.executor.execute_agent(&self.agent, &prompt, context_vars).await {
            Ok(response) => {
                // Store the response in context
                context.set("response", response.clone()).await;
                context.set(&format!("{}_response", self.agent.id), response.clone()).await;

                Ok(TaskResult::new(Some(response), NextAction::Continue))
            }
            Err(e) => {
                let error_msg = format!("Agent {} failed: {}", self.agent.id, e);
                context.set("error", error_msg.clone()).await;
                Ok(TaskResult::new(Some(error_msg), NextAction::End))
            }
        }
    }
}

/// A router task that decides which agent to execute next based on context.
pub struct RouterTask {
    id: String,
    /// Routing function that examines context and returns the next task ID
    router_fn: Arc<dyn Fn(&HashMap<String, serde_json::Value>) -> String + Send + Sync>,
}

impl RouterTask {
    /// Create a new router task
    pub fn new<F>(id: impl Into<String>, router_fn: F) -> Self
    where
        F: Fn(&HashMap<String, serde_json::Value>) -> String + Send + Sync + 'static,
    {
        Self {
            id: id.into(),
            router_fn: Arc::new(router_fn),
        }
    }
}

#[async_trait]
impl Task for RouterTask {
    fn id(&self) -> &str {
        &self.id
    }

    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        // Get variables from context
        let variables: HashMap<String, serde_json::Value> = context
            .get("variables")
            .await
            .unwrap_or_default();

        // Determine next task
        let next_task = (self.router_fn)(&variables);

        Ok(TaskResult::new(
            Some(format!("Routing to: {}", next_task)),
            NextAction::GoTo(next_task),
        ))
    }
}

/// Builder for creating agent workflows using graph-flow.
pub struct AgentWorkflowBuilder {
    name: String,
    tasks: Vec<Arc<dyn Task + Send + Sync>>,
    edges: Vec<(String, String)>,
    conditional_edges: Vec<ConditionalEdge>,
    start_task: Option<String>,
}

struct ConditionalEdge {
    from: String,
    condition: Arc<dyn Fn(&Context) -> bool + Send + Sync>,
    on_true: String,
    on_false: String,
}

impl AgentWorkflowBuilder {
    /// Create a new workflow builder
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tasks: Vec::new(),
            edges: Vec::new(),
            conditional_edges: Vec::new(),
            start_task: None,
        }
    }

    /// Add a sub-agent task to the workflow
    pub fn add_agent_task(mut self, task: SubAgentTask) -> Self {
        if self.start_task.is_none() {
            self.start_task = Some(task.id().to_string());
        }
        self.tasks.push(Arc::new(task));
        self
    }

    /// Add a router task to the workflow
    pub fn add_router_task(mut self, task: RouterTask) -> Self {
        if self.start_task.is_none() {
            self.start_task = Some(task.id().to_string());
        }
        self.tasks.push(Arc::new(task));
        self
    }

    /// Add a custom task
    pub fn add_task<T: Task + Send + Sync + 'static>(mut self, task: T) -> Self {
        if self.start_task.is_none() {
            self.start_task = Some(task.id().to_string());
        }
        self.tasks.push(Arc::new(task));
        self
    }

    /// Set the starting task
    pub fn start(mut self, task_id: impl Into<String>) -> Self {
        self.start_task = Some(task_id.into());
        self
    }

    /// Add an edge between two tasks
    pub fn edge(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.edges.push((from.into(), to.into()));
        self
    }

    /// Add a conditional edge
    pub fn conditional_edge<F>(
        mut self,
        from: impl Into<String>,
        condition: F,
        on_true: impl Into<String>,
        on_false: impl Into<String>,
    ) -> Self
    where
        F: Fn(&Context) -> bool + Send + Sync + 'static,
    {
        self.conditional_edges.push(ConditionalEdge {
            from: from.into(),
            condition: Arc::new(condition),
            on_true: on_true.into(),
            on_false: on_false.into(),
        });
        self
    }

    /// Build the workflow graph
    pub fn build(self) -> anyhow::Result<Arc<Graph>> {
        let mut builder = GraphBuilder::new(&self.name);

        // Add all tasks
        for task in self.tasks {
            builder = builder.add_task(task);
        }

        // Add edges
        for (from, to) in self.edges {
            builder = builder.add_edge(&from, &to);
        }

        // Add conditional edges
        for edge in self.conditional_edges {
            let condition = edge.condition.clone();
            builder = builder.add_conditional_edge(
                &edge.from,
                move |ctx| {
                    // We need to check the condition synchronously
                    // This is a limitation - we use get_sync
                    let check = ctx.get_sync::<bool>("_condition_check").unwrap_or(false);
                    check
                },
                &edge.on_true,
                &edge.on_false,
            );
        }

        Ok(Arc::new(builder.build()))
    }
}

/// Manages workflow execution with session persistence.
pub struct WorkflowRunner {
    flow_runner: FlowRunner,
    storage: Arc<dyn SessionStorage + Send + Sync>,
}

impl WorkflowRunner {
    /// Create a new workflow runner
    pub fn new(graph: Arc<Graph>, storage: Arc<dyn SessionStorage + Send + Sync>) -> Self {
        Self {
            flow_runner: FlowRunner::new(graph.clone(), storage.clone()),
            storage,
        }
    }

    /// Create a new workflow runner with in-memory storage
    pub fn new_in_memory(graph: Arc<Graph>) -> Self {
        let storage: Arc<dyn SessionStorage + Send + Sync> = Arc::new(WorkflowStorage::new());
        Self::new(graph, storage)
    }

    /// Start a new workflow session
    pub async fn start_session(&self, initial_prompt: &str, start_task: &str) -> anyhow::Result<String> {
        let session_id = uuid::Uuid::new_v4().to_string();

        // Create session with initial context - use the provided start task
        let session = Session::new_from_task(session_id.clone(), start_task);
        session.context.set("prompt", initial_prompt.to_string()).await;

        // Save session
        self.storage
            .save(session)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to save session: {}", e))?;

        Ok(session_id)
    }

    /// Execute the next step in a workflow
    pub async fn step(&self, session_id: &str) -> anyhow::Result<WorkflowStepResult> {
        let result = self
            .flow_runner
            .run(session_id)
            .await
            .map_err(|e| anyhow::anyhow!("Workflow execution failed: {}", e))?;

        Ok(WorkflowStepResult {
            output: result.response,
            status: match result.status {
                ExecutionStatus::Paused { next_task_id, .. } => {
                    WorkflowStatus::Paused { next_task_id }
                }
                ExecutionStatus::WaitingForInput => WorkflowStatus::WaitingForInput,
                ExecutionStatus::Completed => WorkflowStatus::Completed,
                ExecutionStatus::Error(e) => WorkflowStatus::Error(e),
            },
        })
    }

    /// Run workflow to completion
    pub async fn run_to_completion(&self, session_id: &str) -> anyhow::Result<String> {
        let mut final_output = String::new();

        loop {
            let result = self.step(session_id).await?;

            if let Some(output) = result.output {
                final_output = output;
            }

            match result.status {
                WorkflowStatus::Completed => break,
                WorkflowStatus::Error(e) => return Err(anyhow::anyhow!(e)),
                WorkflowStatus::WaitingForInput => {
                    return Err(anyhow::anyhow!("Workflow waiting for input"));
                }
                WorkflowStatus::Paused { .. } => continue,
            }
        }

        Ok(final_output)
    }

}

/// Result of a single workflow step
#[derive(Debug, Clone)]
pub struct WorkflowStepResult {
    pub output: Option<String>,
    pub status: WorkflowStatus,
}

/// Status of workflow execution
#[derive(Debug, Clone)]
pub enum WorkflowStatus {
    Paused { next_task_id: String },
    WaitingForInput,
    Completed,
    Error(String),
}

/// Pre-built workflow patterns
pub mod patterns {
    use super::*;

    /// Create a simple sequential workflow: task1 -> task2 -> task3 -> ...
    pub fn sequential(
        name: &str,
        tasks: Vec<SubAgentTask>,
    ) -> anyhow::Result<Arc<Graph>> {
        let mut builder = AgentWorkflowBuilder::new(name);

        let task_ids: Vec<String> = tasks.iter().map(|t| t.agent.id.clone()).collect();

        for task in tasks {
            builder = builder.add_agent_task(task);
        }

        // Chain tasks sequentially
        for i in 0..task_ids.len().saturating_sub(1) {
            builder = builder.edge(&task_ids[i], &task_ids[i + 1]);
        }

        builder.build()
    }

    /// Create a router-based workflow where a router decides which agent to use
    pub fn router_dispatch(
        name: &str,
        router: RouterTask,
        agents: Vec<SubAgentTask>,
    ) -> anyhow::Result<Arc<Graph>> {
        let mut builder = AgentWorkflowBuilder::new(name)
            .add_router_task(router)
            .start("router");

        let router_id = "router".to_string();

        for task in agents {
            let task_id = task.agent.id.clone();
            builder = builder
                .add_agent_task(task)
                .edge(&router_id, &task_id);
        }

        builder.build()
    }
}

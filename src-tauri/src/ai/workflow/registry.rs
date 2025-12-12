//! Registry for workflow definitions.
//!
//! The registry manages all available workflow types and allows
//! looking them up by name at runtime.

use std::collections::HashMap;
use std::sync::Arc;

use super::models::{WorkflowDefinition, WorkflowInfo};

/// Registry of workflow definitions.
///
/// Workflows register themselves here at startup, and can then
/// be looked up by name when a user wants to start one.
pub struct WorkflowRegistry {
    definitions: HashMap<String, Arc<dyn WorkflowDefinition>>,
}

impl WorkflowRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
        }
    }

    /// Register a workflow definition.
    ///
    /// # Arguments
    /// * `definition` - The workflow definition to register
    ///
    /// # Panics
    /// Panics if a workflow with the same name is already registered.
    pub fn register(&mut self, definition: Arc<dyn WorkflowDefinition>) {
        let name = definition.name().to_string();
        if self.definitions.contains_key(&name) {
            panic!("Workflow '{}' is already registered", name);
        }
        self.definitions.insert(name, definition);
    }

    /// Register a workflow, replacing any existing one with the same name.
    #[allow(dead_code)]
    pub fn register_or_replace(&mut self, definition: Arc<dyn WorkflowDefinition>) {
        let name = definition.name().to_string();
        self.definitions.insert(name, definition);
    }

    /// Get a workflow definition by name.
    #[allow(dead_code)]
    pub fn get(&self, name: &str) -> Option<Arc<dyn WorkflowDefinition>> {
        self.definitions.get(name).cloned()
    }

    /// List all registered workflow names.
    #[allow(dead_code)]
    pub fn list(&self) -> Vec<String> {
        self.definitions.keys().cloned().collect()
    }

    /// Get info about all registered workflows.
    pub fn list_info(&self) -> Vec<WorkflowInfo> {
        self.definitions
            .values()
            .map(|def| WorkflowInfo {
                name: def.name().to_string(),
                description: def.description().to_string(),
            })
            .collect()
    }

    /// Check if a workflow exists.
    #[allow(dead_code)]
    pub fn contains(&self, name: &str) -> bool {
        self.definitions.contains_key(name)
    }

    /// Get the number of registered workflows.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    /// Check if the registry is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }
}

impl Default for WorkflowRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::workflow::models::WorkflowLlmExecutor;
    use graph_flow::Graph;

    struct TestWorkflow;

    impl WorkflowDefinition for TestWorkflow {
        fn name(&self) -> &str {
            "test_workflow"
        }

        fn description(&self) -> &str {
            "A test workflow"
        }

        fn build_graph(&self, _executor: Arc<dyn WorkflowLlmExecutor>) -> Arc<Graph> {
            Arc::new(Graph::new("test"))
        }

        fn init_state(&self, _input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
            Ok(serde_json::json!({"initialized": true}))
        }

        fn start_task(&self) -> &str {
            "start"
        }

        fn state_key(&self) -> &str {
            "test_state"
        }
    }

    #[test]
    fn test_register_and_get() {
        let mut registry = WorkflowRegistry::new();
        registry.register(Arc::new(TestWorkflow));

        assert!(registry.contains("test_workflow"));
        assert!(!registry.contains("nonexistent"));

        let def = registry.get("test_workflow").unwrap();
        assert_eq!(def.name(), "test_workflow");
    }

    #[test]
    fn test_list_info() {
        let mut registry = WorkflowRegistry::new();
        registry.register(Arc::new(TestWorkflow));

        let info = registry.list_info();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].name, "test_workflow");
        assert_eq!(info[0].description, "A test workflow");
    }
}

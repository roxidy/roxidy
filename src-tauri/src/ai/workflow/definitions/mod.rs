//! Workflow definitions.
//!
//! Each workflow type is defined in its own submodule and implements
//! the `WorkflowDefinition` trait.

#![allow(unused)]

pub mod git_commit;

use std::sync::Arc;

use super::registry::WorkflowRegistry;

// Re-export workflow definitions for convenience
pub use git_commit::GitCommitWorkflow;

/// Register all built-in workflows with the registry.
pub fn register_builtin_workflows(registry: &mut WorkflowRegistry) {
    registry.register(Arc::new(GitCommitWorkflow));
    // Add more workflows here as they're implemented:
    // registry.register(Arc::new(CodeReviewWorkflow));
    // registry.register(Arc::new(RefactorWorkflow));
}

/// Create a registry with all built-in workflows pre-registered.
pub fn create_default_registry() -> WorkflowRegistry {
    let mut registry = WorkflowRegistry::new();
    register_builtin_workflows(&mut registry);
    registry
}

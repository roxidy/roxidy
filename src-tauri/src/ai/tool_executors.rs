//! Tool execution logic for the agent bridge.
//!
//! This module contains the logic for executing various types of tools:
//! - Indexer tools (code search, file analysis)
//! - Tavily tools (web search)
//! - Workflow tools (multi-step AI workflows)
//!
//! Note: Workflow execution requires the `tauri` feature as it depends on
//! `WorkflowState` and `BridgeLlmExecutor` from the commands module.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
#[cfg(feature = "tauri")]
use tokio::sync::RwLock;
use vtcode_core::tools::tree_sitter::analysis::CodeAnalyzer;
#[cfg(feature = "tauri")]
use vtcode_core::tools::ToolRegistry;

#[cfg(feature = "tauri")]
use crate::ai::commands::workflow::{BridgeLlmExecutor, WorkflowState};
#[cfg(feature = "tauri")]
use crate::ai::events::AiEvent;
#[cfg(feature = "tauri")]
use crate::ai::llm_client::LlmClient;
#[cfg(feature = "tauri")]
use crate::ai::workflow::{WorkflowLlmExecutor, WorkflowRunner};
use crate::indexer::IndexerState;
use crate::tavily::TavilyState;
use crate::web_fetch::WebFetcher;

/// Result type for tool execution: (json_result, success_flag)
type ToolResult = (serde_json::Value, bool);

/// Helper to create an error result
fn error_result(msg: impl Into<String>) -> ToolResult {
    (json!({"error": msg.into()}), false)
}

/// Execute an indexer tool by name.
pub async fn execute_indexer_tool(
    indexer_state: Option<&Arc<IndexerState>>,
    tool_name: &str,
    args: &serde_json::Value,
) -> ToolResult {
    let Some(indexer) = indexer_state else {
        return error_result("Indexer not initialized");
    };

    // Helper to get a string argument
    let get_str = |key: &str| args.get(key).and_then(|v| v.as_str()).unwrap_or("");

    match tool_name {
        "indexer_search_code" => {
            let pattern = get_str("pattern");
            let path_filter = args.get("path_filter").and_then(|v| v.as_str());

            match indexer.with_indexer(|idx| idx.search(pattern, path_filter)) {
                Ok(results) => (
                    json!({
                        "matches": results.iter().map(|r| json!({
                            "file": r.file_path,
                            "line": r.line_number,
                            "content": r.line_content,
                            "matches": r.matches
                        })).collect::<Vec<_>>(),
                        "count": results.len()
                    }),
                    true,
                ),
                Err(e) => error_result(e.to_string()),
            }
        }
        "indexer_search_files" => {
            let pattern = get_str("pattern");

            match indexer.with_indexer(|idx| idx.find_files(pattern)) {
                Ok(files) => (json!({"files": files, "count": files.len()}), true),
                Err(e) => error_result(e.to_string()),
            }
        }
        "indexer_analyze_file" => {
            let file_path = get_str("file_path");
            let mut analyzer = match indexer.get_analyzer() {
                Ok(a) => a,
                Err(e) => return error_result(e.to_string()),
            };

            let path = PathBuf::from(file_path);
            let tree = match analyzer.parse_file(&path).await {
                Ok(t) => t,
                Err(e) => return error_result(format!("Failed to parse file: {}", e)),
            };

            let code_analyzer = CodeAnalyzer::new(&tree.language);
            let analysis = code_analyzer.analyze(&tree, file_path);

            (
                json!({
                    "symbols": analysis.symbols.iter().map(|s| json!({
                        "name": s.name,
                        "kind": format!("{:?}", s.kind),
                        "line": s.position.row,
                        "column": s.position.column
                    })).collect::<Vec<_>>(),
                    "metrics": {
                        "lines_of_code": analysis.metrics.lines_of_code,
                        "lines_of_comments": analysis.metrics.lines_of_comments,
                        "blank_lines": analysis.metrics.blank_lines,
                        "functions_count": analysis.metrics.functions_count,
                        "classes_count": analysis.metrics.classes_count,
                        "comment_ratio": analysis.metrics.comment_ratio
                    },
                    "dependencies": analysis.dependencies.iter().map(|d| json!({
                        "name": d.name,
                        "kind": format!("{:?}", d.kind)
                    })).collect::<Vec<_>>()
                }),
                true,
            )
        }
        "indexer_extract_symbols" => {
            use vtcode_core::tools::tree_sitter::languages::LanguageAnalyzer;

            let file_path = get_str("file_path");
            let mut analyzer = match indexer.get_analyzer() {
                Ok(a) => a,
                Err(e) => return error_result(e.to_string()),
            };

            let path = PathBuf::from(file_path);
            let tree = match analyzer.parse_file(&path).await {
                Ok(t) => t,
                Err(e) => return error_result(format!("Failed to parse file: {}", e)),
            };

            let lang_analyzer = LanguageAnalyzer::new(&tree.language);
            let symbols = lang_analyzer.extract_symbols(&tree);

            (
                json!({
                    "symbols": symbols.iter().map(|s| json!({
                        "name": s.name,
                        "kind": format!("{:?}", s.kind),
                        "line": s.position.row,
                        "column": s.position.column,
                        "scope": s.scope,
                        "signature": s.signature,
                        "documentation": s.documentation
                    })).collect::<Vec<_>>(),
                    "count": symbols.len()
                }),
                true,
            )
        }
        "indexer_get_metrics" => {
            let file_path = get_str("file_path");
            let mut analyzer = match indexer.get_analyzer() {
                Ok(a) => a,
                Err(e) => return error_result(e.to_string()),
            };

            let path = PathBuf::from(file_path);
            let tree = match analyzer.parse_file(&path).await {
                Ok(t) => t,
                Err(e) => return error_result(format!("Failed to parse file: {}", e)),
            };

            let code_analyzer = CodeAnalyzer::new(&tree.language);
            let metrics = code_analyzer.analyze(&tree, file_path).metrics;

            (
                json!({
                    "lines_of_code": metrics.lines_of_code,
                    "lines_of_comments": metrics.lines_of_comments,
                    "blank_lines": metrics.blank_lines,
                    "functions_count": metrics.functions_count,
                    "classes_count": metrics.classes_count,
                    "variables_count": metrics.variables_count,
                    "imports_count": metrics.imports_count,
                    "comment_ratio": metrics.comment_ratio
                }),
                true,
            )
        }
        "indexer_detect_language" => {
            let file_path = get_str("file_path");
            let analyzer = match indexer.get_analyzer() {
                Ok(a) => a,
                Err(e) => return error_result(e.to_string()),
            };

            let path = PathBuf::from(file_path);
            match analyzer.detect_language_from_path(&path) {
                Ok(lang) => (json!({"language": format!("{:?}", lang)}), true),
                Err(e) => error_result(e.to_string()),
            }
        }
        _ => error_result(format!("Unknown indexer tool: {}", tool_name)),
    }
}

/// Execute a Tavily web search tool.
pub async fn execute_tavily_tool(
    tavily_state: Option<&Arc<TavilyState>>,
    tool_name: &str,
    args: &serde_json::Value,
) -> ToolResult {
    let Some(tavily) = tavily_state else {
        return error_result("Web search not available - TAVILY_API_KEY not configured");
    };

    // Helper to get a string argument
    let get_str = |key: &str| args.get(key).and_then(|v| v.as_str()).unwrap_or("");

    match tool_name {
        "web_search" => {
            let query = get_str("query");
            let max_results = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);

            match tavily.search(query, max_results).await {
                Ok(results) => (
                    json!({
                        "query": results.query,
                        "results": results.results.iter().map(|r| json!({
                            "title": r.title,
                            "url": r.url,
                            "content": r.content,
                            "score": r.score
                        })).collect::<Vec<_>>(),
                        "answer": results.answer,
                        "count": results.results.len()
                    }),
                    true,
                ),
                Err(e) => error_result(e.to_string()),
            }
        }
        "web_search_answer" => {
            let query = get_str("query");

            match tavily.answer(query).await {
                Ok(result) => (
                    json!({
                        "query": result.query,
                        "answer": result.answer,
                        "sources": result.sources.iter().map(|r| json!({
                            "title": r.title,
                            "url": r.url,
                            "content": r.content,
                            "score": r.score
                        })).collect::<Vec<_>>()
                    }),
                    true,
                ),
                Err(e) => error_result(e.to_string()),
            }
        }
        "web_extract" => {
            let urls: Vec<String> = args
                .get("urls")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            match tavily.extract(urls).await {
                Ok(results) => (
                    json!({
                        "results": results.results.iter().map(|r| json!({
                            "url": r.url,
                            "content": r.raw_content
                        })).collect::<Vec<_>>(),
                        "failed_urls": results.failed_urls,
                        "count": results.results.len()
                    }),
                    true,
                ),
                Err(e) => error_result(e.to_string()),
            }
        }
        _ => error_result(format!("Unknown web search tool: {}", tool_name)),
    }
}

/// Execute a web fetch tool using readability-based content extraction.
pub async fn execute_web_fetch_tool(tool_name: &str, args: &serde_json::Value) -> ToolResult {
    if tool_name != "web_fetch" {
        return error_result(format!("Unknown web fetch tool: {}", tool_name));
    }

    // web_fetch expects a single "url" parameter (not "urls" array)
    let url = match args.get("url").and_then(|v| v.as_str()) {
        Some(u) => u.to_string(),
        None => {
            return error_result(
                "web_fetch requires a 'url' parameter (string). Example: {\"url\": \"https://example.com\"}"
            )
        }
    };

    let fetcher = WebFetcher::new();

    match fetcher.fetch(&url).await {
        Ok(result) => (
            json!({
                "url": result.url,
                "content": result.content
            }),
            true,
        ),
        Err(e) => error_result(format!("Failed to fetch {}: {}", url, e)),
    }
}

/// Context for workflow tool execution.
///
/// Only available when the `tauri` feature is enabled.
#[cfg(feature = "tauri")]
pub struct WorkflowToolContext<'a> {
    pub workflow_state: Option<&'a Arc<WorkflowState>>,
    pub client: &'a Arc<RwLock<LlmClient>>,
    pub event_tx: &'a tokio::sync::mpsc::UnboundedSender<AiEvent>,
    pub tool_registry: &'a Arc<RwLock<ToolRegistry>>,
    pub workspace: &'a Arc<RwLock<PathBuf>>,
    pub indexer_state: Option<&'a Arc<IndexerState>>,
    pub tavily_state: Option<&'a Arc<TavilyState>>,
}

/// Execute a workflow tool.
///
/// This runs a workflow to completion and returns the final output.
///
/// Only available when the `tauri` feature is enabled.
#[cfg(feature = "tauri")]
pub async fn execute_workflow_tool(
    ctx: WorkflowToolContext<'_>,
    args: &serde_json::Value,
) -> ToolResult {
    let Some(workflow_state) = ctx.workflow_state else {
        return error_result("Workflow system not initialized");
    };

    // Get workflow name
    let workflow_name = match args.get("workflow_name").and_then(|v| v.as_str()) {
        Some(name) => name.to_string(),
        None => return error_result("workflow_name is required"),
    };

    // Get input (default to empty object)
    let input = args.get("input").cloned().unwrap_or(json!({}));

    // Generate a unique workflow ID for this execution
    let workflow_id = uuid::Uuid::new_v4().to_string();

    // Get the workflow definition and its task count
    let registry = workflow_state.registry.read().await;
    let workflow = match registry.get(&workflow_name) {
        Some(w) => w,
        None => {
            let available: Vec<_> = registry.list_info().into_iter().map(|w| w.name).collect();
            return error_result(format!(
                "Unknown workflow: '{}'. Available workflows: {:?}",
                workflow_name, available
            ));
        }
    };

    // Create the LLM executor with full agent capabilities and workflow context
    let executor: Arc<dyn WorkflowLlmExecutor> =
        Arc::new(BridgeLlmExecutor::with_workflow_context(
            ctx.client.clone(),
            ctx.event_tx.clone(),
            ctx.tool_registry.clone(),
            ctx.workspace.clone(),
            ctx.indexer_state.cloned(),
            ctx.tavily_state.cloned(),
            workflow_id.clone(),
            workflow_name.clone(),
        ));

    // Build the workflow graph
    let graph = workflow.build_graph(executor);

    // Emit workflow starting event with proper ID
    let _ = ctx.event_tx.send(AiEvent::WorkflowStarted {
        workflow_id: workflow_id.clone(),
        workflow_name: workflow_name.clone(),
        session_id: workflow_id.clone(),
    });

    // Create a runner
    let runner = WorkflowRunner::new(graph, workflow_state.storage.clone());

    // Initialize state
    let initial_state = match workflow.init_state(input.clone()) {
        Ok(state) => state,
        Err(e) => {
            let _ = ctx.event_tx.send(AiEvent::WorkflowError {
                workflow_id: workflow_id.clone(),
                step_name: None,
                error: format!("Failed to initialize workflow state: {}", e),
            });
            return error_result(format!("Failed to initialize workflow state: {}", e));
        }
    };

    // Start the session
    let session_id = match runner.start_session("", workflow.start_task()).await {
        Ok(id) => id,
        Err(e) => {
            let _ = ctx.event_tx.send(AiEvent::WorkflowError {
                workflow_id: workflow_id.clone(),
                step_name: None,
                error: format!("Failed to start workflow session: {}", e),
            });
            return error_result(format!("Failed to start workflow session: {}", e));
        }
    };

    // Set initial state in session context
    if let Ok(Some(session)) = workflow_state.storage.get(&session_id).await {
        session
            .context
            .set(workflow.state_key(), initial_state)
            .await;
        if let Err(e) = workflow_state.storage.save(session).await {
            let _ = ctx.event_tx.send(AiEvent::WorkflowError {
                workflow_id: workflow_id.clone(),
                step_name: None,
                error: format!("Failed to save session: {}", e),
            });
            return error_result(format!("Failed to save session: {}", e));
        }
    }

    // Drop registry read lock before running
    drop(registry);

    // Run workflow steps - tasks emit their own step started/completed events
    let start_time = std::time::Instant::now();
    let mut final_output = String::new();

    loop {
        // Execute the step - tasks emit their own step started/completed events
        let result = match runner.step(&session_id).await {
            Ok(result) => result,
            Err(e) => {
                let _ = ctx.event_tx.send(AiEvent::WorkflowError {
                    workflow_id: workflow_id.clone(),
                    step_name: None,
                    error: e.to_string(),
                });
                return error_result(format!("Workflow execution failed: {}", e));
            }
        };

        if let Some(output) = result.output {
            final_output = output;
        }

        match result.status {
            crate::ai::workflow::runner::WorkflowStatus::Completed => break,
            crate::ai::workflow::runner::WorkflowStatus::Error(e) => {
                let _ = ctx.event_tx.send(AiEvent::WorkflowError {
                    workflow_id: workflow_id.clone(),
                    step_name: None,
                    error: e.clone(),
                });
                return error_result(format!("Workflow execution failed: {}", e));
            }
            crate::ai::workflow::runner::WorkflowStatus::WaitingForInput => {
                let _ = ctx.event_tx.send(AiEvent::WorkflowError {
                    workflow_id: workflow_id.clone(),
                    step_name: None,
                    error: "Workflow waiting for input".to_string(),
                });
                return error_result("Workflow waiting for input");
            }
            crate::ai::workflow::runner::WorkflowStatus::Paused { .. } => {
                continue;
            }
        }
    }

    let total_duration = start_time.elapsed().as_millis() as u64;

    // Emit completion event
    let _ = ctx.event_tx.send(AiEvent::WorkflowCompleted {
        workflow_id: workflow_id.clone(),
        final_output: final_output.clone(),
        total_duration_ms: total_duration,
    });

    (
        json!({
            "workflow_name": workflow_name,
            "session_id": session_id,
            "output": final_output,
            "success": true
        }),
        true,
    )
}

/// Normalize tool arguments for run_pty_cmd.
/// If the command is passed as an array, convert it to a space-joined string.
/// This prevents shell_words::join() from quoting metacharacters like &&, ||, |, etc.
pub fn normalize_run_pty_cmd_args(mut args: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = args.as_object_mut() {
        if let Some(command) = obj.get_mut("command") {
            if let Some(arr) = command.as_array() {
                // Convert array to space-joined string
                let cmd_str: String = arr
                    .iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                *command = serde_json::Value::String(cmd_str);
            }
        }
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_normalize_run_pty_cmd_array_to_string() {
        // Command as array with shell operators
        let args = json!({
            "command": ["cd", "/path", "&&", "pwd"],
            "cwd": "."
        });

        let normalized = normalize_run_pty_cmd_args(args);

        assert_eq!(normalized["command"].as_str().unwrap(), "cd /path && pwd");
        // Other fields should be preserved
        assert_eq!(normalized["cwd"].as_str().unwrap(), ".");
    }

    #[test]
    fn test_normalize_run_pty_cmd_string_unchanged() {
        // Command already as string - should be unchanged
        let args = json!({
            "command": "cd /path && pwd",
            "cwd": "."
        });

        let normalized = normalize_run_pty_cmd_args(args);

        assert_eq!(normalized["command"].as_str().unwrap(), "cd /path && pwd");
    }

    #[test]
    fn test_normalize_run_pty_cmd_pipe_operator() {
        let args = json!({
            "command": ["ls", "-la", "|", "grep", "foo"]
        });

        let normalized = normalize_run_pty_cmd_args(args);

        assert_eq!(normalized["command"].as_str().unwrap(), "ls -la | grep foo");
    }

    #[test]
    fn test_normalize_run_pty_cmd_redirect() {
        let args = json!({
            "command": ["echo", "hello", ">", "output.txt"]
        });

        let normalized = normalize_run_pty_cmd_args(args);

        assert_eq!(
            normalized["command"].as_str().unwrap(),
            "echo hello > output.txt"
        );
    }

    #[test]
    fn test_normalize_run_pty_cmd_empty_array() {
        let args = json!({
            "command": []
        });

        let normalized = normalize_run_pty_cmd_args(args);

        assert_eq!(normalized["command"].as_str().unwrap(), "");
    }

    #[test]
    fn test_normalize_run_pty_cmd_no_command_field() {
        // Args without command field should pass through unchanged
        let args = json!({
            "cwd": "/some/path"
        });

        let normalized = normalize_run_pty_cmd_args(args.clone());

        assert_eq!(normalized, args);
    }
}

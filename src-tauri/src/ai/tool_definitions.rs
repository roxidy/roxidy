//! Tool definitions for the agent system.
//!
//! This module contains tool definitions and schema sanitization logic
//! for various tool types: standard tools, indexer tools, Tavily tools, and sub-agent tools.

use std::collections::HashSet;

use rig::completion::ToolDefinition;
use serde_json::json;
use vtcode_core::tools::registry::build_function_declarations;

use super::sub_agent::SubAgentRegistry;
use crate::tavily::TavilyState;
use std::sync::Arc;

/// Get tool definitions in rig format from vtcode's function declarations.
/// Sanitizes schemas to remove anyOf/allOf/oneOf which Anthropic doesn't support.
/// Also overrides descriptions for specific tools (e.g., run_pty_cmd).
pub fn get_standard_tool_definitions() -> Vec<ToolDefinition> {
    build_function_declarations()
        .into_iter()
        .map(|fd| {
            // Override description for run_pty_cmd to instruct agent not to repeat output
            let description = if fd.name == "run_pty_cmd" {
                format!(
                    "{}. IMPORTANT: The command output is displayed directly in the user's terminal. \
                     Do NOT repeat or summarize the command output in your response - the user can already see it. \
                     Only mention significant errors or ask clarifying questions if needed.",
                    fd.description
                )
            } else {
                fd.description
            };

            ToolDefinition {
                name: fd.name,
                description,
                parameters: sanitize_schema(fd.parameters),
            }
        })
        .collect()
}

/// Get tool definitions for the code indexer.
pub fn get_indexer_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "indexer_search_code".to_string(),
            description: "Search for code patterns using regex in the indexed workspace. Returns matching lines with file paths and line numbers.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for"
                    },
                    "path_filter": {
                        "type": "string",
                        "description": "Optional file path filter (glob pattern)"
                    }
                },
                "required": ["pattern"]
            }),
        },
        ToolDefinition {
            name: "indexer_search_files".to_string(),
            description: "Find files by name pattern (glob-style) in the indexed workspace.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern to match file names (e.g., '*.rs', 'src/**/*.ts')"
                    }
                },
                "required": ["pattern"]
            }),
        },
        ToolDefinition {
            name: "indexer_analyze_file".to_string(),
            description: "Get semantic analysis of a file using tree-sitter. Returns symbols, code metrics, and dependencies.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path to the file to analyze"
                    }
                },
                "required": ["file_path"]
            }),
        },
        ToolDefinition {
            name: "indexer_extract_symbols".to_string(),
            description: "Extract all symbols (functions, classes, structs, variables, imports) from a file.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path to the file to extract symbols from"
                    }
                },
                "required": ["file_path"]
            }),
        },
        ToolDefinition {
            name: "indexer_get_metrics".to_string(),
            description: "Get code metrics for a file: lines of code, comment lines, blank lines, function count, class count, etc.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path to the file to get metrics for"
                    }
                },
                "required": ["file_path"]
            }),
        },
        ToolDefinition {
            name: "indexer_detect_language".to_string(),
            description: "Detect the programming language of a file based on its extension and content.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path to the file"
                    }
                },
                "required": ["file_path"]
            }),
        },
    ]
}

/// Get tool definitions for web search (Tavily).
pub fn get_tavily_tool_definitions(tavily_state: Option<&Arc<TavilyState>>) -> Vec<ToolDefinition> {
    // Only return tools if Tavily is available
    if tavily_state.map(|s| s.is_available()).unwrap_or(false) {
        vec![
            ToolDefinition {
                name: "web_search".to_string(),
                description: "Search the web for information. Returns relevant results with titles, URLs, and content snippets. Use this when you need current information, news, documentation, or facts beyond your training data.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query"
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Maximum number of results to return (default: 5)"
                        }
                    },
                    "required": ["query"]
                }),
            },
            ToolDefinition {
                name: "web_search_answer".to_string(),
                description: "Get an AI-generated answer from web search results. Best for direct questions that need a synthesized answer from multiple sources.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The question to answer"
                        }
                    },
                    "required": ["query"]
                }),
            },
            ToolDefinition {
                name: "web_extract".to_string(),
                description: "Extract and parse content from specific URLs. Use this to get the full content of web pages for deeper analysis.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "urls": {
                            "type": "array",
                            "items": {
                                "type": "string"
                            },
                            "description": "List of URLs to extract content from"
                        }
                    },
                    "required": ["urls"]
                }),
            },
        ]
    } else {
        vec![]
    }
}

/// Get sub-agent tool definitions from the registry.
pub async fn get_sub_agent_tool_definitions(registry: &SubAgentRegistry) -> Vec<ToolDefinition> {
    registry
        .all()
        .map(|agent| ToolDefinition {
            name: format!("sub_agent_{}", agent.id),
            description: format!(
                "[SUB-AGENT: {}] {}",
                agent.name, agent.description
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "The specific task or question for this sub-agent to handle"
                    },
                    "context": {
                        "type": "string",
                        "description": "Optional additional context to help the sub-agent understand the task"
                    }
                },
                "required": ["task"]
            }),
        })
        .collect()
}

/// Get all tool definitions (standard + indexer).
pub fn get_all_tool_definitions() -> Vec<ToolDefinition> {
    let mut tools = get_standard_tool_definitions();
    tools.extend(get_indexer_tool_definitions());
    tools
}

/// Filter tools by allowed set.
pub fn filter_tools_by_allowed(
    tools: Vec<ToolDefinition>,
    allowed_tools: &[String],
) -> Vec<ToolDefinition> {
    if allowed_tools.is_empty() {
        tools
    } else {
        let allowed_set: HashSet<&str> = allowed_tools.iter().map(|s| s.as_str()).collect();
        tools
            .into_iter()
            .filter(|t| allowed_set.contains(t.name.as_str()))
            .collect()
    }
}

/// Remove anyOf, allOf, oneOf from JSON schema as Anthropic doesn't support them.
/// Also simplifies nested oneOf in properties to just use the first option.
pub fn sanitize_schema(mut schema: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = schema.as_object_mut() {
        // Remove top-level anyOf/allOf/oneOf
        obj.remove("anyOf");
        obj.remove("allOf");
        obj.remove("oneOf");

        // Recursively sanitize properties
        if let Some(props) = obj.get_mut("properties") {
            if let Some(props_obj) = props.as_object_mut() {
                for (_key, prop_value) in props_obj.iter_mut() {
                    if let Some(prop_obj) = prop_value.as_object_mut() {
                        // If property has oneOf, replace with first option or simplify to string
                        if prop_obj.contains_key("oneOf") {
                            if let Some(one_of) = prop_obj.remove("oneOf") {
                                if let Some(arr) = one_of.as_array() {
                                    if let Some(first) = arr.first() {
                                        // Merge the first oneOf option into this property
                                        if let Some(first_obj) = first.as_object() {
                                            for (k, v) in first_obj {
                                                prop_obj.insert(k.clone(), v.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        // Remove anyOf/allOf from properties too
                        prop_obj.remove("anyOf");
                        prop_obj.remove("allOf");
                    }
                }
            }
        }
    }
    schema
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_schema_removes_any_of() {
        let schema = json!({
            "type": "object",
            "anyOf": [{"type": "string"}, {"type": "number"}],
            "properties": {
                "name": {"type": "string"}
            }
        });

        let sanitized = sanitize_schema(schema);

        assert!(sanitized.get("anyOf").is_none());
        assert!(sanitized.get("properties").is_some());
    }

    #[test]
    fn test_sanitize_schema_handles_one_of_in_properties() {
        let schema = json!({
            "type": "object",
            "properties": {
                "value": {
                    "oneOf": [
                        {"type": "string"},
                        {"type": "number"}
                    ]
                }
            }
        });

        let sanitized = sanitize_schema(schema);

        let value_prop = sanitized
            .get("properties")
            .and_then(|p| p.get("value"))
            .unwrap();
        assert!(value_prop.get("oneOf").is_none());
        assert_eq!(
            value_prop.get("type").and_then(|t| t.as_str()),
            Some("string")
        );
    }

    #[test]
    fn test_get_standard_tool_definitions() {
        let tools = get_standard_tool_definitions();
        assert!(!tools.is_empty());
    }

    #[test]
    fn test_get_indexer_tool_definitions() {
        let tools = get_indexer_tool_definitions();
        assert_eq!(tools.len(), 6);

        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"indexer_search_code"));
        assert!(tool_names.contains(&"indexer_search_files"));
        assert!(tool_names.contains(&"indexer_analyze_file"));
    }

    #[test]
    fn test_filter_tools_by_allowed() {
        let tools = vec![
            ToolDefinition {
                name: "tool_a".to_string(),
                description: "A".to_string(),
                parameters: json!({}),
            },
            ToolDefinition {
                name: "tool_b".to_string(),
                description: "B".to_string(),
                parameters: json!({}),
            },
            ToolDefinition {
                name: "tool_c".to_string(),
                description: "C".to_string(),
                parameters: json!({}),
            },
        ];

        let allowed = vec!["tool_a".to_string(), "tool_c".to_string()];
        let filtered = filter_tools_by_allowed(tools, &allowed);

        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|t| t.name == "tool_a"));
        assert!(filtered.iter().any(|t| t.name == "tool_c"));
    }

    #[test]
    fn test_filter_tools_empty_allowed() {
        let tools = vec![ToolDefinition {
            name: "tool_a".to_string(),
            description: "A".to_string(),
            parameters: json!({}),
        }];

        let filtered = filter_tools_by_allowed(tools.clone(), &[]);

        assert_eq!(filtered.len(), 1);
    }
}

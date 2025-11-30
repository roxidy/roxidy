//! Tool definitions for the agent system.
//!
//! This module contains tool definitions and schema sanitization logic
//! for various tool types: standard tools, indexer tools, Tavily tools, and sub-agent tools.
//!
//! ## Tool Selection
//!
//! Tools from vtcode-core can be filtered using presets or custom configuration:
//! - `ToolPreset::Minimal` - Essential file operations only
//! - `ToolPreset::Standard` - Core development tools (recommended)
//! - `ToolPreset::Full` - All vtcode tools
//!
//! Use `ToolConfig` to override presets with custom allow/block lists.

use std::collections::HashSet;

use rig::completion::ToolDefinition;
use serde::Deserialize;
use serde_json::json;
use vtcode_core::tools::registry::build_function_declarations;

use super::sub_agent::SubAgentRegistry;
use crate::tavily::TavilyState;
use std::sync::Arc;

/// Tool preset levels for different use cases.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolPreset {
    /// Minimal tools: read, edit, write files + shell command
    Minimal,
    /// Standard tools for most development tasks (default)
    #[default]
    Standard,
    /// Standard tools + indexer tools for code exploration
    Coder,
    /// All tools from vtcode-core
    Full,
}

impl ToolPreset {
    /// Get the list of tool names for this preset.
    pub fn tool_names(&self) -> Option<Vec<&'static str>> {
        match self {
            ToolPreset::Minimal => Some(vec![
                "read_file",
                "edit_file",
                "write_file",
                "run_pty_cmd",
            ]),
            ToolPreset::Standard => Some(vec![
                // Search & discovery
                "grep_file",
                "list_files",
                // File operations
                "read_file",
                "create_file",
                "edit_file",
                "write_file",
                "delete_file",
                // Shell execution
                "run_pty_cmd",
                // Web
                "web_fetch",
                // Planning
                "update_plan",
            ]),
            ToolPreset::Coder => Some(vec![
                // Search & discovery
                "grep_file",
                "list_files",
                // File operations
                "read_file",
                "create_file",
                "edit_file",
                "write_file",
                "delete_file",
                // Shell execution
                "run_pty_cmd",
                // Web
                "web_fetch",
                // Planning
                "update_plan",
                // Indexer tools for code exploration
                "indexer_search_code",
                "indexer_search_files",
                "indexer_analyze_file",
                "indexer_extract_symbols",
                "indexer_get_metrics",
                "indexer_detect_language",
            ]),
            ToolPreset::Full => None, // None means all tools
        }
    }
}

/// Configuration for tool selection with optional overrides.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ToolConfig {
    /// Base preset to use
    #[serde(default)]
    pub preset: ToolPreset,
    /// Additional tools to enable (on top of preset)
    #[serde(default)]
    pub additional: Vec<String>,
    /// Tools to disable (removed from preset)
    #[serde(default)]
    pub disabled: Vec<String>,
}

impl ToolConfig {
    /// Create a new config with the given preset.
    #[allow(dead_code)] // Public API for external configuration
    pub fn with_preset(preset: ToolPreset) -> Self {
        Self {
            preset,
            additional: vec![],
            disabled: vec![],
        }
    }

    /// Create the default tool config for the main agent.
    ///
    /// This is the recommended configuration for qbit's primary AI agent.
    /// It uses the Standard preset with additional tools that are useful
    /// for the main agent but not sub-agents.
    pub fn main_agent() -> Self {
        Self {
            preset: ToolPreset::Standard,
            additional: vec![
                // Code execution for complex operations
                "execute_code".to_string(),
                // Patch-based editing for large changes
                "apply_patch".to_string(),
            ],
            disabled: vec![
                // Sub-agents are disabled for the main agent
                "sub_agent_researcher".to_string(),
                "sub_agent_shell_executor".to_string(),
                "sub_agent_test_runner".to_string(),
                "sub_agent_code_analyzer".to_string(),
                "sub_agent_code_writer".to_string(),
            ],
        }
    }

    /// Check if a tool name is enabled by this config.
    pub fn is_tool_enabled(&self, tool_name: &str) -> bool {
        // Check disabled list first
        if self.disabled.iter().any(|t| t == tool_name) {
            return false;
        }

        // Check additional list
        if self.additional.iter().any(|t| t == tool_name) {
            return true;
        }

        // Check preset
        match self.preset.tool_names() {
            Some(names) => names.contains(&tool_name),
            None => true, // Full preset allows all
        }
    }
}

/// Get tool definitions using the default Standard preset.
///
/// This is the recommended entry point for most use cases.
/// Uses `ToolPreset::Standard` which includes core development tools.
#[allow(dead_code)] // Public API - used externally or for future internal use
pub fn get_standard_tool_definitions() -> Vec<ToolDefinition> {
    get_tool_definitions_with_config(&ToolConfig::default())
}

/// Get tool definitions with a specific preset.
#[allow(dead_code)] // Public API - used externally or for future internal use
pub fn get_tool_definitions_for_preset(preset: ToolPreset) -> Vec<ToolDefinition> {
    get_tool_definitions_with_config(&ToolConfig::with_preset(preset))
}

/// Get tool definitions with full configuration control.
///
/// Filters vtcode's function declarations based on the provided config,
/// sanitizes schemas for Anthropic compatibility, and applies description overrides.
pub fn get_tool_definitions_with_config(config: &ToolConfig) -> Vec<ToolDefinition> {
    build_function_declarations()
        .into_iter()
        .filter(|fd| config.is_tool_enabled(&fd.name))
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

/// Get all tool definitions (standard + indexer) using the default preset.
pub fn get_all_tool_definitions() -> Vec<ToolDefinition> {
    get_all_tool_definitions_with_config(&ToolConfig::default())
}

/// Get all tool definitions with configuration control.
pub fn get_all_tool_definitions_with_config(config: &ToolConfig) -> Vec<ToolDefinition> {
    let mut tools = get_tool_definitions_with_config(config);
    tools.extend(
        get_indexer_tool_definitions()
            .into_iter()
            .filter(|t| config.is_tool_enabled(&t.name)),
    );
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

    #[test]
    fn test_tool_preset_minimal() {
        let preset = ToolPreset::Minimal;
        let names = preset.tool_names().unwrap();

        assert_eq!(names.len(), 4);
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"edit_file"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"run_pty_cmd"));
    }

    #[test]
    fn test_tool_preset_standard() {
        let preset = ToolPreset::Standard;
        let names = preset.tool_names().unwrap();

        // Should have core tools
        assert!(names.contains(&"grep_file"));
        assert!(names.contains(&"list_files"));
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"edit_file"));
        assert!(names.contains(&"run_pty_cmd"));
        assert!(names.contains(&"web_fetch"));

        // Should NOT have skill tools or PTY session management
        assert!(!names.contains(&"save_skill"));
        assert!(!names.contains(&"create_pty_session"));
    }

    #[test]
    fn test_tool_preset_coder() {
        let preset = ToolPreset::Coder;
        let names = preset.tool_names().unwrap();

        // Should have all standard tools
        assert!(names.contains(&"grep_file"));
        assert!(names.contains(&"list_files"));
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"edit_file"));
        assert!(names.contains(&"run_pty_cmd"));
        assert!(names.contains(&"web_fetch"));

        // Should also have indexer tools
        assert!(names.contains(&"indexer_search_code"));
        assert!(names.contains(&"indexer_search_files"));
        assert!(names.contains(&"indexer_analyze_file"));
        assert!(names.contains(&"indexer_extract_symbols"));
        assert!(names.contains(&"indexer_get_metrics"));
        assert!(names.contains(&"indexer_detect_language"));
    }

    #[test]
    fn test_tool_preset_full() {
        let preset = ToolPreset::Full;
        // Full preset returns None (meaning all tools)
        assert!(preset.tool_names().is_none());
    }

    #[test]
    fn test_tool_config_default_is_standard() {
        let config = ToolConfig::default();
        assert_eq!(config.preset, ToolPreset::Standard);
    }

    #[test]
    fn test_tool_config_is_tool_enabled() {
        let config = ToolConfig::with_preset(ToolPreset::Standard);

        // Standard tools should be enabled
        assert!(config.is_tool_enabled("read_file"));
        assert!(config.is_tool_enabled("grep_file"));

        // Non-standard tools should be disabled
        assert!(!config.is_tool_enabled("save_skill"));
        assert!(!config.is_tool_enabled("create_pty_session"));
    }

    #[test]
    fn test_tool_config_additional_tools() {
        let config = ToolConfig {
            preset: ToolPreset::Minimal,
            additional: vec!["grep_file".to_string()],
            disabled: vec![],
        };

        // Minimal preset tools
        assert!(config.is_tool_enabled("read_file"));
        // Additional tool
        assert!(config.is_tool_enabled("grep_file"));
        // Not in minimal or additional
        assert!(!config.is_tool_enabled("web_fetch"));
    }

    #[test]
    fn test_tool_config_disabled_tools() {
        let config = ToolConfig {
            preset: ToolPreset::Standard,
            additional: vec![],
            disabled: vec!["delete_file".to_string()],
        };

        // Standard tool that's not disabled
        assert!(config.is_tool_enabled("read_file"));
        // Disabled even though in preset
        assert!(!config.is_tool_enabled("delete_file"));
    }

    #[test]
    fn test_tool_config_disabled_overrides_additional() {
        let config = ToolConfig {
            preset: ToolPreset::Minimal,
            additional: vec!["grep_file".to_string()],
            disabled: vec!["grep_file".to_string()],
        };

        // Disabled takes precedence over additional
        assert!(!config.is_tool_enabled("grep_file"));
    }

    #[test]
    fn test_get_tool_definitions_for_preset_minimal() {
        let tools = get_tool_definitions_for_preset(ToolPreset::Minimal);
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

        assert_eq!(tools.len(), 4);
        assert!(tool_names.contains(&"read_file"));
        assert!(tool_names.contains(&"edit_file"));
        assert!(tool_names.contains(&"write_file"));
        assert!(tool_names.contains(&"run_pty_cmd"));
    }

    #[test]
    fn test_get_tool_definitions_for_preset_full() {
        let full_tools = get_tool_definitions_for_preset(ToolPreset::Full);
        let standard_tools = get_tool_definitions_for_preset(ToolPreset::Standard);

        // Full should have more tools than standard
        assert!(full_tools.len() > standard_tools.len());
    }

    #[test]
    fn test_tool_config_with_config() {
        let config = ToolConfig {
            preset: ToolPreset::Minimal,
            additional: vec!["grep_file".to_string(), "list_files".to_string()],
            disabled: vec![],
        };

        let tools = get_tool_definitions_with_config(&config);
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

        // Minimal preset (4) + additional (2) = 6
        assert_eq!(tools.len(), 6);
        assert!(tool_names.contains(&"read_file"));
        assert!(tool_names.contains(&"grep_file"));
        assert!(tool_names.contains(&"list_files"));
    }

    #[test]
    fn test_tool_config_main_agent() {
        let config = ToolConfig::main_agent();

        // Should be based on Standard preset
        assert_eq!(config.preset, ToolPreset::Standard);

        // Should have additional tools
        assert!(config.additional.contains(&"execute_code".to_string()));
        assert!(config.additional.contains(&"apply_patch".to_string()));

        // Should have sub-agents disabled
        assert!(config.disabled.contains(&"sub_agent_researcher".to_string()));
        assert!(config.disabled.contains(&"sub_agent_shell_executor".to_string()));
        assert!(config.disabled.contains(&"sub_agent_test_runner".to_string()));
        assert!(config.disabled.contains(&"sub_agent_code_analyzer".to_string()));
        assert!(config.disabled.contains(&"sub_agent_code_writer".to_string()));

        // Verify the tools are actually enabled
        assert!(config.is_tool_enabled("read_file")); // From Standard
        assert!(config.is_tool_enabled("grep_file")); // From Standard
        assert!(config.is_tool_enabled("execute_code")); // From additional
        assert!(config.is_tool_enabled("apply_patch")); // From additional

        // Verify sub-agents are disabled
        assert!(!config.is_tool_enabled("sub_agent_researcher"));
        assert!(!config.is_tool_enabled("sub_agent_code_writer"));

        // Verify non-standard tools are still disabled
        assert!(!config.is_tool_enabled("save_skill"));
        assert!(!config.is_tool_enabled("create_pty_session"));
    }

    #[test]
    fn test_main_agent_tool_definitions() {
        let config = ToolConfig::main_agent();
        let tools = get_tool_definitions_with_config(&config);
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

        // Should have Standard preset tools + additional
        assert!(tool_names.contains(&"grep_file"));
        assert!(tool_names.contains(&"read_file"));
        assert!(tool_names.contains(&"edit_file"));
        assert!(tool_names.contains(&"run_pty_cmd"));
        assert!(tool_names.contains(&"execute_code"));
        assert!(tool_names.contains(&"apply_patch"));

        // Should NOT have skill tools or PTY session management
        assert!(!tool_names.contains(&"save_skill"));
        assert!(!tool_names.contains(&"create_pty_session"));
    }
}

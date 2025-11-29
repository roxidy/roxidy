# Tree-Sitter Integration Plan for Qbit

## Overview

Integrate vtcode-core's tree_sitter module into qbit to provide semantic code analysis capabilities for the AI agent.

## Current State

- **vtcode-core** (v0.47) contains a full tree_sitter module at `src/tools/tree_sitter/`
- Qbit uses vtcode-core and automatically discovers tools via `build_function_declarations()`
- The tree_sitter analyzer is exposed as a **library component**, not as a registered tool in vtcode-core's tool registry
- Qbit's `AgentBridge` wraps vtcode-core's `ToolRegistry` for tool execution

## vtcode tree_sitter Module Structure

```
vtcode-core/src/tools/tree_sitter/
├── mod.rs              # Module declarations and re-exports
├── analyzer.rs         # Core TreeSitterAnalyzer implementation
├── languages.rs        # Language-specific query definitions
├── analysis.rs         # Code analysis and metrics
├── cache.rs            # LRU AST caching system
├── highlighting.rs     # Syntax highlighting (scaffolding)
├── navigation.rs       # Code navigation capabilities
└── refactoring.rs      # Code refactoring operations
```

## Capabilities Provided by tree_sitter

| Capability | Description |
|------------|-------------|
| **Parsing** | Parse source code into ASTs for 8 languages (Rust, Python, JS/TS, Go, Java, Bash, Swift) |
| **Symbol Extraction** | Identify functions, classes, variables, imports |
| **Code Metrics** | LOC, comment ratio, cyclomatic/cognitive complexity |
| **Navigation** | Go to definition, find references, scope hierarchy |
| **Refactoring** | Rename, extract function/variable, inline, etc. |
| **Caching** | LRU AST cache (128 entries) for performance |

---

## Integration Strategy

### Approach: Direct Library Integration

Use `vtcode_core::TreeSitterAnalyzer` directly in qbit, wrapping it as a custom tool registered with the agent.

**Rationale:**
- vtcode-core already provides the full implementation
- No need to wait for upstream vtcode-core changes
- Full control over tool schema and behavior in qbit

---

## Implementation Plan

### Phase 1: Dependencies

**File:** `src-tauri/Cargo.toml`

Add tree-sitter crates:

```toml
[dependencies]
# Tree-sitter core
tree-sitter = "0.25"

# Language grammars (match vtcode-core versions)
tree-sitter-rust = "0.24"
tree-sitter-python = "0.25"
tree-sitter-javascript = "0.25"
tree-sitter-typescript = "0.23"
tree-sitter-go = "0.25"
tree-sitter-java = "0.23"
tree-sitter-bash = "0.25"

# Optional
tree-sitter-swift = { version = "0.7.1", optional = true }
```

Add feature flag:

```toml
[features]
default = []
swift = ["tree-sitter-swift"]
```

---

### Phase 2: Create Tree-Sitter Tool Module

**File:** `src-tauri/src/ai/tree_sitter_tool.rs`

```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use vtcode_core::TreeSitterAnalyzer;

/// Custom tool wrapper for tree-sitter code analysis
pub struct TreeSitterTool {
    analyzer: TreeSitterAnalyzer,
    workspace: PathBuf,
}

impl TreeSitterTool {
    pub async fn new(workspace: PathBuf) -> Result<Self> {
        let analyzer = TreeSitterAnalyzer::new()?;
        Ok(Self { analyzer, workspace })
    }

    /// Execute a tree-sitter operation
    pub async fn execute(&mut self, operation: &str, args: Value) -> Result<Value> {
        match operation {
            "analyze_code" => self.analyze_code(args).await,
            "find_symbols" => self.find_symbols(args).await,
            "get_code_structure" => self.get_code_structure(args).await,
            "find_references" => self.find_references(args).await,
            "get_complexity" => self.get_complexity(args).await,
            _ => Err(anyhow::anyhow!("Unknown operation: {}", operation)),
        }
    }

    async fn analyze_code(&mut self, args: Value) -> Result<Value> {
        // Implementation using self.analyzer
        todo!()
    }

    async fn find_symbols(&mut self, args: Value) -> Result<Value> {
        todo!()
    }

    async fn get_code_structure(&mut self, args: Value) -> Result<Value> {
        todo!()
    }

    async fn find_references(&mut self, args: Value) -> Result<Value> {
        todo!()
    }

    async fn get_complexity(&mut self, args: Value) -> Result<Value> {
        todo!()
    }
}
```

---

### Phase 3: Tool Definitions

Define JSON schemas for each operation:

#### `analyze_code`
```json
{
  "name": "analyze_code",
  "description": "Analyze source code using tree-sitter. Extracts symbols (functions, classes, variables), calculates metrics (LOC, complexity), and identifies code structure.",
  "parameters": {
    "type": "object",
    "properties": {
      "file_path": {
        "type": "string",
        "description": "Path to the file to analyze (relative to workspace)"
      },
      "include_metrics": {
        "type": "boolean",
        "description": "Include code metrics (LOC, complexity)",
        "default": true
      },
      "include_symbols": {
        "type": "boolean",
        "description": "Include extracted symbols",
        "default": true
      },
      "include_dependencies": {
        "type": "boolean",
        "description": "Include import/dependency analysis",
        "default": false
      }
    },
    "required": ["file_path"]
  }
}
```

#### `find_symbols`
```json
{
  "name": "find_symbols",
  "description": "Search for symbols (functions, classes, variables) in a file or directory by name pattern.",
  "parameters": {
    "type": "object",
    "properties": {
      "path": {
        "type": "string",
        "description": "File or directory path to search"
      },
      "pattern": {
        "type": "string",
        "description": "Symbol name pattern (supports regex)"
      },
      "kind": {
        "type": "string",
        "enum": ["function", "class", "variable", "import", "all"],
        "description": "Filter by symbol kind",
        "default": "all"
      }
    },
    "required": ["path", "pattern"]
  }
}
```

#### `get_code_structure`
```json
{
  "name": "get_code_structure",
  "description": "Get a high-level overview of a file's structure including all top-level definitions.",
  "parameters": {
    "type": "object",
    "properties": {
      "file_path": {
        "type": "string",
        "description": "Path to the file"
      },
      "max_depth": {
        "type": "integer",
        "description": "Maximum nesting depth to show",
        "default": 2
      }
    },
    "required": ["file_path"]
  }
}
```

#### `find_references`
```json
{
  "name": "find_references",
  "description": "Find all references to a symbol within a file or across the workspace.",
  "parameters": {
    "type": "object",
    "properties": {
      "symbol_name": {
        "type": "string",
        "description": "Name of the symbol to find references for"
      },
      "file_path": {
        "type": "string",
        "description": "File containing the symbol definition"
      },
      "search_scope": {
        "type": "string",
        "enum": ["file", "workspace"],
        "description": "Scope of the search",
        "default": "file"
      }
    },
    "required": ["symbol_name", "file_path"]
  }
}
```

#### `get_complexity`
```json
{
  "name": "get_complexity",
  "description": "Calculate complexity metrics for a file or specific function.",
  "parameters": {
    "type": "object",
    "properties": {
      "file_path": {
        "type": "string",
        "description": "Path to the file"
      },
      "function_name": {
        "type": "string",
        "description": "Optional: specific function to analyze"
      }
    },
    "required": ["file_path"]
  }
}
```

---

### Phase 4: Integration into AgentBridge

**File:** `src-tauri/src/ai/agent_bridge.rs`

#### 4.1 Add field to AgentBridge struct

```rust
pub struct AgentBridge {
    // ... existing fields ...
    tree_sitter_tool: Arc<RwLock<TreeSitterTool>>,
}
```

#### 4.2 Initialize in `AgentBridge::new()`

```rust
impl AgentBridge {
    pub async fn new(/* ... */) -> Result<Self> {
        // ... existing initialization ...

        let tree_sitter_tool = Arc::new(RwLock::new(
            TreeSitterTool::new(workspace.clone()).await?
        ));

        Ok(Self {
            // ... existing fields ...
            tree_sitter_tool,
        })
    }
}
```

#### 4.3 Add tool definitions to `get_tool_definitions()`

```rust
fn get_tool_definitions() -> Vec<ToolDefinition> {
    let mut tools = build_function_declarations()
        .into_iter()
        .map(|fd| /* existing mapping */)
        .collect::<Vec<_>>();

    // Add tree-sitter tools
    tools.extend(Self::get_tree_sitter_tool_definitions());

    tools
}

fn get_tree_sitter_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "analyze_code".to_string(),
            description: "Analyze source code using tree-sitter...".to_string(),
            parameters: json!({ /* schema */ }),
        },
        // ... other tools ...
    ]
}
```

#### 4.4 Handle execution in tool loop

```rust
// In the tool execution section
if Self::is_tree_sitter_tool(&tool_name) {
    let mut ts_tool = self.tree_sitter_tool.write().await;
    let result = ts_tool.execute(&tool_name, tool_args).await;
    // ... handle result ...
} else {
    // Existing vtcode-core tool execution
    let mut registry = self.tool_registry.write().await;
    let result = registry.execute_tool(&tool_name, tool_args).await;
}

fn is_tree_sitter_tool(name: &str) -> bool {
    matches!(name,
        "analyze_code" | "find_symbols" | "get_code_structure" |
        "find_references" | "get_complexity"
    )
}
```

---

### Phase 5: Module Exports

**File:** `src-tauri/src/ai/mod.rs`

```rust
pub mod agent_bridge;
pub mod commands;
pub mod events;
pub mod sub_agent;
pub mod tree_sitter_tool;  // Add this
pub mod workflow;

pub use agent_bridge::AgentBridge;
pub use commands::*;
pub use events::*;
pub use tree_sitter_tool::TreeSitterTool;  // Add this
```

---

## File Summary

| File | Action | Description |
|------|--------|-------------|
| `src-tauri/Cargo.toml` | Modify | Add tree-sitter dependencies |
| `src-tauri/src/ai/tree_sitter_tool.rs` | Create | Custom tool wrapper |
| `src-tauri/src/ai/mod.rs` | Modify | Export new module |
| `src-tauri/src/ai/agent_bridge.rs` | Modify | Register and execute tool |

---

## Testing Plan

1. **Unit Tests:** Test each tool operation with sample code files
2. **Integration Test:** Send agent prompt asking to analyze a file
3. **Multi-language Test:** Verify parsing works for Rust, Python, JS, TS

Example test prompt:
> "Analyze the structure of `src-tauri/src/ai/agent_bridge.rs` and tell me about its main functions and complexity"

---

## Future Enhancements

- [ ] Add workspace-wide symbol search
- [ ] Implement go-to-definition navigation
- [ ] Add refactoring suggestions tool
- [ ] Cache parsed ASTs across agent sessions
- [ ] Support for additional languages via feature flags

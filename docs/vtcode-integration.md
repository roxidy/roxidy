# Integrating vtcode-core into Roxidy (Without TUI)

This document outlines how to integrate the `vtcode-core` library into Roxidy to gain AI agent capabilities without using vtcode's terminal UI (since Roxidy already has its own xterm.js-based terminal).

## Overview

[vtcode](https://github.com/vinhnx/vtcode) is a Rust-based terminal coding agent that provides:

- **Multi-provider LLM support**: OpenAI, Anthropic (Claude), Google Gemini, DeepSeek, xAI (Grok), Ollama, OpenRouter
- **Semantic code intelligence**: Tree-sitter parsers for Rust, Python, JavaScript/TypeScript, Go, Java, Swift, Bash
- **Tool system**: File operations, shell execution, code manipulation, MCP (Model Context Protocol) integration
- **Security**: Workspace boundaries, tool policies, human-in-the-loop approvals
- **Context management**: Token budgeting, conversation trimming, prompt caching

We integrate `vtcode-core` as a library to leverage these capabilities while using Roxidy's own UI.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Roxidy Application                          │
├─────────────────────────────────────────────────────────────────────┤
│  React Frontend (xterm.js)                                          │
│  ├── Terminal UI (existing implementation)                          │
│  ├── AI Chat Panel                                                  │
│  └── Tool Approval Dialogs                                          │
├─────────────────────────────────────────────────────────────────────┤
│  Tauri Backend (Rust)                                               │
│  ├── Terminal Sessions (portable-pty) ◄── Existing code             │
│  ├── AI Agent Bridge ◄────────────────── vtcode integration         │
│  │   ├── AgentComponentBuilder                                      │
│  │   ├── LLM Client (streaming)                                     │
│  │   ├── ToolRegistry                                               │
│  │   └── EventSink → Tauri Events                                   │
│  └── File System Operations                                         │
├─────────────────────────────────────────────────────────────────────┤
│  vtcode-core (library)                                              │
│  ├── LLM Providers (OpenAI, Anthropic, Gemini, etc.)               │
│  ├── Tool System (file ops, grep, code analysis)                   │
│  ├── Context Management (token budgeting, trimming)                │
│  ├── Tree-sitter (semantic code analysis)                          │
│  └── EventSink (ThreadEvent streaming)                              │
└─────────────────────────────────────────────────────────────────────┘
```

## Step 1: Add Dependencies

Update `src-tauri/Cargo.toml`:

```toml
[dependencies]
# ... existing deps ...

# VTCode core library - use git since it may not be published to crates.io
vtcode-core = { git = "https://github.com/vinhnx/vtcode", branch = "main" }

# For streaming responses
futures = "0.3"

# For UUID generation (turn IDs, request IDs)
uuid = { version = "1.0", features = ["v4"] }
```

> **Note:** vtcode may not be published to crates.io yet. Use the git dependency as shown above. You may need to check if vtcode-core has feature flags to exclude the TUI. If not, the TUI deps (ratatui, crossterm) will be included but unused.

> **Alternative:** If vtcode is published, you can use version syntax:
> ```toml
> vtcode-core = { version = "0.47", default-features = false }
> ```

## Step 2: Create the Agent Bridge Module

Create the module structure:

```
src-tauri/src/
├── ai/
│   ├── mod.rs
│   ├── agent_bridge.rs
│   ├── events.rs
│   └── commands.rs
├── main.rs
└── ...
```

### `src-tauri/src/ai/mod.rs`

```rust
pub mod agent_bridge;
pub mod commands;
pub mod events;

pub use commands::AiState;
```

### `src-tauri/src/ai/agent_bridge.rs`

```rust
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use anyhow::Result;
use futures::StreamExt;
use vtcode_core::{
    llm::{make_client, AnyClient, LLMStreamEvent},
    tools::ToolRegistry,
    utils::dot_config::{ProviderConfig, ProviderConfigs},
};

use super::events::AiEvent;

/// Bridge between Roxidy and vtcode-core agent
pub struct AgentBridge {
    workspace: PathBuf,
    provider_name: String,
    model_name: String,
    // ToolRegistry requires &mut self for execute_tool, so we need RwLock
    tool_registry: Arc<RwLock<ToolRegistry>>,
    client: AnyClient,
    event_tx: mpsc::UnboundedSender<AiEvent>,
}

impl AgentBridge {
    /// Create a new AgentBridge (sync - ToolRegistry::new is not async)
    pub fn new(
        workspace: PathBuf,
        provider: &str,
        model: &str,
        api_key: &str,
        event_tx: mpsc::UnboundedSender<AiEvent>,
    ) -> Result<Self> {
        // Build provider config with Option<String> fields
        let providers = build_provider_config(provider, model, api_key)?;

        // Create LLM client
        let client = make_client(&providers, provider)?;

        // Create tool registry (NOT async)
        let tool_registry = Arc::new(RwLock::new(ToolRegistry::new(workspace.clone())));

        Ok(Self {
            workspace,
            provider_name: provider.to_string(),
            model_name: model.to_string(),
            tool_registry,
            client,
            event_tx,
        })
    }

    /// Execute a prompt and stream events back
    pub async fn execute(&self, prompt: &str) -> Result<String> {
        // Construct message with struct fields (no helper method)
        let messages = vec![
            vtcode_core::llm::types::Message {
                role: "user".to_string(),
                content: prompt.to_string(),
            },
        ];

        // Emit turn started event
        let turn_id = uuid::Uuid::new_v4().to_string();
        let _ = self.event_tx.send(AiEvent::Started {
            turn_id: turn_id.clone(),
        });

        // Stream response - returns LLMStreamEvent enum variants
        let mut stream = self.client.chat_stream(&messages, None).await?;
        let mut full_response = String::new();

        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(LLMStreamEvent::Token { delta }) => {
                    full_response.push_str(&delta);

                    // Emit streaming event
                    let _ = self.event_tx.send(AiEvent::TextDelta {
                        delta: delta.clone(),
                        accumulated: full_response.clone(),
                    });
                }
                Ok(LLMStreamEvent::Reasoning { delta }) => {
                    // Emit reasoning event for models that support it
                    let _ = self.event_tx.send(AiEvent::Reasoning {
                        content: delta,
                    });
                }
                Ok(LLMStreamEvent::Completed { response }) => {
                    // Use the final response content if available
                    if let Some(content) = response.content {
                        if !content.is_empty() {
                            full_response = content;
                        }
                    }
                }
                Err(e) => {
                    let _ = self.event_tx.send(AiEvent::Error {
                        message: e.to_string(),
                        error_type: "llm_error".to_string(),
                    });
                    return Err(e.into());
                }
            }
        }

        // Emit turn completed event
        let _ = self.event_tx.send(AiEvent::Completed {
            response: full_response.clone(),
            tokens_used: None,
            duration_ms: None,
        });

        Ok(full_response)
    }

    /// Execute a tool by name
    /// Note: execute_tool requires &mut self, hence RwLock
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let request_id = uuid::Uuid::new_v4().to_string();

        // Emit tool request event
        let _ = self.event_tx.send(AiEvent::ToolRequest {
            tool_name: tool_name.to_string(),
            args: args.clone(),
            request_id: request_id.clone(),
        });

        // execute_tool requires &mut self
        let mut registry = self.tool_registry.write().await;
        let result = registry.execute_tool(tool_name, args).await;

        // Emit tool result event
        let (result_value, success) = match &result {
            Ok(v) => (v.clone(), true),
            Err(e) => (serde_json::json!({ "error": e.to_string() }), false),
        };

        let _ = self.event_tx.send(AiEvent::ToolResult {
            tool_name: tool_name.to_string(),
            result: result_value,
            success,
            request_id,
        });

        result
    }

    /// Get available tools for the LLM
    /// Returns Vec<ToolRegistration>, not ToolDefinition
    pub async fn available_tools(&self) -> Vec<vtcode_core::tools::ToolRegistration> {
        let registry = self.tool_registry.read().await;
        registry.available_tools()
    }

    /// Get the workspace path
    pub fn workspace(&self) -> &std::path::Path {
        &self.workspace
    }

    /// Get provider name
    pub fn provider(&self) -> &str {
        &self.provider_name
    }

    /// Get model name
    pub fn model(&self) -> &str {
        &self.model_name
    }
}

fn build_provider_config(
    provider: &str,
    model: &str,
    api_key: &str,
) -> Result<ProviderConfigs> {
    // ProviderConfig fields are Option<String>, not String
    let config = ProviderConfig {
        api_key: Some(api_key.to_string()),
        model: Some(model.to_string()),
        enabled: true,
        ..Default::default()
    };

    let mut providers = ProviderConfigs::default();
    match provider {
        "openai" => providers.openai = Some(config),
        "anthropic" => providers.anthropic = Some(config),
        "gemini" => providers.gemini = Some(config),
        "deepseek" => providers.deepseek = Some(config),
        "ollama" => providers.ollama = Some(config),
        "openrouter" => providers.openrouter = Some(config),
        "xai" => providers.xai = Some(config),
        "lmstudio" => providers.lmstudio = Some(config),
        _ => anyhow::bail!("Unknown provider: {}", provider),
    }

    Ok(providers)
}
```

### `src-tauri/src/ai/events.rs`

```rust
use serde::{Deserialize, Serialize};

// Note: We emit AiEvent directly from AgentBridge instead of converting
// from ThreadEvent, since ThreadEvent uses tuple structs that are harder
// to work with. This gives us more control over the event format.

/// Simplified events for the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AiEvent {
    /// Agent started processing
    Started {
        turn_id: String,
    },

    /// Streaming text chunk
    TextDelta {
        delta: String,
        accumulated: String,
    },

    /// Tool execution requested (for approval UI)
    ToolRequest {
        tool_name: String,
        args: serde_json::Value,
        request_id: String,
    },

    /// Tool execution completed
    ToolResult {
        tool_name: String,
        result: serde_json::Value,
        success: bool,
        request_id: String,
    },

    /// Agent reasoning/thinking (for models that support it)
    Reasoning {
        content: String,
    },

    /// Turn completed
    Completed {
        response: String,
        tokens_used: Option<u32>,
        duration_ms: Option<u64>,
    },

    /// Error occurred
    Error {
        message: String,
        error_type: String,
    },
}

// Optional: If you need to convert from vtcode's ThreadEvent
// Note: ThreadEvent variants use tuple structs, not named fields
//
// Example ThreadEvent variants:
//   ThreadEvent::TurnStarted(TurnStartedEvent)
//   ThreadEvent::ItemUpdated(ItemUpdatedEvent)
//   ThreadEvent::Error(ErrorItem)
//
// You would need to access the inner struct fields:
//
// impl From<vtcode_exec_events::ThreadEvent> for AiEvent {
//     fn from(event: vtcode_exec_events::ThreadEvent) -> Self {
//         use vtcode_exec_events::ThreadEvent;
//         match event {
//             ThreadEvent::TurnStarted(e) => AiEvent::Started {
//                 turn_id: format!("turn-{}", e.turn_number),
//             },
//             ThreadEvent::ItemUpdated(e) => AiEvent::TextDelta {
//                 delta: e.delta,
//                 accumulated: e.accumulated,
//             },
//             ThreadEvent::TurnCompleted(e) => AiEvent::Completed {
//                 response: String::new(),
//                 tokens_used: e.total_tokens,
//                 duration_ms: e.duration_ms,
//             },
//             ThreadEvent::TurnFailed(e) => AiEvent::Error {
//                 message: e.error,
//                 error_type: "turn_failed".to_string(),
//             },
//             ThreadEvent::Error(e) => AiEvent::Error {
//                 message: e.message,
//                 error_type: e.error_type,
//             },
//             ThreadEvent::Reasoning(e) => AiEvent::Reasoning {
//                 content: e.content,
//             },
//             // Handle other variants as needed
//             _ => AiEvent::Error {
//                 message: "Unhandled event type".to_string(),
//                 error_type: "unknown".to_string(),
//             },
//         }
//     }
// }
```

### `src-tauri/src/ai/commands.rs`

```rust
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::{mpsc, RwLock};

use super::agent_bridge::AgentBridge;
use super::events::AiEvent;

/// Shared AI state managed by Tauri
/// Uses tokio RwLock for async compatibility with AgentBridge methods
pub struct AiState {
    pub bridge: Arc<RwLock<Option<AgentBridge>>>,
}

impl Default for AiState {
    fn default() -> Self {
        Self {
            bridge: Arc::new(RwLock::new(None)),
        }
    }
}

/// Initialize the AI agent with the specified configuration
#[tauri::command]
pub async fn init_ai_agent(
    state: State<'_, AiState>,
    app: AppHandle,
    workspace: String,
    provider: String,
    model: String,
    api_key: String,
) -> Result<(), String> {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AiEvent>();

    // Spawn event forwarder to frontend
    let app_clone = app.clone();
    tokio::spawn(async move {
        while let Some(ai_event) = event_rx.recv().await {
            // Events are already AiEvent, no conversion needed
            if let Err(e) = app_clone.emit("ai-event", &ai_event) {
                tracing::error!("Failed to emit AI event: {}", e);
            }
        }
    });

    // AgentBridge::new is sync (not async)
    let bridge = AgentBridge::new(
        workspace.into(),
        &provider,
        &model,
        &api_key,
        event_tx,
    )
    .map_err(|e| e.to_string())?;

    *state.bridge.write().await = Some(bridge);

    tracing::info!("AI agent initialized with provider: {}, model: {}", provider, model);
    Ok(())
}

/// Send a prompt to the AI agent and receive streaming response via events
#[tauri::command]
pub async fn send_ai_prompt(
    state: State<'_, AiState>,
    prompt: String,
) -> Result<String, String> {
    let bridge_guard = state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge.execute(&prompt).await.map_err(|e| e.to_string())
}

/// Execute a specific tool with the given arguments
#[tauri::command]
pub async fn execute_ai_tool(
    state: State<'_, AiState>,
    tool_name: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let bridge_guard = state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    bridge
        .execute_tool(&tool_name, args)
        .await
        .map_err(|e| e.to_string())
}

/// Get the list of available tools
/// Note: This is now async because available_tools needs to acquire a read lock
#[tauri::command]
pub async fn get_available_tools(state: State<'_, AiState>) -> Result<Vec<serde_json::Value>, String> {
    let bridge_guard = state.bridge.read().await;
    let bridge = bridge_guard
        .as_ref()
        .ok_or("AI agent not initialized. Call init_ai_agent first.")?;

    let tools = bridge.available_tools().await;
    let tools_json: Vec<serde_json::Value> = tools
        .into_iter()
        .filter_map(|t| serde_json::to_value(t).ok())
        .collect();

    Ok(tools_json)
}

/// Shutdown the AI agent and cleanup resources
#[tauri::command]
pub async fn shutdown_ai_agent(state: State<'_, AiState>) -> Result<(), String> {
    let mut bridge_guard = state.bridge.write().await;
    *bridge_guard = None;
    tracing::info!("AI agent shut down");
    Ok(())
}
```

## Step 3: Register in main.rs

Update `src-tauri/src/main.rs`:

```rust
mod ai;

use ai::{
    AiState,
    commands::{
        init_ai_agent,
        send_ai_prompt,
        execute_ai_tool,
        get_available_tools,
        shutdown_ai_agent,
    },
};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AiState::default())
        .invoke_handler(tauri::generate_handler![
            // ... your existing commands ...
            init_ai_agent,
            send_ai_prompt,
            execute_ai_tool,
            get_available_tools,
            shutdown_ai_agent,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

## Step 4: Frontend Integration

### `src/lib/ai.ts`

```typescript
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type AiProvider =
  | "openai"
  | "anthropic"
  | "gemini"
  | "deepseek"
  | "ollama"
  | "openrouter";

export interface AiConfig {
  workspace: string;
  provider: AiProvider;
  model: string;
  apiKey: string;
}

export type AiEvent =
  | { type: "started"; turn_id: string }
  | { type: "text_delta"; delta: string; accumulated: string }
  | {
      type: "tool_request";
      tool_name: string;
      args: unknown;
      request_id: string;
    }
  | {
      type: "tool_result";
      tool_name: string;
      result: unknown;
      success: boolean;
      request_id: string;
    }
  | { type: "reasoning"; content: string }
  | {
      type: "completed";
      response: string;
      tokens_used?: number;
      duration_ms?: number;
    }
  | { type: "error"; message: string; error_type: string };

export interface ToolDefinition {
  name: string;
  description: string;
  parameters: Record<string, unknown>;
}

/**
 * Initialize the AI agent with the specified configuration
 */
export async function initAiAgent(config: AiConfig): Promise<void> {
  return invoke("init_ai_agent", {
    workspace: config.workspace,
    provider: config.provider,
    model: config.model,
    apiKey: config.apiKey,
  });
}

/**
 * Send a prompt to the AI agent
 * Response will be streamed via the ai-event listener
 */
export async function sendPrompt(prompt: string): Promise<string> {
  return invoke("send_ai_prompt", { prompt });
}

/**
 * Execute a specific tool with arguments
 */
export async function executeTool(
  toolName: string,
  args: unknown
): Promise<unknown> {
  return invoke("execute_ai_tool", { toolName, args });
}

/**
 * Get list of available tools
 */
export async function getAvailableTools(): Promise<ToolDefinition[]> {
  return invoke("get_available_tools");
}

/**
 * Shutdown the AI agent
 */
export async function shutdownAiAgent(): Promise<void> {
  return invoke("shutdown_ai_agent");
}

/**
 * Subscribe to AI events
 * Returns an unlisten function to stop listening
 */
export function onAiEvent(callback: (event: AiEvent) => void): Promise<UnlistenFn> {
  return listen<AiEvent>("ai-event", (event) => callback(event.payload));
}
```

### `src/hooks/useAiAgent.ts`

```typescript
import { useState, useEffect, useCallback, useRef } from "react";
import {
  initAiAgent,
  sendPrompt,
  onAiEvent,
  shutdownAiAgent,
  type AiConfig,
  type AiEvent,
} from "../lib/ai";

export interface Message {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: Date;
  isStreaming?: boolean;
}

export interface UseAiAgentOptions {
  onError?: (error: string) => void;
  onToolRequest?: (toolName: string, args: unknown, requestId: string) => void;
}

export function useAiAgent(options: UseAiAgentOptions = {}) {
  const [isInitialized, setIsInitialized] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [messages, setMessages] = useState<Message[]>([]);
  const [streamingContent, setStreamingContent] = useState("");
  const unlistenRef = useRef<(() => void) | null>(null);

  // Subscribe to AI events
  useEffect(() => {
    const setupListener = async () => {
      const unlisten = await onAiEvent((event) => {
        handleAiEvent(event);
      });
      unlistenRef.current = unlisten;
    };

    setupListener();

    return () => {
      if (unlistenRef.current) {
        unlistenRef.current();
      }
    };
  }, []);

  const handleAiEvent = useCallback(
    (event: AiEvent) => {
      switch (event.type) {
        case "started":
          setIsLoading(true);
          setStreamingContent("");
          break;

        case "text_delta":
          setStreamingContent(event.accumulated);
          break;

        case "tool_request":
          options.onToolRequest?.(
            event.tool_name,
            event.args,
            event.request_id
          );
          break;

        case "reasoning":
          // Optionally display reasoning in UI
          console.log("AI Reasoning:", event.content);
          break;

        case "completed":
          setMessages((prev) => [
            ...prev,
            {
              id: crypto.randomUUID(),
              role: "assistant",
              content: event.response || streamingContent,
              timestamp: new Date(),
            },
          ]);
          setStreamingContent("");
          setIsLoading(false);
          break;

        case "error":
          options.onError?.(event.message);
          setIsLoading(false);
          break;
      }
    },
    [options, streamingContent]
  );

  const initialize = useCallback(async (config: AiConfig) => {
    try {
      await initAiAgent(config);
      setIsInitialized(true);
    } catch (error) {
      options.onError?.(String(error));
      throw error;
    }
  }, [options]);

  const send = useCallback(
    async (content: string) => {
      if (!isInitialized) {
        throw new Error("AI agent not initialized");
      }

      // Add user message
      setMessages((prev) => [
        ...prev,
        {
          id: crypto.randomUUID(),
          role: "user",
          content,
          timestamp: new Date(),
        },
      ]);

      try {
        await sendPrompt(content);
      } catch (error) {
        options.onError?.(String(error));
        throw error;
      }
    },
    [isInitialized, options]
  );

  const shutdown = useCallback(async () => {
    await shutdownAiAgent();
    setIsInitialized(false);
    setMessages([]);
  }, []);

  const clearMessages = useCallback(() => {
    setMessages([]);
  }, []);

  return {
    isInitialized,
    isLoading,
    messages,
    streamingContent,
    initialize,
    send,
    shutdown,
    clearMessages,
  };
}
```

### `src/components/AiChat.tsx`

```tsx
import { useState } from "react";
import { useAiAgent } from "../hooks/useAiAgent";
import { toast } from "sonner";

interface AiChatProps {
  workspace: string;
}

export function AiChat({ workspace }: AiChatProps) {
  const [input, setInput] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [provider, setProvider] = useState<"openai" | "anthropic" | "gemini">("openai");
  const [model, setModel] = useState("gpt-4");

  const {
    isInitialized,
    isLoading,
    messages,
    streamingContent,
    initialize,
    send,
  } = useAiAgent({
    onError: (error) => toast.error(error),
    onToolRequest: (toolName, args, requestId) => {
      // Handle tool approval UI
      console.log("Tool requested:", toolName, args, requestId);
    },
  });

  const handleInitialize = async () => {
    await initialize({
      workspace,
      provider,
      model,
      apiKey,
    });
    toast.success("AI agent initialized");
  };

  const handleSend = async () => {
    if (!input.trim()) return;
    const message = input;
    setInput("");
    await send(message);
  };

  if (!isInitialized) {
    return (
      <div className="flex flex-col gap-4 p-4">
        <h2 className="text-lg font-semibold">Initialize AI Agent</h2>

        <select
          value={provider}
          onChange={(e) => setProvider(e.target.value as typeof provider)}
          className="border rounded px-3 py-2"
        >
          <option value="openai">OpenAI</option>
          <option value="anthropic">Anthropic</option>
          <option value="gemini">Google Gemini</option>
        </select>

        <input
          type="text"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          placeholder="Model (e.g., gpt-4, claude-3-opus)"
          className="border rounded px-3 py-2"
        />

        <input
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="API Key"
          className="border rounded px-3 py-2"
        />

        <button
          onClick={handleInitialize}
          disabled={!apiKey}
          className="bg-blue-500 text-white px-4 py-2 rounded disabled:opacity-50"
        >
          Initialize
        </button>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      {/* Messages */}
      <div className="flex-1 overflow-auto p-4 space-y-4">
        {messages.map((msg) => (
          <div
            key={msg.id}
            className={`p-3 rounded-lg ${
              msg.role === "user"
                ? "bg-blue-100 ml-auto max-w-[80%]"
                : "bg-gray-100 mr-auto max-w-[80%]"
            }`}
          >
            <p className="whitespace-pre-wrap">{msg.content}</p>
            <span className="text-xs text-gray-500">
              {msg.timestamp.toLocaleTimeString()}
            </span>
          </div>
        ))}

        {/* Streaming response */}
        {streamingContent && (
          <div className="bg-gray-100 p-3 rounded-lg mr-auto max-w-[80%]">
            <p className="whitespace-pre-wrap">{streamingContent}</p>
            <span className="text-xs text-gray-500">Typing...</span>
          </div>
        )}
      </div>

      {/* Input */}
      <div className="border-t p-4 flex gap-2">
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && !e.shiftKey && handleSend()}
          placeholder="Type a message..."
          disabled={isLoading}
          className="flex-1 border rounded px-3 py-2"
        />
        <button
          onClick={handleSend}
          disabled={isLoading || !input.trim()}
          className="bg-blue-500 text-white px-4 py-2 rounded disabled:opacity-50"
        >
          {isLoading ? "..." : "Send"}
        </button>
      </div>
    </div>
  );
}
```

## Available Tools from vtcode-core

Once integrated, you get access to these built-in tools:

| Category | Tools | Description |
|----------|-------|-------------|
| **File Operations** | `list_files`, `read_file`, `write_file`, `edit_file`, `apply_patch` | Full filesystem access within workspace |
| **Code Search** | `grep_file` | Regex search powered by ripgrep |
| **Shell Execution** | `run_pty_cmd`, `shell` | Execute commands with PTY support |
| **Web** | `web_fetch` | HTTP requests with security controls |
| **Diagnostics** | `get_errors`, `debug_agent` | Error aggregation and state inspection |
| **Planning** | `update_plan` | Task management and planning |
| **Code Execution** | `execute_code` | Run Python/JavaScript snippets |

## What You Get vs What You Skip

### Included from vtcode-core

| Feature | Description |
|---------|-------------|
| Multi-provider LLM | OpenAI, Anthropic, Gemini, DeepSeek, Ollama support |
| Streaming responses | Token-by-token output via events |
| ToolRegistry | 40+ built-in tools |
| Tool policies | Allow/prompt/deny security rules |
| Tree-sitter | Semantic code parsing for 8 languages |
| Context management | Token budgeting and conversation trimming |
| ThreadEvent | Structured execution events |
| MCP support | Model Context Protocol for extensibility |

### Excluded (TUI-only components)

| Component | Why Excluded |
|-----------|--------------|
| `Session` / `InlineSession` | Roxidy uses xterm.js |
| `InlineHandle` / `InlineCommand` | Custom event system instead |
| `AnsiRenderer` | xterm.js handles ANSI |
| `ratatui` widgets | React components instead |
| `crossterm` input handling | Tauri events instead |

## Configuration

vtcode-core supports configuration via `vtcode.toml`. Create this file in your workspace root:

```toml
[agent]
provider = "openai"
default_model = "gpt-4"
temperature = 0.7
max_tokens = 4096

[tools]
default_policy = "prompt"  # ask user before executing
max_tool_loops = 50

[tools.policies]
read_file = "allow"
write_file = "prompt"
run_pty_cmd = "prompt"
```

## Security Considerations

1. **Workspace Boundaries**: vtcode-core restricts file operations to the workspace directory
2. **Tool Policies**: Configure allow/prompt/deny rules per tool
3. **API Key Storage**: Consider using Tauri's secure storage for API keys
4. **Human-in-the-Loop**: Implement approval dialogs for dangerous operations

## Next Steps

1. Implement tool approval UI in React
2. Add conversation persistence
3. Integrate with Roxidy's terminal for command execution
4. Add support for MCP providers for extended capabilities

---

## API Notes

This document has been updated (2025-11-27) to reflect the actual vtcode-core API:

- **ProviderConfig**: Fields are `Option<String>` with an `enabled: bool` field
- **ToolRegistry**: `new()` is sync, `execute_tool()` requires `&mut self` (hence `RwLock`)
- **LLMStreamEvent**: Returns `Token { delta }`, `Reasoning { delta }`, and `Completed { response }` variants
- **Message**: Constructed via struct fields, not helper methods
- **ThreadEvent**: Uses tuple structs (we emit `AiEvent` directly instead of converting)

The code above should be close to the actual API, but you may encounter minor differences. Let the compiler guide you through any remaining issues.

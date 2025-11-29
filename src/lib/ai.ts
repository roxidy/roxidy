import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type AiProvider =
  | "openai"
  | "anthropic"
  | "gemini"
  | "deepseek"
  | "ollama"
  | "openrouter"
  | "anthropic_vertex";

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
 * Context information to inject into user messages.
 * This context is prepended as XML tags and not shown to the user.
 */
export interface PromptContext {
  /** The current working directory in the terminal */
  workingDirectory?: string;
  /** The session ID of the user's terminal (for running commands in the same terminal) */
  sessionId?: string;
}

/**
 * Send a prompt to the AI agent
 * Response will be streamed via the ai-event listener
 *
 * @param prompt - The user's message
 * @param context - Optional context to inject (working directory, etc.)
 */
export async function sendPrompt(prompt: string, context?: PromptContext): Promise<string> {
  // Convert to snake_case for Rust backend
  const contextPayload = context
    ? {
      working_directory: context.workingDirectory,
      session_id: context.sessionId,
    }
    : undefined;

  return invoke("send_ai_prompt", { prompt, context: contextPayload });
}

/**
 * Execute a specific tool with arguments
 */
export async function executeTool(toolName: string, args: unknown): Promise<unknown> {
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

/**
 * Check if AI agent is initialized
 */
export async function isAiInitialized(): Promise<boolean> {
  return invoke("is_ai_initialized");
}

/**
 * Update the AI agent's workspace/working directory.
 * This keeps the agent in sync with the user's terminal directory.
 *
 * @param workspace - New workspace/working directory path
 */
export async function updateAiWorkspace(workspace: string): Promise<void> {
  return invoke("update_ai_workspace", { workspace });
}

/**
 * Clear the AI agent's conversation history.
 * Call this when starting a new conversation or when the user wants to reset context.
 */
export async function clearAiConversation(): Promise<void> {
  return invoke("clear_ai_conversation");
}

/**
 * Get the current conversation history length.
 * Useful for debugging or showing context status in the UI.
 */
export async function getAiConversationLength(): Promise<number> {
  return invoke("get_ai_conversation_length");
}

/**
 * Get the OpenRouter API key from environment variables.
 * Returns null if not set.
 */
export async function getOpenRouterApiKey(): Promise<string | null> {
  return invoke("get_openrouter_api_key");
}

/**
 * Load environment variables from a .env file.
 * Returns the number of variables loaded.
 */
export async function loadEnvFile(path: string): Promise<number> {
  return invoke("load_env_file", { path });
}

/**
 * Vertex AI configuration from environment variables.
 */
export interface VertexAiEnvConfig {
  credentials_path: string | null;
  project_id: string | null;
  location: string | null;
}

/**
 * Get Vertex AI configuration from environment variables.
 * Reads from:
 * - VERTEX_AI_CREDENTIALS_PATH or GOOGLE_APPLICATION_CREDENTIALS
 * - VERTEX_AI_PROJECT_ID or GOOGLE_CLOUD_PROJECT
 * - VERTEX_AI_LOCATION (defaults to "us-east5" if not set)
 */
export async function getVertexAiConfig(): Promise<VertexAiEnvConfig> {
  return invoke("get_vertex_ai_config");
}

/**
 * Default configuration for Claude Opus 4.5 via OpenRouter.
 * API key should be provided from environment or user input.
 */
export const DEFAULT_AI_CONFIG = {
  provider: "openrouter" as AiProvider,
  // OpenRouter model ID for Claude Opus 4.5
  model: "anthropic/claude-opus-4.5",
};

/**
 * Initialize AI with Claude Opus 4.5 via OpenRouter.
 * This is a convenience function that uses sensible defaults.
 */
export async function initClaudeOpus(workspace: string, apiKey: string): Promise<void> {
  return initAiAgent({
    workspace,
    provider: DEFAULT_AI_CONFIG.provider,
    model: DEFAULT_AI_CONFIG.model,
    apiKey,
  });
}

/**
 * Configuration for Vertex AI Anthropic.
 */
export interface VertexAiConfig {
  workspace: string;
  credentialsPath: string;
  projectId: string;
  location: string;
  model: string;
}

/**
 * Available Claude models on Vertex AI.
 */
export const VERTEX_AI_MODELS = {
  CLAUDE_OPUS_4_5: "claude-opus-4-5@20251101",
  CLAUDE_SONNET_4_5: "claude-sonnet-4-5@20250929",
  CLAUDE_HAIKU_4_5: "claude-haiku-4-5@20251001",
} as const;

/**
 * Initialize AI with Anthropic on Google Cloud Vertex AI.
 * This uses a service account JSON file for authentication.
 */
export async function initVertexAiAgent(config: VertexAiConfig): Promise<void> {
  return invoke("init_ai_agent_vertex", {
    workspace: config.workspace,
    credentialsPath: config.credentialsPath,
    projectId: config.projectId,
    location: config.location,
    model: config.model,
  });
}

/**
 * Initialize AI with Claude Opus 4.5 on Vertex AI.
 * This is a convenience function that uses the latest Opus 4.5 model.
 */
export async function initVertexClaudeOpus(
  workspace: string,
  credentialsPath: string,
  projectId: string,
  location: string = "us-east5"
): Promise<void> {
  return initVertexAiAgent({
    workspace,
    credentialsPath,
    projectId,
    location,
    model: VERTEX_AI_MODELS.CLAUDE_OPUS_4_5,
  });
}

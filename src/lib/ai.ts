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
 * Send a prompt to the AI agent
 * Response will be streamed via the ai-event listener
 */
export async function sendPrompt(prompt: string): Promise<string> {
  return invoke("send_ai_prompt", { prompt });
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
  CLAUDE_SONNET_4: "claude-sonnet-4-20250514",
  CLAUDE_3_5_SONNET: "claude-3-5-sonnet-v2@20241022",
  CLAUDE_3_5_HAIKU: "claude-3-5-haiku@20241022",
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

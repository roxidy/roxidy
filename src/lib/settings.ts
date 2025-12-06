/**
 * Settings API for Qbit configuration management.
 *
 * Settings are stored in `~/.qbit/settings.toml` and support environment variable
 * interpolation. The backend provides fallback to environment variables for
 * backward compatibility.
 */

import { invoke } from "@tauri-apps/api/core";

// =============================================================================
// Type Definitions
// =============================================================================

/**
 * Root settings structure for Qbit.
 */
export interface QbitSettings {
  version: number;
  ai: AiSettings;
  api_keys: ApiKeysSettings;
  ui: UiSettings;
  terminal: TerminalSettings;
  agent: AgentSettings;
  mcp_servers: Record<string, McpServerConfig>;
  trust: TrustSettings;
  privacy: PrivacySettings;
  advanced: AdvancedSettings;
}

/**
 * AI provider configuration.
 */
export interface AiSettings {
  default_provider: AiProvider;
  default_model: string;
  vertex_ai: VertexAiSettings;
  openrouter: OpenRouterSettings;
  anthropic: AnthropicSettings;
  openai: OpenAiSettings;
  ollama: OllamaSettings;
}

export type AiProvider = "vertex_ai" | "openrouter" | "anthropic" | "openai" | "ollama";

/**
 * Vertex AI (Anthropic on Google Cloud) settings.
 */
export interface VertexAiSettings {
  credentials_path: string | null;
  project_id: string | null;
  location: string | null;
}

/**
 * OpenRouter API settings.
 */
export interface OpenRouterSettings {
  api_key: string | null;
}

/**
 * Direct Anthropic API settings.
 */
export interface AnthropicSettings {
  api_key: string | null;
}

/**
 * OpenAI API settings.
 */
export interface OpenAiSettings {
  api_key: string | null;
  base_url: string | null;
}

/**
 * Ollama local LLM settings.
 */
export interface OllamaSettings {
  base_url: string;
}

/**
 * API keys for external services.
 */
export interface ApiKeysSettings {
  tavily: string | null;
  github: string | null;
}

/**
 * User interface preferences.
 */
export interface UiSettings {
  theme: "dark" | "light" | "system";
  show_tips: boolean;
  hide_banner: boolean;
}

/**
 * Terminal configuration.
 */
export interface TerminalSettings {
  shell: string | null;
  font_family: string;
  font_size: number;
  scrollback: number;
}

/**
 * Agent behavior settings.
 */
export interface AgentSettings {
  session_persistence: boolean;
  session_retention_days: number;
  pattern_learning: boolean;
  min_approvals_for_auto: number;
  approval_threshold: number;
}

/**
 * MCP (Model Context Protocol) server configuration.
 */
export interface McpServerConfig {
  command: string | null;
  args: string[];
  env: Record<string, string>;
  url: string | null;
}

/**
 * Repository trust settings.
 */
export interface TrustSettings {
  full_trust: string[];
  read_only_trust: string[];
  never_trust: string[];
}

/**
 * Privacy and telemetry settings.
 */
export interface PrivacySettings {
  usage_statistics: boolean;
  log_prompts: boolean;
}

/**
 * Advanced/debug settings.
 */
export interface AdvancedSettings {
  enable_experimental: boolean;
  log_level: "error" | "warn" | "info" | "debug" | "trace";
}

// =============================================================================
// API Functions
// =============================================================================

/**
 * Get all settings.
 */
export async function getSettings(): Promise<QbitSettings> {
  return invoke("get_settings");
}

/**
 * Update all settings.
 */
export async function updateSettings(settings: QbitSettings): Promise<void> {
  return invoke("update_settings", { settings });
}

/**
 * Get a specific setting by dot-notation key.
 * @example getSetting("ai.vertex_ai.project_id")
 */
export async function getSetting<T = unknown>(key: string): Promise<T> {
  return invoke("get_setting", { key });
}

/**
 * Set a specific setting by dot-notation key.
 * @example setSetting("ui.theme", "light")
 */
export async function setSetting(key: string, value: unknown): Promise<void> {
  return invoke("set_setting", { key, value });
}

/**
 * Reset all settings to defaults.
 */
export async function resetSettings(): Promise<void> {
  return invoke("reset_settings");
}

/**
 * Reload settings from disk.
 */
export async function reloadSettings(): Promise<void> {
  return invoke("reload_settings");
}

/**
 * Check if settings file exists.
 */
export async function settingsFileExists(): Promise<boolean> {
  return invoke("settings_file_exists");
}

/**
 * Get the path to the settings file.
 */
export async function getSettingsPath(): Promise<string> {
  return invoke("get_settings_path");
}

// =============================================================================
// Default Settings
// =============================================================================

/**
 * Default settings matching the Rust defaults.
 */
export const DEFAULT_SETTINGS: QbitSettings = {
  version: 1,
  ai: {
    default_provider: "vertex_ai",
    default_model: "claude-opus-4-5@20251101",
    vertex_ai: {
      credentials_path: null,
      project_id: null,
      location: null,
    },
    openrouter: {
      api_key: null,
    },
    anthropic: {
      api_key: null,
    },
    openai: {
      api_key: null,
      base_url: null,
    },
    ollama: {
      base_url: "http://localhost:11434",
    },
  },
  api_keys: {
    tavily: null,
    github: null,
  },
  ui: {
    theme: "dark",
    show_tips: true,
    hide_banner: false,
  },
  terminal: {
    shell: null,
    font_family: "JetBrains Mono",
    font_size: 14,
    scrollback: 10000,
  },
  agent: {
    session_persistence: true,
    session_retention_days: 30,
    pattern_learning: true,
    min_approvals_for_auto: 3,
    approval_threshold: 0.8,
  },
  mcp_servers: {},
  trust: {
    full_trust: [],
    read_only_trust: [],
    never_trust: [],
  },
  privacy: {
    usage_statistics: false,
    log_prompts: false,
  },
  advanced: {
    enable_experimental: false,
    log_level: "info",
  },
};

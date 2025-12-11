import { invoke } from "@tauri-apps/api/core";

// ============================================================================
// Types - Simplified for Markdown-based Session Storage
// ============================================================================

/**
 * Status of the sidecar system
 */
export interface SidecarStatus {
  /** Whether a session is currently active */
  active_session: boolean;
  /** Current session ID if any */
  session_id: string | null;
  /** Whether the sidecar is enabled */
  enabled: boolean;
  /** Sessions directory path */
  sessions_dir: string;
  /** Workspace path (cwd of current session) */
  workspace_path: string | null;
}

/**
 * Session status enum
 */
export type SessionStatus = "Active" | "Completed" | "Abandoned";

/**
 * Session metadata (from meta.toml)
 */
export interface SessionMeta {
  /** Unique session identifier */
  session_id: string;
  /** When the session was created (ISO 8601) */
  created_at: string;
  /** When the session was last updated (ISO 8601) */
  updated_at: string;
  /** Session status */
  status: SessionStatus;
  /** Working directory */
  cwd: string;
  /** Git repository root (if in a git repo) */
  git_root: string | null;
  /** Git branch name (if in a git repo) */
  git_branch: string | null;
  /** Initial user request that started the session */
  initial_request: string;
}

/**
 * Sidecar configuration
 */
export interface SidecarConfig {
  /** Enable the sidecar system */
  enabled: boolean;
  /** Directory for session storage (null = default ~/.qbit/sessions) */
  sessions_dir: string | null;
  /** Days to retain session data (0 = unlimited) */
  retention_days: number;
  /** Maximum size for state.md in bytes */
  max_state_size: number;
  /** Whether to write raw events to events.jsonl */
  write_raw_events: boolean;
  /** Whether to use LLM for state updates (false = rule-based only) */
  use_llm_for_state: boolean;
  /** Capture tool call events */
  capture_tool_calls: boolean;
  /** Capture agent reasoning events */
  capture_reasoning: boolean;
}

// ============================================================================
// API Functions - Status & Initialization
// ============================================================================

/**
 * Get the current sidecar status
 */
export async function getSidecarStatus(): Promise<SidecarStatus> {
  return invoke<SidecarStatus>("sidecar_status");
}

/**
 * Initialize the sidecar for a workspace
 */
export async function initializeSidecar(workspacePath: string): Promise<void> {
  return invoke("sidecar_initialize", { workspacePath });
}

// ============================================================================
// API Functions - Session Lifecycle
// ============================================================================

/**
 * Start a new capture session
 * @param initialRequest The user's initial request/prompt
 * @returns The new session ID
 */
export async function startSession(initialRequest: string): Promise<string> {
  return invoke<string>("sidecar_start_session", { initialRequest });
}

/**
 * End the current session
 * @returns The session metadata if there was an active session
 */
export async function endSession(): Promise<SessionMeta | null> {
  return invoke<SessionMeta | null>("sidecar_end_session");
}

/**
 * Get the current session ID
 * @returns The current session ID or null if no active session
 */
export async function getCurrentSession(): Promise<string | null> {
  return invoke<string | null>("sidecar_current_session");
}

// ============================================================================
// API Functions - Session Content (Markdown-based)
// ============================================================================

/**
 * Get the state.md content for a session
 * This is the LLM-managed current state that can be injected into agent context
 * @param sessionId The session ID to get state for
 * @returns The state.md markdown content
 */
export async function getSessionState(sessionId: string): Promise<string> {
  return invoke<string>("sidecar_get_session_state", { sessionId });
}

/**
 * Get injectable context for the current session
 * This is the state.md content formatted for injection into agent prompts
 * @returns The context string or null if no active session
 */
export async function getInjectableContext(): Promise<string | null> {
  return invoke<string | null>("sidecar_get_injectable_context");
}

/**
 * Get the log.md content for a session
 * This is the append-only event log with timestamps and diffs
 * @param sessionId The session ID to get log for
 * @returns The log.md markdown content
 */
export async function getSessionLog(sessionId: string): Promise<string> {
  return invoke<string>("sidecar_get_session_log", { sessionId });
}

/**
 * Get the metadata for a session (from meta.toml)
 * @param sessionId The session ID to get metadata for
 * @returns The session metadata
 */
export async function getSessionMeta(sessionId: string): Promise<SessionMeta> {
  return invoke<SessionMeta>("sidecar_get_session_meta", { sessionId });
}

/**
 * List all sessions
 * @returns Array of session metadata for all sessions
 */
export async function listSessions(): Promise<SessionMeta[]> {
  return invoke<SessionMeta[]>("sidecar_list_sessions");
}

// ============================================================================
// API Functions - Configuration
// ============================================================================

/**
 * Get the sidecar configuration
 */
export async function getSidecarConfig(): Promise<SidecarConfig> {
  return invoke<SidecarConfig>("sidecar_get_config");
}

/**
 * Update the sidecar configuration
 */
export async function setSidecarConfig(config: SidecarConfig): Promise<void> {
  return invoke("sidecar_set_config", { config });
}

// ============================================================================
// API Functions - Lifecycle
// ============================================================================

/**
 * Shutdown the sidecar (ends any active session)
 */
export async function shutdownSidecar(): Promise<void> {
  return invoke("sidecar_shutdown");
}

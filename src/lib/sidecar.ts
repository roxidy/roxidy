import { invoke } from "@tauri-apps/api/core";

// ============================================================================
// Event Types - Real-time updates from backend
// ============================================================================

/**
 * Sidecar events emitted via the "sidecar-event" channel.
 * These provide real-time updates for session, patch, and artifact operations.
 */
export type SidecarEventType =
  | { event_type: "session_started"; session_id: string }
  | { event_type: "session_ended"; session_id: string }
  | {
      event_type: "patch_created";
      session_id: string;
      patch_id: number;
      subject: string;
    }
  | {
      event_type: "patch_applied";
      session_id: string;
      patch_id: number;
      commit_sha: string;
    }
  | { event_type: "patch_discarded"; session_id: string; patch_id: number }
  | {
      event_type: "patch_message_updated";
      session_id: string;
      patch_id: number;
      new_subject: string;
    }
  | {
      event_type: "artifact_created";
      session_id: string;
      filename: string;
      target: string;
    }
  | {
      event_type: "artifact_applied";
      session_id: string;
      filename: string;
      target: string;
    }
  | { event_type: "artifact_discarded"; session_id: string; filename: string };

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

// ============================================================================
// Types - L2 Staged Patches
// ============================================================================

/**
 * Reason why a commit boundary was detected
 */
export type BoundaryReason =
  | "CompletionSignal"
  | "UserApproval"
  | "ActivityPause"
  | "SessionEnd"
  | "Manual";

/**
 * Patch metadata
 */
export interface PatchMeta {
  /** Patch sequence ID */
  id: number;
  /** When the patch was created (ISO 8601) */
  created_at: string;
  /** Why this patch was created */
  boundary_reason: BoundaryReason;
  /** Git SHA if the patch has been applied */
  applied_sha: string | null;
}

/**
 * A staged patch ready for review
 */
export interface StagedPatch {
  /** Patch metadata */
  meta: PatchMeta;
  /** Subject line of the commit */
  subject: string;
  /** Full commit message */
  message: string;
  /** List of files in this patch */
  files: string[];
  /** The patch content (git format-patch style) */
  patch_content: string;
}

// ============================================================================
// API Functions - L2 Staged Patches
// ============================================================================

/**
 * Get all staged patches for a session
 * @param sessionId The session ID to get patches for
 * @returns Array of staged patches
 */
export async function getStagedPatches(sessionId: string): Promise<StagedPatch[]> {
  return invoke<StagedPatch[]>("sidecar_get_staged_patches", { sessionId });
}

/**
 * Get all applied patches for a session
 * @param sessionId The session ID to get patches for
 * @returns Array of applied patches
 */
export async function getAppliedPatches(sessionId: string): Promise<StagedPatch[]> {
  return invoke<StagedPatch[]>("sidecar_get_applied_patches", { sessionId });
}

/**
 * Get a specific patch by ID
 * @param sessionId The session ID
 * @param patchId The patch ID
 * @returns The patch if found
 */
export async function getPatch(sessionId: string, patchId: number): Promise<StagedPatch | null> {
  return invoke<StagedPatch | null>("sidecar_get_patch", {
    sessionId,
    patchId,
  });
}

/**
 * Discard a staged patch
 * @param sessionId The session ID
 * @param patchId The patch ID to discard
 * @returns True if the patch was discarded
 */
export async function discardPatch(sessionId: string, patchId: number): Promise<boolean> {
  return invoke<boolean>("sidecar_discard_patch", { sessionId, patchId });
}

/**
 * Get staged patches for the current active session
 * @returns Array of staged patches
 */
export async function getCurrentStagedPatches(): Promise<StagedPatch[]> {
  return invoke<StagedPatch[]>("sidecar_get_current_staged_patches");
}

/**
 * Apply a staged patch using git am
 * @param sessionId The session ID
 * @param patchId The patch ID to apply
 * @returns The git commit SHA of the applied patch
 */
export async function applyPatch(sessionId: string, patchId: number): Promise<string> {
  return invoke<string>("sidecar_apply_patch", { sessionId, patchId });
}

/**
 * Apply all staged patches in order
 * @param sessionId The session ID
 * @returns Array of [patchId, commitSha] tuples for applied patches
 */
export async function applyAllPatches(sessionId: string): Promise<[number, string][]> {
  return invoke<[number, string][]>("sidecar_apply_all_patches", { sessionId });
}

/**
 * Regenerate a patch's commit message using LLM synthesis
 * @param sessionId The session ID
 * @param patchId The patch ID to regenerate
 * @returns The updated patch with new message
 */
export async function regeneratePatch(sessionId: string, patchId: number): Promise<StagedPatch> {
  return invoke<StagedPatch>("sidecar_regenerate_patch", {
    sessionId,
    patchId,
  });
}

/**
 * Update a patch's commit message manually
 * @param sessionId The session ID
 * @param patchId The patch ID to update
 * @param newMessage The new commit message
 * @returns The updated patch
 */
export async function updatePatchMessage(
  sessionId: string,
  patchId: number,
  newMessage: string
): Promise<StagedPatch> {
  return invoke<StagedPatch>("sidecar_update_patch_message", {
    sessionId,
    patchId,
    newMessage,
  });
}

// ============================================================================
// Types - L3 Artifacts
// ============================================================================

/**
 * Artifact metadata (stored as HTML comment header in artifact files)
 */
export interface ArtifactMeta {
  /** Target file path (e.g., /path/to/README.md) */
  target: string;
  /** When the artifact was created (ISO 8601) */
  created_at: string;
  /** Why the artifact was created */
  reason: string;
  /** Patch IDs this artifact is based on */
  based_on_patches: number[];
}

/**
 * A pending or applied artifact
 */
export interface Artifact {
  /** Artifact metadata */
  meta: ArtifactMeta;
  /** Filename (e.g., "README.md", "CLAUDE.md") */
  filename: string;
  /** The proposed content */
  content: string;
}

/**
 * Artifact synthesis backend
 */
export type ArtifactSynthesisBackend = "Template" | "VertexAnthropic" | "OpenAi" | "Grok";

// ============================================================================
// API Functions - L3 Artifacts
// ============================================================================

/**
 * Get all pending artifacts for a session
 * @param sessionId The session ID
 * @returns Array of pending artifacts
 */
export async function getPendingArtifacts(sessionId: string): Promise<Artifact[]> {
  return invoke<Artifact[]>("sidecar_get_pending_artifacts", { sessionId });
}

/**
 * Get all applied artifacts for a session
 * @param sessionId The session ID
 * @returns Array of applied artifacts
 */
export async function getAppliedArtifacts(sessionId: string): Promise<Artifact[]> {
  return invoke<Artifact[]>("sidecar_get_applied_artifacts", { sessionId });
}

/**
 * Get a specific artifact by filename
 * @param sessionId The session ID
 * @param filename The artifact filename (e.g., "README.md")
 * @returns The artifact if found
 */
export async function getArtifact(sessionId: string, filename: string): Promise<Artifact | null> {
  return invoke<Artifact | null>("sidecar_get_artifact", {
    sessionId,
    filename,
  });
}

/**
 * Discard a pending artifact
 * @param sessionId The session ID
 * @param filename The artifact filename to discard
 * @returns True if the artifact was discarded
 */
export async function discardArtifact(sessionId: string, filename: string): Promise<boolean> {
  return invoke<boolean>("sidecar_discard_artifact", { sessionId, filename });
}

/**
 * Preview an artifact (show diff against current file)
 * @param sessionId The session ID
 * @param filename The artifact filename
 * @returns Diff preview string
 */
export async function previewArtifact(sessionId: string, filename: string): Promise<string> {
  return invoke<string>("sidecar_preview_artifact", { sessionId, filename });
}

/**
 * Get pending artifacts for the current active session
 * @returns Array of pending artifacts
 */
export async function getCurrentPendingArtifacts(): Promise<Artifact[]> {
  return invoke<Artifact[]>("sidecar_get_current_pending_artifacts");
}

/**
 * Apply a pending artifact (copy to target, git add, move to applied)
 * @param sessionId The session ID
 * @param filename The artifact filename to apply
 * @returns The target path that was written to
 */
export async function applyArtifact(sessionId: string, filename: string): Promise<string> {
  return invoke<string>("sidecar_apply_artifact", { sessionId, filename });
}

/**
 * Apply all pending artifacts
 * @param sessionId The session ID
 * @returns Array of [filename, targetPath] tuples for applied artifacts
 */
export async function applyAllArtifacts(sessionId: string): Promise<[string, string][]> {
  return invoke<[string, string][]>("sidecar_apply_all_artifacts", {
    sessionId,
  });
}

/**
 * Regenerate artifacts from current patches
 * @param sessionId The session ID
 * @param backendOverride Optional backend to use instead of configured default
 * @returns Array of filenames for generated artifacts
 */
export async function regenerateArtifacts(
  sessionId: string,
  backendOverride?: ArtifactSynthesisBackend
): Promise<string[]> {
  return invoke<string[]>("sidecar_regenerate_artifacts", {
    sessionId,
    backendOverride,
  });
}

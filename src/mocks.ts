/**
 * Tauri IPC Mock Adapter
 *
 * This module provides mock implementations for all Tauri IPC commands and events,
 * enabling browser-only development without the Rust backend.
 *
 * Usage: This file is automatically loaded in browser environments
 * (when window.__TAURI_INTERNALS__ is undefined).
 *
 * Events can be emitted using the exported helper functions:
 * - emitTerminalOutput(sessionId, data)
 * - emitCommandBlock(block)
 * - emitDirectoryChanged(sessionId, directory)
 * - emitSessionEnded(sessionId)
 * - emitAiEvent(event)
 */

import * as tauriEvent from "@tauri-apps/api/event";
import { clearMocks, mockIPC, mockWindows } from "@tauri-apps/api/mocks";

// =============================================================================
// Browser Mode Flag
// =============================================================================

// This flag is set to true when mocks are initialized.
// It's exposed globally so App.tsx can check it even after mockWindows()
// creates __TAURI_INTERNALS__.
declare global {
  interface Window {
    __MOCK_BROWSER_MODE__?: boolean;
  }
}

/**
 * Check if we're running in mock browser mode.
 * Use this instead of checking __TAURI_INTERNALS__ in components.
 */
export function isMockBrowserMode(): boolean {
  return window.__MOCK_BROWSER_MODE__ === true;
}

// =============================================================================
// Event System (custom implementation for browser mode)
// =============================================================================

// Auto-incrementing handler ID
let nextHandlerId = 1;

// Map of event name -> array of { handlerId, callback }
const mockEventListeners: Map<
  string,
  Array<{ handlerId: number; callback: (event: { event: string; payload: unknown }) => void }>
> = new Map();

// Map of handler ID -> { event, callback } (for unlisten)
const handlerToEvent: Map<number, string> = new Map();

/**
 * Register an event listener with its callback
 */
export function mockRegisterListener(
  event: string,
  callback: (event: { event: string; payload: unknown }) => void
): number {
  const handlerId = nextHandlerId++;
  if (!mockEventListeners.has(event)) {
    mockEventListeners.set(event, []);
  }
  mockEventListeners.get(event)?.push({ handlerId, callback });
  handlerToEvent.set(handlerId, event);
  console.log(`[Mock Events] Registered listener for "${event}" (handler: ${handlerId})`);
  return handlerId;
}

/**
 * Unregister an event listener by handler ID
 */
export function mockUnregisterListener(handlerId: number): void {
  const eventName = handlerToEvent.get(handlerId);
  if (!eventName) return;

  handlerToEvent.delete(handlerId);
  const listeners = mockEventListeners.get(eventName);
  if (listeners) {
    const filtered = listeners.filter((l) => l.handlerId !== handlerId);
    mockEventListeners.set(eventName, filtered);
    console.log(`[Mock Events] Unregistered listener for "${eventName}" (handler: ${handlerId})`);
  }
}

/**
 * Dispatch an event to all registered listeners
 */
function dispatchMockEvent(eventName: string, payload: unknown): void {
  const listeners = mockEventListeners.get(eventName);
  if (listeners && listeners.length > 0) {
    console.log(
      `[Mock Events] Dispatching "${eventName}" to ${listeners.length} listener(s)`,
      payload
    );
    for (const { callback } of listeners) {
      try {
        callback({ event: eventName, payload });
      } catch (e) {
        console.error(`[Mock Events] Error in listener for "${eventName}":`, e);
      }
    }
  } else {
    console.log(`[Mock Events] No listeners for "${eventName}"`, payload);
  }
}

// =============================================================================
// Mock Data
// =============================================================================

// Mock PTY session - use consistent ID so MockDevTools presets work
let mockPtySession = {
  id: "mock-session-001",
  working_directory: "/home/user",
  rows: 24,
  cols: 80,
};

// Mock AI state
let mockAiInitialized = false;
let mockConversationLength = 0;
let mockSessionPersistenceEnabled = true;

// Mock tool definitions
const mockTools = [
  {
    name: "read_file",
    description: "Read the contents of a file",
    parameters: {
      type: "object",
      properties: {
        path: { type: "string", description: "Path to the file" },
      },
      required: ["path"],
    },
  },
  {
    name: "write_file",
    description: "Write content to a file",
    parameters: {
      type: "object",
      properties: {
        path: { type: "string", description: "Path to the file" },
        content: { type: "string", description: "Content to write" },
      },
      required: ["path", "content"],
    },
  },
  {
    name: "run_command",
    description: "Execute a shell command",
    parameters: {
      type: "object",
      properties: {
        command: { type: "string", description: "Command to execute" },
      },
      required: ["command"],
    },
  },
];

// Mock workflows
const mockWorkflows = [
  { name: "code-review", description: "Review code changes and provide feedback" },
  { name: "test-generation", description: "Generate unit tests for code" },
  { name: "refactor", description: "Suggest code refactoring improvements" },
];

// Mock sub-agents
const mockSubAgents = [
  { id: "explorer", name: "Code Explorer", description: "Explores and understands codebases" },
  { id: "debugger", name: "Debug Assistant", description: "Helps debug issues" },
  { id: "documenter", name: "Documentation Writer", description: "Generates documentation" },
];

// Mock sessions
const mockSessions = [
  {
    identifier: "session-2024-01-15-001",
    path: "/home/user/.qbit/sessions/session-2024-01-15-001.json",
    workspace_label: "qbit",
    workspace_path: "/home/user/qbit",
    model: "claude-opus-4.5",
    provider: "anthropic_vertex",
    started_at: "2024-01-15T10:00:00Z",
    ended_at: "2024-01-15T11:30:00Z",
    total_messages: 24,
    distinct_tools: ["read_file", "write_file", "run_command"],
    first_prompt_preview: "Can you help me refactor the authentication module?",
    first_reply_preview: "I'll help you refactor the authentication module...",
  },
  {
    identifier: "session-2024-01-14-002",
    path: "/home/user/.qbit/sessions/session-2024-01-14-002.json",
    workspace_label: "qbit",
    workspace_path: "/home/user/qbit",
    model: "claude-opus-4.5",
    provider: "anthropic_vertex",
    started_at: "2024-01-14T14:00:00Z",
    ended_at: "2024-01-14T16:45:00Z",
    total_messages: 42,
    distinct_tools: ["read_file", "run_command"],
    first_prompt_preview: "Help me add unit tests for the PTY manager",
    first_reply_preview: "I'll help you add unit tests for the PTY manager...",
  },
];

// Mock approval patterns
const mockApprovalPatterns = [
  {
    tool_name: "read_file",
    total_requests: 50,
    approvals: 50,
    denials: 0,
    always_allow: true,
    last_updated: "2024-01-15T10:00:00Z",
    justifications: [],
  },
  {
    tool_name: "write_file",
    total_requests: 20,
    approvals: 18,
    denials: 2,
    always_allow: false,
    last_updated: "2024-01-15T09:30:00Z",
    justifications: ["Writing config file", "Updating source code"],
  },
  {
    tool_name: "run_command",
    total_requests: 30,
    approvals: 25,
    denials: 5,
    always_allow: false,
    last_updated: "2024-01-15T11:00:00Z",
    justifications: ["Running tests", "Building project"],
  },
];

// Mock HITL config
let mockHitlConfig = {
  always_allow: ["read_file"],
  always_require_approval: ["run_command"],
  pattern_learning_enabled: true,
  min_approvals: 3,
  approval_threshold: 0.8,
};

// Mock prompts
const mockPrompts = [
  { name: "review", path: "/home/user/.qbit/prompts/review.md", source: "global" as const },
  { name: "explain", path: "/home/user/.qbit/prompts/explain.md", source: "global" as const },
  { name: "project-context", path: ".qbit/prompts/project-context.md", source: "local" as const },
];

// Mock indexer state
let mockIndexerInitialized = false;
let mockIndexerWorkspace: string | null = null;
let mockIndexedFileCount = 0;

// =============================================================================
// Event Types (matching backend events)
// =============================================================================

export interface TerminalOutputEvent {
  session_id: string;
  data: string;
}

// Command block events are lifecycle events, not full blocks
export interface CommandBlockEvent {
  session_id: string;
  command: string | null;
  exit_code: number | null;
  event_type: "prompt_start" | "prompt_end" | "command_start" | "command_end";
}

export interface DirectoryChangedEvent {
  session_id: string;
  path: string;
}

export interface SessionEndedEvent {
  session_id: string;
}

export type AiEventType =
  | { type: "started"; turn_id: string }
  | { type: "text_delta"; delta: string; accumulated: string }
  | { type: "tool_request"; tool_name: string; args: unknown; request_id: string }
  | {
      type: "tool_result";
      tool_name: string;
      result: unknown;
      success: boolean;
      request_id: string;
    }
  | { type: "completed"; response: string; tokens_used?: number; duration_ms?: number }
  | { type: "error"; message: string; error_type: string };

// =============================================================================
// Event Emitter Helpers
// =============================================================================

/**
 * Emit a terminal output event.
 * Use this to simulate terminal output in browser mode.
 */
export async function emitTerminalOutput(sessionId: string, data: string): Promise<void> {
  dispatchMockEvent("terminal_output", { session_id: sessionId, data });
}

/**
 * Emit a command block lifecycle event.
 * Use this to simulate command lifecycle events in browser mode.
 *
 * To simulate a full command execution, call in sequence:
 * 1. emitCommandBlockEvent(sessionId, "prompt_start")
 * 2. emitCommandBlockEvent(sessionId, "command_start", command)
 * 3. emitTerminalOutput(sessionId, output)  // The actual command output
 * 4. emitCommandBlockEvent(sessionId, "command_end", command, exitCode)
 * 5. emitCommandBlockEvent(sessionId, "prompt_end")
 */
export async function emitCommandBlockEvent(
  sessionId: string,
  eventType: CommandBlockEvent["event_type"],
  command: string | null = null,
  exitCode: number | null = null
): Promise<void> {
  dispatchMockEvent("command_block", {
    session_id: sessionId,
    command,
    exit_code: exitCode,
    event_type: eventType,
  });
}

/**
 * Helper to simulate a complete command execution with output.
 * This emits the proper sequence of events that the app expects.
 */
export async function simulateCommand(
  sessionId: string,
  command: string,
  output: string,
  exitCode: number = 0
): Promise<void> {
  // Start command
  await emitCommandBlockEvent(sessionId, "command_start", command);

  // Send output
  await emitTerminalOutput(sessionId, `$ ${command}\r\n`);
  await emitTerminalOutput(sessionId, output);
  if (!output.endsWith("\n")) {
    await emitTerminalOutput(sessionId, "\r\n");
  }

  // End command
  await emitCommandBlockEvent(sessionId, "command_end", command, exitCode);
}

/**
 * @deprecated Use emitCommandBlockEvent() or simulateCommand() instead.
 * This function signature doesn't match the actual event format.
 */
export async function emitCommandBlock(
  sessionId: string,
  command: string,
  output: string,
  exitCode: number | null = 0,
  _workingDirectory: string = "/home/user"
): Promise<void> {
  // Redirect to the proper simulation
  await simulateCommand(sessionId, command, output, exitCode ?? 0);
}

/**
 * Emit a directory changed event.
 * Use this to simulate directory changes in browser mode.
 */
export async function emitDirectoryChanged(sessionId: string, directory: string): Promise<void> {
  dispatchMockEvent("directory_changed", { session_id: sessionId, directory });
}

/**
 * Emit a session ended event.
 * Use this to simulate session termination in browser mode.
 */
export async function emitSessionEnded(sessionId: string): Promise<void> {
  dispatchMockEvent("session_ended", { session_id: sessionId });
}

/**
 * Emit an AI event.
 * Use this to simulate AI streaming responses in browser mode.
 */
export async function emitAiEvent(event: AiEventType): Promise<void> {
  dispatchMockEvent("ai-event", event);
}

/**
 * Simulate a complete AI response with streaming.
 * This emits started -> text_delta(s) -> completed events.
 */
export async function simulateAiResponse(response: string, delayMs: number = 50): Promise<void> {
  const turnId = `mock-turn-${Date.now()}`;

  // Emit started
  await emitAiEvent({ type: "started", turn_id: turnId });

  // Emit text deltas (word by word)
  const words = response.split(" ");
  let accumulated = "";
  for (const word of words) {
    const delta = accumulated ? ` ${word}` : word;
    accumulated += delta;
    await emitAiEvent({ type: "text_delta", delta, accumulated });
    await new Promise((resolve) => setTimeout(resolve, delayMs));
  }

  // Emit completed
  await emitAiEvent({
    type: "completed",
    response: accumulated,
    tokens_used: Math.floor(accumulated.length / 4),
    duration_ms: words.length * delayMs,
  });
}

// =============================================================================
// Setup Mock IPC
// =============================================================================

/**
 * Clean up mocks. Call this when unmounting or resetting.
 */
export function cleanupMocks(): void {
  clearMocks();
  console.log("[Mocks] Tauri mocks cleared");
}

export function setupMocks(): void {
  console.log("[Mocks] Setting up Tauri IPC mocks for browser development");

  // Set the browser mode flag BEFORE mockWindows creates __TAURI_INTERNALS__
  // This allows components to check isMockBrowserMode() after mocks are set up
  window.__MOCK_BROWSER_MODE__ = true;

  try {
    // Setup mock window context (required for Tauri internals)
    mockWindows("main");

    // Patch the Tauri event module's listen function to use our mock event system
    // ES module exports are read-only, so we use Object.defineProperty to override
    const originalListen = tauriEvent.listen;

    // Create our mock listen function
    const mockListen = async <T>(
      eventName: string,
      callback: (event: { event: string; payload: T }) => void
    ): Promise<() => void> => {
      console.log(`[Mock Events] listen("${eventName}") called`);

      // Register the callback with our mock event system
      const handlerId = mockRegisterListener(
        eventName,
        callback as (event: { event: string; payload: unknown }) => void
      );

      // Return an unlisten function
      return () => {
        mockUnregisterListener(handlerId);
      };
    };

    // Try to override the listen export using Object.defineProperty
    // Note: This usually fails because ES modules have read-only exports,
    // but we try anyway in case the bundler makes it writable
    try {
      Object.defineProperty(tauriEvent, "listen", {
        value: mockListen,
        writable: true,
        configurable: true,
      });
    } catch {
      // Expected to fail - we use the global fallback instead
      // Hooks check for window.__MOCK_LISTEN__ when in browser mode
    }

    // Store mock listen function globally as a fallback
    // Hooks can check for this when the module patch doesn't work
    (window as unknown as { __MOCK_LISTEN__?: typeof mockListen }).__MOCK_LISTEN__ = mockListen;

    // Store reference to original for cleanup
    (
      window as unknown as { __MOCK_ORIGINAL_LISTEN__?: typeof originalListen }
    ).__MOCK_ORIGINAL_LISTEN__ = originalListen;
  } catch (error) {
    console.error("[Mocks] Error during initial setup:", error);
  }

  mockIPC((cmd, args) => {
    console.log(`[Mock IPC] Command: ${cmd}`, args);

    switch (cmd) {
      // =========================================================================
      // PTY Commands
      // =========================================================================
      case "pty_create": {
        const payload = args as { workingDirectory?: string; rows?: number; cols?: number };
        // Use consistent session ID so MockDevTools presets work correctly
        mockPtySession = {
          id: "mock-session-001",
          working_directory: payload.workingDirectory ?? "/home/user",
          rows: payload.rows ?? 24,
          cols: payload.cols ?? 80,
        };
        return mockPtySession;
      }

      case "pty_write":
        // Simulate writing to PTY - in real app this would send data to the terminal
        return undefined;

      case "pty_resize": {
        const resizePayload = args as { sessionId: string; rows: number; cols: number };
        mockPtySession.rows = resizePayload.rows;
        mockPtySession.cols = resizePayload.cols;
        return undefined;
      }

      case "pty_destroy":
        return undefined;

      case "pty_get_session":
        return mockPtySession;

      // =========================================================================
      // Shell Integration Commands
      // =========================================================================
      case "shell_integration_status":
        return { type: "Installed", version: "1.0.0" };

      case "shell_integration_install":
        return undefined;

      case "shell_integration_uninstall":
        return undefined;

      // =========================================================================
      // Prompt Commands
      // =========================================================================
      case "list_prompts":
        return mockPrompts;

      case "read_prompt":
        return "# Mock Prompt\n\nThis is a mock prompt content for browser development.";

      // =========================================================================
      // AI Agent Commands
      // =========================================================================
      case "init_ai_agent":
      case "init_ai_agent_vertex":
        mockAiInitialized = true;
        mockConversationLength = 0;
        return undefined;

      case "send_ai_prompt":
        // In browser mode, we just return a mock response
        // Real streaming events would come from the backend
        mockConversationLength += 2; // User message + AI response
        return `mock-turn-id-${Date.now()}`;

      case "execute_ai_tool":
        return { success: true, result: "Mock tool execution result" };

      case "get_available_tools":
        return mockTools;

      case "list_workflows":
        return mockWorkflows;

      case "list_sub_agents":
        return mockSubAgents;

      case "shutdown_ai_agent":
        mockAiInitialized = false;
        mockConversationLength = 0;
        return undefined;

      case "is_ai_initialized":
        return mockAiInitialized;

      case "update_ai_workspace":
        return undefined;

      case "clear_ai_conversation":
        mockConversationLength = 0;
        return undefined;

      case "get_ai_conversation_length":
        return mockConversationLength;

      case "get_openrouter_api_key":
        return null; // No API key in mock mode

      case "load_env_file":
        return 0; // No variables loaded in mock mode

      case "get_vertex_ai_config":
        return {
          credentials_path: null,
          project_id: null,
          location: "us-east5",
        };

      // =========================================================================
      // Session Persistence Commands
      // =========================================================================
      case "list_ai_sessions":
        return mockSessions;

      case "find_ai_session": {
        const findPayload = args as { identifier: string };
        return mockSessions.find((s) => s.identifier === findPayload.identifier) ?? null;
      }

      case "load_ai_session": {
        const loadPayload = args as { identifier: string };
        const session = mockSessions.find((s) => s.identifier === loadPayload.identifier);
        if (!session) return null;
        return {
          ...session,
          transcript: ["User: Hello", "Assistant: Hi! How can I help you?"],
          messages: [
            { role: "user", content: "Hello" },
            { role: "assistant", content: "Hi! How can I help you?" },
          ],
        };
      }

      case "export_ai_session_transcript":
        return undefined;

      case "set_ai_session_persistence": {
        const persistPayload = args as { enabled: boolean };
        mockSessionPersistenceEnabled = persistPayload.enabled;
        return undefined;
      }

      case "is_ai_session_persistence_enabled":
        return mockSessionPersistenceEnabled;

      case "finalize_ai_session":
        return "/home/user/.qbit/sessions/mock-session.json";

      case "restore_ai_session": {
        const restorePayload = args as { identifier: string };
        const restoredSession = mockSessions.find(
          (s) => s.identifier === restorePayload.identifier
        );
        if (!restoredSession) {
          throw new Error(`Session not found: ${restorePayload.identifier}`);
        }
        mockConversationLength = restoredSession.total_messages;
        return {
          ...restoredSession,
          transcript: ["User: Hello", "Assistant: Hi! How can I help you?"],
          messages: [
            { role: "user", content: "Hello" },
            { role: "assistant", content: "Hi! How can I help you?" },
          ],
        };
      }

      // =========================================================================
      // HITL (Human-in-the-Loop) Commands
      // =========================================================================
      case "get_approval_patterns":
        return mockApprovalPatterns;

      case "get_tool_approval_pattern": {
        const patternPayload = args as { toolName: string };
        return mockApprovalPatterns.find((p) => p.tool_name === patternPayload.toolName) ?? null;
      }

      case "get_hitl_config":
        return mockHitlConfig;

      case "set_hitl_config": {
        const configPayload = args as { config: typeof mockHitlConfig };
        mockHitlConfig = configPayload.config;
        return undefined;
      }

      case "add_tool_always_allow": {
        const addPayload = args as { toolName: string };
        if (!mockHitlConfig.always_allow.includes(addPayload.toolName)) {
          mockHitlConfig.always_allow.push(addPayload.toolName);
        }
        return undefined;
      }

      case "remove_tool_always_allow": {
        const removePayload = args as { toolName: string };
        mockHitlConfig.always_allow = mockHitlConfig.always_allow.filter(
          (t) => t !== removePayload.toolName
        );
        return undefined;
      }

      case "reset_approval_patterns":
        return undefined;

      case "respond_to_tool_approval":
        return undefined;

      // =========================================================================
      // Indexer Commands
      // =========================================================================
      case "init_indexer": {
        const initPayload = args as { workspacePath: string };
        mockIndexerInitialized = true;
        mockIndexerWorkspace = initPayload.workspacePath;
        mockIndexedFileCount = 42; // Mock some indexed files
        return {
          files_indexed: 42,
          success: true,
          message: "Mock indexer initialized successfully",
        };
      }

      case "is_indexer_initialized":
        return mockIndexerInitialized;

      case "get_indexer_workspace":
        return mockIndexerWorkspace;

      case "get_indexed_file_count":
        return mockIndexedFileCount;

      case "index_file":
        mockIndexedFileCount += 1;
        return {
          files_indexed: 1,
          success: true,
          message: "File indexed successfully",
        };

      case "index_directory":
        mockIndexedFileCount += 10;
        return {
          files_indexed: 10,
          success: true,
          message: "Directory indexed successfully",
        };

      case "search_code":
        return [
          {
            file_path: "/home/user/qbit/src/lib/ai.ts",
            line_number: 42,
            line_content: "export async function initAiAgent(config: AiConfig): Promise<void> {",
            matches: ["initAiAgent"],
          },
          {
            file_path: "/home/user/qbit/src/lib/tauri.ts",
            line_number: 15,
            line_content: "export async function ptyCreate(",
            matches: ["ptyCreate"],
          },
        ];

      case "search_files":
        return [
          "/home/user/qbit/src/lib/ai.ts",
          "/home/user/qbit/src/lib/tauri.ts",
          "/home/user/qbit/src/lib/indexer.ts",
        ];

      case "analyze_file":
        return {
          symbols: [
            {
              name: "initAiAgent",
              kind: "function",
              line: 42,
              column: 0,
              scope: null,
              signature: "(config: AiConfig): Promise<void>",
              documentation: "Initialize the AI agent with the specified configuration",
            },
          ],
          metrics: {
            lines_of_code: 150,
            lines_of_comments: 30,
            blank_lines: 20,
            functions_count: 12,
            classes_count: 0,
            variables_count: 5,
            imports_count: 3,
            comment_ratio: 0.15,
          },
          dependencies: [
            { name: "@tauri-apps/api/core", kind: "import", source: null },
            { name: "@tauri-apps/api/event", kind: "import", source: null },
          ],
        };

      case "extract_symbols":
        return [
          {
            name: "initAiAgent",
            kind: "function",
            line: 42,
            column: 0,
            scope: null,
            signature: "(config: AiConfig): Promise<void>",
            documentation: "Initialize the AI agent",
          },
          {
            name: "sendPrompt",
            kind: "function",
            line: 100,
            column: 0,
            scope: null,
            signature: "(prompt: string): Promise<string>",
            documentation: "Send a prompt to the AI",
          },
        ];

      case "get_file_metrics":
        return {
          lines_of_code: 150,
          lines_of_comments: 30,
          blank_lines: 20,
          functions_count: 12,
          classes_count: 0,
          variables_count: 5,
          imports_count: 3,
          comment_ratio: 0.15,
        };

      case "detect_language":
        return "typescript";

      case "shutdown_indexer":
        mockIndexerInitialized = false;
        mockIndexerWorkspace = null;
        mockIndexedFileCount = 0;
        return undefined;

      // =========================================================================
      // Tauri Plugin Commands (event system)
      // Note: We patch tauriEvent.listen directly, so these handlers are just
      // for compatibility if any code calls invoke() directly
      // =========================================================================
      case "plugin:event|listen": {
        const payload = args as { event: string; handler: number };
        // Return the handler ID - actual registration happens via patched listen()
        return payload.handler;
      }

      case "plugin:event|unlisten": {
        const payload = args as { event: string; eventId: number };
        mockUnregisterListener(payload.eventId);
        return undefined;
      }

      case "plugin:event|emit": {
        // Emit is handled by our emit() calls, just acknowledge it
        return undefined;
      }

      // =========================================================================
      // Default: Unhandled command
      // =========================================================================
      default:
        // Don't warn for plugin commands we might not have implemented yet
        if (!cmd.startsWith("plugin:")) {
          console.warn(`[Mock IPC] Unhandled command: ${cmd}`, args);
        }
        return undefined;
    }
  });

  console.log("[Mocks] Tauri IPC mocks initialized successfully");
}

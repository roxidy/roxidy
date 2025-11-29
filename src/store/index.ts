import { create } from "zustand";
import { devtools } from "zustand/middleware";
import { immer } from "zustand/middleware/immer";

// Types
export type SessionMode = "terminal" | "agent";
export type InputMode = "terminal" | "agent";
export type AiStatus = "disconnected" | "initializing" | "ready" | "error";

export interface AiConfig {
  provider: string;
  model: string;
  status: AiStatus;
  errorMessage?: string;
  // Vertex AI specific config (for model switching)
  vertexConfig?: {
    workspace: string;
    credentialsPath: string;
    projectId: string;
    location: string;
  };
}

export interface Session {
  id: string;
  name: string;
  workingDirectory: string;
  createdAt: string;
  mode: SessionMode;
  inputMode?: InputMode; // Toggle button state for unified input (defaults to "agent")
}

// Unified timeline block types
export type UnifiedBlock =
  | { id: string; type: "command"; timestamp: string; data: CommandBlock }
  | {
      id: string;
      type: "agent_message";
      timestamp: string;
      data: AgentMessage;
    }
  | {
      id: string;
      type: "agent_streaming";
      timestamp: string;
      data: { content: string; toolCalls?: ToolCall[] };
    };

export interface CommandBlock {
  id: string;
  sessionId: string;
  command: string;
  output: string;
  exitCode: number | null;
  startTime: string;
  durationMs: number | null;
  workingDirectory: string;
  isCollapsed: boolean;
}

/** Finalized streaming block for persisted messages */
export type FinalizedStreamingBlock =
  | { type: "text"; content: string }
  | { type: "tool"; toolCall: ToolCall };

export interface AgentMessage {
  id: string;
  sessionId: string;
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: string;
  isStreaming?: boolean;
  toolCalls?: ToolCall[];
  /** Interleaved text and tool call blocks from streaming (preserves order) */
  streamingHistory?: FinalizedStreamingBlock[];
}

export interface ToolCall {
  id: string;
  name: string;
  args: Record<string, unknown>;
  status: "pending" | "approved" | "denied" | "running" | "completed" | "error";
  result?: unknown;
  /** True if this tool was executed by the agent (vs user-initiated) */
  executedByAgent?: boolean;
}

/** Tool call being actively executed by the agent */
export interface ActiveToolCall {
  id: string;
  name: string;
  args: Record<string, unknown>;
  status: "running" | "completed" | "error";
  result?: unknown;
  startedAt: string;
  completedAt?: string;
  /** True if this tool was executed by the agent (vs user-initiated) */
  executedByAgent?: boolean;
}

/** Streaming block types for interleaved text and tool calls */
export type StreamingBlock =
  | { type: "text"; content: string }
  | { type: "tool"; toolCall: ActiveToolCall };

interface PendingCommand {
  command: string | null;
  output: string;
  startTime: string;
  workingDirectory: string;
}

interface QbitState {
  // Sessions
  sessions: Record<string, Session>;
  activeSessionId: string | null;

  // AI configuration
  aiConfig: AiConfig;

  // Unified timeline (Phase 1)
  timelines: Record<string, UnifiedBlock[]>;

  // Terminal state (kept for backward compatibility)
  commandBlocks: Record<string, CommandBlock[]>;
  pendingCommand: Record<string, PendingCommand | null>;

  // Agent state (kept for backward compatibility)
  agentMessages: Record<string, AgentMessage[]>;
  agentStreaming: Record<string, string>;
  streamingBlocks: Record<string, StreamingBlock[]>; // Interleaved text and tool blocks
  streamingTextOffset: Record<string, number>; // Tracks how much text has been assigned to blocks
  agentInitialized: Record<string, boolean>;
  isAgentThinking: Record<string, boolean>; // True when waiting for first content from agent
  pendingToolApproval: Record<string, ToolCall | null>;
  processedToolRequests: Set<string>; // Track processed request IDs to prevent duplicates
  activeToolCalls: Record<string, ActiveToolCall[]>; // Tool calls currently in progress per session

  // Session actions
  addSession: (session: Session) => void;
  removeSession: (sessionId: string) => void;
  setActiveSession: (sessionId: string) => void;
  updateWorkingDirectory: (sessionId: string, path: string) => void;
  setSessionMode: (sessionId: string, mode: SessionMode) => void;
  setInputMode: (sessionId: string, mode: InputMode) => void;

  // Terminal actions
  handlePromptStart: (sessionId: string) => void;
  handlePromptEnd: (sessionId: string) => void;
  handleCommandStart: (sessionId: string, command: string | null) => void;
  handleCommandEnd: (sessionId: string, exitCode: number) => void;
  appendOutput: (sessionId: string, data: string) => void;
  toggleBlockCollapse: (blockId: string) => void;
  clearBlocks: (sessionId: string) => void;

  // Agent actions
  addAgentMessage: (sessionId: string, message: AgentMessage) => void;
  updateAgentStreaming: (sessionId: string, content: string) => void;
  clearAgentStreaming: (sessionId: string) => void;
  setAgentInitialized: (sessionId: string, initialized: boolean) => void;
  setAgentThinking: (sessionId: string, thinking: boolean) => void;
  setPendingToolApproval: (sessionId: string, tool: ToolCall | null) => void;
  markToolRequestProcessed: (requestId: string) => void;
  isToolRequestProcessed: (requestId: string) => boolean;
  updateToolCallStatus: (
    sessionId: string,
    toolId: string,
    status: ToolCall["status"],
    result?: unknown
  ) => void;
  clearAgentMessages: (sessionId: string) => void;
  restoreAgentMessages: (sessionId: string, messages: AgentMessage[]) => void;
  addActiveToolCall: (
    sessionId: string,
    toolCall: { id: string; name: string; args: Record<string, unknown> }
  ) => void;
  completeActiveToolCall: (
    sessionId: string,
    toolId: string,
    success: boolean,
    result?: unknown
  ) => void;
  clearActiveToolCalls: (sessionId: string) => void;
  // Streaming blocks actions
  addStreamingToolBlock: (
    sessionId: string,
    toolCall: { id: string; name: string; args: Record<string, unknown> }
  ) => void;
  updateStreamingToolBlock: (
    sessionId: string,
    toolId: string,
    success: boolean,
    result?: unknown
  ) => void;
  clearStreamingBlocks: (sessionId: string) => void;

  // Timeline actions
  clearTimeline: (sessionId: string) => void;

  // AI config actions
  setAiConfig: (config: Partial<AiConfig>) => void;
}

export const useStore = create<QbitState>()(
  devtools(
    immer((set, _get) => ({
      sessions: {},
      activeSessionId: null,
      aiConfig: {
        provider: "",
        model: "",
        status: "disconnected" as AiStatus,
      },
      timelines: {},
      commandBlocks: {},
      pendingCommand: {},
      agentMessages: {},
      agentStreaming: {},
      streamingBlocks: {},
      streamingTextOffset: {},
      agentInitialized: {},
      isAgentThinking: {},
      pendingToolApproval: {},
      processedToolRequests: new Set<string>(),
      activeToolCalls: {},

      addSession: (session) =>
        set((state) => {
          state.sessions[session.id] = {
            ...session,
            inputMode: session.inputMode ?? "terminal", // Default to terminal mode
          };
          state.activeSessionId = session.id;
          state.timelines[session.id] = [];
          state.commandBlocks[session.id] = [];
          state.pendingCommand[session.id] = null;
          state.agentMessages[session.id] = [];
          state.agentStreaming[session.id] = "";
          state.streamingBlocks[session.id] = [];
          state.streamingTextOffset[session.id] = 0;
          state.agentInitialized[session.id] = false;
          state.isAgentThinking[session.id] = false;
          state.pendingToolApproval[session.id] = null;
          state.activeToolCalls[session.id] = [];
        }),

      removeSession: (sessionId) =>
        set((state) => {
          delete state.sessions[sessionId];
          delete state.timelines[sessionId];
          delete state.commandBlocks[sessionId];
          delete state.pendingCommand[sessionId];
          delete state.agentMessages[sessionId];
          delete state.agentStreaming[sessionId];
          delete state.streamingBlocks[sessionId];
          delete state.streamingTextOffset[sessionId];
          delete state.agentInitialized[sessionId];
          delete state.isAgentThinking[sessionId];
          delete state.pendingToolApproval[sessionId];
          delete state.activeToolCalls[sessionId];

          if (state.activeSessionId === sessionId) {
            const remaining = Object.keys(state.sessions);
            state.activeSessionId = remaining[0] ?? null;
          }
        }),

      setActiveSession: (sessionId) =>
        set((state) => {
          state.activeSessionId = sessionId;
        }),

      updateWorkingDirectory: (sessionId, path) =>
        set((state) => {
          if (state.sessions[sessionId]) {
            state.sessions[sessionId].workingDirectory = path;
          }
        }),

      setSessionMode: (sessionId, mode) =>
        set((state) => {
          if (state.sessions[sessionId]) {
            state.sessions[sessionId].mode = mode;
          }
        }),

      setInputMode: (sessionId, mode) =>
        set((state) => {
          if (state.sessions[sessionId]) {
            state.sessions[sessionId].inputMode = mode;
          }
        }),

      handlePromptStart: (sessionId) =>
        set((state) => {
          // Finalize any pending command without exit code
          const pending = state.pendingCommand[sessionId];
          if (pending?.command) {
            const blockId = crypto.randomUUID();
            const block: CommandBlock = {
              id: blockId,
              sessionId,
              command: pending.command,
              output: pending.output,
              exitCode: null,
              startTime: pending.startTime,
              durationMs: null,
              workingDirectory: pending.workingDirectory,
              isCollapsed: false,
            };
            if (!state.commandBlocks[sessionId]) {
              state.commandBlocks[sessionId] = [];
            }
            state.commandBlocks[sessionId].push(block);

            // Also push to unified timeline
            if (!state.timelines[sessionId]) {
              state.timelines[sessionId] = [];
            }
            state.timelines[sessionId].push({
              id: blockId,
              type: "command",
              timestamp: pending.startTime,
              data: block,
            });
          }
          state.pendingCommand[sessionId] = null;
        }),

      handlePromptEnd: (_sessionId) => {
        // Ready for input - nothing to do for now
      },

      handleCommandStart: (sessionId, command) =>
        set((state) => {
          const session = state.sessions[sessionId];
          state.pendingCommand[sessionId] = {
            command,
            output: "",
            startTime: new Date().toISOString(),
            workingDirectory: session?.workingDirectory || "",
          };
        }),

      handleCommandEnd: (sessionId, exitCode) =>
        set((state) => {
          const pending = state.pendingCommand[sessionId];
          if (pending) {
            // Only create a command block if there was an actual command
            // Skip empty commands (e.g., just pressing Enter at prompt)
            if (pending.command) {
              const blockId = crypto.randomUUID();
              const block: CommandBlock = {
                id: blockId,
                sessionId,
                command: pending.command,
                output: pending.output,
                exitCode,
                startTime: pending.startTime,
                durationMs: Date.now() - new Date(pending.startTime).getTime(),
                workingDirectory: pending.workingDirectory,
                isCollapsed: false,
              };
              if (!state.commandBlocks[sessionId]) {
                state.commandBlocks[sessionId] = [];
              }
              state.commandBlocks[sessionId].push(block);

              // Also push to unified timeline
              if (!state.timelines[sessionId]) {
                state.timelines[sessionId] = [];
              }
              state.timelines[sessionId].push({
                id: blockId,
                type: "command",
                timestamp: pending.startTime,
                data: block,
              });
            }

            state.pendingCommand[sessionId] = null;
          }
        }),

      appendOutput: (sessionId, data) =>
        set((state) => {
          const pending = state.pendingCommand[sessionId];
          // Only append output if we have an active command (command_start was received)
          // This prevents capturing prompt text as command output
          if (pending) {
            pending.output += data;
          }
        }),

      toggleBlockCollapse: (blockId) =>
        set((state) => {
          // Update in legacy commandBlocks
          for (const blocks of Object.values(state.commandBlocks)) {
            const block = blocks.find((b) => b.id === blockId);
            if (block) {
              block.isCollapsed = !block.isCollapsed;
              break;
            }
          }
          // Also update in unified timeline
          for (const timeline of Object.values(state.timelines)) {
            const unifiedBlock = timeline.find((b) => b.type === "command" && b.id === blockId);
            if (unifiedBlock && unifiedBlock.type === "command") {
              unifiedBlock.data.isCollapsed = !unifiedBlock.data.isCollapsed;
              break;
            }
          }
        }),

      clearBlocks: (sessionId) =>
        set((state) => {
          state.commandBlocks[sessionId] = [];
          state.pendingCommand[sessionId] = null;
        }),

      // Agent actions
      addAgentMessage: (sessionId, message) =>
        set((state) => {
          if (!state.agentMessages[sessionId]) {
            state.agentMessages[sessionId] = [];
          }
          state.agentMessages[sessionId].push(message);

          // Also push to unified timeline
          if (!state.timelines[sessionId]) {
            state.timelines[sessionId] = [];
          }
          state.timelines[sessionId].push({
            id: message.id,
            type: "agent_message",
            timestamp: message.timestamp,
            data: message,
          });
        }),

      updateAgentStreaming: (sessionId, content) =>
        set((state) => {
          state.agentStreaming[sessionId] = content;
          // Also update streaming blocks - track offset to handle interleaved text
          if (!state.streamingBlocks[sessionId]) {
            state.streamingBlocks[sessionId] = [];
          }
          const blocks = state.streamingBlocks[sessionId];
          const offset = state.streamingTextOffset[sessionId] || 0;
          // Get the text for the current segment (since last tool call)
          const segmentText = content.substring(offset);

          if (!segmentText) return; // No text in current segment

          const lastBlock = blocks[blocks.length - 1];
          if (lastBlock && lastBlock.type === "text") {
            // Update the content of current text block with full segment
            lastBlock.content = segmentText;
          } else if (segmentText) {
            // Add new text block (after a tool block or as first block)
            blocks.push({ type: "text", content: segmentText });
          }
        }),

      clearAgentStreaming: (sessionId) =>
        set((state) => {
          state.agentStreaming[sessionId] = "";
          state.streamingBlocks[sessionId] = [];
          state.streamingTextOffset[sessionId] = 0;
        }),

      setAgentInitialized: (sessionId, initialized) =>
        set((state) => {
          state.agentInitialized[sessionId] = initialized;
        }),

      setAgentThinking: (sessionId, thinking) =>
        set((state) => {
          state.isAgentThinking[sessionId] = thinking;
        }),

      setPendingToolApproval: (sessionId, tool) =>
        set((state) => {
          state.pendingToolApproval[sessionId] = tool;
        }),

      markToolRequestProcessed: (requestId) =>
        set((state) => {
          state.processedToolRequests.add(requestId);
        }),

      isToolRequestProcessed: (requestId) => {
        return _get().processedToolRequests.has(requestId);
      },

      updateToolCallStatus: (sessionId, toolId, status, result) =>
        set((state) => {
          const messages = state.agentMessages[sessionId];
          if (messages) {
            for (const msg of messages) {
              const tool = msg.toolCalls?.find((t) => t.id === toolId);
              if (tool) {
                tool.status = status;
                if (result !== undefined) tool.result = result;
                break;
              }
            }
          }
        }),

      clearAgentMessages: (sessionId) =>
        set((state) => {
          state.agentMessages[sessionId] = [];
          state.agentStreaming[sessionId] = "";
        }),

      restoreAgentMessages: (sessionId, messages) =>
        set((state) => {
          state.agentMessages[sessionId] = messages;
          state.agentStreaming[sessionId] = "";
          // Also populate the timeline so messages appear in UnifiedTimeline
          if (!state.timelines[sessionId]) {
            state.timelines[sessionId] = [];
          }
          for (const message of messages) {
            state.timelines[sessionId].push({
              id: message.id,
              type: "agent_message",
              timestamp: message.timestamp,
              data: message,
            });
          }
        }),

      addActiveToolCall: (sessionId, toolCall) =>
        set((state) => {
          if (!state.activeToolCalls[sessionId]) {
            state.activeToolCalls[sessionId] = [];
          }
          state.activeToolCalls[sessionId].push({
            ...toolCall,
            status: "running",
            startedAt: new Date().toISOString(),
          });
        }),

      completeActiveToolCall: (sessionId, toolId, success, result) =>
        set((state) => {
          const tools = state.activeToolCalls[sessionId];
          if (tools) {
            const tool = tools.find((t) => t.id === toolId);
            if (tool) {
              tool.status = success ? "completed" : "error";
              tool.result = result;
              tool.completedAt = new Date().toISOString();
            }
          }
        }),

      clearActiveToolCalls: (sessionId) =>
        set((state) => {
          state.activeToolCalls[sessionId] = [];
        }),

      // Streaming blocks actions
      addStreamingToolBlock: (sessionId, toolCall) =>
        set((state) => {
          if (!state.streamingBlocks[sessionId]) {
            state.streamingBlocks[sessionId] = [];
          }

          const blocks = state.streamingBlocks[sessionId];
          const currentText = state.agentStreaming[sessionId] || "";
          const currentOffset = state.streamingTextOffset[sessionId] || 0;

          // Commit pending text as a text block BEFORE the tool block
          const pendingText = currentText.substring(currentOffset);
          if (pendingText) {
            const lastBlock = blocks[blocks.length - 1];
            if (lastBlock && lastBlock.type === "text") {
              lastBlock.content = pendingText;
            } else {
              blocks.push({ type: "text", content: pendingText });
            }
          }

          // Update offset to lock in current text segment
          state.streamingTextOffset[sessionId] = currentText.length;

          // Append the tool block after any pending text
          blocks.push({
            type: "tool",
            toolCall: {
              ...toolCall,
              status: "running",
              startedAt: new Date().toISOString(),
            },
          });
        }),

      updateStreamingToolBlock: (sessionId, toolId, success, result) =>
        set((state) => {
          const blocks = state.streamingBlocks[sessionId];
          if (blocks) {
            for (const block of blocks) {
              if (block.type === "tool" && block.toolCall.id === toolId) {
                block.toolCall.status = success ? "completed" : "error";
                block.toolCall.result = result;
                block.toolCall.completedAt = new Date().toISOString();
                break;
              }
            }
          }
        }),

      clearStreamingBlocks: (sessionId) =>
        set((state) => {
          state.streamingBlocks[sessionId] = [];
        }),

      // Timeline actions
      clearTimeline: (sessionId) =>
        set((state) => {
          state.timelines[sessionId] = [];
          // Also clear the legacy stores for consistency
          state.commandBlocks[sessionId] = [];
          state.pendingCommand[sessionId] = null;
          state.agentMessages[sessionId] = [];
          state.agentStreaming[sessionId] = "";
          state.streamingBlocks[sessionId] = [];
        }),

      // AI config actions
      setAiConfig: (config) =>
        set((state) => {
          state.aiConfig = { ...state.aiConfig, ...config };
        }),
    })),
    { name: "qbit" }
  )
);

// Stable empty arrays to avoid re-render loops
const EMPTY_BLOCKS: CommandBlock[] = [];
const EMPTY_MESSAGES: AgentMessage[] = [];

// Selectors
export const useActiveSession = () =>
  useStore((state) => {
    const id = state.activeSessionId;
    return id ? state.sessions[id] : null;
  });

export const useSessionBlocks = (sessionId: string) =>
  useStore((state) => state.commandBlocks[sessionId] ?? EMPTY_BLOCKS);

export const usePendingCommand = (sessionId: string) =>
  useStore((state) => state.pendingCommand[sessionId]);

export const useSessionMode = (sessionId: string) =>
  useStore((state) => state.sessions[sessionId]?.mode ?? "terminal");

export const useAgentMessages = (sessionId: string) =>
  useStore((state) => state.agentMessages[sessionId] ?? EMPTY_MESSAGES);

export const useAgentStreaming = (sessionId: string) =>
  useStore((state) => state.agentStreaming[sessionId] ?? "");

export const useAgentInitialized = (sessionId: string) =>
  useStore((state) => state.agentInitialized[sessionId] ?? false);

export const usePendingToolApproval = (sessionId: string) =>
  useStore((state) => state.pendingToolApproval[sessionId] ?? null);

// Timeline selectors
const EMPTY_TIMELINE: UnifiedBlock[] = [];

export const useSessionTimeline = (sessionId: string) =>
  useStore((state) => state.timelines[sessionId] ?? EMPTY_TIMELINE);

export const useInputMode = (sessionId: string) =>
  useStore((state) => state.sessions[sessionId]?.inputMode ?? "terminal");

// Active tool calls selector
const EMPTY_TOOL_CALLS: ActiveToolCall[] = [];

export const useActiveToolCalls = (sessionId: string) =>
  useStore((state) => state.activeToolCalls[sessionId] ?? EMPTY_TOOL_CALLS);

// Streaming blocks selector
const EMPTY_STREAMING_BLOCKS: StreamingBlock[] = [];

export const useStreamingBlocks = (sessionId: string) =>
  useStore((state) => state.streamingBlocks[sessionId] ?? EMPTY_STREAMING_BLOCKS);

// AI config selector
export const useAiConfig = () => useStore((state) => state.aiConfig);

// Agent thinking selector
export const useIsAgentThinking = (sessionId: string) =>
  useStore((state) => state.isAgentThinking[sessionId] ?? false);

// Helper function to clear conversation (both frontend and backend)
// This should be called instead of clearTimeline when you want to reset AI context
export async function clearConversation(sessionId: string): Promise<void> {
  // Clear frontend state
  useStore.getState().clearTimeline(sessionId);

  // Clear backend conversation history
  try {
    const { clearAiConversation } = await import("@/lib/ai");
    await clearAiConversation();
  } catch (error) {
    console.warn("Failed to clear backend conversation history:", error);
  }
}

// Helper function to restore a previous session (both frontend and backend)
export async function restoreSession(sessionId: string, identifier: string): Promise<void> {
  const { restoreAiSession } = await import("@/lib/ai");

  // Restore backend conversation history and get the session data
  const session = await restoreAiSession(identifier);

  // Convert session messages to AgentMessages for the UI
  const agentMessages: AgentMessage[] = session.messages
    .filter((msg) => msg.role === "user" || msg.role === "assistant")
    .map((msg, index) => ({
      id: `restored-${identifier}-${index}`,
      sessionId,
      role: msg.role as "user" | "assistant",
      content: msg.content,
      timestamp: index === 0 ? session.started_at : session.ended_at,
      isStreaming: false,
    }));

  // Clear existing state first
  useStore.getState().clearTimeline(sessionId);

  // Restore the messages to the store (this also populates the timeline)
  useStore.getState().restoreAgentMessages(sessionId, agentMessages);

  // Switch to agent mode since we're restoring an AI conversation
  useStore.getState().setInputMode(sessionId, "agent");
}

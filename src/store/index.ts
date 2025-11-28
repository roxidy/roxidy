import { create } from "zustand";
import { devtools } from "zustand/middleware";
import { immer } from "zustand/middleware/immer";

// Types
export type SessionMode = "terminal" | "agent";
export type InputMode = "terminal" | "agent";

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

export interface AgentMessage {
  id: string;
  sessionId: string;
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: string;
  isStreaming?: boolean;
  toolCalls?: ToolCall[];
}

export interface ToolCall {
  id: string;
  name: string;
  args: Record<string, unknown>;
  status: "pending" | "approved" | "denied" | "running" | "completed" | "error";
  result?: unknown;
}

interface PendingCommand {
  command: string | null;
  output: string;
  startTime: string;
  workingDirectory: string;
}

interface RoxidyState {
  // Sessions
  sessions: Record<string, Session>;
  activeSessionId: string | null;

  // Unified timeline (Phase 1)
  timelines: Record<string, UnifiedBlock[]>;

  // Terminal state (kept for backward compatibility)
  commandBlocks: Record<string, CommandBlock[]>;
  pendingCommand: Record<string, PendingCommand | null>;

  // Agent state (kept for backward compatibility)
  agentMessages: Record<string, AgentMessage[]>;
  agentStreaming: Record<string, string>;
  agentInitialized: Record<string, boolean>;
  pendingToolApproval: Record<string, ToolCall | null>;
  processedToolRequests: Set<string>; // Track processed request IDs to prevent duplicates

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

  // Timeline actions
  clearTimeline: (sessionId: string) => void;
}

export const useStore = create<RoxidyState>()(
  devtools(
    immer((set, _get) => ({
      sessions: {},
      activeSessionId: null,
      timelines: {},
      commandBlocks: {},
      pendingCommand: {},
      agentMessages: {},
      agentStreaming: {},
      agentInitialized: {},
      pendingToolApproval: {},
      processedToolRequests: new Set<string>(),

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
          state.agentInitialized[session.id] = false;
          state.pendingToolApproval[session.id] = null;
        }),

      removeSession: (sessionId) =>
        set((state) => {
          delete state.sessions[sessionId];
          delete state.timelines[sessionId];
          delete state.commandBlocks[sessionId];
          delete state.pendingCommand[sessionId];
          delete state.agentMessages[sessionId];
          delete state.agentStreaming[sessionId];
          delete state.agentInitialized[sessionId];
          delete state.pendingToolApproval[sessionId];

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
            const unifiedBlock = timeline.find(
              (b) => b.type === "command" && b.id === blockId
            );
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
        }),

      clearAgentStreaming: (sessionId) =>
        set((state) => {
          state.agentStreaming[sessionId] = "";
        }),

      setAgentInitialized: (sessionId, initialized) =>
        set((state) => {
          state.agentInitialized[sessionId] = initialized;
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

      // Timeline actions
      clearTimeline: (sessionId) =>
        set((state) => {
          state.timelines[sessionId] = [];
          // Also clear the legacy stores for consistency
          state.commandBlocks[sessionId] = [];
          state.pendingCommand[sessionId] = null;
          state.agentMessages[sessionId] = [];
          state.agentStreaming[sessionId] = "";
        }),
    })),
    { name: "roxidy" }
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

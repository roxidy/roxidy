# Frontend State Management

This document covers state management for the React frontend using Zustand.

## Why Zustand

- Lightweight (~1KB) compared to Redux
- No boilerplate, simple API
- Works great with TypeScript
- Easy integration with Tauri events
- Supports middleware (persist, devtools)

## Current Implementation

The store is implemented in a single file `src/store/index.ts` using Zustand with immer middleware:

```typescript
// src/store/index.ts (actual implementation)

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import { devtools } from "zustand/middleware";

// Types
export interface Session {
  id: string;
  name: string;
  workingDirectory: string;
  createdAt: string;
}

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

  // Terminal
  commandBlocks: Record<string, CommandBlock[]>;
  pendingCommand: Record<string, PendingCommand | null>;

  // Actions
  addSession: (session: Session) => void;
  removeSession: (sessionId: string) => void;
  setActiveSession: (sessionId: string) => void;
  updateWorkingDirectory: (sessionId: string, path: string) => void;

  // Terminal actions
  handlePromptStart: (sessionId: string) => void;
  handlePromptEnd: (sessionId: string) => void;
  handleCommandStart: (sessionId: string, command: string | null) => void;
  handleCommandEnd: (sessionId: string, exitCode: number) => void;
  appendOutput: (sessionId: string, data: string) => void;
  toggleBlockCollapse: (blockId: string) => void;
  clearBlocks: (sessionId: string) => void;
}

export const useStore = create<RoxidyState>()(
  devtools(
    immer((set, _get) => ({
      sessions: {},
      activeSessionId: null,
      commandBlocks: {},
      pendingCommand: {},
      // ... actions
    })),
    { name: "roxidy" }
  )
);
```

## Stable Selectors

The store includes stable selectors to prevent re-render loops:

```typescript
// Stable empty array to avoid re-render loops
const EMPTY_BLOCKS: CommandBlock[] = [];

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
```

## Tauri Event Integration

Events from the Rust backend are connected to the store via `useTauriEvents` hook:

```typescript
// src/hooks/useTauriEvents.ts (actual implementation)

export function useTauriEvents() {
  const {
    handlePromptStart,
    handlePromptEnd,
    handleCommandStart,
    handleCommandEnd,
    appendOutput,
    updateWorkingDirectory,
    removeSession,
  } = useStore();

  useEffect(() => {
    const unlisteners: Promise<UnlistenFn>[] = [];

    // Command block events
    unlisteners.push(
      listen<CommandBlockEvent>("command_block", (event) => {
        const { session_id, command, exit_code, event_type } = event.payload;

        switch (event_type) {
          case "prompt_start":
            handlePromptStart(session_id);
            break;
          case "prompt_end":
            handlePromptEnd(session_id);
            break;
          case "command_start":
            handleCommandStart(session_id, command);
            break;
          case "command_end":
            if (exit_code !== null) {
              handleCommandEnd(session_id, exit_code);
            }
            break;
        }
      })
    );

    // Terminal output - capture for command blocks
    unlisteners.push(
      listen<TerminalOutputEvent>("terminal_output", (event) => {
        appendOutput(event.payload.session_id, event.payload.data);
      })
    );

    // ... directory_changed and session_ended listeners

    return () => {
      unlisteners.forEach((p) => p.then((unlisten) => unlisten()));
    };
  }, [/* dependencies */]);
}
```

---

## Planned Store Structure (for AI integration)

The following slices are designed but not yet implemented:

## Sessions Slice (Current)

Manages terminal sessions (tabs).

```typescript
// src/store/sessions.ts

import { StateCreator } from "zustand";
import type { RoxidyStore } from "./index";

export interface Session {
  id: string;
  name: string;
  workingDirectory: string;
  createdAt: string;
  isActive: boolean;
}

export interface SessionsSlice {
  sessions: Record<string, Session>;
  activeSessionId: string | null;

  // Actions
  addSession: (session: Session) => void;
  removeSession: (sessionId: string) => void;
  setActiveSession: (sessionId: string) => void;
  updateSession: (sessionId: string, updates: Partial<Session>) => void;
  updateWorkingDirectory: (sessionId: string, path: string) => void;
}

export const createSessionsSlice: StateCreator<
  RoxidyStore,
  [["zustand/immer", never]],
  [],
  SessionsSlice
> = (set) => ({
  sessions: {},
  activeSessionId: null,

  addSession: (session) =>
    set((state) => {
      state.sessions[session.id] = session;
      state.activeSessionId = session.id;
    }),

  removeSession: (sessionId) =>
    set((state) => {
      delete state.sessions[sessionId];

      // Switch to another session if we closed the active one
      if (state.activeSessionId === sessionId) {
        const remaining = Object.keys(state.sessions);
        state.activeSessionId = remaining[0] ?? null;
      }
    }),

  setActiveSession: (sessionId) =>
    set((state) => {
      state.activeSessionId = sessionId;
    }),

  updateSession: (sessionId, updates) =>
    set((state) => {
      if (state.sessions[sessionId]) {
        Object.assign(state.sessions[sessionId], updates);
      }
    }),

  updateWorkingDirectory: (sessionId, path) =>
    set((state) => {
      if (state.sessions[sessionId]) {
        state.sessions[sessionId].workingDirectory = path;
      }
    }),
});
```

## Terminal Slice

Manages terminal output and command blocks per session.

```typescript
// src/store/terminal.ts

import { StateCreator } from "zustand";
import type { RoxidyStore } from "./index";

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

export interface TerminalSlice {
  // Command blocks per session
  commandBlocks: Record<string, CommandBlock[]>;

  // Currently executing command (for live output)
  pendingBlock: Record<string, Partial<CommandBlock> | null>;

  // Scroll position per session (for restoring when switching tabs)
  scrollPositions: Record<string, number>;

  // Actions
  addCommandBlock: (sessionId: string, block: CommandBlock) => void;
  updatePendingBlock: (sessionId: string, updates: Partial<CommandBlock>) => void;
  finalizePendingBlock: (sessionId: string, exitCode: number) => void;
  toggleBlockCollapse: (blockId: string) => void;
  clearBlocks: (sessionId: string) => void;
  setScrollPosition: (sessionId: string, position: number) => void;
}

export const createTerminalSlice: StateCreator<
  RoxidyStore,
  [["zustand/immer", never]],
  [],
  TerminalSlice
> = (set, get) => ({
  commandBlocks: {},
  pendingBlock: {},
  scrollPositions: {},

  addCommandBlock: (sessionId, block) =>
    set((state) => {
      if (!state.commandBlocks[sessionId]) {
        state.commandBlocks[sessionId] = [];
      }
      state.commandBlocks[sessionId].push(block);
    }),

  updatePendingBlock: (sessionId, updates) =>
    set((state) => {
      if (!state.pendingBlock[sessionId]) {
        state.pendingBlock[sessionId] = {
          sessionId,
          output: "",
          isCollapsed: false,
        };
      }
      Object.assign(state.pendingBlock[sessionId]!, updates);
    }),

  finalizePendingBlock: (sessionId, exitCode) =>
    set((state) => {
      const pending = state.pendingBlock[sessionId];
      if (pending) {
        const block: CommandBlock = {
          id: crypto.randomUUID(),
          sessionId,
          command: pending.command || "",
          output: pending.output || "",
          exitCode,
          startTime: pending.startTime || new Date().toISOString(),
          durationMs: pending.durationMs ?? null,
          workingDirectory: pending.workingDirectory || "",
          isCollapsed: false,
        };

        if (!state.commandBlocks[sessionId]) {
          state.commandBlocks[sessionId] = [];
        }
        state.commandBlocks[sessionId].push(block);
        state.pendingBlock[sessionId] = null;
      }
    }),

  toggleBlockCollapse: (blockId) =>
    set((state) => {
      for (const blocks of Object.values(state.commandBlocks)) {
        const block = blocks.find((b) => b.id === blockId);
        if (block) {
          block.isCollapsed = !block.isCollapsed;
          break;
        }
      }
    }),

  clearBlocks: (sessionId) =>
    set((state) => {
      state.commandBlocks[sessionId] = [];
      state.pendingBlock[sessionId] = null;
    }),

  setScrollPosition: (sessionId, position) =>
    set((state) => {
      state.scrollPositions[sessionId] = position;
    }),
});
```

## AI Slice

Manages AI conversations and streaming state.

```typescript
// src/store/ai.ts

import { StateCreator } from "zustand";
import type { RoxidyStore } from "./index";

export interface AIMessage {
  id: string;
  role: "user" | "assistant" | "tool";
  content: string;
  timestamp: string;
  toolCalls?: ToolCall[];
  toolResults?: ToolResult[];
}

export interface ToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
  status: "pending" | "approved" | "rejected" | "running" | "completed" | "failed";
}

export interface ToolResult {
  callId: string;
  result: unknown;
  success: boolean;
}

export interface AIConversation {
  sessionId: string;
  messages: AIMessage[];
}

export interface AISlice {
  conversations: Record<string, AIConversation>;
  isStreaming: Record<string, boolean>;
  streamingContent: Record<string, string>;
  pendingApproval: ToolCall | null;

  // Panel visibility
  aiPanelOpen: boolean;
  aiPanelWidth: number;

  // Actions
  addUserMessage: (sessionId: string, content: string) => void;
  startStreaming: (sessionId: string) => void;
  appendStreamContent: (sessionId: string, delta: string) => void;
  finishStreaming: (sessionId: string) => void;
  addToolCall: (sessionId: string, toolCall: ToolCall) => void;
  updateToolCallStatus: (sessionId: string, callId: string, status: ToolCall["status"]) => void;
  addToolResult: (sessionId: string, result: ToolResult) => void;
  setPendingApproval: (toolCall: ToolCall | null) => void;
  clearConversation: (sessionId: string) => void;
  toggleAIPanel: () => void;
  setAIPanelWidth: (width: number) => void;
}

export const createAISlice: StateCreator<
  RoxidyStore,
  [["zustand/immer", never]],
  [],
  AISlice
> = (set, get) => ({
  conversations: {},
  isStreaming: {},
  streamingContent: {},
  pendingApproval: null,
  aiPanelOpen: true,
  aiPanelWidth: 400,

  addUserMessage: (sessionId, content) =>
    set((state) => {
      if (!state.conversations[sessionId]) {
        state.conversations[sessionId] = { sessionId, messages: [] };
      }
      state.conversations[sessionId].messages.push({
        id: crypto.randomUUID(),
        role: "user",
        content,
        timestamp: new Date().toISOString(),
      });
    }),

  startStreaming: (sessionId) =>
    set((state) => {
      state.isStreaming[sessionId] = true;
      state.streamingContent[sessionId] = "";
    }),

  appendStreamContent: (sessionId, delta) =>
    set((state) => {
      state.streamingContent[sessionId] =
        (state.streamingContent[sessionId] || "") + delta;
    }),

  finishStreaming: (sessionId) =>
    set((state) => {
      const content = state.streamingContent[sessionId] || "";
      if (content) {
        if (!state.conversations[sessionId]) {
          state.conversations[sessionId] = { sessionId, messages: [] };
        }
        state.conversations[sessionId].messages.push({
          id: crypto.randomUUID(),
          role: "assistant",
          content,
          timestamp: new Date().toISOString(),
        });
      }
      state.isStreaming[sessionId] = false;
      state.streamingContent[sessionId] = "";
    }),

  addToolCall: (sessionId, toolCall) =>
    set((state) => {
      const messages = state.conversations[sessionId]?.messages || [];
      const lastMessage = messages[messages.length - 1];
      if (lastMessage?.role === "assistant") {
        if (!lastMessage.toolCalls) {
          lastMessage.toolCalls = [];
        }
        lastMessage.toolCalls.push(toolCall);
      }
    }),

  updateToolCallStatus: (sessionId, callId, status) =>
    set((state) => {
      const messages = state.conversations[sessionId]?.messages || [];
      for (const msg of messages) {
        const call = msg.toolCalls?.find((c) => c.id === callId);
        if (call) {
          call.status = status;
          break;
        }
      }
    }),

  addToolResult: (sessionId, result) =>
    set((state) => {
      const messages = state.conversations[sessionId]?.messages || [];
      const lastMessage = messages[messages.length - 1];
      if (lastMessage?.role === "assistant") {
        if (!lastMessage.toolResults) {
          lastMessage.toolResults = [];
        }
        lastMessage.toolResults.push(result);
      }
    }),

  setPendingApproval: (toolCall) =>
    set((state) => {
      state.pendingApproval = toolCall;
    }),

  clearConversation: (sessionId) =>
    set((state) => {
      if (state.conversations[sessionId]) {
        state.conversations[sessionId].messages = [];
      }
    }),

  toggleAIPanel: () =>
    set((state) => {
      state.aiPanelOpen = !state.aiPanelOpen;
    }),

  setAIPanelWidth: (width) =>
    set((state) => {
      state.aiPanelWidth = width;
    }),
});
```

## Settings Slice

Manages application settings with persistence.

```typescript
// src/store/settings.ts

import { StateCreator } from "zustand";
import type { RoxidyStore } from "./index";

export interface Settings {
  // AI
  aiProvider: string;
  aiModel: string;

  // Appearance
  theme: string;
  fontSize: number;
  fontFamily: string;

  // Terminal
  scrollbackLines: number;
  cursorStyle: "block" | "underline" | "bar";
  cursorBlink: boolean;

  // Behavior
  confirmClose: boolean;
  copyOnSelect: boolean;
}

export interface SettingsSlice {
  settings: Settings;
  settingsLoaded: boolean;

  // Actions
  loadSettings: (settings: Settings) => void;
  updateSetting: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

const defaultSettings: Settings = {
  aiProvider: "anthropic",
  aiModel: "claude-sonnet-4-20250514",
  theme: "roxidy-dark",
  fontSize: 14,
  fontFamily: "JetBrains Mono, Menlo, monospace",
  scrollbackLines: 10000,
  cursorStyle: "block",
  cursorBlink: true,
  confirmClose: true,
  copyOnSelect: false,
};

export const createSettingsSlice: StateCreator<
  RoxidyStore,
  [["zustand/immer", never]],
  [],
  SettingsSlice
> = (set) => ({
  settings: defaultSettings,
  settingsLoaded: false,

  loadSettings: (settings) =>
    set((state) => {
      state.settings = { ...defaultSettings, ...settings };
      state.settingsLoaded = true;
    }),

  updateSetting: (key, value) =>
    set((state) => {
      state.settings[key] = value;
    }),
});
```

## Theme Slice

Manages terminal and UI themes.

```typescript
// src/store/theme.ts

import { StateCreator } from "zustand";
import type { RoxidyStore } from "./index";

export interface TerminalColors {
  background: string;
  foreground: string;
  cursor: string;
  selection: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
}

export interface UIColors {
  panelBackground: string;
  border: string;
  commandBlockBackground: string;
  commandBlockBorder: string;
  successBadge: string;
  errorBadge: string;
  aiPanelBackground: string;
}

export interface Theme {
  id: string;
  name: string;
  colors: TerminalColors;
  ui: UIColors;
}

export interface ThemeSlice {
  themes: Record<string, Theme>;
  activeThemeId: string;

  // Computed
  activeTheme: Theme | null;

  // Actions
  loadThemes: (themes: Theme[]) => void;
  setTheme: (themeId: string) => void;
  addCustomTheme: (theme: Theme) => void;
  removeCustomTheme: (themeId: string) => void;
}

// Default theme (Tokyo Night inspired)
const defaultTheme: Theme = {
  id: "roxidy-dark",
  name: "Roxidy Dark",
  colors: {
    background: "#1a1b26",
    foreground: "#c0caf5",
    cursor: "#c0caf5",
    selection: "#33467c",
    black: "#15161e",
    red: "#f7768e",
    green: "#9ece6a",
    yellow: "#e0af68",
    blue: "#7aa2f7",
    magenta: "#bb9af7",
    cyan: "#7dcfff",
    white: "#a9b1d6",
    brightBlack: "#414868",
    brightRed: "#f7768e",
    brightGreen: "#9ece6a",
    brightYellow: "#e0af68",
    brightBlue: "#7aa2f7",
    brightMagenta: "#bb9af7",
    brightCyan: "#7dcfff",
    brightWhite: "#c0caf5",
  },
  ui: {
    panelBackground: "#16161e",
    border: "#27293d",
    commandBlockBackground: "#1f2335",
    commandBlockBorder: "#3b4261",
    successBadge: "#9ece6a",
    errorBadge: "#f7768e",
    aiPanelBackground: "#1a1b26",
  },
};

export const createThemeSlice: StateCreator<
  RoxidyStore,
  [["zustand/immer", never]],
  [],
  ThemeSlice
> = (set, get) => ({
  themes: { [defaultTheme.id]: defaultTheme },
  activeThemeId: defaultTheme.id,

  get activeTheme() {
    const state = get();
    return state.themes[state.activeThemeId] ?? null;
  },

  loadThemes: (themes) =>
    set((state) => {
      for (const theme of themes) {
        state.themes[theme.id] = theme;
      }
    }),

  setTheme: (themeId) =>
    set((state) => {
      if (state.themes[themeId]) {
        state.activeThemeId = themeId;
      }
    }),

  addCustomTheme: (theme) =>
    set((state) => {
      state.themes[theme.id] = theme;
    }),

  removeCustomTheme: (themeId) =>
    set((state) => {
      // Don't allow removing the default theme
      if (themeId !== defaultTheme.id) {
        delete state.themes[themeId];
        // Switch to default if we removed the active theme
        if (state.activeThemeId === themeId) {
          state.activeThemeId = defaultTheme.id;
        }
      }
    }),
});
```

## Type Serialization Notes

When passing data between Rust and TypeScript via Tauri IPC:

| Rust Type | TypeScript Type | Notes |
|-----------|-----------------|-------|
| `Uuid` | `string` | Serialized as hyphenated string (e.g., `"550e8400-e29b-41d4-a716-446655440000"`) |
| `DateTime<Utc>` | `string` | ISO 8601 format (e.g., `"2025-11-26T10:30:00Z"`) |
| `PathBuf` | `string` | Platform-specific path string |
| `Duration` | `number` | Milliseconds (use `chrono::Duration::num_milliseconds()`) |
| `Option<T>` | `T \| null` | Serde serializes `None` as `null` |
| `HashMap<K, V>` | `Record<K, V>` | Keys must be strings for JSON |

## Tauri Event Listeners

Hook to connect Tauri events to the store:

```typescript
// src/hooks/useTauriEvents.ts

import { useEffect } from "react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useStore } from "../store";

export function useTauriEvents() {
  const {
    addCommandBlock,
    updateWorkingDirectory,
    removeSession,
    appendStreamContent,
    finishStreaming,
    addToolCall,
    updateToolCallStatus,
    addToolResult,
    setPendingApproval,
  } = useStore();

  useEffect(() => {
    const unlisteners: Promise<UnlistenFn>[] = [];

    // Command block completed
    unlisteners.push(
      listen<{ sessionId: string; block: CommandBlock }>("command_block", (event) => {
        addCommandBlock(event.payload.sessionId, {
          ...event.payload.block,
          isCollapsed: false,
        });
      })
    );

    // Directory changed
    unlisteners.push(
      listen<{ sessionId: string; path: string }>("directory_changed", (event) => {
        updateWorkingDirectory(event.payload.sessionId, event.payload.path);
      })
    );

    // Session ended
    unlisteners.push(
      listen<{ sessionId: string }>("session_ended", (event) => {
        removeSession(event.payload.sessionId);
      })
    );

    // AI stream
    unlisteners.push(
      listen<AIStreamEvent>("ai_stream", (event) => {
        const { sessionId, delta, done, error } = event.payload;

        if (delta) {
          appendStreamContent(sessionId, delta);
        }

        if (done || error) {
          finishStreaming(sessionId);
        }
      })
    );

    // Tool started
    unlisteners.push(
      listen<ToolStartEvent>("tool_start", (event) => {
        const { sessionId, toolCallId, toolName, arguments: args, requiresApproval } = event.payload;

        const toolCall: ToolCall = {
          id: toolCallId,
          name: toolName,
          arguments: args,
          status: requiresApproval ? "pending" : "running",
        };

        addToolCall(sessionId, toolCall);

        if (requiresApproval) {
          setPendingApproval(toolCall);
        }
      })
    );

    // Tool result
    unlisteners.push(
      listen<ToolResultEvent>("tool_result", (event) => {
        const { sessionId, toolCallId, result, success } = event.payload;

        updateToolCallStatus(sessionId, toolCallId, success ? "completed" : "failed");
        addToolResult(sessionId, {
          callId: toolCallId,
          result,
          success,
        });
      })
    );

    // Cleanup
    return () => {
      unlisteners.forEach((p) => p.then((unlisten) => unlisten()));
    };
  }, []);
}
```

## Using the Store in Components

```typescript
// src/components/Terminal/BlockList.tsx

import { useStore } from "../../store";

export function BlockList({ sessionId }: { sessionId: string }) {
  // Only re-render when blocks for this session change
  const blocks = useStore(
    (state) => state.commandBlocks[sessionId] || []
  );
  const toggleCollapse = useStore((state) => state.toggleBlockCollapse);

  return (
    <div className="block-list">
      {blocks.map((block) => (
        <CommandBlockCard
          key={block.id}
          block={block}
          onToggleCollapse={() => toggleCollapse(block.id)}
        />
      ))}
    </div>
  );
}

// src/components/AI/AIPanel.tsx

export function AIPanel() {
  const activeSessionId = useStore((state) => state.activeSessionId);
  const conversation = useStore(
    (state) => activeSessionId ? state.conversations[activeSessionId] : null
  );
  const isStreaming = useStore(
    (state) => activeSessionId ? state.isStreaming[activeSessionId] : false
  );
  const streamingContent = useStore(
    (state) => activeSessionId ? state.streamingContent[activeSessionId] : ""
  );
  const panelOpen = useStore((state) => state.aiPanelOpen);
  const panelWidth = useStore((state) => state.aiPanelWidth);

  if (!panelOpen || !activeSessionId) return null;

  return (
    <aside className="ai-panel" style={{ width: panelWidth }}>
      <div className="messages">
        {conversation?.messages.map((msg) => (
          <ChatMessage key={msg.id} message={msg} />
        ))}
        {isStreaming && (
          <div className="streaming-message">
            <Markdown>{streamingContent}</Markdown>
            <span className="cursor blink" />
          </div>
        )}
      </div>
      <ChatInput sessionId={activeSessionId} disabled={isStreaming} />
    </aside>
  );
}
```

## Selector Patterns for Performance

```typescript
// src/store/selectors.ts

import { useStore } from "./index";
import { shallow } from "zustand/shallow";

// Memoized selectors to prevent unnecessary re-renders

export function useActiveSession() {
  return useStore((state) => {
    const id = state.activeSessionId;
    return id ? state.sessions[id] : null;
  });
}

export function useSessionBlocks(sessionId: string) {
  return useStore((state) => state.commandBlocks[sessionId] || []);
}

export function useAIState(sessionId: string) {
  return useStore(
    (state) => ({
      isStreaming: state.isStreaming[sessionId] || false,
      streamingContent: state.streamingContent[sessionId] || "",
      messages: state.conversations[sessionId]?.messages || [],
    }),
    shallow // Shallow compare to avoid re-renders
  );
}

export function useTerminalSettings() {
  return useStore(
    (state) => ({
      fontSize: state.settings.fontSize,
      fontFamily: state.settings.fontFamily,
      cursorStyle: state.settings.cursorStyle,
      cursorBlink: state.settings.cursorBlink,
    }),
    shallow
  );
}
```

## Initializing on App Start

```typescript
// src/App.tsx

import { useEffect } from "react";
import { useStore } from "./store";
import { useTauriEvents } from "./hooks/useTauriEvents";
import { settingsGetAll, ptyCreate } from "./lib/tauri";

function App() {
  const { loadSettings, addSession, settingsLoaded } = useStore();

  // Connect Tauri events to store
  useTauriEvents();

  // Load settings and create initial session
  useEffect(() => {
    async function init() {
      // Load settings from backend
      const settings = await settingsGetAll();
      loadSettings(settings);

      // Create initial terminal session
      const session = await ptyCreate();
      addSession({
        id: session.id,
        name: "Terminal",
        workingDirectory: session.workingDirectory,
        createdAt: new Date().toISOString(),
        isActive: true,
      });
    }

    init();
  }, []);

  if (!settingsLoaded) {
    return <LoadingScreen />;
  }

  return (
    <div className="app">
      <TabBar />
      <main className="main-content">
        <TerminalPane />
        <AIPanel />
      </main>
      <StatusBar />
    </div>
  );
}
```

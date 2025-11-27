# Agent/Terminal Mode Switching

This document outlines how to implement mode switching between "Agent" mode (AI-powered interactions) and "Terminal" mode (traditional shell commands) in Roxidy.

## Overview

Roxidy supports two interaction modes per session:

| Mode | Purpose | Input Handling |
|------|---------|----------------|
| **Terminal** | Execute shell commands directly | Sends input to PTY |
| **Agent** | Converse with AI coding assistant | Sends prompts to vtcode agent |

Users can switch modes at any time. The mode is per-session, allowing different tabs to operate in different modes simultaneously.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                              App                                     │
├─────────────────────────────────────────────────────────────────────┤
│  TabBar                                                              │
│  ├── Tab 1: Terminal Mode [⌘]                                       │
│  ├── Tab 2: Agent Mode [🤖]                                         │
│  └── [+] New Tab                                                    │
├─────────────────────────────────────────────────────────────────────┤
│  Content Area (based on mode)                                        │
│  ├── Terminal Mode: CommandBlockList (shell output blocks)          │
│  └── Agent Mode: AgentChatList (AI conversation messages)           │
├─────────────────────────────────────────────────────────────────────┤
│  UnifiedInput                                                        │
│  ├── Mode Toggle: [Terminal] [Agent]                                │
│  ├── Input Field (context-aware placeholder)                        │
│  └── Submit Button / Keyboard Hints                                 │
└─────────────────────────────────────────────────────────────────────┘
```

## Store Updates

### New Types

```typescript
// src/store/index.ts

export type SessionMode = "terminal" | "agent";

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

export interface Session {
  id: string;
  name: string;
  workingDirectory: string;
  createdAt: string;
  mode: SessionMode;  // NEW: current mode
}
```

### Updated State

```typescript
interface RoxidyState {
  // Existing
  sessions: Record<string, Session>;
  activeSessionId: string | null;
  commandBlocks: Record<string, CommandBlock[]>;
  pendingCommand: Record<string, PendingCommand | null>;

  // NEW: Agent state
  agentMessages: Record<string, AgentMessage[]>;
  agentStreaming: Record<string, string>;  // Accumulated streaming content
  agentInitialized: Record<string, boolean>;
  pendingToolApproval: Record<string, ToolCall | null>;

  // NEW: Actions
  setSessionMode: (sessionId: string, mode: SessionMode) => void;
  addAgentMessage: (sessionId: string, message: AgentMessage) => void;
  updateAgentStreaming: (sessionId: string, content: string) => void;
  clearAgentStreaming: (sessionId: string) => void;
  setAgentInitialized: (sessionId: string, initialized: boolean) => void;
  setPendingToolApproval: (sessionId: string, tool: ToolCall | null) => void;
  updateToolCallStatus: (sessionId: string, toolId: string, status: ToolCall["status"], result?: unknown) => void;
}
```

### Store Implementation

```typescript
// Add to existing store

setSessionMode: (sessionId, mode) =>
  set((state) => {
    if (state.sessions[sessionId]) {
      state.sessions[sessionId].mode = mode;
    }
  }),

addAgentMessage: (sessionId, message) =>
  set((state) => {
    if (!state.agentMessages[sessionId]) {
      state.agentMessages[sessionId] = [];
    }
    state.agentMessages[sessionId].push(message);
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

updateToolCallStatus: (sessionId, toolId, status, result) =>
  set((state) => {
    const messages = state.agentMessages[sessionId];
    if (messages) {
      for (const msg of messages) {
        const tool = msg.toolCalls?.find(t => t.id === toolId);
        if (tool) {
          tool.status = status;
          if (result !== undefined) tool.result = result;
          break;
        }
      }
    }
  }),
```

### Selectors

```typescript
export const useSessionMode = (sessionId: string) =>
  useStore((state) => state.sessions[sessionId]?.mode ?? "terminal");

export const useAgentMessages = (sessionId: string) =>
  useStore((state) => state.agentMessages[sessionId] ?? []);

export const useAgentStreaming = (sessionId: string) =>
  useStore((state) => state.agentStreaming[sessionId] ?? "");

export const useAgentInitialized = (sessionId: string) =>
  useStore((state) => state.agentInitialized[sessionId] ?? false);

export const usePendingToolApproval = (sessionId: string) =>
  useStore((state) => state.pendingToolApproval[sessionId] ?? null);
```

## Component Structure

### New Components

```
src/components/
├── ModeToggle/
│   ├── index.ts
│   └── ModeToggle.tsx          # Toggle between Terminal/Agent
├── AgentChat/
│   ├── index.ts
│   ├── AgentChatList.tsx       # List of AI conversation messages
│   ├── AgentMessage.tsx        # Single message bubble
│   ├── ToolCallCard.tsx        # Tool execution display
│   └── ToolApprovalDialog.tsx  # HITL approval modal
├── UnifiedInput/
│   ├── index.ts
│   └── UnifiedInput.tsx        # Mode-aware input component
└── AgentSetup/
    ├── index.ts
    └── AgentSetupDialog.tsx    # Configure API key, provider, model
```

### ModeToggle Component

```tsx
// src/components/ModeToggle/ModeToggle.tsx
import { Terminal, Bot } from "lucide-react";
import { cn } from "@/lib/utils";
import { useStore, useSessionMode } from "@/store";
import type { SessionMode } from "@/store";

interface ModeToggleProps {
  sessionId: string;
}

export function ModeToggle({ sessionId }: ModeToggleProps) {
  const mode = useSessionMode(sessionId);
  const setSessionMode = useStore((state) => state.setSessionMode);

  const options: { value: SessionMode; label: string; icon: typeof Terminal }[] = [
    { value: "terminal", label: "Terminal", icon: Terminal },
    { value: "agent", label: "Agent", icon: Bot },
  ];

  return (
    <div className="flex items-center bg-[#1f2335] rounded-lg p-0.5">
      {options.map(({ value, label, icon: Icon }) => (
        <button
          key={value}
          onClick={() => setSessionMode(sessionId, value)}
          className={cn(
            "flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm font-medium transition-colors",
            mode === value
              ? "bg-[#7aa2f7] text-[#1a1b26]"
              : "text-[#565f89] hover:text-[#c0caf5]"
          )}
        >
          <Icon className="w-4 h-4" />
          {label}
        </button>
      ))}
    </div>
  );
}
```

### UnifiedInput Component

```tsx
// src/components/UnifiedInput/UnifiedInput.tsx
import { useState, useRef, useCallback } from "react";
import { ChevronRight, Send, Loader2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { useStore, useSessionMode, useAgentStreaming } from "@/store";
import { ModeToggle } from "@/components/ModeToggle";
import { ptyWrite } from "@/lib/tauri";
import { sendPrompt } from "@/lib/ai";
import { toast } from "sonner";

interface UnifiedInputProps {
  sessionId: string;
  workingDirectory?: string;
}

export function UnifiedInput({ sessionId, workingDirectory }: UnifiedInputProps) {
  const [input, setInput] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const mode = useSessionMode(sessionId);
  const streaming = useAgentStreaming(sessionId);
  const addAgentMessage = useStore((state) => state.addAgentMessage);

  const isAgentBusy = mode === "agent" && (isSubmitting || streaming.length > 0);

  const handleSubmit = useCallback(async () => {
    if (!input.trim() || isAgentBusy) return;

    const value = input.trim();
    setInput("");

    if (mode === "terminal") {
      // Terminal mode: send to PTY
      await ptyWrite(sessionId, value + "\n");
    } else {
      // Agent mode: send to AI
      setIsSubmitting(true);

      // Add user message to store
      addAgentMessage(sessionId, {
        id: crypto.randomUUID(),
        sessionId,
        role: "user",
        content: value,
        timestamp: new Date().toISOString(),
      });

      try {
        await sendPrompt(value);
      } catch (error) {
        toast.error(`Agent error: ${error}`);
      } finally {
        setIsSubmitting(false);
      }
    }
  }, [input, mode, sessionId, isAgentBusy, addAgentMessage]);

  const handleKeyDown = useCallback(
    async (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        await handleSubmit();
        return;
      }

      // Terminal-specific shortcuts
      if (mode === "terminal") {
        if (e.key === "Tab") {
          e.preventDefault();
          await ptyWrite(sessionId, "\t");
          return;
        }
        if (e.ctrlKey && e.key === "c") {
          e.preventDefault();
          await ptyWrite(sessionId, "\x03");
          setInput("");
          return;
        }
      }

      // Agent-specific shortcuts
      if (mode === "agent") {
        // Ctrl+Enter to force submit even with empty input (for continuation)
        if (e.ctrlKey && e.key === "Enter") {
          e.preventDefault();
          await handleSubmit();
          return;
        }
      }
    },
    [mode, sessionId, handleSubmit]
  );

  const displayPath = workingDirectory?.replace(/^\/Users\/[^/]+/, "~") || "~";

  const placeholder = mode === "terminal"
    ? "Enter command..."
    : "Ask the AI assistant...";

  const icon = mode === "terminal" ? (
    <ChevronRight className="w-4 h-4 text-[#7aa2f7] flex-shrink-0" />
  ) : (
    <Send className="w-4 h-4 text-[#bb9af7] flex-shrink-0" />
  );

  return (
    <div className="bg-[#1a1b26] border-t border-[#1f2335] px-4 py-3">
      {/* Header row: path + mode toggle */}
      <div className="flex items-center justify-between mb-2">
        <div className="text-xs font-mono text-[#565f89] truncate">
          {displayPath}
        </div>
        <ModeToggle sessionId={sessionId} />
      </div>

      {/* Input row */}
      <div className="flex items-center gap-2">
        {icon}
        <input
          ref={inputRef}
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          disabled={isAgentBusy}
          className={cn(
            "flex-1 bg-transparent border-none outline-none",
            "font-mono text-sm text-[#c0caf5]",
            "placeholder:text-[#565f89]",
            "disabled:opacity-50"
          )}
          placeholder={placeholder}
          spellCheck={false}
          autoComplete="off"
        />

        {/* Submit button for agent mode */}
        {mode === "agent" && (
          <button
            onClick={handleSubmit}
            disabled={!input.trim() || isAgentBusy}
            className={cn(
              "p-2 rounded-md transition-colors",
              "disabled:opacity-50 disabled:cursor-not-allowed",
              "bg-[#7aa2f7] hover:bg-[#7aa2f7]/80 text-[#1a1b26]"
            )}
          >
            {isAgentBusy ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Send className="w-4 h-4" />
            )}
          </button>
        )}
      </div>

      {/* Keyboard hints */}
      <div className="flex items-center gap-3 mt-2 text-xs text-[#565f89]">
        {mode === "terminal" ? (
          <>
            <span>↵ Execute</span>
            <span>Tab Autocomplete</span>
            <span>^C Cancel</span>
          </>
        ) : (
          <>
            <span>↵ Send</span>
            <span>^↵ Continue</span>
          </>
        )}
      </div>
    </div>
  );
}
```

### AgentChatList Component

```tsx
// src/components/AgentChat/AgentChatList.tsx
import { useEffect, useRef } from "react";
import { useAgentMessages, useAgentStreaming } from "@/store";
import { AgentMessage } from "./AgentMessage";
import { Bot } from "lucide-react";

interface AgentChatListProps {
  sessionId: string;
}

export function AgentChatList({ sessionId }: AgentChatListProps) {
  const messages = useAgentMessages(sessionId);
  const streaming = useAgentStreaming(sessionId);
  const containerRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom
  useEffect(() => {
    if (containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [messages.length, streaming]);

  if (messages.length === 0 && !streaming) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-[#565f89]">
        <Bot className="w-12 h-12 mb-4 opacity-50" />
        <p className="text-sm">No messages yet</p>
        <p className="text-xs mt-1">Start a conversation with the AI assistant</p>
      </div>
    );
  }

  return (
    <div ref={containerRef} className="flex-1 overflow-auto p-4 space-y-4">
      {messages.map((message) => (
        <AgentMessage key={message.id} message={message} />
      ))}

      {/* Streaming indicator */}
      {streaming && (
        <div className="flex gap-3">
          <div className="w-8 h-8 rounded-full bg-[#bb9af7]/20 flex items-center justify-center flex-shrink-0">
            <Bot className="w-4 h-4 text-[#bb9af7]" />
          </div>
          <div className="flex-1 bg-[#1f2335] rounded-lg p-3">
            <p className="text-sm text-[#c0caf5] whitespace-pre-wrap">{streaming}</p>
            <span className="inline-block w-2 h-4 bg-[#7aa2f7] animate-pulse ml-1" />
          </div>
        </div>
      )}
    </div>
  );
}
```

### AgentMessage Component

```tsx
// src/components/AgentChat/AgentMessage.tsx
import { User, Bot } from "lucide-react";
import { cn } from "@/lib/utils";
import type { AgentMessage as AgentMessageType } from "@/store";
import { ToolCallCard } from "./ToolCallCard";

interface AgentMessageProps {
  message: AgentMessageType;
}

export function AgentMessage({ message }: AgentMessageProps) {
  const isUser = message.role === "user";
  const isSystem = message.role === "system";

  return (
    <div className={cn("flex gap-3", isUser && "flex-row-reverse")}>
      {/* Avatar */}
      <div
        className={cn(
          "w-8 h-8 rounded-full flex items-center justify-center flex-shrink-0",
          isUser ? "bg-[#7aa2f7]/20" : "bg-[#bb9af7]/20"
        )}
      >
        {isUser ? (
          <User className="w-4 h-4 text-[#7aa2f7]" />
        ) : (
          <Bot className="w-4 h-4 text-[#bb9af7]" />
        )}
      </div>

      {/* Content */}
      <div
        className={cn(
          "flex-1 max-w-[80%] rounded-lg p-3",
          isUser ? "bg-[#7aa2f7]/20" : "bg-[#1f2335]",
          isSystem && "bg-[#e0af68]/20 border border-[#e0af68]/30"
        )}
      >
        <p className="text-sm text-[#c0caf5] whitespace-pre-wrap">
          {message.content}
        </p>

        {/* Tool calls */}
        {message.toolCalls && message.toolCalls.length > 0 && (
          <div className="mt-3 space-y-2">
            {message.toolCalls.map((tool) => (
              <ToolCallCard key={tool.id} tool={tool} />
            ))}
          </div>
        )}

        {/* Timestamp */}
        <div className="mt-2 text-xs text-[#565f89]">
          {new Date(message.timestamp).toLocaleTimeString()}
        </div>
      </div>
    </div>
  );
}
```

### ToolCallCard Component

```tsx
// src/components/AgentChat/ToolCallCard.tsx
import {
  FileText, Terminal, Search, Globe,
  CheckCircle, XCircle, Loader2, AlertCircle
} from "lucide-react";
import { cn } from "@/lib/utils";
import type { ToolCall } from "@/store";

interface ToolCallCardProps {
  tool: ToolCall;
}

const toolIcons: Record<string, typeof FileText> = {
  read_file: FileText,
  write_file: FileText,
  list_files: FileText,
  grep_file: Search,
  run_pty_cmd: Terminal,
  shell: Terminal,
  web_fetch: Globe,
};

const statusConfig = {
  pending: { icon: AlertCircle, color: "text-[#e0af68]", label: "Pending approval" },
  approved: { icon: CheckCircle, color: "text-[#9ece6a]", label: "Approved" },
  denied: { icon: XCircle, color: "text-[#f7768e]", label: "Denied" },
  running: { icon: Loader2, color: "text-[#7aa2f7]", label: "Running", animate: true },
  completed: { icon: CheckCircle, color: "text-[#9ece6a]", label: "Completed" },
  error: { icon: XCircle, color: "text-[#f7768e]", label: "Error" },
};

export function ToolCallCard({ tool }: ToolCallCardProps) {
  const Icon = toolIcons[tool.name] || Terminal;
  const status = statusConfig[tool.status];
  const StatusIcon = status.icon;

  return (
    <div className="bg-[#16161e] rounded-md p-3 border border-[#27293d]">
      {/* Header */}
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          <Icon className="w-4 h-4 text-[#7aa2f7]" />
          <span className="text-sm font-mono text-[#c0caf5]">{tool.name}</span>
        </div>
        <div className={cn("flex items-center gap-1", status.color)}>
          <StatusIcon className={cn("w-3 h-3", status.animate && "animate-spin")} />
          <span className="text-xs">{status.label}</span>
        </div>
      </div>

      {/* Arguments (collapsed by default) */}
      {Object.keys(tool.args).length > 0 && (
        <details className="mt-2">
          <summary className="text-xs text-[#565f89] cursor-pointer hover:text-[#7aa2f7]">
            Arguments
          </summary>
          <pre className="mt-1 text-xs text-[#a9b1d6] bg-[#1a1b26] p-2 rounded overflow-x-auto">
            {JSON.stringify(tool.args, null, 2)}
          </pre>
        </details>
      )}

      {/* Result */}
      {tool.result !== undefined && (
        <details className="mt-2" open>
          <summary className="text-xs text-[#565f89] cursor-pointer hover:text-[#7aa2f7]">
            Result
          </summary>
          <pre className="mt-1 text-xs text-[#a9b1d6] bg-[#1a1b26] p-2 rounded overflow-x-auto max-h-40">
            {typeof tool.result === "string"
              ? tool.result
              : JSON.stringify(tool.result, null, 2)}
          </pre>
        </details>
      )}
    </div>
  );
}
```

### ToolApprovalDialog Component

```tsx
// src/components/AgentChat/ToolApprovalDialog.tsx
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { AlertTriangle, Terminal, FileText } from "lucide-react";
import { useStore, usePendingToolApproval } from "@/store";
import { executeTool } from "@/lib/ai";
import { toast } from "sonner";

interface ToolApprovalDialogProps {
  sessionId: string;
}

export function ToolApprovalDialog({ sessionId }: ToolApprovalDialogProps) {
  const tool = usePendingToolApproval(sessionId);
  const setPendingToolApproval = useStore((state) => state.setPendingToolApproval);
  const updateToolCallStatus = useStore((state) => state.updateToolCallStatus);

  if (!tool) return null;

  const handleApprove = async () => {
    setPendingToolApproval(sessionId, null);
    updateToolCallStatus(sessionId, tool.id, "running");

    try {
      const result = await executeTool(tool.name, tool.args);
      updateToolCallStatus(sessionId, tool.id, "completed", result);
    } catch (error) {
      updateToolCallStatus(sessionId, tool.id, "error", String(error));
      toast.error(`Tool execution failed: ${error}`);
    }
  };

  const handleDeny = () => {
    setPendingToolApproval(sessionId, null);
    updateToolCallStatus(sessionId, tool.id, "denied");
  };

  const isDangerous = ["write_file", "run_pty_cmd", "shell", "apply_patch"].includes(tool.name);

  return (
    <Dialog open={true} onOpenChange={() => handleDeny()}>
      <DialogContent className="bg-[#1f2335] border-[#27293d] text-[#c0caf5]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {isDangerous && <AlertTriangle className="w-5 h-5 text-[#e0af68]" />}
            Tool Approval Required
          </DialogTitle>
          <DialogDescription className="text-[#565f89]">
            The AI assistant wants to execute the following tool:
          </DialogDescription>
        </DialogHeader>

        <div className="bg-[#16161e] rounded-md p-4 border border-[#27293d]">
          <div className="flex items-center gap-2 mb-3">
            <Terminal className="w-4 h-4 text-[#7aa2f7]" />
            <span className="font-mono text-sm">{tool.name}</span>
          </div>

          <div className="text-xs text-[#565f89] mb-2">Arguments:</div>
          <pre className="text-xs text-[#a9b1d6] bg-[#1a1b26] p-2 rounded overflow-x-auto max-h-40">
            {JSON.stringify(tool.args, null, 2)}
          </pre>
        </div>

        {isDangerous && (
          <div className="flex items-start gap-2 p-3 bg-[#e0af68]/10 rounded-md border border-[#e0af68]/30">
            <AlertTriangle className="w-4 h-4 text-[#e0af68] flex-shrink-0 mt-0.5" />
            <p className="text-xs text-[#e0af68]">
              This tool can modify files or execute commands. Review carefully before approving.
            </p>
          </div>
        )}

        <DialogFooter>
          <Button
            variant="outline"
            onClick={handleDeny}
            className="border-[#3b4261] text-[#c0caf5] hover:bg-[#3b4261]"
          >
            Deny
          </Button>
          <Button
            onClick={handleApprove}
            className={isDangerous
              ? "bg-[#e0af68] hover:bg-[#e0af68]/80 text-[#1a1b26]"
              : "bg-[#7aa2f7] hover:bg-[#7aa2f7]/80 text-[#1a1b26]"
            }
          >
            Approve
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

## Updated App.tsx

```tsx
// src/App.tsx
import { useEffect, useState, useCallback, useRef } from "react";
import { useStore, useSessionBlocks, useSessionMode, useAgentMessages } from "./store";
import { useTauriEvents } from "./hooks/useTauriEvents";
import { useAiEvents } from "./hooks/useAiEvents";
import { ptyCreate, shellIntegrationStatus, shellIntegrationInstall } from "./lib/tauri";
import { CommandBlockList } from "./components/CommandBlock";
import { AgentChatList } from "./components/AgentChat";
import { ToolApprovalDialog } from "./components/AgentChat/ToolApprovalDialog";
import { UnifiedInput } from "./components/UnifiedInput";
import { TabBar } from "./components/TabBar";
import { Toaster, toast } from "sonner";

function ContentArea({ sessionId }: { sessionId: string }) {
  const mode = useSessionMode(sessionId);
  const blocks = useSessionBlocks(sessionId);
  const messages = useAgentMessages(sessionId);

  if (mode === "terminal") {
    if (blocks.length === 0) {
      return (
        <div className="flex items-center justify-center h-full text-[#565f89] text-sm">
          <div className="text-center">
            <p>No command blocks yet</p>
            <p className="text-xs mt-1">Run commands in the terminal below</p>
          </div>
        </div>
      );
    }
    return <CommandBlockList sessionId={sessionId} />;
  }

  // Agent mode
  return <AgentChatList sessionId={sessionId} />;
}

function App() {
  const { addSession, activeSessionId, sessions } = useStore();
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const contentRef = useRef<HTMLDivElement>(null);

  const activeSession = activeSessionId ? sessions[activeSessionId] : null;
  const workingDirectory = activeSession?.workingDirectory;

  // Connect Tauri events to store
  useTauriEvents();

  // Connect AI events to store
  useAiEvents();

  const handleNewTab = useCallback(async () => {
    try {
      const session = await ptyCreate();
      addSession({
        id: session.id,
        name: "Terminal",
        workingDirectory: session.working_directory,
        createdAt: new Date().toISOString(),
        mode: "terminal",  // Default to terminal mode
      });
    } catch (e) {
      console.error("Failed to create new tab:", e);
      toast.error("Failed to create new tab");
    }
  }, [addSession]);

  useEffect(() => {
    async function init() {
      try {
        const status = await shellIntegrationStatus();
        if (status.type === "NotInstalled") {
          toast.info("Installing shell integration...");
          await shellIntegrationInstall();
          toast.success("Shell integration installed!");
        } else if (status.type === "Outdated") {
          toast.info("Updating shell integration...");
          await shellIntegrationInstall();
          toast.success("Shell integration updated!");
        }

        const session = await ptyCreate();
        addSession({
          id: session.id,
          name: "Terminal",
          workingDirectory: session.working_directory,
          createdAt: new Date().toISOString(),
          mode: "terminal",
        });

        setIsLoading(false);
      } catch (e) {
        console.error("Failed to initialize:", e);
        setError(e instanceof Error ? e.message : String(e));
        setIsLoading(false);
      }
    }

    init();
  }, [addSession]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "t") {
        e.preventDefault();
        handleNewTab();
      }

      // Cmd+1/2 to switch modes
      if ((e.metaKey || e.ctrlKey) && activeSessionId) {
        if (e.key === "1") {
          e.preventDefault();
          useStore.getState().setSessionMode(activeSessionId, "terminal");
        } else if (e.key === "2") {
          e.preventDefault();
          useStore.getState().setSessionMode(activeSessionId, "agent");
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleNewTab, activeSessionId]);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-screen bg-[#1a1b26]">
        <div className="text-[#c0caf5] text-lg">Loading...</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-screen bg-[#1a1b26]">
        <div className="text-[#f7768e] text-lg">Error: {error}</div>
      </div>
    );
  }

  return (
    <div className="h-screen w-screen bg-[#1a1b26] flex flex-col overflow-hidden">
      <TabBar onNewTab={handleNewTab} />

      <div className="flex-1 min-h-0 flex flex-col">
        {activeSessionId ? (
          <>
            <div ref={contentRef} className="flex-1 overflow-auto bg-[#1a1b26]">
              <ContentArea sessionId={activeSessionId} />
            </div>

            <UnifiedInput
              sessionId={activeSessionId}
              workingDirectory={workingDirectory}
            />

            {/* Tool approval dialog */}
            <ToolApprovalDialog sessionId={activeSessionId} />
          </>
        ) : (
          <div className="flex items-center justify-center h-full">
            <span className="text-[#565f89]">No active session</span>
          </div>
        )}
      </div>

      <Toaster
        position="bottom-right"
        theme="dark"
        toastOptions={{
          style: {
            background: "#1f2335",
            border: "1px solid #3b4261",
            color: "#c0caf5",
          },
        }}
      />
    </div>
  );
}

export default App;
```

## AI Event Hook

```typescript
// src/hooks/useAiEvents.ts
import { useEffect } from "react";
import { useStore } from "@/store";
import { onAiEvent, type AiEvent } from "@/lib/ai";

export function useAiEvents() {
  const {
    addAgentMessage,
    updateAgentStreaming,
    clearAgentStreaming,
    setPendingToolApproval,
    activeSessionId,
  } = useStore();

  useEffect(() => {
    const handleEvent = (event: AiEvent) => {
      if (!activeSessionId) return;

      switch (event.type) {
        case "started":
          clearAgentStreaming(activeSessionId);
          break;

        case "text_delta":
          updateAgentStreaming(activeSessionId, event.accumulated);
          break;

        case "tool_request":
          setPendingToolApproval(activeSessionId, {
            id: event.request_id,
            name: event.tool_name,
            args: event.args as Record<string, unknown>,
            status: "pending",
          });
          break;

        case "completed":
          // Finalize streaming content as assistant message
          const streaming = useStore.getState().agentStreaming[activeSessionId];
          if (streaming || event.response) {
            addAgentMessage(activeSessionId, {
              id: crypto.randomUUID(),
              sessionId: activeSessionId,
              role: "assistant",
              content: event.response || streaming,
              timestamp: new Date().toISOString(),
            });
          }
          clearAgentStreaming(activeSessionId);
          break;

        case "error":
          addAgentMessage(activeSessionId, {
            id: crypto.randomUUID(),
            sessionId: activeSessionId,
            role: "system",
            content: `Error: ${event.message}`,
            timestamp: new Date().toISOString(),
          });
          clearAgentStreaming(activeSessionId);
          break;
      }
    };

    const unlistenPromise = onAiEvent(handleEvent);

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, [
    activeSessionId,
    addAgentMessage,
    updateAgentStreaming,
    clearAgentStreaming,
    setPendingToolApproval,
  ]);
}
```

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd+T` | New tab |
| `Cmd+1` | Switch to Terminal mode |
| `Cmd+2` | Switch to Agent mode |
| `Enter` | Execute command / Send message |
| `Tab` | Autocomplete (Terminal mode only) |
| `Ctrl+C` | Cancel/Interrupt (Terminal mode only) |
| `Ctrl+Enter` | Continue conversation (Agent mode) |

## Tab Indicator Update

Update the TabBar to show the current mode:

```tsx
// In TabBar.tsx - update Tab component
function Tab({ session, index, isActive, onSelect, onClose, canClose }: TabProps) {
  const dirName = session.workingDirectory.split("/").pop() || "Terminal";
  const modeIcon = session.mode === "agent" ? "🤖" : "⌘";

  return (
    <div
      onClick={onSelect}
      className={cn(
        "group flex items-center gap-2 px-3 h-7 rounded cursor-pointer transition-colors min-w-0 max-w-[200px]",
        isActive
          ? "bg-[#1a1b26] text-[#c0caf5]"
          : "text-[#565f89] hover:bg-[#1f2335] hover:text-[#a9b1d6]"
      )}
    >
      {/* Mode indicator */}
      <span className="flex-shrink-0 text-xs">{modeIcon}</span>

      {/* Tab name */}
      <span className="truncate text-sm">{dirName}</span>

      {/* Close button */}
      {canClose && (
        <button
          onClick={onClose}
          className={cn(
            "flex-shrink-0 p-0.5 rounded opacity-0 group-hover:opacity-100 transition-opacity",
            "hover:bg-[#3b4261] text-[#565f89] hover:text-[#c0caf5]"
          )}
        >
          <X className="w-3 h-3" />
        </button>
      )}
    </div>
  );
}
```

## Implementation Checklist

1. [x] Update store with new types and actions
2. [x] Create `ModeToggle` component
3. [x] Create `UnifiedInput` component (replace `CommandInput`)
4. [x] Create `AgentChatList` component
5. [x] Create `AgentMessage` component
6. [x] Create `ToolCallCard` component
7. [x] Create `ToolApprovalDialog` component
8. [x] Create `useAiEvents` hook
9. [x] Update `App.tsx` with mode-aware content rendering
10. [x] Update `TabBar` with mode indicators
11. [x] Add keyboard shortcuts for mode switching
12. [ ] Test Terminal mode (existing functionality)
13. [ ] Test Agent mode with vtcode integration
14. [ ] Test tool approval flow
15. [ ] Implement Tauri backend AI commands (see vtcode-integration.md)

## Future Enhancements

- **Agent Setup Dialog**: Configure API keys, provider, model per-session
- **Context Sharing**: Allow agent to access terminal history
- **Hybrid Commands**: Detect when user wants agent to run a command
- **Session Persistence**: Save/restore agent conversations
- **Multi-turn Tool Chains**: Handle sequential tool executions

import { useState, useRef, useEffect, useCallback } from "react";
import { ChevronRight, Send, Loader2, Bot } from "lucide-react";
import { cn } from "@/lib/utils";
import { useStore, useSessionMode, useAgentStreaming } from "@/store";
import { ModeToggle } from "@/components/ModeToggle";
import { ptyWrite } from "@/lib/tauri";
import { toast } from "sonner";

interface UnifiedInputProps {
  sessionId: string;
  workingDirectory?: string;
}

// Commands that require full terminal (interactive programs)
const INTERACTIVE_COMMANDS = [
  "vim", "vi", "nvim", "nano", "emacs", "pico",
  "less", "more", "man",
  "htop", "top", "btop",
  "ssh", "telnet", "ftp", "sftp",
  "python", "python3", "node", "irb", "ruby", "ghci",
  "mysql", "psql", "sqlite3", "redis-cli", "mongo",
  "tmux", "screen",
  "watch",
];

export function UnifiedInput({ sessionId, workingDirectory }: UnifiedInputProps) {
  const [input, setInput] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [history, setHistory] = useState<string[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

  const mode = useSessionMode(sessionId);
  const streaming = useAgentStreaming(sessionId);
  const addAgentMessage = useStore((state) => state.addAgentMessage);

  const isAgentBusy = mode === "agent" && (isSubmitting || streaming.length > 0);

  // Focus input on mount and mode change
  useEffect(() => {
    inputRef.current?.focus();
  }, [mode]);

  // Check if command is interactive and needs full terminal
  const isInteractiveCommand = useCallback((cmd: string) => {
    const firstWord = cmd.trim().split(/\s+/)[0];
    return INTERACTIVE_COMMANDS.includes(firstWord);
  }, []);

  const handleSubmit = useCallback(async () => {
    if (!input.trim() || isAgentBusy) return;

    const value = input.trim();
    setInput("");
    setHistoryIndex(-1);

    if (mode === "terminal") {
      // Terminal mode: send to PTY
      // Block interactive commands for now
      if (isInteractiveCommand(value)) {
        const cmd = value.split(/\s+/)[0];
        toast.error(`Interactive command "${cmd}" is not supported yet`);
        return;
      }

      // Add to history
      setHistory((prev) => [...prev, value]);

      // Send command + newline to PTY
      await ptyWrite(sessionId, value + "\n");
    } else {
      // Agent mode: send to AI
      setIsSubmitting(true);

      // Add to history
      setHistory((prev) => [...prev, value]);

      // Add user message to store
      addAgentMessage(sessionId, {
        id: crypto.randomUUID(),
        sessionId,
        role: "user",
        content: value,
        timestamp: new Date().toISOString(),
      });

      // TODO: Actually send to AI backend
      // For now, just simulate with a timeout
      try {
        // This will be replaced with actual AI call:
        // await sendPrompt(value);

        // Placeholder: simulate response after delay
        setTimeout(() => {
          addAgentMessage(sessionId, {
            id: crypto.randomUUID(),
            sessionId,
            role: "assistant",
            content: `AI integration not yet connected. You said: "${value}"`,
            timestamp: new Date().toISOString(),
          });
          setIsSubmitting(false);
        }, 500);
      } catch (error) {
        toast.error(`Agent error: ${error}`);
        setIsSubmitting(false);
      }
    }
  }, [input, mode, sessionId, isAgentBusy, addAgentMessage, isInteractiveCommand]);

  const handleKeyDown = useCallback(
    async (e: React.KeyboardEvent<HTMLInputElement>) => {
      // Handle Enter - execute/send
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        await handleSubmit();
        return;
      }

      // Terminal-specific shortcuts
      if (mode === "terminal") {
        // Handle Tab - send to PTY for completion
        if (e.key === "Tab") {
          e.preventDefault();
          await ptyWrite(sessionId, "\t");
          return;
        }

        // Handle Up arrow - history navigation
        if (e.key === "ArrowUp") {
          e.preventDefault();
          if (history.length > 0) {
            const newIndex = historyIndex < history.length - 1 ? historyIndex + 1 : historyIndex;
            setHistoryIndex(newIndex);
            setInput(history[history.length - 1 - newIndex] || "");
          }
          return;
        }

        // Handle Down arrow - history navigation
        if (e.key === "ArrowDown") {
          e.preventDefault();
          if (historyIndex > 0) {
            const newIndex = historyIndex - 1;
            setHistoryIndex(newIndex);
            setInput(history[history.length - 1 - newIndex] || "");
          } else if (historyIndex === 0) {
            setHistoryIndex(-1);
            setInput("");
          }
          return;
        }

        // Handle Ctrl+C - send interrupt
        if (e.ctrlKey && e.key === "c") {
          e.preventDefault();
          await ptyWrite(sessionId, "\x03");
          setInput("");
          return;
        }

        // Handle Ctrl+D - send EOF
        if (e.ctrlKey && e.key === "d") {
          e.preventDefault();
          await ptyWrite(sessionId, "\x04");
          return;
        }

        // Handle Ctrl+L - clear
        if (e.ctrlKey && e.key === "l") {
          e.preventDefault();
          await ptyWrite(sessionId, "\x0c");
          return;
        }
      }

      // Agent-specific shortcuts
      if (mode === "agent") {
        // Up/Down for history in agent mode too
        if (e.key === "ArrowUp") {
          e.preventDefault();
          if (history.length > 0) {
            const newIndex = historyIndex < history.length - 1 ? historyIndex + 1 : historyIndex;
            setHistoryIndex(newIndex);
            setInput(history[history.length - 1 - newIndex] || "");
          }
          return;
        }

        if (e.key === "ArrowDown") {
          e.preventDefault();
          if (historyIndex > 0) {
            const newIndex = historyIndex - 1;
            setHistoryIndex(newIndex);
            setInput(history[history.length - 1 - newIndex] || "");
          } else if (historyIndex === 0) {
            setHistoryIndex(-1);
            setInput("");
          }
          return;
        }
      }
    },
    [mode, sessionId, handleSubmit, history, historyIndex]
  );

  const displayPath = workingDirectory?.replace(/^\/Users\/[^/]+/, "~") || "~";

  const placeholder = mode === "terminal"
    ? "Enter command..."
    : "Ask the AI assistant...";

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
        {mode === "terminal" ? (
          <ChevronRight className="w-4 h-4 text-[#7aa2f7] flex-shrink-0" />
        ) : (
          <Bot className="w-4 h-4 text-[#bb9af7] flex-shrink-0" />
        )}

        <input
          ref={inputRef}
          type="text"
          value={input}
          onChange={(e) => {
            setInput(e.target.value);
            setHistoryIndex(-1);
          }}
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
          autoCorrect="off"
          autoCapitalize="off"
        />

        {/* Submit button for agent mode */}
        {mode === "agent" && (
          <button
            onClick={handleSubmit}
            disabled={!input.trim() || isAgentBusy}
            className={cn(
              "p-2 rounded-md transition-colors",
              "disabled:opacity-50 disabled:cursor-not-allowed",
              "bg-[#bb9af7] hover:bg-[#bb9af7]/80 text-[#1a1b26]"
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
      <div className="flex items-center gap-3 mt-2 text-[10px] text-[#565f89]">
        {mode === "terminal" ? (
          <>
            <span><kbd className="px-1 py-0.5 bg-[#1f2335] rounded text-[#7aa2f7]">↵</kbd> Execute</span>
            <span><kbd className="px-1 py-0.5 bg-[#1f2335] rounded text-[#7aa2f7]">Tab</kbd> Autocomplete</span>
            <span><kbd className="px-1 py-0.5 bg-[#1f2335] rounded text-[#7aa2f7]">^C</kbd> Cancel</span>
            <span><kbd className="px-1 py-0.5 bg-[#1f2335] rounded text-[#7aa2f7]">⌘2</kbd> Agent mode</span>
          </>
        ) : (
          <>
            <span><kbd className="px-1 py-0.5 bg-[#1f2335] rounded text-[#bb9af7]">↵</kbd> Send</span>
            <span><kbd className="px-1 py-0.5 bg-[#1f2335] rounded text-[#bb9af7]">↑↓</kbd> History</span>
            <span><kbd className="px-1 py-0.5 bg-[#1f2335] rounded text-[#bb9af7]">⌘1</kbd> Terminal mode</span>
          </>
        )}
      </div>
    </div>
  );
}

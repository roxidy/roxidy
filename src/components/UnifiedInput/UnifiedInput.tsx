import { Bot, ChevronRight, Loader2, Send, Terminal } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { sendPrompt } from "@/lib/ai";
import { ptyWrite } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useAgentStreaming, useInputMode, useStore } from "@/store";

interface UnifiedInputProps {
  sessionId: string;
  workingDirectory?: string;
}

// Commands that require full terminal (interactive programs)
const INTERACTIVE_COMMANDS = [
  "vim",
  "vi",
  "nvim",
  "nano",
  "emacs",
  "pico",
  "less",
  "more",
  "man",
  "htop",
  "top",
  "btop",
  "ssh",
  "telnet",
  "ftp",
  "sftp",
  "python",
  "python3",
  "node",
  "irb",
  "ruby",
  "ghci",
  "mysql",
  "psql",
  "sqlite3",
  "redis-cli",
  "mongo",
  "tmux",
  "screen",
  "watch",
];

export function UnifiedInput({ sessionId, workingDirectory }: UnifiedInputProps) {
  const [input, setInput] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [history, setHistory] = useState<string[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

  // Use inputMode for unified input toggle (not session mode)
  const inputMode = useInputMode(sessionId);
  const setInputMode = useStore((state) => state.setInputMode);
  const streaming = useAgentStreaming(sessionId);
  const addAgentMessage = useStore((state) => state.addAgentMessage);
  const agentMessages = useStore((state) => state.agentMessages[sessionId] ?? []);

  const isAgentBusy = inputMode === "agent" && (isSubmitting || streaming.length > 0);

  // Reset isSubmitting when AI response completes
  const prevMessagesLengthRef = useRef(agentMessages.length);
  useEffect(() => {
    // If a new message was added and we were submitting, check if it's from assistant/system
    if (agentMessages.length > prevMessagesLengthRef.current && isSubmitting) {
      const lastMessage = agentMessages[agentMessages.length - 1];
      // Reset if assistant or system (error) responded
      if (lastMessage && (lastMessage.role === "assistant" || lastMessage.role === "system")) {
        setIsSubmitting(false);
      }
    }
    prevMessagesLengthRef.current = agentMessages.length;
  }, [agentMessages, isSubmitting]);

  // Focus input on mount
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Toggle input mode
  const toggleInputMode = useCallback(() => {
    setInputMode(sessionId, inputMode === "terminal" ? "agent" : "terminal");
  }, [sessionId, inputMode, setInputMode]);

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

    if (inputMode === "terminal") {
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
      await ptyWrite(sessionId, `${value}\n`);
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

      // Send to AI backend - response will come via useAiEvents hook
      try {
        await sendPrompt(value);
        // Response will be handled by useAiEvents when AI completes
        // Don't set isSubmitting to false here - wait for completed/error event
      } catch (error) {
        toast.error(`Agent error: ${error}`);
        setIsSubmitting(false);
      }
    }
  }, [input, inputMode, sessionId, isAgentBusy, addAgentMessage, isInteractiveCommand]);

  const handleKeyDown = useCallback(
    async (e: React.KeyboardEvent<HTMLInputElement>) => {
      // Handle Enter - execute/send
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        await handleSubmit();
        return;
      }

      // Cmd+Shift+T to toggle input mode
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "t") {
        e.preventDefault();
        toggleInputMode();
        return;
      }

      // Terminal-specific shortcuts
      if (inputMode === "terminal") {
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
      if (inputMode === "agent") {
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
    [inputMode, sessionId, handleSubmit, history, historyIndex, toggleInputMode]
  );

  const displayPath = workingDirectory?.replace(/^\/Users\/[^/]+/, "~") || "~";

  const placeholder = inputMode === "terminal" ? "Enter command..." : "Ask the AI assistant...";

  return (
    <div className="bg-[#1a1b26] border-t border-[#1f2335] px-4 py-3">
      {/* Header row: path + input mode toggle */}
      <div className="flex items-center justify-between mb-2">
        <div className="text-xs font-mono text-[#565f89] truncate">{displayPath}</div>

        {/* Input mode toggle button */}
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="outline"
                size="sm"
                onClick={toggleInputMode}
                className={cn(
                  "h-7 px-2 gap-1.5 border transition-colors",
                  inputMode === "terminal"
                    ? "bg-[#7aa2f7]/10 border-[#7aa2f7]/30 text-[#7aa2f7] hover:bg-[#7aa2f7]/20"
                    : "bg-[#bb9af7]/10 border-[#bb9af7]/30 text-[#bb9af7] hover:bg-[#bb9af7]/20"
                )}
              >
                {inputMode === "terminal" ? (
                  <>
                    <Terminal className="w-3.5 h-3.5" />
                    <span className="text-xs">Terminal</span>
                  </>
                ) : (
                  <>
                    <Bot className="w-3.5 h-3.5" />
                    <span className="text-xs">Agent</span>
                  </>
                )}
              </Button>
            </TooltipTrigger>
            <TooltipContent side="left" className="text-xs">
              <p>Toggle input mode</p>
              <p className="text-[#565f89]">⌘⇧T</p>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
      </div>

      {/* Input row */}
      <div className="flex items-center gap-2">
        {inputMode === "terminal" ? (
          <ChevronRight className="w-4 h-4 text-[#7aa2f7] flex-shrink-0" />
        ) : (
          <Bot className="w-4 h-4 text-[#bb9af7] flex-shrink-0" />
        )}

        <Input
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
            "flex-1 h-auto py-0 px-0",
            "bg-transparent border-none shadow-none",
            "font-mono text-sm text-[#c0caf5]",
            "placeholder:text-[#565f89]",
            "focus-visible:ring-0 focus-visible:border-none",
            "disabled:opacity-50"
          )}
          placeholder={placeholder}
          spellCheck={false}
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
        />

        {/* Submit button for agent mode */}
        {inputMode === "agent" && (
          <Button
            onClick={handleSubmit}
            disabled={!input.trim() || isAgentBusy}
            size="icon-sm"
            className="bg-[#bb9af7] hover:bg-[#bb9af7]/80 text-[#1a1b26]"
          >
            {isAgentBusy ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Send className="w-4 h-4" />
            )}
          </Button>
        )}
      </div>

      {/* Keyboard hints */}
      <div className="flex items-center gap-3 mt-2 text-[10px] text-[#565f89]">
        {inputMode === "terminal" ? (
          <>
            <span>
              <kbd className="px-1 py-0.5 bg-[#1f2335] rounded text-[#7aa2f7]">↵</kbd> Execute
            </span>
            <span>
              <kbd className="px-1 py-0.5 bg-[#1f2335] rounded text-[#7aa2f7]">Tab</kbd>{" "}
              Autocomplete
            </span>
            <span>
              <kbd className="px-1 py-0.5 bg-[#1f2335] rounded text-[#7aa2f7]">^C</kbd> Cancel
            </span>
            <span>
              <kbd className="px-1 py-0.5 bg-[#1f2335] rounded text-[#7aa2f7]">⌘⇧T</kbd> Agent
            </span>
          </>
        ) : (
          <>
            <span>
              <kbd className="px-1 py-0.5 bg-[#1f2335] rounded text-[#bb9af7]">↵</kbd> Send
            </span>
            <span>
              <kbd className="px-1 py-0.5 bg-[#1f2335] rounded text-[#bb9af7]">↑↓</kbd> History
            </span>
            <span>
              <kbd className="px-1 py-0.5 bg-[#1f2335] rounded text-[#bb9af7]">⌘⇧T</kbd> Terminal
            </span>
          </>
        )}
      </div>
    </div>
  );
}

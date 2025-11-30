import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { sendPrompt } from "@/lib/ai";
import { type PromptInfo, ptyWrite, readPrompt } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useInputMode, useStore, useStreamingBlocks } from "@/store";
import { useSlashCommands } from "@/hooks/useSlashCommands";
import { SlashCommandPopup, filterPrompts } from "@/components/SlashCommandPopup";

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
  const [showSlashPopup, setShowSlashPopup] = useState(false);
  const [slashSelectedIndex, setSlashSelectedIndex] = useState(0);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Slash commands
  const { prompts } = useSlashCommands(workingDirectory);
  const slashQuery = input.startsWith("/") ? input.slice(1) : "";
  const filteredSlashPrompts = filterPrompts(prompts, slashQuery);

  // Use inputMode for unified input toggle (not session mode)
  const inputMode = useInputMode(sessionId);
  const setInputMode = useStore((state) => state.setInputMode);
  const streamingBlocks = useStreamingBlocks(sessionId);
  const addAgentMessage = useStore((state) => state.addAgentMessage);
  const agentMessages = useStore((state) => state.agentMessages[sessionId] ?? []);

  const isAgentBusy = inputMode === "agent" && (isSubmitting || streamingBlocks.length > 0);

  // Auto-resize textarea
  const adjustTextareaHeight = useCallback(() => {
    const textarea = textareaRef.current;
    if (textarea) {
      textarea.style.height = "auto";
      textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
    }
  }, []);

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
    textareaRef.current?.focus();
  }, []);

  // Adjust height when input changes
  // biome-ignore lint/correctness/useExhaustiveDependencies: input triggers re-measurement of textarea scrollHeight
  useEffect(() => {
    adjustTextareaHeight();
  }, [input, adjustTextareaHeight]);

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
        // Pass working directory and session context so the agent knows where the user is working
        // and can execute commands in the same terminal
        await sendPrompt(value, { workingDirectory, sessionId });
        // Response will be handled by useAiEvents when AI completes
        // Don't set isSubmitting to false here - wait for completed/error event
      } catch (error) {
        toast.error(`Agent error: ${error}`);
        setIsSubmitting(false);
      }
    }
  }, [
    input,
    inputMode,
    sessionId,
    isAgentBusy,
    addAgentMessage,
    isInteractiveCommand,
    workingDirectory,
  ]);

  // Handle slash command selection
  const handleSlashSelect = useCallback(
    async (prompt: PromptInfo) => {
      setShowSlashPopup(false);
      setInput("");

      // Switch to agent mode if in terminal mode
      if (inputMode === "terminal") {
        setInputMode(sessionId, "agent");
      }

      // Read and send the prompt
      try {
        const content = await readPrompt(prompt.path);
        setIsSubmitting(true);

        // Add user message to store (show the slash command name)
        addAgentMessage(sessionId, {
          id: crypto.randomUUID(),
          sessionId,
          role: "user",
          content: `/${prompt.name}`,
          timestamp: new Date().toISOString(),
        });

        // Send the actual prompt content to AI
        await sendPrompt(content, { workingDirectory, sessionId });
      } catch (error) {
        toast.error(`Failed to run prompt: ${error}`);
        setIsSubmitting(false);
      }
    },
    [sessionId, inputMode, setInputMode, addAgentMessage, workingDirectory]
  );

  const handleKeyDown = useCallback(
    async (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      // Cmd+I to toggle input mode - handle first to ensure it works in all modes
      // Check both lowercase 'i' and the key code for reliability across platforms
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && (e.key === "i" || e.key === "I")) {
        e.preventDefault();
        e.stopPropagation();
        toggleInputMode();
        return;
      }

      // When slash popup is open, handle navigation
      if (showSlashPopup && filteredSlashPrompts.length > 0) {
        if (e.key === "Escape") {
          e.preventDefault();
          setShowSlashPopup(false);
          return;
        }

        // Arrow down - move selection down
        if (e.key === "ArrowDown") {
          e.preventDefault();
          setSlashSelectedIndex((prev) =>
            prev < filteredSlashPrompts.length - 1 ? prev + 1 : prev
          );
          return;
        }

        // Arrow up - move selection up
        if (e.key === "ArrowUp") {
          e.preventDefault();
          setSlashSelectedIndex((prev) => (prev > 0 ? prev - 1 : 0));
          return;
        }

        // Tab - complete the selected option into the input field
        if (e.key === "Tab") {
          e.preventDefault();
          const selectedPrompt = filteredSlashPrompts[slashSelectedIndex];
          if (selectedPrompt) {
            setInput(`/${selectedPrompt.name}`);
            setShowSlashPopup(false);
          }
          return;
        }

        // Enter - execute the selected option
        if (e.key === "Enter" && !e.shiftKey) {
          e.preventDefault();
          const selectedPrompt = filteredSlashPrompts[slashSelectedIndex];
          if (selectedPrompt) {
            handleSlashSelect(selectedPrompt);
          }
          return;
        }
      }

      // Cmd+Shift+T to toggle input mode
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "t") {
        e.preventDefault();
        toggleInputMode();
        return;
      }

      // Handle Enter - execute/send (Shift+Enter for newline)
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        await handleSubmit();
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
    [inputMode, sessionId, handleSubmit, history, historyIndex, toggleInputMode, showSlashPopup, filteredSlashPrompts, slashSelectedIndex, handleSlashSelect]
  );

  const displayPath = workingDirectory?.replace(/^\/Users\/[^/]+/, "~") || "~";

  return (
    <div className="bg-[#1a1b26] border-t border-[#1f2335] px-4 py-2">
      {/* Working directory */}
      <div className="text-xs font-mono text-[#565f89] truncate mb-2">{displayPath}</div>

      {/* Input row */}
      <div className="flex items-center gap-2 relative">
        <SlashCommandPopup
          open={showSlashPopup}
          onOpenChange={setShowSlashPopup}
          searchQuery={slashQuery}
          prompts={prompts}
          selectedIndex={slashSelectedIndex}
          onSelect={handleSlashSelect}
        >
          <textarea
            ref={textareaRef}
            value={input}
            onChange={(e) => {
              const value = e.target.value;
              setInput(value);
              setHistoryIndex(-1);

              // Show slash popup when "/" is typed at the start
              if (value.startsWith("/") && value.length >= 1) {
                setShowSlashPopup(true);
                // Reset selection when query changes
                setSlashSelectedIndex(0);
              } else {
                setShowSlashPopup(false);
              }
            }}
            onKeyDown={handleKeyDown}
            disabled={isAgentBusy}
            placeholder={inputMode === "terminal" ? "Enter command..." : "Ask the AI..."}
            rows={1}
            className={cn(
              "flex-1 min-h-[24px] max-h-[200px] py-1 px-0",
              "bg-transparent border-none shadow-none resize-none",
              "font-mono text-sm text-[#c0caf5]",
              "focus:outline-none focus:ring-0",
              "disabled:opacity-50",
              "placeholder:text-[#565f89]"
            )}
            spellCheck={false}
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
          />
        </SlashCommandPopup>
      </div>
    </div>
  );
}

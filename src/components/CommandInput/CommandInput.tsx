import { useState, useRef, useEffect, useCallback } from "react";
import { ChevronRight } from "lucide-react";
import { ptyWrite } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { toast } from "sonner";

interface CommandInputProps {
  sessionId: string;
  workingDirectory?: string;
}

// Commands that require full terminal (interactive programs)
const INTERACTIVE_COMMANDS = [
  'vim', 'vi', 'nvim', 'nano', 'emacs', 'pico',
  'less', 'more', 'man',
  'htop', 'top', 'btop',
  'ssh', 'telnet', 'ftp', 'sftp',
  'python', 'python3', 'node', 'irb', 'ruby', 'ghci',
  'mysql', 'psql', 'sqlite3', 'redis-cli', 'mongo',
  'tmux', 'screen',
  'watch',
];

export function CommandInput({ sessionId, workingDirectory }: CommandInputProps) {
  const [input, setInput] = useState("");
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [history, setHistory] = useState<string[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

  // Focus input on mount and when clicking the container
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Check if command is interactive and needs full terminal
  const isInteractiveCommand = useCallback((cmd: string) => {
    const firstWord = cmd.trim().split(/\s+/)[0];
    return INTERACTIVE_COMMANDS.includes(firstWord);
  }, []);

  const handleKeyDown = useCallback(async (e: React.KeyboardEvent<HTMLInputElement>) => {
    // Handle Enter - execute command
    if (e.key === "Enter") {
      e.preventDefault();
      if (input.trim()) {
        // Block interactive commands for now
        if (isInteractiveCommand(input)) {
          const cmd = input.trim().split(/\s+/)[0];
          toast.error(`Interactive command "${cmd}" is not supported yet`);
          return;
        }

        // Add to history
        setHistory(prev => [...prev, input]);
        setHistoryIndex(-1);
        // Send command + newline to PTY
        await ptyWrite(sessionId, input + "\n");
        setInput("");
      } else {
        // Empty enter - just send newline
        await ptyWrite(sessionId, "\n");
      }
      return;
    }

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

    // Handle Ctrl+L - clear (show terminal briefly to clear)
    if (e.ctrlKey && e.key === "l") {
      e.preventDefault();
      await ptyWrite(sessionId, "\x0c");
      return;
    }

  }, [sessionId, input, history, historyIndex, isInteractiveCommand]);

  // Get display path (shorten home directory)
  const displayPath = workingDirectory?.replace(/^\/Users\/[^/]+/, "~") || "~";

  return (
    <div
      className="bg-[#1a1b26] border-t border-[#1f2335] px-4 py-3"
      onClick={() => inputRef.current?.focus()}
    >
      {/* Current directory */}
      <div className="text-xs font-mono text-[#565f89] mb-2 truncate">
        {displayPath}
      </div>

      {/* Input line */}
      <div className="flex items-center gap-2">
        <ChevronRight className="w-4 h-4 text-[#7aa2f7] flex-shrink-0" />
        <input
          ref={inputRef}
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          className={cn(
            "flex-1 bg-transparent border-none outline-none",
            "font-mono text-sm text-[#c0caf5]",
            "placeholder:text-[#565f89]"
          )}
          placeholder="Enter command..."
          spellCheck={false}
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
        />
      </div>
    </div>
  );
}

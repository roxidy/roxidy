import { listen } from "@tauri-apps/api/event";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal as XTerm } from "@xterm/xterm";
import { useCallback, useEffect, useRef } from "react";
import { ptyResize, ptyWrite } from "../../lib/tauri";
import "@xterm/xterm/css/xterm.css";

interface TerminalProps {
  sessionId: string;
}

interface TerminalOutputEvent {
  session_id: string;
  data: string;
}

interface CommandBlockEvent {
  session_id: string;
  event_type: "prompt_start" | "prompt_end" | "command_start" | "command_end";
}

export function Terminal({ sessionId }: TerminalProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<XTerm | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const cleanupFnsRef = useRef<(() => void)[]>([]);

  // Handle resize
  const handleResize = useCallback(() => {
    if (fitAddonRef.current && terminalRef.current) {
      fitAddonRef.current.fit();
      const { rows, cols } = terminalRef.current;
      ptyResize(sessionId, rows, cols).catch(console.error);
    }
  }, [sessionId]);

  useEffect(() => {
    if (!containerRef.current) return;

    // Prevent duplicate setup in StrictMode - if terminal already exists, just focus
    if (terminalRef.current) {
      terminalRef.current.focus();
      return;
    }

    // Clear any previous cleanup functions before setting up new ones
    for (const fn of cleanupFnsRef.current) {
      fn();
    }
    cleanupFnsRef.current = [];

    // Create terminal
    const terminal = new XTerm({
      cursorBlink: true,
      cursorStyle: "block",
      fontSize: 14,
      fontFamily: "JetBrains Mono, Menlo, Monaco, Consolas, monospace",
      theme: {
        background: "#1a1b26",
        foreground: "#c0caf5",
        cursor: "#c0caf5",
        selectionBackground: "#33467c",
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
      allowProposedApi: true,
    });

    // Add addons
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.loadAddon(new WebLinksAddon());

    // Open terminal
    terminal.open(containerRef.current);

    // Try to load WebGL addon for better performance
    try {
      const webglAddon = new WebglAddon();
      terminal.loadAddon(webglAddon);
    } catch (e) {
      console.warn("WebGL not available, falling back to canvas", e);
    }

    // Initial fit
    fitAddon.fit();

    // Handle user input
    terminal.onData((data) => {
      ptyWrite(sessionId, data).catch(console.error);
    });

    terminalRef.current = terminal;
    fitAddonRef.current = fitAddon;

    // Listen for terminal output
    listen<TerminalOutputEvent>("terminal_output", (event) => {
      if (event.payload.session_id === sessionId && terminalRef.current) {
        terminalRef.current.write(event.payload.data);
      }
    }).then((unlisten) => {
      cleanupFnsRef.current.push(unlisten);
    });

    // Listen for command block events - clear terminal when command completes
    // This prevents duplicate output (blocks show history, terminal shows current)
    listen<CommandBlockEvent>("command_block", (event) => {
      if (event.payload.session_id === sessionId && terminalRef.current) {
        // Clear terminal when prompt_start fires (after command completes)
        if (event.payload.event_type === "prompt_start") {
          terminalRef.current.clear();
        }
      }
    }).then((unlisten) => {
      cleanupFnsRef.current.push(unlisten);
    });

    // Handle window resize
    const resizeObserver = new ResizeObserver(() => {
      handleResize();
    });
    resizeObserver.observe(containerRef.current);

    // Initial resize notification
    const { rows, cols } = terminal;
    ptyResize(sessionId, rows, cols).catch(console.error);

    // Focus terminal
    terminal.focus();

    return () => {
      resizeObserver.disconnect();
      for (const fn of cleanupFnsRef.current) {
        fn();
      }
      cleanupFnsRef.current = [];
      terminal.dispose();
      terminalRef.current = null;
      fitAddonRef.current = null;
    };
  }, [sessionId, handleResize]);

  return (
    <div
      ref={containerRef}
      className="w-full h-full min-h-0"
      style={{ backgroundColor: "#1a1b26" }}
    />
  );
}

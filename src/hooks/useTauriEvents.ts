import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { isAiInitialized, updateAiWorkspace } from "../lib/ai";
import { ptyGetForegroundProcess } from "../lib/tauri";
import { useStore } from "../store";

// In browser mode, use the mock listen function if available
declare global {
  interface Window {
    __MOCK_LISTEN__?: typeof tauriListen;
    __MOCK_BROWSER_MODE__?: boolean;
  }
}

// Use mock listen in browser mode, otherwise use real Tauri listen
const listen: typeof tauriListen = (...args) => {
  if (window.__MOCK_BROWSER_MODE__ && window.__MOCK_LISTEN__) {
    return window.__MOCK_LISTEN__(...args);
  }
  return tauriListen(...args);
};

interface TerminalOutputEvent {
  session_id: string;
  data: string;
}

interface CommandBlockEvent {
  session_id: string;
  command: string | null;
  exit_code: number | null;
  event_type: "prompt_start" | "prompt_end" | "command_start" | "command_end";
}

interface DirectoryChangedEvent {
  session_id: string;
  path: string;
}

interface SessionEndedEvent {
  sessionId: string;
}

// Commands that are typically fast and shouldn't trigger tab name updates
// This is a minimal fallback - the main filtering is duration-based
const FAST_COMMANDS = new Set([
  'ls', 'pwd', 'cd', 'echo', 'cat', 'which', 'whoami',
  'date', 'clear', 'exit', 'history', 'env', 'printenv',
]);

function isFastCommand(command: string | null): boolean {
  if (!command) return true;
  const firstWord = command.trim().split(/\s+/)[0];
  return FAST_COMMANDS.has(firstWord);
}

/**
 * Extract the process name from a command string.
 * Returns just the base command (first word) without arguments.
 * Handles edge cases like sudo, env vars, and path prefixes.
 */
function extractProcessName(command: string | null): string | null {
  if (!command) return null;
  
  const trimmed = command.trim();
  if (!trimmed) return null;

  // Remove environment variable assignments at the start (e.g., "ENV=val command")
  const withoutEnv = trimmed.replace(/^[A-Z_][A-Z0-9_]*=\S+\s+/g, '');
  
  // Handle sudo/doas prefix
  const withoutSudo = withoutEnv.replace(/^(sudo|doas)\s+/, '');
  
  // Get the first word (the actual command)
  const firstWord = withoutSudo.split(/\s+/)[0];
  
  // Strip path if present (e.g., "/usr/bin/npm" -> "npm")
  const baseName = firstWord.split('/').pop() || firstWord;
  
  return baseName;
}

export function useTauriEvents() {
  // Get store actions directly - these are stable references from zustand
  const store = useStore;

  // biome-ignore lint/correctness/useExhaustiveDependencies: store.getState is stable zustand API
  useEffect(() => {
    const unlisteners: Promise<UnlistenFn>[] = [];
    // Track pending process detection timers per session
    const processDetectionTimers = new Map<string, NodeJS.Timeout>();

    // Command block events
    unlisteners.push(
      listen<CommandBlockEvent>("command_block", (event) => {
        const { session_id, command, exit_code, event_type } = event.payload;
        const state = store.getState();

        switch (event_type) {
          case "prompt_start":
            state.handlePromptStart(session_id);
            break;
          case "prompt_end":
            state.handlePromptEnd(session_id);
            break;
          case "command_start": {
            state.handleCommandStart(session_id, command);
            
            // Skip process detection for known-fast commands
            if (isFastCommand(command)) {
              break;
            }

            // Extract process name from command
            const commandProcess = extractProcessName(command);

            // Clear any existing timer for this session
            const existingTimer = processDetectionTimers.get(session_id);
            if (existingTimer) {
              clearTimeout(existingTimer);
            }

            // Wait 300ms to verify the process is still running
            // This filters out fast commands while allowing long-running ones
            const timer = setTimeout(async () => {
              try {
                // Check if something is still running (OS verification)
                const osProcess = await ptyGetForegroundProcess(session_id);
                
                // If shell returned to foreground, the command finished quickly
                if (!osProcess || ['zsh', 'bash', 'sh', 'fish'].includes(osProcess)) {
                  return; // Don't update tab name
                }
                
                // Command is still running - use the command name we extracted
                // This gives us "pnpm" instead of "node", "just" instead of child process
                if (commandProcess) {
                  state.setProcessName(session_id, commandProcess);
                }
              } catch (err) {
                // Silently ignore - process detection is best-effort
                console.debug("Failed to verify foreground process:", err);
              } finally {
                processDetectionTimers.delete(session_id);
              }
            }, 300);

            processDetectionTimers.set(session_id, timer);
            break;
          }
          case "command_end":
            if (exit_code !== null) {
              state.handleCommandEnd(session_id, exit_code);
            }
            // Cancel any pending process detection for this session
            const timer = processDetectionTimers.get(session_id);
            if (timer) {
              clearTimeout(timer);
              processDetectionTimers.delete(session_id);
            }
            // Clear process name when command ends
            state.setProcessName(session_id, null);
            break;
        }
      })
    );

    // Terminal output - capture for command blocks
    unlisteners.push(
      listen<TerminalOutputEvent>("terminal_output", (event) => {
        store.getState().appendOutput(event.payload.session_id, event.payload.data);
      })
    );

    // Directory changed
    unlisteners.push(
      listen<DirectoryChangedEvent>("directory_changed", async (event) => {
        const { session_id, path } = event.payload;
        store.getState().updateWorkingDirectory(session_id, path);

        // Also update the AI agent's workspace if initialized
        try {
          const initialized = await isAiInitialized();
          if (initialized) {
            await updateAiWorkspace(path);
          }
        } catch (error) {
          console.error("Error updating AI workspace:", error);
        }
      })
    );

    // Session ended
    unlisteners.push(
      listen<SessionEndedEvent>("session_ended", (event) => {
        store.getState().removeSession(event.payload.sessionId);
      })
    );

    // Cleanup
    return () => {
      // Clear all pending timers
      for (const timer of processDetectionTimers.values()) {
        clearTimeout(timer);
      }
      processDetectionTimers.clear();

      // Unlisten from events
      for (const p of unlisteners) {
        p.then((unlisten) => unlisten());
      }
    };
  }, []);
}

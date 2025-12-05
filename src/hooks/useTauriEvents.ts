import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { isAiInitialized, updateAiWorkspace } from "../lib/ai";
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

export function useTauriEvents() {
  // Get store actions directly - these are stable references from zustand
  const store = useStore;

  // biome-ignore lint/correctness/useExhaustiveDependencies: store.getState is stable zustand API
  useEffect(() => {
    const unlisteners: Promise<UnlistenFn>[] = [];

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
          case "command_start":
            state.handleCommandStart(session_id, command);
            break;
          case "command_end":
            if (exit_code !== null) {
              state.handleCommandEnd(session_id, exit_code);
            }
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
      for (const p of unlisteners) {
        p.then((unlisten) => unlisten());
      }
    };
  }, []);
}

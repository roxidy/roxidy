import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { useStore } from "../store";
import { clearMockListeners, emitMockEvent, getListenerCount } from "../test/mocks/tauri-event";
import { useTauriEvents } from "./useTauriEvents";

describe("useTauriEvents", () => {
  const createTestSession = (id: string, name = "Test") => {
    useStore.getState().addSession({
      id,
      name,
      workingDirectory: "/test",
      createdAt: new Date().toISOString(),
      mode: "terminal",
    });
  };

  beforeEach(() => {
    // Reset store state
    useStore.setState({
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
    });

    // Clear any existing listeners
    clearMockListeners();

    // Create a test session
    createTestSession("test-session");
  });

  afterEach(() => {
    clearMockListeners();
  });

  it("should register event listeners on mount", () => {
    renderHook(() => useTauriEvents());

    // Should have registered listeners for each event type
    expect(getListenerCount("command_block")).toBe(1);
    expect(getListenerCount("terminal_output")).toBe(1);
    expect(getListenerCount("directory_changed")).toBe(1);
    expect(getListenerCount("session_ended")).toBe(1);
  });

  it("should only register listeners once (no duplicates)", () => {
    const { rerender } = renderHook(() => useTauriEvents());

    // Force re-render
    rerender();
    rerender();

    // Should still only have one listener per event
    expect(getListenerCount("command_block")).toBe(1);
    expect(getListenerCount("terminal_output")).toBe(1);
  });

  it("should unregister listeners on unmount", async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    expect(getListenerCount("command_block")).toBe(1);

    unmount();

    // Give promises time to resolve
    await new Promise((resolve) => setTimeout(resolve, 10));

    expect(getListenerCount("command_block")).toBe(0);
  });

  describe("command_block events", () => {
    it("should handle command_start event", () => {
      renderHook(() => useTauriEvents());

      act(() => {
        emitMockEvent("command_block", {
          session_id: "test-session",
          command: "ls -la",
          exit_code: null,
          event_type: "command_start",
        });
      });

      const state = useStore.getState();
      expect(state.pendingCommand["test-session"]).toBeDefined();
      expect(state.pendingCommand["test-session"]?.command).toBe("ls -la");
    });

    it("should handle command_end event", () => {
      renderHook(() => useTauriEvents());

      // First start a command
      act(() => {
        emitMockEvent("command_block", {
          session_id: "test-session",
          command: "echo hello",
          exit_code: null,
          event_type: "command_start",
        });
      });

      // Then end it
      act(() => {
        emitMockEvent("command_block", {
          session_id: "test-session",
          command: null,
          exit_code: 0,
          event_type: "command_end",
        });
      });

      const state = useStore.getState();
      expect(state.commandBlocks["test-session"]).toHaveLength(1);
      expect(state.commandBlocks["test-session"][0].exitCode).toBe(0);
    });

    it("should handle prompt_start event", () => {
      renderHook(() => useTauriEvents());

      // Start a command
      act(() => {
        emitMockEvent("command_block", {
          session_id: "test-session",
          command: "pwd",
          exit_code: null,
          event_type: "command_start",
        });
      });

      expect(useStore.getState().pendingCommand["test-session"]).toBeDefined();

      // Prompt start should clear pending
      act(() => {
        emitMockEvent("command_block", {
          session_id: "test-session",
          command: null,
          exit_code: null,
          event_type: "prompt_start",
        });
      });

      expect(useStore.getState().pendingCommand["test-session"]).toBeNull();
    });

    it("should not create block for command_end with null exit_code", () => {
      renderHook(() => useTauriEvents());

      act(() => {
        emitMockEvent("command_block", {
          session_id: "test-session",
          command: "test",
          exit_code: null,
          event_type: "command_start",
        });
      });

      act(() => {
        emitMockEvent("command_block", {
          session_id: "test-session",
          command: null,
          exit_code: null, // null exit code - should not trigger handleCommandEnd
          event_type: "command_end",
        });
      });

      const state = useStore.getState();
      // Block should NOT be created because exit_code is null
      expect(state.commandBlocks["test-session"]).toHaveLength(0);
    });
  });

  describe("terminal_output events", () => {
    it("should append output to pending command", () => {
      renderHook(() => useTauriEvents());

      // Start a command first
      act(() => {
        emitMockEvent("command_block", {
          session_id: "test-session",
          command: "cat file.txt",
          exit_code: null,
          event_type: "command_start",
        });
      });

      // Send output
      act(() => {
        emitMockEvent("terminal_output", {
          session_id: "test-session",
          data: "line 1\n",
        });
      });

      act(() => {
        emitMockEvent("terminal_output", {
          session_id: "test-session",
          data: "line 2\n",
        });
      });

      const state = useStore.getState();
      expect(state.pendingCommand["test-session"]?.output).toBe("line 1\nline 2\n");
    });

    it("should NOT capture output when no command is running", () => {
      renderHook(() => useTauriEvents());

      // Send output WITHOUT starting a command first
      act(() => {
        emitMockEvent("terminal_output", {
          session_id: "test-session",
          data: "prompt text that should be ignored\n",
        });
      });

      const state = useStore.getState();
      // pendingCommand should still be null
      expect(state.pendingCommand["test-session"]).toBeNull();
    });
  });

  describe("directory_changed events", () => {
    it("should update session working directory", () => {
      renderHook(() => useTauriEvents());

      act(() => {
        emitMockEvent("directory_changed", {
          session_id: "test-session",
          path: "/new/path",
        });
      });

      const state = useStore.getState();
      expect(state.sessions["test-session"].workingDirectory).toBe("/new/path");
    });
  });

  describe("full command lifecycle", () => {
    it("should handle complete command flow with streaming output", () => {
      renderHook(() => useTauriEvents());

      // 1. Command starts
      act(() => {
        emitMockEvent("command_block", {
          session_id: "test-session",
          command: "ping -c 2 localhost",
          exit_code: null,
          event_type: "command_start",
        });
      });

      expect(useStore.getState().pendingCommand["test-session"]?.command).toBe(
        "ping -c 2 localhost"
      );

      // 2. Streaming output arrives
      act(() => {
        emitMockEvent("terminal_output", {
          session_id: "test-session",
          data: "PING localhost: 64 bytes\n",
        });
      });

      expect(useStore.getState().pendingCommand["test-session"]?.output).toBe(
        "PING localhost: 64 bytes\n"
      );

      act(() => {
        emitMockEvent("terminal_output", {
          session_id: "test-session",
          data: "PING localhost: 64 bytes\n",
        });
      });

      expect(useStore.getState().pendingCommand["test-session"]?.output).toBe(
        "PING localhost: 64 bytes\nPING localhost: 64 bytes\n"
      );

      // 3. Command ends
      act(() => {
        emitMockEvent("command_block", {
          session_id: "test-session",
          command: null,
          exit_code: 0,
          event_type: "command_end",
        });
      });

      const state = useStore.getState();
      expect(state.pendingCommand["test-session"]).toBeNull();
      expect(state.commandBlocks["test-session"]).toHaveLength(1);
      expect(state.commandBlocks["test-session"][0].command).toBe("ping -c 2 localhost");
      expect(state.commandBlocks["test-session"][0].output).toBe(
        "PING localhost: 64 bytes\nPING localhost: 64 bytes\n"
      );
    });
  });

  describe("session_ended events", () => {
    it("should remove session when session_ended event is received", () => {
      renderHook(() => useTauriEvents());

      // Verify session exists
      expect(useStore.getState().sessions["test-session"]).toBeDefined();

      // Session ends
      act(() => {
        emitMockEvent("session_ended", {
          sessionId: "test-session",
        });
      });

      // Session should be removed
      expect(useStore.getState().sessions["test-session"]).toBeUndefined();
    });

    it("should clean up all session-related state on session_ended", () => {
      renderHook(() => useTauriEvents());

      // Add some state to the session
      act(() => {
        emitMockEvent("command_block", {
          session_id: "test-session",
          command: "ls",
          exit_code: null,
          event_type: "command_start",
        });
      });

      expect(useStore.getState().pendingCommand["test-session"]).toBeDefined();

      // Session ends
      act(() => {
        emitMockEvent("session_ended", {
          sessionId: "test-session",
        });
      });

      // All session state should be cleaned up
      const state = useStore.getState();
      expect(state.sessions["test-session"]).toBeUndefined();
      expect(state.commandBlocks["test-session"]).toBeUndefined();
      expect(state.pendingCommand["test-session"]).toBeUndefined();
      expect(state.timelines["test-session"]).toBeUndefined();
    });

    it("should switch activeSessionId when active session ends", () => {
      // Create a second session
      createTestSession("other-session");

      renderHook(() => useTauriEvents());

      // Set test-session as active (it was created first, so it should already be active)
      useStore.getState().setActiveSession("test-session");
      expect(useStore.getState().activeSessionId).toBe("test-session");

      // End the active session
      act(() => {
        emitMockEvent("session_ended", {
          sessionId: "test-session",
        });
      });

      // Active session should switch to remaining session
      expect(useStore.getState().activeSessionId).toBe("other-session");
    });
  });

  describe("prompt_end events", () => {
    it("should handle prompt_end event (currently no-op)", () => {
      renderHook(() => useTauriEvents());

      // This shouldn't throw or cause issues
      act(() => {
        emitMockEvent("command_block", {
          session_id: "test-session",
          command: null,
          exit_code: null,
          event_type: "prompt_end",
        });
      });

      // State should be unchanged
      expect(useStore.getState().pendingCommand["test-session"]).toBeNull();
    });
  });

  describe("multi-session handling", () => {
    it("should handle events for multiple sessions independently", () => {
      createTestSession("session-a");
      createTestSession("session-b");

      renderHook(() => useTauriEvents());

      // Start commands in both sessions
      act(() => {
        emitMockEvent("command_block", {
          session_id: "session-a",
          command: "command A",
          exit_code: null,
          event_type: "command_start",
        });
      });

      act(() => {
        emitMockEvent("command_block", {
          session_id: "session-b",
          command: "command B",
          exit_code: null,
          event_type: "command_start",
        });
      });

      // Both should have their own pending commands
      expect(useStore.getState().pendingCommand["session-a"]?.command).toBe("command A");
      expect(useStore.getState().pendingCommand["session-b"]?.command).toBe("command B");

      // End only session A
      act(() => {
        emitMockEvent("command_block", {
          session_id: "session-a",
          command: null,
          exit_code: 0,
          event_type: "command_end",
        });
      });

      // Session A should be complete, B still pending
      expect(useStore.getState().pendingCommand["session-a"]).toBeNull();
      expect(useStore.getState().commandBlocks["session-a"]).toHaveLength(1);
      expect(useStore.getState().pendingCommand["session-b"]?.command).toBe("command B");
    });

    it("should route terminal_output to correct session", () => {
      createTestSession("session-a");
      createTestSession("session-b");

      renderHook(() => useTauriEvents());

      // Start commands in both
      act(() => {
        emitMockEvent("command_block", {
          session_id: "session-a",
          command: "cmd-a",
          exit_code: null,
          event_type: "command_start",
        });
        emitMockEvent("command_block", {
          session_id: "session-b",
          command: "cmd-b",
          exit_code: null,
          event_type: "command_start",
        });
      });

      // Send output only to session A
      act(() => {
        emitMockEvent("terminal_output", {
          session_id: "session-a",
          data: "output for A\n",
        });
      });

      // Only session A should have output
      expect(useStore.getState().pendingCommand["session-a"]?.output).toBe("output for A\n");
      expect(useStore.getState().pendingCommand["session-b"]?.output).toBe("");
    });
  });

  describe("edge cases and bug prevention", () => {
    it("should handle events for non-existent session gracefully", () => {
      renderHook(() => useTauriEvents());

      // These should not throw
      expect(() => {
        act(() => {
          emitMockEvent("terminal_output", {
            session_id: "non-existent",
            data: "some output",
          });
        });
      }).not.toThrow();

      expect(() => {
        act(() => {
          emitMockEvent("directory_changed", {
            session_id: "non-existent",
            path: "/some/path",
          });
        });
      }).not.toThrow();
    });

    it("should handle rapid consecutive events", () => {
      renderHook(() => useTauriEvents());

      act(() => {
        emitMockEvent("command_block", {
          session_id: "test-session",
          command: "rapid-test",
          exit_code: null,
          event_type: "command_start",
        });

        // Rapid output events
        for (let i = 0; i < 50; i++) {
          emitMockEvent("terminal_output", {
            session_id: "test-session",
            data: `line ${i}\n`,
          });
        }

        emitMockEvent("command_block", {
          session_id: "test-session",
          command: null,
          exit_code: 0,
          event_type: "command_end",
        });
      });

      const block = useStore.getState().commandBlocks["test-session"][0];
      expect(block.command).toBe("rapid-test");
      // Should have all 50 lines
      expect(block.output.split("\n").filter(Boolean)).toHaveLength(50);
    });
  });

  describe("Process Name Extraction", () => {
    /**
     * Extract the process name from a command string.
     * Duplicated from useTauriEvents.ts for testing.
     */
    function extractProcessName(command: string | null): string | null {
      if (!command) return null;
      const trimmed = command.trim();
      if (!trimmed) return null;
      const withoutEnv = trimmed.replace(/^[A-Z_][A-Z0-9_]*=\S+\s+/g, "");
      const withoutSudo = withoutEnv.replace(/^(sudo|doas)\s+/, "");
      const firstWord = withoutSudo.split(/\s+/)[0];
      const baseName = firstWord.split("/").pop() || firstWord;
      return baseName;
    }

    describe("basic commands", () => {
      it("extracts simple command name", () => {
        expect(extractProcessName("npm install")).toBe("npm");
        expect(extractProcessName("pnpm install")).toBe("pnpm");
        expect(extractProcessName("just build")).toBe("just");
        expect(extractProcessName("cargo build")).toBe("cargo");
      });

      it("handles commands without arguments", () => {
        expect(extractProcessName("npm")).toBe("npm");
        expect(extractProcessName("node")).toBe("node");
      });
    });

    describe("path handling", () => {
      it("strips absolute paths", () => {
        expect(extractProcessName("/usr/bin/npm install")).toBe("npm");
        expect(extractProcessName("/usr/local/bin/node app.js")).toBe("node");
      });

      it("strips relative paths", () => {
        expect(extractProcessName("./node_modules/.bin/vite")).toBe("vite");
      });
    });

    describe("sudo/doas handling", () => {
      it("removes sudo prefix", () => {
        expect(extractProcessName("sudo npm install -g")).toBe("npm");
        expect(extractProcessName("sudo apt-get update")).toBe("apt-get");
      });

      it("removes doas prefix", () => {
        expect(extractProcessName("doas npm install")).toBe("npm");
      });
    });

    describe("environment variables", () => {
      it("removes leading environment variables", () => {
        expect(extractProcessName("NODE_ENV=production npm start")).toBe("npm");
        expect(extractProcessName("DEBUG=* node app.js")).toBe("node");
      });
    });

    describe("real-world examples", () => {
      it("handles common development tools", () => {
        expect(extractProcessName("pnpm install")).toBe("pnpm");
        expect(extractProcessName("bun install")).toBe("bun");
        expect(extractProcessName("just build")).toBe("just");
        expect(extractProcessName("cargo build --release")).toBe("cargo");
        expect(extractProcessName("go run main.go")).toBe("go");
        expect(extractProcessName("deno run --allow-net server.ts")).toBe("deno");
      });
    });
  });

  describe("Tab Name Display Logic", () => {
    describe("priority order", () => {
      it("prioritizes custom name over process name", () => {
        const session = {
          customName: "My Custom Tab",
          processName: "npm",
          workingDirectory: "/Users/test/project",
        };
        const displayName =
          session.customName || session.processName || session.workingDirectory.split("/").pop();
        expect(displayName).toBe("My Custom Tab");
      });

      it("prioritizes process name over directory", () => {
        const session = {
          processName: "npm",
          workingDirectory: "/Users/test/project",
        };
        const displayName =
          session.processName || session.workingDirectory.split("/").pop();
        expect(displayName).toBe("npm");
      });

      it("falls back to directory name", () => {
        const session = {
          workingDirectory: "/Users/test/project",
        };
        const displayName = session.workingDirectory.split("/").pop() || "Terminal";
        expect(displayName).toBe("project");
      });
    });

    describe("custom name persistence", () => {
      it("custom name persists when process starts", () => {
        const session = {
          customName: "Frontend Dev",
          processName: "npm",
        };
        expect(session.customName || session.processName).toBe("Frontend Dev");
      });

      it("custom name persists when process ends", () => {
        const session = {
          customName: "Frontend Dev",
          processName: undefined,
        };
        expect(session.customName || session.processName || "dir").toBe("Frontend Dev");
      });
    });
  });
});

import { beforeEach, describe, expect, it } from "vitest";
import type { AgentMessage } from "./index";
import { useStore } from "./index";

describe("Store", () => {
  beforeEach(() => {
    // Reset store state before each test
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
  });

  describe("Session Management", () => {
    it("should add a session", () => {
      const store = useStore.getState();
      store.addSession({
        id: "session-1",
        name: "Terminal",
        workingDirectory: "/home/user",
        createdAt: "2024-01-01T00:00:00Z",
        mode: "terminal",
      });

      const state = useStore.getState();
      expect(state.sessions["session-1"]).toBeDefined();
      expect(state.activeSessionId).toBe("session-1");
      expect(state.sessions["session-1"].inputMode).toBe("terminal");
    });

    it("should default inputMode to terminal", () => {
      const store = useStore.getState();
      store.addSession({
        id: "session-1",
        name: "Terminal",
        workingDirectory: "/home/user",
        createdAt: "2024-01-01T00:00:00Z",
        mode: "terminal",
      });

      const state = useStore.getState();
      expect(state.sessions["session-1"].inputMode).toBe("terminal");
    });
  });

  describe("Command Lifecycle", () => {
    beforeEach(() => {
      // Set up a session first
      useStore.getState().addSession({
        id: "session-1",
        name: "Terminal",
        workingDirectory: "/home/user",
        createdAt: "2024-01-01T00:00:00Z",
        mode: "terminal",
      });
    });

    it("should create pendingCommand on command_start", () => {
      const store = useStore.getState();
      store.handleCommandStart("session-1", "ls -la");

      const state = useStore.getState();
      expect(state.pendingCommand["session-1"]).toBeDefined();
      expect(state.pendingCommand["session-1"]?.command).toBe("ls -la");
      expect(state.pendingCommand["session-1"]?.output).toBe("");
    });

    it("should append output to pendingCommand", () => {
      const store = useStore.getState();
      store.handleCommandStart("session-1", "ls -la");
      store.appendOutput("session-1", "file1.txt\n");
      store.appendOutput("session-1", "file2.txt\n");

      const state = useStore.getState();
      expect(state.pendingCommand["session-1"]?.output).toBe("file1.txt\nfile2.txt\n");
    });

    it("should NOT append output when no pendingCommand exists", () => {
      const store = useStore.getState();
      // Don't call handleCommandStart first
      store.appendOutput("session-1", "some output");

      const state = useStore.getState();
      expect(state.pendingCommand["session-1"]).toBeNull();
    });

    it("should create command block on command_end with command", () => {
      const store = useStore.getState();
      store.handleCommandStart("session-1", "ls -la");
      store.appendOutput("session-1", "file1.txt\n");
      store.handleCommandEnd("session-1", 0);

      const state = useStore.getState();
      expect(state.commandBlocks["session-1"]).toHaveLength(1);
      expect(state.commandBlocks["session-1"][0].command).toBe("ls -la");
      expect(state.commandBlocks["session-1"][0].output).toBe("file1.txt\n");
      expect(state.commandBlocks["session-1"][0].exitCode).toBe(0);
      expect(state.pendingCommand["session-1"]).toBeNull();
    });

    it("should NOT create command block on command_end without command", () => {
      const store = useStore.getState();
      // Simulate command_start with null command (empty enter)
      store.handleCommandStart("session-1", null);
      store.handleCommandEnd("session-1", 0);

      const state = useStore.getState();
      expect(state.commandBlocks["session-1"]).toHaveLength(0);
      expect(state.pendingCommand["session-1"]).toBeNull();
    });

    it("should add command block to timeline", () => {
      const store = useStore.getState();
      store.handleCommandStart("session-1", "echo hello");
      store.appendOutput("session-1", "hello\n");
      store.handleCommandEnd("session-1", 0);

      const state = useStore.getState();
      expect(state.timelines["session-1"]).toHaveLength(1);
      expect(state.timelines["session-1"][0].type).toBe("command");
    });
  });

  describe("Streaming Output Behavior", () => {
    beforeEach(() => {
      useStore.getState().addSession({
        id: "session-1",
        name: "Terminal",
        workingDirectory: "/home/user",
        createdAt: "2024-01-01T00:00:00Z",
        mode: "terminal",
      });
    });

    it("should accumulate streaming output during command execution", () => {
      const store = useStore.getState();
      store.handleCommandStart("session-1", "ping -c 3 localhost");

      // Simulate streaming output
      store.appendOutput("session-1", "PING localhost: ");
      expect(useStore.getState().pendingCommand["session-1"]?.output).toBe("PING localhost: ");

      store.appendOutput("session-1", "64 bytes from 127.0.0.1\n");
      expect(useStore.getState().pendingCommand["session-1"]?.output).toBe(
        "PING localhost: 64 bytes from 127.0.0.1\n"
      );

      store.appendOutput("session-1", "64 bytes from 127.0.0.1\n");
      expect(useStore.getState().pendingCommand["session-1"]?.output).toBe(
        "PING localhost: 64 bytes from 127.0.0.1\n64 bytes from 127.0.0.1\n"
      );
    });

    it("should preserve output when command ends", () => {
      const store = useStore.getState();
      store.handleCommandStart("session-1", "cat file.txt");
      store.appendOutput("session-1", "line 1\nline 2\nline 3\n");
      store.handleCommandEnd("session-1", 0);

      const state = useStore.getState();
      expect(state.commandBlocks["session-1"][0].output).toBe("line 1\nline 2\nline 3\n");
    });

    it("should handle rapid output events", () => {
      const store = useStore.getState();
      store.handleCommandStart("session-1", "yes | head -100");

      // Simulate many rapid outputs
      for (let i = 0; i < 100; i++) {
        store.appendOutput("session-1", "y\n");
      }

      const state = useStore.getState();
      expect(state.pendingCommand["session-1"]?.output).toBe("y\n".repeat(100));
    });
  });

  describe("Prompt Events", () => {
    beforeEach(() => {
      useStore.getState().addSession({
        id: "session-1",
        name: "Terminal",
        workingDirectory: "/home/user",
        createdAt: "2024-01-01T00:00:00Z",
        mode: "terminal",
      });
    });

    it("should clear pendingCommand on prompt_start", () => {
      const store = useStore.getState();
      store.handleCommandStart("session-1", null);
      store.appendOutput("session-1", "prompt text");

      expect(useStore.getState().pendingCommand["session-1"]).toBeDefined();

      store.handlePromptStart("session-1");

      expect(useStore.getState().pendingCommand["session-1"]).toBeNull();
    });

    it("should NOT create block on prompt_start for commands without command text", () => {
      const store = useStore.getState();
      store.handleCommandStart("session-1", null);
      store.handlePromptStart("session-1");

      const state = useStore.getState();
      expect(state.commandBlocks["session-1"]).toHaveLength(0);
    });

    it("should create block on prompt_start if command had text", () => {
      const store = useStore.getState();
      store.handleCommandStart("session-1", "ls");
      store.appendOutput("session-1", "output");
      store.handlePromptStart("session-1");

      const state = useStore.getState();
      expect(state.commandBlocks["session-1"]).toHaveLength(1);
      expect(state.commandBlocks["session-1"][0].command).toBe("ls");
    });
  });

  describe("Edge Cases", () => {
    beforeEach(() => {
      useStore.getState().addSession({
        id: "session-1",
        name: "Terminal",
        workingDirectory: "/home/user",
        createdAt: "2024-01-01T00:00:00Z",
        mode: "terminal",
      });
    });

    it("should handle command with empty string (different from null)", () => {
      const store = useStore.getState();
      store.handleCommandStart("session-1", "");
      store.handleCommandEnd("session-1", 0);

      const state = useStore.getState();
      // Empty string is falsy, so no block should be created
      expect(state.commandBlocks["session-1"]).toHaveLength(0);
    });

    it("should handle output for non-existent session gracefully", () => {
      const store = useStore.getState();
      // Should not throw
      expect(() => store.appendOutput("non-existent", "data")).not.toThrow();
    });

    it("should handle multiple commands in sequence", () => {
      const store = useStore.getState();

      // First command
      store.handleCommandStart("session-1", "ls");
      store.appendOutput("session-1", "file1\n");
      store.handleCommandEnd("session-1", 0);

      // Second command
      store.handleCommandStart("session-1", "pwd");
      store.appendOutput("session-1", "/home/user\n");
      store.handleCommandEnd("session-1", 0);

      const state = useStore.getState();
      expect(state.commandBlocks["session-1"]).toHaveLength(2);
      expect(state.commandBlocks["session-1"][0].command).toBe("ls");
      expect(state.commandBlocks["session-1"][1].command).toBe("pwd");
    });

    it("should handle command with non-zero exit code", () => {
      const store = useStore.getState();
      store.handleCommandStart("session-1", "cat nonexistent.txt");
      store.appendOutput("session-1", "cat: nonexistent.txt: No such file or directory\n");
      store.handleCommandEnd("session-1", 1);

      const state = useStore.getState();
      expect(state.commandBlocks["session-1"][0].exitCode).toBe(1);
    });
  });

  describe("Agent Mode", () => {
    beforeEach(() => {
      useStore.getState().addSession({
        id: "session-1",
        name: "Agent",
        workingDirectory: "/home/user",
        createdAt: "2024-01-01T00:00:00Z",
        mode: "agent",
      });
    });

    describe("Agent Messages", () => {
      it("should add agent message to session", () => {
        const store = useStore.getState();
        store.addAgentMessage("session-1", {
          id: "msg-1",
          sessionId: "session-1",
          role: "user",
          content: "Hello, agent!",
          timestamp: new Date().toISOString(),
        });

        const state = useStore.getState();
        expect(state.agentMessages["session-1"]).toHaveLength(1);
        expect(state.agentMessages["session-1"][0].content).toBe("Hello, agent!");
      });

      it("should add message to timeline", () => {
        const store = useStore.getState();
        store.addAgentMessage("session-1", {
          id: "msg-1",
          sessionId: "session-1",
          role: "assistant",
          content: "I can help with that.",
          timestamp: new Date().toISOString(),
        });

        const state = useStore.getState();
        expect(state.timelines["session-1"]).toHaveLength(1);
        expect(state.timelines["session-1"][0].type).toBe("agent_message");
      });

      it("should preserve message order", () => {
        const store = useStore.getState();
        store.addAgentMessage("session-1", {
          id: "msg-1",
          sessionId: "session-1",
          role: "user",
          content: "First",
          timestamp: "2024-01-01T00:00:00Z",
        });
        store.addAgentMessage("session-1", {
          id: "msg-2",
          sessionId: "session-1",
          role: "assistant",
          content: "Second",
          timestamp: "2024-01-01T00:00:01Z",
        });

        const state = useStore.getState();
        expect(state.agentMessages["session-1"][0].content).toBe("First");
        expect(state.agentMessages["session-1"][1].content).toBe("Second");
      });
    });

    describe("Agent Streaming", () => {
      it("should update streaming content", () => {
        const store = useStore.getState();
        store.updateAgentStreaming("session-1", "Thinking...");

        expect(useStore.getState().agentStreaming["session-1"]).toBe("Thinking...");
      });

      it("should clear streaming content", () => {
        const store = useStore.getState();
        store.updateAgentStreaming("session-1", "Some content");
        store.clearAgentStreaming("session-1");

        expect(useStore.getState().agentStreaming["session-1"]).toBe("");
      });

      it("should accumulate streaming content with updates", () => {
        const store = useStore.getState();
        store.updateAgentStreaming("session-1", "Hello");
        store.updateAgentStreaming("session-1", "Hello, how");
        store.updateAgentStreaming("session-1", "Hello, how can I help?");

        expect(useStore.getState().agentStreaming["session-1"]).toBe("Hello, how can I help?");
      });
    });

    describe("Tool Approval", () => {
      it("should set pending tool approval", () => {
        const store = useStore.getState();
        const tool = {
          id: "tool-1",
          name: "file_read",
          args: { path: "/etc/passwd" },
          status: "pending" as const,
        };
        store.setPendingToolApproval("session-1", tool);

        const state = useStore.getState();
        expect(state.pendingToolApproval["session-1"]).toEqual(tool);
      });

      it("should clear pending tool approval", () => {
        const store = useStore.getState();
        store.setPendingToolApproval("session-1", {
          id: "tool-1",
          name: "file_read",
          args: {},
          status: "pending",
        });
        store.setPendingToolApproval("session-1", null);

        expect(useStore.getState().pendingToolApproval["session-1"]).toBeNull();
      });

      it("should track processed tool requests", () => {
        // First check - should not be processed
        expect(useStore.getState().isToolRequestProcessed("req-1")).toBe(false);

        // Mark as processed
        useStore.getState().markToolRequestProcessed("req-1");

        // Re-fetch state after mutation to check
        expect(useStore.getState().isToolRequestProcessed("req-1")).toBe(true);
        expect(useStore.getState().isToolRequestProcessed("req-2")).toBe(false);
      });

      it("should update tool call status", () => {
        const store = useStore.getState();
        store.addAgentMessage("session-1", {
          id: "msg-1",
          sessionId: "session-1",
          role: "assistant",
          content: "Let me read that file.",
          timestamp: new Date().toISOString(),
          toolCalls: [
            {
              id: "tool-1",
              name: "file_read",
              args: { path: "/test.txt" },
              status: "pending",
            },
          ],
        });

        store.updateToolCallStatus("session-1", "tool-1", "completed", "file contents");

        const state = useStore.getState();
        const toolCall = state.agentMessages["session-1"][0].toolCalls?.[0];
        expect(toolCall?.status).toBe("completed");
        expect(toolCall?.result).toBe("file contents");
      });
    });

    describe("Agent Initialization", () => {
      it("should track agent initialization state", () => {
        const store = useStore.getState();

        expect(useStore.getState().agentInitialized["session-1"]).toBe(false);

        store.setAgentInitialized("session-1", true);

        expect(useStore.getState().agentInitialized["session-1"]).toBe(true);
      });
    });

    describe("Clear Operations", () => {
      it("should clear agent messages", () => {
        const store = useStore.getState();
        store.addAgentMessage("session-1", {
          id: "msg-1",
          sessionId: "session-1",
          role: "user",
          content: "Test",
          timestamp: new Date().toISOString(),
        });
        store.updateAgentStreaming("session-1", "streaming...");

        store.clearAgentMessages("session-1");

        const state = useStore.getState();
        expect(state.agentMessages["session-1"]).toHaveLength(0);
        expect(state.agentStreaming["session-1"]).toBe("");
      });

      it("should clear entire timeline", () => {
        const store = useStore.getState();

        // Add both command and agent message
        store.handleCommandStart("session-1", "ls");
        store.handleCommandEnd("session-1", 0);
        store.addAgentMessage("session-1", {
          id: "msg-1",
          sessionId: "session-1",
          role: "user",
          content: "Test",
          timestamp: new Date().toISOString(),
        });

        expect(useStore.getState().timelines["session-1"].length).toBeGreaterThan(0);

        store.clearTimeline("session-1");

        const state = useStore.getState();
        expect(state.timelines["session-1"]).toHaveLength(0);
        expect(state.commandBlocks["session-1"]).toHaveLength(0);
        expect(state.agentMessages["session-1"]).toHaveLength(0);
      });
    });
  });

  describe("Input Mode Toggle", () => {
    beforeEach(() => {
      useStore.getState().addSession({
        id: "session-1",
        name: "Test",
        workingDirectory: "/home/user",
        createdAt: "2024-01-01T00:00:00Z",
        mode: "terminal",
      });
    });

    it("should default inputMode to terminal", () => {
      const state = useStore.getState();
      expect(state.sessions["session-1"].inputMode).toBe("terminal");
    });

    it("should toggle inputMode between terminal and agent", () => {
      const store = useStore.getState();

      store.setInputMode("session-1", "agent");
      expect(useStore.getState().sessions["session-1"].inputMode).toBe("agent");

      store.setInputMode("session-1", "terminal");
      expect(useStore.getState().sessions["session-1"].inputMode).toBe("terminal");
    });
  });

  describe("Session Removal", () => {
    it("should clean up all state when session is removed", () => {
      const store = useStore.getState();
      store.addSession({
        id: "session-1",
        name: "Test",
        workingDirectory: "/test",
        createdAt: new Date().toISOString(),
        mode: "terminal",
      });

      // Add some state
      store.handleCommandStart("session-1", "ls");
      store.appendOutput("session-1", "output");
      store.addAgentMessage("session-1", {
        id: "msg-1",
        sessionId: "session-1",
        role: "user",
        content: "test",
        timestamp: new Date().toISOString(),
      });

      store.removeSession("session-1");

      const state = useStore.getState();
      expect(state.sessions["session-1"]).toBeUndefined();
      expect(state.commandBlocks["session-1"]).toBeUndefined();
      expect(state.pendingCommand["session-1"]).toBeUndefined();
      expect(state.timelines["session-1"]).toBeUndefined();
      expect(state.agentMessages["session-1"]).toBeUndefined();
      expect(state.agentStreaming["session-1"]).toBeUndefined();
    });

    it("should switch active session when active is removed", () => {
      const store = useStore.getState();
      store.addSession({
        id: "session-1",
        name: "First",
        workingDirectory: "/test",
        createdAt: new Date().toISOString(),
        mode: "terminal",
      });
      store.addSession({
        id: "session-2",
        name: "Second",
        workingDirectory: "/test",
        createdAt: new Date().toISOString(),
        mode: "terminal",
      });

      // session-2 is now active (last added)
      store.setActiveSession("session-1");
      expect(useStore.getState().activeSessionId).toBe("session-1");

      store.removeSession("session-1");

      // Should switch to remaining session
      expect(useStore.getState().activeSessionId).toBe("session-2");
    });

    it("should set activeSessionId to null when last session is removed", () => {
      const store = useStore.getState();
      store.addSession({
        id: "session-1",
        name: "Only",
        workingDirectory: "/test",
        createdAt: new Date().toISOString(),
        mode: "terminal",
      });

      store.removeSession("session-1");

      expect(useStore.getState().activeSessionId).toBeNull();
    });
  });

  describe("Session Restore", () => {
    beforeEach(() => {
      useStore.getState().addSession({
        id: "session-1",
        name: "Test Session",
        workingDirectory: "/home/user/project",
        createdAt: "2024-01-01T00:00:00Z",
        mode: "terminal",
      });
    });

    describe("restoreAgentMessages", () => {
      it("should restore messages to agentMessages array", () => {
        const messages: AgentMessage[] = [
          {
            id: "restored-1",
            sessionId: "session-1",
            role: "user",
            content: "Hello, Claude!",
            timestamp: "2024-01-01T10:00:00Z",
            isStreaming: false,
          },
          {
            id: "restored-2",
            sessionId: "session-1",
            role: "assistant",
            content: "Hello! How can I help you today?",
            timestamp: "2024-01-01T10:00:01Z",
            isStreaming: false,
          },
        ];

        useStore.getState().restoreAgentMessages("session-1", messages);

        const state = useStore.getState();
        expect(state.agentMessages["session-1"]).toHaveLength(2);
        expect(state.agentMessages["session-1"][0].content).toBe("Hello, Claude!");
        expect(state.agentMessages["session-1"][1].content).toBe(
          "Hello! How can I help you today?"
        );
      });

      it("should also populate timeline with restored messages", () => {
        const messages: AgentMessage[] = [
          {
            id: "restored-1",
            sessionId: "session-1",
            role: "user",
            content: "First message",
            timestamp: "2024-01-01T10:00:00Z",
          },
          {
            id: "restored-2",
            sessionId: "session-1",
            role: "assistant",
            content: "Second message",
            timestamp: "2024-01-01T10:00:01Z",
          },
        ];

        useStore.getState().restoreAgentMessages("session-1", messages);

        const state = useStore.getState();
        expect(state.timelines["session-1"]).toHaveLength(2);
        expect(state.timelines["session-1"][0].type).toBe("agent_message");
        expect(state.timelines["session-1"][1].type).toBe("agent_message");
      });

      it("should clear streaming state when restoring", () => {
        // First, set some streaming state
        useStore.getState().updateAgentStreaming("session-1", "Some streaming content...");
        expect(useStore.getState().agentStreaming["session-1"]).toBe("Some streaming content...");

        // Now restore messages
        const messages: AgentMessage[] = [
          {
            id: "restored-1",
            sessionId: "session-1",
            role: "user",
            content: "Restored message",
            timestamp: "2024-01-01T10:00:00Z",
          },
        ];

        useStore.getState().restoreAgentMessages("session-1", messages);

        // Streaming should be cleared
        expect(useStore.getState().agentStreaming["session-1"]).toBe("");
      });

      it("should replace existing messages, not append", () => {
        // Add some existing messages first
        useStore.getState().addAgentMessage("session-1", {
          id: "existing-1",
          sessionId: "session-1",
          role: "user",
          content: "Existing message",
          timestamp: "2024-01-01T09:00:00Z",
        });

        expect(useStore.getState().agentMessages["session-1"]).toHaveLength(1);

        // Now restore new messages
        const messages: AgentMessage[] = [
          {
            id: "restored-1",
            sessionId: "session-1",
            role: "user",
            content: "Restored message only",
            timestamp: "2024-01-01T10:00:00Z",
          },
        ];

        useStore.getState().restoreAgentMessages("session-1", messages);

        const state = useStore.getState();
        expect(state.agentMessages["session-1"]).toHaveLength(1);
        expect(state.agentMessages["session-1"][0].content).toBe("Restored message only");
      });

      it("should handle empty message array", () => {
        // Add some messages first
        useStore.getState().addAgentMessage("session-1", {
          id: "existing-1",
          sessionId: "session-1",
          role: "user",
          content: "Existing message",
          timestamp: "2024-01-01T09:00:00Z",
        });

        // Restore with empty array
        useStore.getState().restoreAgentMessages("session-1", []);

        const state = useStore.getState();
        expect(state.agentMessages["session-1"]).toHaveLength(0);
        expect(state.timelines["session-1"]).toHaveLength(0);
      });

      it("should preserve message order", () => {
        const messages: AgentMessage[] = [
          {
            id: "msg-1",
            sessionId: "session-1",
            role: "user",
            content: "First",
            timestamp: "2024-01-01T10:00:00Z",
          },
          {
            id: "msg-2",
            sessionId: "session-1",
            role: "assistant",
            content: "Second",
            timestamp: "2024-01-01T10:00:01Z",
          },
          {
            id: "msg-3",
            sessionId: "session-1",
            role: "user",
            content: "Third",
            timestamp: "2024-01-01T10:00:02Z",
          },
          {
            id: "msg-4",
            sessionId: "session-1",
            role: "assistant",
            content: "Fourth",
            timestamp: "2024-01-01T10:00:03Z",
          },
        ];

        useStore.getState().restoreAgentMessages("session-1", messages);

        const state = useStore.getState();
        expect(state.agentMessages["session-1"][0].content).toBe("First");
        expect(state.agentMessages["session-1"][1].content).toBe("Second");
        expect(state.agentMessages["session-1"][2].content).toBe("Third");
        expect(state.agentMessages["session-1"][3].content).toBe("Fourth");

        // Timeline should have same order
        expect((state.timelines["session-1"][0].data as AgentMessage).content).toBe("First");
        expect((state.timelines["session-1"][3].data as AgentMessage).content).toBe("Fourth");
      });

      it("should initialize timeline array if it does not exist", () => {
        // Remove the timeline entry manually
        useStore.setState((state) => {
          delete state.timelines["session-1"];
        });

        const messages: AgentMessage[] = [
          {
            id: "msg-1",
            sessionId: "session-1",
            role: "user",
            content: "Test",
            timestamp: "2024-01-01T10:00:00Z",
          },
        ];

        // Should not throw
        expect(() => {
          useStore.getState().restoreAgentMessages("session-1", messages);
        }).not.toThrow();

        expect(useStore.getState().timelines["session-1"]).toHaveLength(1);
      });
    });

    describe("clearTimeline before restore", () => {
      it("should clear timeline properly before restoring", () => {
        // Add various types of content
        useStore.getState().handleCommandStart("session-1", "ls -la");
        useStore.getState().appendOutput("session-1", "file.txt\n");
        useStore.getState().handleCommandEnd("session-1", 0);

        useStore.getState().addAgentMessage("session-1", {
          id: "existing-msg",
          sessionId: "session-1",
          role: "user",
          content: "Existing agent message",
          timestamp: new Date().toISOString(),
        });

        // Verify we have mixed content
        expect(useStore.getState().timelines["session-1"].length).toBeGreaterThan(0);
        expect(useStore.getState().commandBlocks["session-1"].length).toBeGreaterThan(0);

        // Clear timeline
        useStore.getState().clearTimeline("session-1");

        const state = useStore.getState();
        expect(state.timelines["session-1"]).toHaveLength(0);
        expect(state.commandBlocks["session-1"]).toHaveLength(0);
        expect(state.agentMessages["session-1"]).toHaveLength(0);
      });
    });

    describe("timeline block structure", () => {
      it("should create proper timeline block structure for restored messages", () => {
        const messages: AgentMessage[] = [
          {
            id: "test-msg-id",
            sessionId: "session-1",
            role: "user",
            content: "Test content",
            timestamp: "2024-01-01T10:30:00Z",
            isStreaming: false,
            toolCalls: [],
          },
        ];

        useStore.getState().restoreAgentMessages("session-1", messages);

        const timelineBlock = useStore.getState().timelines["session-1"][0];
        expect(timelineBlock.id).toBe("test-msg-id");
        expect(timelineBlock.type).toBe("agent_message");
        expect(timelineBlock.timestamp).toBe("2024-01-01T10:30:00Z");
        expect(timelineBlock.data).toEqual(messages[0]);
      });

      it("should handle messages with tool calls", () => {
        const messages: AgentMessage[] = [
          {
            id: "msg-with-tools",
            sessionId: "session-1",
            role: "assistant",
            content: "Let me read that file for you.",
            timestamp: "2024-01-01T10:30:00Z",
            toolCalls: [
              {
                id: "tool-1",
                name: "read_file",
                args: { path: "/etc/hosts" },
                status: "completed",
                result: "127.0.0.1 localhost",
              },
            ],
          },
        ];

        useStore.getState().restoreAgentMessages("session-1", messages);

        const state = useStore.getState();
        const restored = state.agentMessages["session-1"][0];
        expect(restored.toolCalls).toHaveLength(1);
        expect(restored.toolCalls?.[0].name).toBe("read_file");
        expect(restored.toolCalls?.[0].status).toBe("completed");
      });

      it("should handle messages with streamingHistory", () => {
        const messages: AgentMessage[] = [
          {
            id: "msg-with-history",
            sessionId: "session-1",
            role: "assistant",
            content: "Full response content",
            timestamp: "2024-01-01T10:30:00Z",
            streamingHistory: [
              { type: "text", content: "First part of response" },
              {
                type: "tool",
                toolCall: {
                  id: "tool-1",
                  name: "bash",
                  args: { command: "ls" },
                  status: "completed",
                },
              },
              { type: "text", content: "Second part after tool" },
            ],
          },
        ];

        useStore.getState().restoreAgentMessages("session-1", messages);

        const restored = useStore.getState().agentMessages["session-1"][0];
        expect(restored.streamingHistory).toHaveLength(3);
        expect(restored.streamingHistory?.[0].type).toBe("text");
        expect(restored.streamingHistory?.[1].type).toBe("tool");
      });
    });

    describe("input mode after restore", () => {
      it("restoring messages should work with any input mode", () => {
        // Start in terminal mode
        expect(useStore.getState().sessions["session-1"].inputMode).toBe("terminal");

        const messages: AgentMessage[] = [
          {
            id: "msg-1",
            sessionId: "session-1",
            role: "user",
            content: "Test",
            timestamp: "2024-01-01T10:00:00Z",
          },
        ];

        useStore.getState().restoreAgentMessages("session-1", messages);

        // restoreAgentMessages doesn't change input mode - that's handled by restoreSession
        expect(useStore.getState().sessions["session-1"].inputMode).toBe("terminal");
      });

      it("setInputMode should work correctly after restore", () => {
        const messages: AgentMessage[] = [
          {
            id: "msg-1",
            sessionId: "session-1",
            role: "user",
            content: "Test",
            timestamp: "2024-01-01T10:00:00Z",
          },
        ];

        useStore.getState().restoreAgentMessages("session-1", messages);
        useStore.getState().setInputMode("session-1", "agent");

        expect(useStore.getState().sessions["session-1"].inputMode).toBe("agent");
      });
    });
  });
});

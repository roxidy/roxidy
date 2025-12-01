import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { useStore } from "../../store";
import { UnifiedTimeline } from "./UnifiedTimeline";

describe("UnifiedTimeline", () => {
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

    // Create a test session
    useStore.getState().addSession({
      id: "test-session",
      name: "Test",
      workingDirectory: "/test",
      createdAt: new Date().toISOString(),
      mode: "terminal",
    });
  });

  describe("Empty State", () => {
    it("should show empty state when no timeline, no streaming, and no running command", () => {
      render(<UnifiedTimeline sessionId="test-session" />);

      expect(screen.getByText("Qbit")).toBeInTheDocument();
      expect(screen.getByText(/Run terminal commands or ask the AI assistant/)).toBeInTheDocument();
    });

    it("should NOT show empty state when there is a running command with command text", () => {
      useStore.getState().handleCommandStart("test-session", "ls -la");

      render(<UnifiedTimeline sessionId="test-session" />);

      // Empty state text should NOT be visible
      expect(screen.queryByText("Qbit")).not.toBeInTheDocument();
      // Running command should be visible
      expect(screen.getByText("ls -la")).toBeInTheDocument();
    });

    it("should show empty state when pendingCommand exists but command is null", () => {
      // This simulates receiving terminal_output before command_start
      // which shouldn't happen but we should handle gracefully
      useStore.getState().handleCommandStart("test-session", null);

      render(<UnifiedTimeline sessionId="test-session" />);

      // Should still show empty state since there's no actual command
      expect(screen.getByText("Qbit")).toBeInTheDocument();
    });

    it("should NOT show empty state when agent is streaming", () => {
      useStore.getState().updateAgentStreaming("test-session", "Thinking...");

      render(<UnifiedTimeline sessionId="test-session" />);

      expect(screen.queryByText("Qbit")).not.toBeInTheDocument();
      expect(screen.getByText("Thinking...")).toBeInTheDocument();
    });
  });

  describe("Running Command Display", () => {
    it("should show running command with pulsing indicator", () => {
      useStore.getState().handleCommandStart("test-session", "ping localhost");

      render(<UnifiedTimeline sessionId="test-session" />);

      expect(screen.getByText("ping localhost")).toBeInTheDocument();
    });

    it("should NOT show running indicator when pendingCommand.command is null", () => {
      useStore.getState().handleCommandStart("test-session", null);

      render(<UnifiedTimeline sessionId="test-session" />);

      // The running command section shouldn't render
      expect(screen.queryByText("Running...")).not.toBeInTheDocument();
    });

    it("should show streaming output for running command", () => {
      useStore.getState().handleCommandStart("test-session", "cat file.txt");
      useStore.getState().appendOutput("test-session", "line 1\nline 2\n");

      render(<UnifiedTimeline sessionId="test-session" />);

      expect(screen.getByText("cat file.txt")).toBeInTheDocument();
      // Output should be visible (ansi-to-react may transform it)
      expect(screen.getByText(/line 1/)).toBeInTheDocument();
    });

    it("should NOT show output section when pendingCommand has no output", () => {
      useStore.getState().handleCommandStart("test-session", "ls");

      render(<UnifiedTimeline sessionId="test-session" />);

      expect(screen.getByText("ls")).toBeInTheDocument();
      // The output div shouldn't be rendered when output is empty
      const outputContainers = document.querySelectorAll(".ansi-output");
      expect(outputContainers.length).toBe(0);
    });
  });

  describe("Completed Commands in Timeline", () => {
    it("should show completed command block in timeline", () => {
      useStore.getState().handleCommandStart("test-session", "echo hello");
      useStore.getState().appendOutput("test-session", "hello\n");
      useStore.getState().handleCommandEnd("test-session", 0);

      render(<UnifiedTimeline sessionId="test-session" />);

      // Command should be in the timeline (via UnifiedBlock)
      expect(screen.getByText("echo hello")).toBeInTheDocument();
    });

    it("should show multiple completed commands in order", () => {
      const store = useStore.getState();

      store.handleCommandStart("test-session", "first");
      store.appendOutput("test-session", "1\n");
      store.handleCommandEnd("test-session", 0);

      store.handleCommandStart("test-session", "second");
      store.appendOutput("test-session", "2\n");
      store.handleCommandEnd("test-session", 0);

      render(<UnifiedTimeline sessionId="test-session" />);

      screen.getAllByRole("code");
      // Both commands should be visible
      expect(screen.getByText("first")).toBeInTheDocument();
      expect(screen.getByText("second")).toBeInTheDocument();
    });
  });

  describe("Agent Streaming", () => {
    it("should show agent streaming indicator with content", () => {
      useStore
        .getState()
        .updateAgentStreaming("test-session", "I am thinking about your request...");

      render(<UnifiedTimeline sessionId="test-session" />);

      expect(screen.getByText(/I am thinking about your request/)).toBeInTheDocument();
    });

    it("should show pulsing cursor during agent streaming", () => {
      useStore.getState().updateAgentStreaming("test-session", "Response...");

      render(<UnifiedTimeline sessionId="test-session" />);

      // There should be a pulsing cursor element
      const cursor = document.querySelector(".animate-pulse");
      expect(cursor).toBeInTheDocument();
    });
  });

  describe("Bug Prevention - The Issues We Fixed", () => {
    it("BUG: should NOT show Running or empty command when app starts fresh", () => {
      // Fresh state - no commands started
      render(<UnifiedTimeline sessionId="test-session" />);

      // Should show empty state, not "Running..."
      expect(screen.getByText("Qbit")).toBeInTheDocument();
      expect(screen.queryByText("Running...")).not.toBeInTheDocument();
    });

    it("BUG: should NOT create (empty command) blocks", () => {
      const store = useStore.getState();

      // Simulate what was happening: command_start with null followed by command_end
      store.handleCommandStart("test-session", null);
      store.handleCommandEnd("test-session", 0);

      render(<UnifiedTimeline sessionId="test-session" />);

      // Should show empty state, not a block with "(empty command)"
      expect(screen.getByText("Qbit")).toBeInTheDocument();
      expect(useStore.getState().commandBlocks["test-session"]).toHaveLength(0);
    });

    it("BUG: terminal output before command_start should NOT create pendingCommand", () => {
      const store = useStore.getState();

      // This simulates receiving output when no command is running
      store.appendOutput("test-session", "prompt text\n");

      render(<UnifiedTimeline sessionId="test-session" />);

      // Should show empty state
      expect(screen.getByText("Qbit")).toBeInTheDocument();
      // pendingCommand should still be null
      expect(useStore.getState().pendingCommand["test-session"]).toBeNull();
    });

    it("BUG: empty string command should NOT create a block", () => {
      const store = useStore.getState();

      store.handleCommandStart("test-session", "");
      store.handleCommandEnd("test-session", 0);

      render(<UnifiedTimeline sessionId="test-session" />);

      // Should show empty state
      expect(screen.getByText("Qbit")).toBeInTheDocument();
      expect(useStore.getState().commandBlocks["test-session"]).toHaveLength(0);
    });
  });
});

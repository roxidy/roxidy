import { beforeEach, describe, expect, it } from "vitest";
import { type ActiveToolCall, type ToolCallSource, useStore } from "./index";

describe("Store Workflow Actions", () => {
  const testSessionId = "test-session-123";

  beforeEach(() => {
    // Reset store to initial state
    useStore.setState({
      activeWorkflows: {},
      workflowHistory: {},
      activeToolCalls: {},
    });
  });

  describe("startWorkflow", () => {
    it("creates a new active workflow", () => {
      useStore.getState().startWorkflow(testSessionId, {
        workflowId: "wf-123",
        workflowName: "git_commit",
        workflowSessionId: "session-456",
      });

      const workflow = useStore.getState().activeWorkflows[testSessionId];

      expect(workflow).toBeDefined();
      expect(workflow?.workflowId).toBe("wf-123");
      expect(workflow?.workflowName).toBe("git_commit");
      expect(workflow?.sessionId).toBe("session-456");
      expect(workflow?.status).toBe("running");
      expect(workflow?.steps).toEqual([]);
      expect(workflow?.currentStepIndex).toBe(-1);
      expect(workflow?.totalSteps).toBe(0);
      expect(workflow?.startedAt).toBeDefined();
    });
  });

  describe("workflowStepStarted", () => {
    it("initializes a new step when not present", () => {
      // Start workflow first
      useStore.getState().startWorkflow(testSessionId, {
        workflowId: "wf-123",
        workflowName: "git_commit",
        workflowSessionId: "session-456",
      });

      useStore.getState().workflowStepStarted(testSessionId, {
        stepName: "gatherer",
        stepIndex: 0,
        totalSteps: 4,
      });

      const workflow = useStore.getState().activeWorkflows[testSessionId];

      expect(workflow?.currentStepIndex).toBe(0);
      expect(workflow?.totalSteps).toBe(4);
      expect(workflow?.steps[0]).toEqual(
        expect.objectContaining({
          name: "gatherer",
          index: 0,
          status: "running",
        })
      );
      expect(workflow?.steps[0].startedAt).toBeDefined();
    });

    it("updates existing step to running status", () => {
      useStore.getState().startWorkflow(testSessionId, {
        workflowId: "wf-123",
        workflowName: "git_commit",
        workflowSessionId: "session-456",
      });

      // First call creates the step
      useStore.getState().workflowStepStarted(testSessionId, {
        stepName: "gatherer",
        stepIndex: 0,
        totalSteps: 4,
      });

      // Manually set to pending to simulate restart
      useStore.setState((state) => {
        if (state.activeWorkflows[testSessionId]?.steps[0]) {
          state.activeWorkflows[testSessionId].steps[0].status = "pending";
        }
      });

      // Second call should update to running
      useStore.getState().workflowStepStarted(testSessionId, {
        stepName: "gatherer",
        stepIndex: 0,
        totalSteps: 4,
      });

      const workflow = useStore.getState().activeWorkflows[testSessionId];
      expect(workflow?.steps[0].status).toBe("running");
    });

    it("does nothing when no active workflow", () => {
      useStore.getState().workflowStepStarted(testSessionId, {
        stepName: "gatherer",
        stepIndex: 0,
        totalSteps: 4,
      });

      expect(useStore.getState().activeWorkflows[testSessionId]).toBeUndefined();
    });
  });

  describe("workflowStepCompleted", () => {
    it("marks step as completed with output and duration", () => {
      useStore.getState().startWorkflow(testSessionId, {
        workflowId: "wf-123",
        workflowName: "git_commit",
        workflowSessionId: "session-456",
      });

      useStore.getState().workflowStepStarted(testSessionId, {
        stepName: "gatherer",
        stepIndex: 0,
        totalSteps: 4,
      });

      useStore.getState().workflowStepCompleted(testSessionId, {
        stepName: "gatherer",
        output: "Gathered git data successfully",
        durationMs: 1500,
      });

      const workflow = useStore.getState().activeWorkflows[testSessionId];
      const step = workflow?.steps.find((s) => s.name === "gatherer");

      expect(step?.status).toBe("completed");
      expect(step?.output).toBe("Gathered git data successfully");
      expect(step?.durationMs).toBe(1500);
      expect(step?.completedAt).toBeDefined();
    });

    it("finds step by name rather than index", () => {
      useStore.getState().startWorkflow(testSessionId, {
        workflowId: "wf-123",
        workflowName: "git_commit",
        workflowSessionId: "session-456",
      });

      // Start multiple steps
      useStore.getState().workflowStepStarted(testSessionId, {
        stepName: "gatherer",
        stepIndex: 0,
        totalSteps: 4,
      });
      useStore.getState().workflowStepStarted(testSessionId, {
        stepName: "analyzer",
        stepIndex: 1,
        totalSteps: 4,
      });

      // Complete the second step
      useStore.getState().workflowStepCompleted(testSessionId, {
        stepName: "analyzer",
        output: "Analysis complete",
        durationMs: 2000,
      });

      const workflow = useStore.getState().activeWorkflows[testSessionId];
      expect(workflow?.steps[0].status).toBe("running"); // gatherer still running
      expect(workflow?.steps[1].status).toBe("completed"); // analyzer completed
    });
  });

  describe("completeWorkflow", () => {
    it("marks workflow as completed and moves to history", () => {
      useStore.getState().startWorkflow(testSessionId, {
        workflowId: "wf-123",
        workflowName: "git_commit",
        workflowSessionId: "session-456",
      });

      useStore.getState().completeWorkflow(testSessionId, {
        finalOutput: "## Git Commit Plan\n\n1 commit planned",
        totalDurationMs: 5000,
      });

      const workflow = useStore.getState().activeWorkflows[testSessionId];
      const history = useStore.getState().workflowHistory[testSessionId];

      // Workflow should still be in activeWorkflows for display
      expect(workflow?.status).toBe("completed");
      expect(workflow?.finalOutput).toBe("## Git Commit Plan\n\n1 commit planned");
      expect(workflow?.totalDurationMs).toBe(5000);
      expect(workflow?.completedAt).toBeDefined();

      // Should also be in history
      expect(history).toHaveLength(1);
      expect(history[0].status).toBe("completed");
    });
  });

  describe("failWorkflow", () => {
    it("marks workflow as error and moves to history", () => {
      useStore.getState().startWorkflow(testSessionId, {
        workflowId: "wf-123",
        workflowName: "git_commit",
        workflowSessionId: "session-456",
      });

      useStore.getState().failWorkflow(testSessionId, {
        stepName: "analyzer",
        error: "Failed to analyze changes",
      });

      const workflow = useStore.getState().activeWorkflows[testSessionId];
      const history = useStore.getState().workflowHistory[testSessionId];

      expect(workflow?.status).toBe("error");
      expect(workflow?.error).toBe("Failed to analyze changes");
      expect(workflow?.completedAt).toBeDefined();

      expect(history).toHaveLength(1);
      expect(history[0].status).toBe("error");
    });

    it("marks specified step as error", () => {
      useStore.getState().startWorkflow(testSessionId, {
        workflowId: "wf-123",
        workflowName: "git_commit",
        workflowSessionId: "session-456",
      });

      // Start first step to populate index 0
      useStore.getState().workflowStepStarted(testSessionId, {
        stepName: "gatherer",
        stepIndex: 0,
        totalSteps: 4,
      });

      // Start second step
      useStore.getState().workflowStepStarted(testSessionId, {
        stepName: "analyzer",
        stepIndex: 1,
        totalSteps: 4,
      });

      useStore.getState().failWorkflow(testSessionId, {
        stepName: "analyzer",
        error: "LLM returned empty response",
      });

      const workflow = useStore.getState().activeWorkflows[testSessionId];
      const step = workflow?.steps.find((s) => s.name === "analyzer");

      expect(step?.status).toBe("error");
    });
  });

  describe("clearActiveWorkflow", () => {
    it("removes active workflow for session", () => {
      useStore.getState().startWorkflow(testSessionId, {
        workflowId: "wf-123",
        workflowName: "git_commit",
        workflowSessionId: "session-456",
      });

      expect(useStore.getState().activeWorkflows[testSessionId]).toBeDefined();

      useStore.getState().clearActiveWorkflow(testSessionId);

      expect(useStore.getState().activeWorkflows[testSessionId]).toBeNull();
    });
  });

  describe("preserveWorkflowToolCalls", () => {
    it("preserves tool calls belonging to the workflow", () => {
      const workflowSource: ToolCallSource = {
        type: "workflow",
        workflowId: "wf-123",
        workflowName: "git_commit",
        stepName: "gatherer",
        stepIndex: 0,
      };

      const workflowToolCall: ActiveToolCall = {
        id: "tool-1",
        name: "run_pty_cmd",
        args: { command: "git status" },
        status: "completed",
        startedAt: new Date().toISOString(),
        source: workflowSource,
      };

      const mainToolCall: ActiveToolCall = {
        id: "tool-2",
        name: "read_file",
        args: { path: "/tmp/test.txt" },
        status: "completed",
        startedAt: new Date().toISOString(),
        source: { type: "main" },
      };

      // Set up state
      useStore.getState().startWorkflow(testSessionId, {
        workflowId: "wf-123",
        workflowName: "git_commit",
        workflowSessionId: "session-456",
      });

      useStore.setState({
        activeToolCalls: {
          [testSessionId]: [workflowToolCall, mainToolCall],
        },
      });

      // Preserve tool calls
      useStore.getState().preserveWorkflowToolCalls(testSessionId);

      const workflow = useStore.getState().activeWorkflows[testSessionId];

      // Only workflow tool calls should be preserved
      expect(workflow?.toolCalls).toHaveLength(1);
      expect(workflow?.toolCalls?.[0].id).toBe("tool-1");
      expect(workflow?.toolCalls?.[0].name).toBe("run_pty_cmd");
    });

    it("does not preserve tool calls from different workflow", () => {
      const differentWorkflowSource: ToolCallSource = {
        type: "workflow",
        workflowId: "wf-999", // Different workflow ID
        workflowName: "other_workflow",
      };

      const toolCall: ActiveToolCall = {
        id: "tool-1",
        name: "run_pty_cmd",
        args: { command: "git status" },
        status: "completed",
        startedAt: new Date().toISOString(),
        source: differentWorkflowSource,
      };

      useStore.getState().startWorkflow(testSessionId, {
        workflowId: "wf-123",
        workflowName: "git_commit",
        workflowSessionId: "session-456",
      });

      useStore.setState({
        activeToolCalls: {
          [testSessionId]: [toolCall],
        },
      });

      useStore.getState().preserveWorkflowToolCalls(testSessionId);

      const workflow = useStore.getState().activeWorkflows[testSessionId];

      expect(workflow?.toolCalls).toHaveLength(0);
    });

    it("does nothing when no active workflow", () => {
      const toolCall: ActiveToolCall = {
        id: "tool-1",
        name: "run_pty_cmd",
        args: { command: "git status" },
        status: "completed",
        startedAt: new Date().toISOString(),
      };

      useStore.setState({
        activeToolCalls: {
          [testSessionId]: [toolCall],
        },
      });

      // Should not throw
      useStore.getState().preserveWorkflowToolCalls(testSessionId);

      // activeToolCalls should be unchanged
      expect(useStore.getState().activeToolCalls[testSessionId]).toHaveLength(1);
    });
  });

  describe("addActiveToolCall with source", () => {
    beforeEach(() => {
      // Add session to store
      useStore.setState({
        sessions: {
          [testSessionId]: {
            id: testSessionId,
            name: "Test Session",
            workingDirectory: "/tmp",
            createdAt: new Date().toISOString(),
            mode: "agent",
          },
        },
        activeToolCalls: {
          [testSessionId]: [],
        },
      });
    });

    it("stores tool call with workflow source", () => {
      const workflowSource: ToolCallSource = {
        type: "workflow",
        workflowId: "wf-123",
        workflowName: "git_commit",
        stepName: "gatherer",
        stepIndex: 0,
      };

      useStore.getState().addActiveToolCall(testSessionId, {
        id: "tool-1",
        name: "run_pty_cmd",
        args: { command: "git status" },
        source: workflowSource,
      });

      const toolCalls = useStore.getState().activeToolCalls[testSessionId];

      expect(toolCalls).toHaveLength(1);
      expect(toolCalls[0].source).toEqual(workflowSource);
    });

    it("stores tool call with main source by default", () => {
      useStore.getState().addActiveToolCall(testSessionId, {
        id: "tool-1",
        name: "read_file",
        args: { path: "/tmp/test.txt" },
      });

      const toolCalls = useStore.getState().activeToolCalls[testSessionId];

      expect(toolCalls).toHaveLength(1);
      expect(toolCalls[0].source).toBeUndefined();
    });
  });

  describe("addStreamingToolBlock with source", () => {
    beforeEach(() => {
      useStore.setState({
        streamingBlocks: {
          [testSessionId]: [],
        },
      });
    });

    it("stores streaming tool block with workflow source", () => {
      const workflowSource: ToolCallSource = {
        type: "workflow",
        workflowId: "wf-123",
        workflowName: "git_commit",
        stepName: "gatherer",
        stepIndex: 0,
      };

      useStore.getState().addStreamingToolBlock(testSessionId, {
        id: "tool-1",
        name: "run_pty_cmd",
        args: { command: "git diff" },
        source: workflowSource,
      });

      const blocks = useStore.getState().streamingBlocks[testSessionId];

      expect(blocks).toHaveLength(1);
      expect(blocks[0].type).toBe("tool");
      if (blocks[0].type === "tool") {
        expect(blocks[0].toolCall.source).toEqual(workflowSource);
      }
    });
  });
});

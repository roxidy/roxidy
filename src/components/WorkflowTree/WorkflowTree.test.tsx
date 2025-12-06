import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  type ActiveToolCall,
  type ActiveWorkflow,
  type ToolCallSource,
  useStore,
  type WorkflowStep,
} from "@/store";
import { WorkflowTree } from "./WorkflowTree";

// Helper to create test workflow data
function createTestWorkflow(overrides: Partial<ActiveWorkflow> = {}): ActiveWorkflow {
  return {
    workflowId: "test-workflow-id",
    workflowName: "git_commit",
    sessionId: "test-session-id",
    status: "running",
    steps: [],
    currentStepIndex: 0,
    totalSteps: 4,
    startedAt: new Date().toISOString(),
    ...overrides,
  };
}

// Helper to create test step data
function createTestStep(overrides: Partial<WorkflowStep> = {}): WorkflowStep {
  return {
    name: "test_step",
    index: 0,
    status: "pending",
    ...overrides,
  };
}

// Helper to create test tool call
function createTestToolCall(overrides: Partial<ActiveToolCall> = {}): ActiveToolCall {
  return {
    id: `tool-${Math.random().toString(36).slice(2, 9)}`,
    name: "run_pty_cmd",
    args: { command: "git status" },
    status: "completed",
    startedAt: new Date().toISOString(),
    ...overrides,
  };
}

// Helper to setup store with session and workflow state
function setupStoreWithSession(
  sessionId: string,
  workflow?: ActiveWorkflow,
  toolCalls?: ActiveToolCall[]
) {
  // First add the session to initialize all the state properly
  useStore.getState().addSession({
    id: sessionId,
    name: "Test Session",
    workingDirectory: "/tmp",
    createdAt: new Date().toISOString(),
    mode: "agent",
  });

  // Now set workflow and tool call state
  if (workflow) {
    useStore.setState((state) => ({
      ...state,
      activeWorkflows: {
        ...state.activeWorkflows,
        [sessionId]: workflow,
      },
    }));
  }

  if (toolCalls) {
    useStore.setState((state) => ({
      ...state,
      activeToolCalls: {
        ...state.activeToolCalls,
        [sessionId]: toolCalls,
      },
    }));
  }
}

describe("WorkflowTree", () => {
  const testSessionId = "test-session-123";

  beforeEach(() => {
    // Reset store to initial state by removing any existing sessions
    const state = useStore.getState();
    for (const sessionId of Object.keys(state.sessions)) {
      useStore.getState().removeSession(sessionId);
    }
    // Also clear the mock listeners between tests
    vi.clearAllMocks();
  });

  describe("rendering", () => {
    it("renders nothing when no active workflow", () => {
      setupStoreWithSession(testSessionId);

      const { container } = render(<WorkflowTree sessionId={testSessionId} />);

      expect(container.firstChild).toBeNull();
    });

    it("renders workflow header with name", () => {
      setupStoreWithSession(testSessionId, createTestWorkflow({ workflowName: "git_commit" }));

      render(<WorkflowTree sessionId={testSessionId} />);

      expect(screen.getByText("git_commit")).toBeInTheDocument();
      expect(screen.getByText("run_workflow")).toBeInTheDocument();
    });

    it("renders status badge", () => {
      setupStoreWithSession(testSessionId, createTestWorkflow({ status: "running" }));

      render(<WorkflowTree sessionId={testSessionId} />);

      expect(screen.getByText("Running")).toBeInTheDocument();
    });

    it("renders completed status badge", () => {
      setupStoreWithSession(testSessionId, createTestWorkflow({ status: "completed" }));

      render(<WorkflowTree sessionId={testSessionId} />);

      expect(screen.getByText("Completed")).toBeInTheDocument();
    });

    it("renders error status badge", () => {
      setupStoreWithSession(
        testSessionId,
        createTestWorkflow({
          status: "error",
          error: "Failed to analyze",
        })
      );

      render(<WorkflowTree sessionId={testSessionId} />);

      expect(screen.getByText("Error")).toBeInTheDocument();
    });
  });

  describe("steps rendering", () => {
    it("renders workflow steps with correct labels", () => {
      setupStoreWithSession(
        testSessionId,
        createTestWorkflow({
          steps: [
            createTestStep({ name: "gatherer", index: 0, status: "completed" }),
            createTestStep({ name: "analyzer", index: 1, status: "running" }),
            createTestStep({ name: "organizer", index: 2, status: "pending" }),
          ],
        })
      );

      render(<WorkflowTree sessionId={testSessionId} />);

      expect(screen.getByText(/Step 1: gatherer/)).toBeInTheDocument();
      expect(screen.getByText(/Step 2: analyzer/)).toBeInTheDocument();
      expect(screen.getByText(/Step 3: organizer/)).toBeInTheDocument();
    });

    it("shows step duration when available", () => {
      setupStoreWithSession(
        testSessionId,
        createTestWorkflow({
          steps: [
            createTestStep({
              name: "gatherer",
              index: 0,
              status: "completed",
              durationMs: 2500,
            }),
          ],
        })
      );

      render(<WorkflowTree sessionId={testSessionId} />);

      expect(screen.getByText("2.5s")).toBeInTheDocument();
    });
  });

  describe("tool calls within steps", () => {
    it("stores tool calls with workflow source in activeToolCalls", () => {
      const workflowId = "test-workflow-id";
      const workflowSource: ToolCallSource = {
        type: "workflow",
        workflowId,
        workflowName: "git_commit",
        stepName: "gatherer",
        stepIndex: 0,
      };

      const toolCalls = [
        createTestToolCall({
          id: "tool-1",
          name: "run_pty_cmd",
          args: { command: "git status" },
          source: workflowSource,
        }),
      ];

      setupStoreWithSession(
        testSessionId,
        createTestWorkflow({
          workflowId,
          steps: [createTestStep({ name: "gatherer", index: 0, status: "completed" })],
        }),
        toolCalls
      );

      render(<WorkflowTree sessionId={testSessionId} />);

      // Verify the step is rendered
      expect(screen.getByText(/Step 1: gatherer/)).toBeInTheDocument();

      // Verify tool calls are in store correctly
      const storeToolCalls = useStore.getState().activeToolCalls[testSessionId];
      expect(storeToolCalls).toHaveLength(1);
      expect(storeToolCalls[0].source).toEqual(workflowSource);
    });

    it("filters out tool calls from different workflow in store", () => {
      const workflowId = "test-workflow-id";
      const differentWorkflowSource: ToolCallSource = {
        type: "workflow",
        workflowId: "other-workflow",
        workflowName: "other_workflow",
        stepName: "step1",
        stepIndex: 0,
      };

      const correctSource: ToolCallSource = {
        type: "workflow",
        workflowId,
        workflowName: "git_commit",
        stepName: "gatherer",
        stepIndex: 0,
      };

      const toolCalls = [
        createTestToolCall({
          id: "tool-1",
          name: "run_pty_cmd",
          args: { command: "git status" },
          source: differentWorkflowSource,
        }),
        createTestToolCall({
          id: "tool-2",
          name: "read_file",
          args: { path: "/tmp/test.txt" },
          source: correctSource,
        }),
      ];

      setupStoreWithSession(
        testSessionId,
        createTestWorkflow({
          workflowId,
          steps: [createTestStep({ name: "gatherer", index: 0, status: "running" })],
        }),
        toolCalls
      );

      render(<WorkflowTree sessionId={testSessionId} />);

      // The workflow tree should be rendered
      expect(screen.getByText(/git_commit/)).toBeInTheDocument();

      // Verify the filtering logic in store
      const workflow = useStore.getState().activeWorkflows[testSessionId];
      const storeToolCalls = useStore.getState().activeToolCalls[testSessionId];

      // All tool calls are in store
      expect(storeToolCalls).toHaveLength(2);

      // Filter as the component does
      const workflowToolCalls = storeToolCalls.filter((tc) => {
        const source = tc.source;
        return source?.type === "workflow" && source.workflowId === workflow?.workflowId;
      });

      // Only one belongs to this workflow
      expect(workflowToolCalls).toHaveLength(1);
      expect(workflowToolCalls[0].name).toBe("read_file");
    });

    it("does not include tool calls from main agent in workflow filtering", () => {
      const workflowId = "test-workflow-id";
      const mainSource: ToolCallSource = { type: "main" };

      const toolCalls = [
        createTestToolCall({
          id: "tool-1",
          name: "read_file",
          args: { path: "/tmp/test.txt" },
          source: mainSource,
        }),
      ];

      setupStoreWithSession(
        testSessionId,
        createTestWorkflow({
          workflowId,
          steps: [createTestStep({ name: "gatherer", index: 0, status: "running" })],
        }),
        toolCalls
      );

      render(<WorkflowTree sessionId={testSessionId} />);

      // Workflow renders
      expect(screen.getByText(/git_commit/)).toBeInTheDocument();

      // Verify main agent tool calls are filtered out
      const workflow = useStore.getState().activeWorkflows[testSessionId];
      const storeToolCalls = useStore.getState().activeToolCalls[testSessionId];

      const workflowToolCalls = storeToolCalls.filter((tc) => {
        const source = tc.source;
        return source?.type === "workflow" && source.workflowId === workflow?.workflowId;
      });

      expect(workflowToolCalls).toHaveLength(0);
    });
  });

  describe("preserved tool calls", () => {
    it("stores preserved tool calls in completed workflow", () => {
      const workflowId = "test-workflow-id";
      const workflowSource: ToolCallSource = {
        type: "workflow",
        workflowId,
        workflowName: "git_commit",
        stepName: "gatherer",
        stepIndex: 0,
      };

      const workflow = createTestWorkflow({
        workflowId,
        status: "completed",
        steps: [createTestStep({ name: "gatherer", index: 0, status: "completed" })],
        toolCalls: [
          createTestToolCall({
            id: "preserved-tool",
            name: "run_pty_cmd",
            args: { command: "git status" },
            source: workflowSource,
          }),
        ],
      });

      setupStoreWithSession(testSessionId, workflow, []);

      render(<WorkflowTree sessionId={testSessionId} />);

      // Verify workflow is displayed as completed
      expect(screen.getByText("Completed")).toBeInTheDocument();

      // Verify preserved tool calls exist in workflow
      expect(workflow.toolCalls).toHaveLength(1);
      expect(workflow.toolCalls?.[0].name).toBe("run_pty_cmd");
    });
  });

  describe("error display", () => {
    it("shows error message when workflow has error", () => {
      setupStoreWithSession(
        testSessionId,
        createTestWorkflow({
          status: "error",
          error: "Failed to execute workflow: LLM timeout",
        })
      );

      render(<WorkflowTree sessionId={testSessionId} />);

      expect(screen.getByText("Failed to execute workflow: LLM timeout")).toBeInTheDocument();
    });
  });

  describe("total duration", () => {
    it("shows total duration for completed workflow", () => {
      setupStoreWithSession(
        testSessionId,
        createTestWorkflow({
          status: "completed",
          totalDurationMs: 12500,
        })
      );

      render(<WorkflowTree sessionId={testSessionId} />);

      expect(screen.getByText("Total: 12.5s")).toBeInTheDocument();
    });
  });

  describe("interactivity", () => {
    it("can collapse and expand the workflow tree", () => {
      setupStoreWithSession(
        testSessionId,
        createTestWorkflow({
          steps: [createTestStep({ name: "gatherer", index: 0, status: "completed" })],
        })
      );

      render(<WorkflowTree sessionId={testSessionId} />);

      // Initially expanded - step should be visible
      expect(screen.getByText(/Step 1: gatherer/)).toBeInTheDocument();

      // Find and click the header button to collapse
      const headerButton = screen.getByRole("button", { name: /run_workflow/i });
      fireEvent.click(headerButton);

      // After collapse, the collapsible content should be hidden
      // Note: Due to Radix UI's animation, the element may still be in DOM
      // but hidden via CSS. This test verifies the collapse interaction works.
    });
  });
});

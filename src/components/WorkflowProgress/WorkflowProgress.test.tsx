import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { type ActiveWorkflow, useStore, type WorkflowStep } from "@/store";
import { WorkflowProgress } from "./WorkflowProgress";

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

describe("WorkflowProgress", () => {
  const testSessionId = "test-session-123";

  beforeEach(() => {
    // Reset store before each test
    useStore.setState({
      activeWorkflows: {},
    });
  });

  describe("rendering with direct workflow prop", () => {
    it("renders workflow card with name and status", () => {
      const workflow = createTestWorkflow({
        workflowName: "git_commit",
        status: "running",
      });

      render(<WorkflowProgress workflow={workflow} />);

      expect(screen.getByText("git_commit")).toBeInTheDocument();
      expect(screen.getByText("Running")).toBeInTheDocument();
    });

    it("renders completed status badge", () => {
      const workflow = createTestWorkflow({
        status: "completed",
      });

      render(<WorkflowProgress workflow={workflow} />);

      expect(screen.getByText("Completed")).toBeInTheDocument();
    });

    it("renders error status badge", () => {
      const workflow = createTestWorkflow({
        status: "error",
        error: "Something went wrong",
      });

      render(<WorkflowProgress workflow={workflow} />);

      expect(screen.getByText("Error")).toBeInTheDocument();
      expect(screen.getByText("Something went wrong")).toBeInTheDocument();
    });
  });

  describe("progress bar", () => {
    it("shows progress bar when workflow is running", () => {
      const workflow = createTestWorkflow({
        status: "running",
        currentStepIndex: 1,
        totalSteps: 4,
        steps: [
          createTestStep({ name: "gatherer", index: 0, status: "completed" }),
          createTestStep({ name: "analyzer", index: 1, status: "running" }),
        ],
      });

      render(<WorkflowProgress workflow={workflow} />);

      expect(screen.getByText("Step 2 of 4")).toBeInTheDocument();
      expect(screen.getByText("25%")).toBeInTheDocument();
    });

    it("hides progress bar when workflow is completed", () => {
      const workflow = createTestWorkflow({
        status: "completed",
        totalSteps: 4,
        steps: [
          createTestStep({ name: "gatherer", index: 0, status: "completed" }),
          createTestStep({ name: "analyzer", index: 1, status: "completed" }),
        ],
      });

      render(<WorkflowProgress workflow={workflow} />);

      expect(screen.queryByText(/Step \d+ of \d+/)).not.toBeInTheDocument();
    });
  });

  describe("steps list", () => {
    it("renders all workflow steps", () => {
      const workflow = createTestWorkflow({
        steps: [
          createTestStep({ name: "gatherer", index: 0, status: "completed" }),
          createTestStep({ name: "analyzer", index: 1, status: "running" }),
          createTestStep({ name: "organizer", index: 2, status: "pending" }),
          createTestStep({ name: "planner", index: 3, status: "pending" }),
        ],
      });

      render(<WorkflowProgress workflow={workflow} />);

      expect(screen.getByText("gatherer")).toBeInTheDocument();
      expect(screen.getByText("analyzer")).toBeInTheDocument();
      expect(screen.getByText("organizer")).toBeInTheDocument();
      expect(screen.getByText("planner")).toBeInTheDocument();
    });

    it("shows step duration when available", () => {
      const workflow = createTestWorkflow({
        steps: [
          createTestStep({
            name: "gatherer",
            index: 0,
            status: "completed",
            durationMs: 1500,
          }),
        ],
      });

      render(<WorkflowProgress workflow={workflow} />);

      expect(screen.getByText("1.5s")).toBeInTheDocument();
    });

    it("formats duration in milliseconds for short durations", () => {
      const workflow = createTestWorkflow({
        steps: [
          createTestStep({
            name: "gatherer",
            index: 0,
            status: "completed",
            durationMs: 500,
          }),
        ],
      });

      render(<WorkflowProgress workflow={workflow} />);

      expect(screen.getByText("500ms")).toBeInTheDocument();
    });
  });

  describe("total duration", () => {
    it("shows total duration when workflow is completed", () => {
      const workflow = createTestWorkflow({
        status: "completed",
        totalDurationMs: 5500,
      });

      render(<WorkflowProgress workflow={workflow} />);

      expect(screen.getByText("Total: 5.5s")).toBeInTheDocument();
    });
  });

  describe("rendering with sessionId prop", () => {
    it("fetches workflow from store when sessionId is provided", () => {
      const workflow = createTestWorkflow({
        workflowName: "git_commit",
        status: "running",
      });

      useStore.setState({
        activeWorkflows: {
          [testSessionId]: workflow,
        },
      });

      render(<WorkflowProgress sessionId={testSessionId} />);

      expect(screen.getByText("git_commit")).toBeInTheDocument();
    });

    it("returns null when no workflow found for sessionId", () => {
      const { container } = render(<WorkflowProgress sessionId="nonexistent" />);

      expect(container.firstChild).toBeNull();
    });

    it("prefers direct workflow prop over store workflow", () => {
      const storeWorkflow = createTestWorkflow({ workflowName: "from_store" });
      const directWorkflow = createTestWorkflow({ workflowName: "direct" });

      useStore.setState({
        activeWorkflows: {
          [testSessionId]: storeWorkflow,
        },
      });

      render(<WorkflowProgress sessionId={testSessionId} workflow={directWorkflow} />);

      expect(screen.getByText("direct")).toBeInTheDocument();
      expect(screen.queryByText("from_store")).not.toBeInTheDocument();
    });
  });

  describe("returns null when no workflow", () => {
    it("returns null when neither sessionId nor workflow is provided", () => {
      const { container } = render(<WorkflowProgress />);

      expect(container.firstChild).toBeNull();
    });
  });
});

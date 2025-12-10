import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useRef } from "react";
import type {
  Decision,
  ErrorEntry,
  FileContext,
  Goal,
  Layer1Event,
  OpenQuestion,
} from "@/lib/sidecar";

export interface Layer1EventHandlers {
  onStateUpdated?: (sessionId: string, changes: string[]) => void;
  onGoalAdded?: (sessionId: string, goal: Goal) => void;
  onGoalCompleted?: (sessionId: string, goalId: string) => void;
  onNarrativeUpdated?: (sessionId: string, narrative: string) => void;
  onDecisionRecorded?: (sessionId: string, decision: Decision) => void;
  onErrorUpdated?: (sessionId: string, error: ErrorEntry) => void;
  onQuestionAdded?: (sessionId: string, question: OpenQuestion) => void;
  onQuestionAnswered?: (sessionId: string, questionId: string, answer: string) => void;
  onFileContextUpdated?: (sessionId: string, path: string, context: FileContext) => void;
}

/**
 * Hook to subscribe to Layer 1 sidecar events from the Tauri backend
 * and invoke handlers for state updates.
 *
 * Layer 1 events track session state changes including goals, decisions,
 * errors, questions, and file context updates.
 */
export function useLayer1Events(handlers: Layer1EventHandlers) {
  // Use refs to avoid re-subscribing when handlers change
  const handlersRef = useRef(handlers);
  handlersRef.current = handlers;

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;

    const setupListener = async () => {
      unlisten = await listen<Layer1Event>("layer1-event", (event) => {
        const payload = event.payload;
        const h = handlersRef.current;

        switch (payload.type) {
          case "state_updated":
            h.onStateUpdated?.(payload.session_id, payload.changes);
            break;
          case "goal_added":
            h.onGoalAdded?.(payload.session_id, payload.goal);
            break;
          case "goal_completed":
            h.onGoalCompleted?.(payload.session_id, payload.goal_id);
            break;
          case "narrative_updated":
            h.onNarrativeUpdated?.(payload.session_id, payload.narrative);
            break;
          case "decision_recorded":
            h.onDecisionRecorded?.(payload.session_id, payload.decision);
            break;
          case "error_updated":
            h.onErrorUpdated?.(payload.session_id, payload.error);
            break;
          case "question_added":
            h.onQuestionAdded?.(payload.session_id, payload.question);
            break;
          case "question_answered":
            h.onQuestionAnswered?.(payload.session_id, payload.question_id, payload.answer);
            break;
          case "file_context_updated":
            h.onFileContextUpdated?.(payload.session_id, payload.path, payload.context);
            break;
        }
      });
    };

    setupListener();

    return () => {
      unlisten?.();
    };
  }, []);
}

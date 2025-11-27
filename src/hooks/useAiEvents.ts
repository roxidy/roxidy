import { useEffect, useRef } from "react";
import { useStore } from "@/store";
import { onAiEvent, type AiEvent } from "@/lib/ai";

/**
 * Hook to subscribe to AI events from the Tauri backend
 * and update the store accordingly.
 *
 * Note: This hook is prepared for when the Tauri backend
 * AI integration is implemented. For now, it's a no-op
 * since the backend commands don't exist yet.
 */
export function useAiEvents() {
  const addAgentMessage = useStore((state) => state.addAgentMessage);
  const updateAgentStreaming = useStore((state) => state.updateAgentStreaming);
  const clearAgentStreaming = useStore((state) => state.clearAgentStreaming);
  const setPendingToolApproval = useStore(
    (state) => state.setPendingToolApproval
  );
  const activeSessionId = useStore((state) => state.activeSessionId);

  const unlistenRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    const handleEvent = (event: AiEvent) => {
      if (!activeSessionId) return;

      switch (event.type) {
        case "started":
          clearAgentStreaming(activeSessionId);
          break;

        case "text_delta":
          updateAgentStreaming(activeSessionId, event.accumulated);
          break;

        case "tool_request":
          setPendingToolApproval(activeSessionId, {
            id: event.request_id,
            name: event.tool_name,
            args: event.args as Record<string, unknown>,
            status: "pending",
          });
          break;

        case "reasoning":
          // Could display reasoning in a collapsible section
          console.log("AI Reasoning:", event.content);
          break;

        case "completed": {
          // Finalize streaming content as assistant message
          const streaming =
            useStore.getState().agentStreaming[activeSessionId] || "";
          const content = event.response || streaming;

          if (content) {
            addAgentMessage(activeSessionId, {
              id: crypto.randomUUID(),
              sessionId: activeSessionId,
              role: "assistant",
              content,
              timestamp: new Date().toISOString(),
            });
          }
          clearAgentStreaming(activeSessionId);
          break;
        }

        case "error":
          addAgentMessage(activeSessionId, {
            id: crypto.randomUUID(),
            sessionId: activeSessionId,
            role: "system",
            content: `Error: ${event.message}`,
            timestamp: new Date().toISOString(),
          });
          clearAgentStreaming(activeSessionId);
          break;
      }
    };

    // Only set up listener if we have an active session
    // Note: This will fail until Tauri backend is implemented
    // but that's fine - it's a no-op for now
    const setupListener = async () => {
      try {
        const unlisten = await onAiEvent(handleEvent);
        unlistenRef.current = unlisten;
      } catch {
        // AI backend not yet implemented - this is expected
        console.debug("AI events not available - backend not implemented yet");
      }
    };

    setupListener();

    return () => {
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }
    };
  }, [
    activeSessionId,
    addAgentMessage,
    updateAgentStreaming,
    clearAgentStreaming,
    setPendingToolApproval,
  ]);
}

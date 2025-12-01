import { useEffect, useRef } from "react";
import { type AiEvent, onAiEvent } from "@/lib/ai";
import { useStore } from "@/store";

/**
 * Hook to subscribe to AI events from the Tauri backend
 * and update the store accordingly.
 *
 * Note: This hook is prepared for when the Tauri backend
 * AI integration is implemented. For now, it's a no-op
 * since the backend commands don't exist yet.
 */
export function useAiEvents() {
  const unlistenRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    // Track if this effect instance is still mounted (for async cleanup)
    let isMounted = true;

    const handleEvent = (event: AiEvent) => {
      // Get the current session ID and store methods at event time
      const state = useStore.getState();
      const sessionId = state.activeSessionId;
      if (!sessionId) return;

      switch (event.type) {
        case "started":
          state.clearAgentStreaming(sessionId);
          state.clearActiveToolCalls(sessionId);
          state.clearThinkingContent(sessionId);
          state.setAgentThinking(sessionId, true);
          break;

        case "text_delta":
          state.setAgentThinking(sessionId, false);
          state.updateAgentStreaming(sessionId, event.accumulated);
          break;

        case "tool_request": {
          // Deduplicate: ignore already-processed requests
          if (state.isToolRequestProcessed(event.request_id)) {
            console.debug("Ignoring duplicate tool_request:", event.request_id);
            break;
          }
          state.setAgentThinking(sessionId, false);
          const toolCall = {
            id: event.request_id,
            name: event.tool_name,
            args: event.args as Record<string, unknown>,
            // All tool calls from AI events are executed by the agent
            executedByAgent: true,
          };
          // Track the tool call as running (for UI display)
          state.addActiveToolCall(sessionId, toolCall);
          // Also add to streaming blocks for interleaved display
          state.addStreamingToolBlock(sessionId, toolCall);
          break;
        }

        case "tool_approval_request": {
          // Enhanced tool request with HITL metadata
          // Deduplicate: ignore already-processed requests
          if (state.isToolRequestProcessed(event.request_id)) {
            console.debug("Ignoring duplicate tool_approval_request:", event.request_id);
            break;
          }
          state.setAgentThinking(sessionId, false);

          const toolCall = {
            id: event.request_id,
            name: event.tool_name,
            args: event.args as Record<string, unknown>,
            executedByAgent: true,
            riskLevel: event.risk_level,
            stats: event.stats ?? undefined,
            suggestion: event.suggestion ?? undefined,
            canLearn: event.can_learn,
          };

          // Track the tool call
          state.addActiveToolCall(sessionId, toolCall);
          state.addStreamingToolBlock(sessionId, toolCall);

          // Set pending tool approval for the dialog
          state.setPendingToolApproval(sessionId, {
            ...toolCall,
            status: "pending",
          });
          break;
        }

        case "tool_auto_approved": {
          // Tool was auto-approved based on learned patterns
          state.setAgentThinking(sessionId, false);
          const autoApprovedTool = {
            id: event.request_id,
            name: event.tool_name,
            args: event.args as Record<string, unknown>,
            executedByAgent: true,
            autoApproved: true,
            autoApprovalReason: event.reason,
          };
          state.addActiveToolCall(sessionId, autoApprovedTool);
          state.addStreamingToolBlock(sessionId, autoApprovedTool);
          break;
        }

        case "tool_result":
          // Update tool call status to completed/error
          state.completeActiveToolCall(sessionId, event.request_id, event.success, event.result);
          // Also update streaming block
          state.updateStreamingToolBlock(sessionId, event.request_id, event.success, event.result);
          break;

        case "reasoning":
          // Append thinking content to the store for display
          state.appendThinkingContent(sessionId, event.content);
          break;

        case "completed": {
          // Convert streaming blocks to a final assistant message preserving interleaved history
          const blocks = state.streamingBlocks[sessionId] || [];
          const streaming = state.agentStreaming[sessionId] || "";
          const thinkingContent = state.thinkingContent[sessionId] || "";

          // Preserve the interleaved streaming history (text + tool calls in order)
          const streamingHistory: import("@/store").FinalizedStreamingBlock[] = blocks.map(
            (block) => {
              if (block.type === "text") {
                return { type: "text" as const, content: block.content };
              }
              // Convert ActiveToolCall to ToolCall format
              return {
                type: "tool" as const,
                toolCall: {
                  id: block.toolCall.id,
                  name: block.toolCall.name,
                  args: block.toolCall.args,
                  status:
                    block.toolCall.status === "completed"
                      ? ("completed" as const)
                      : block.toolCall.status === "error"
                        ? ("error" as const)
                        : ("completed" as const),
                  result: block.toolCall.result,
                  executedByAgent: block.toolCall.executedByAgent,
                },
              };
            }
          );

          // Extract tool calls for backwards compatibility
          const toolCalls = streamingHistory
            .filter(
              (b): b is { type: "tool"; toolCall: import("@/store").ToolCall } => b.type === "tool"
            )
            .map((b) => b.toolCall);

          // Use full accumulated text as content (fallback to event.response for edge cases)
          const content = streaming || event.response || "";

          if (content || streamingHistory.length > 0) {
            state.addAgentMessage(sessionId, {
              id: crypto.randomUUID(),
              sessionId: sessionId,
              role: "assistant",
              content: content,
              timestamp: new Date().toISOString(),
              toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
              streamingHistory: streamingHistory.length > 0 ? streamingHistory : undefined,
              thinkingContent: thinkingContent || undefined,
            });
          }
          state.clearAgentStreaming(sessionId);
          state.clearStreamingBlocks(sessionId);
          state.clearThinkingContent(sessionId);
          state.setAgentThinking(sessionId, false);
          break;
        }

        case "error":
          state.addAgentMessage(sessionId, {
            id: crypto.randomUUID(),
            sessionId: sessionId,
            role: "system",
            content: `Error: ${event.message}`,
            timestamp: new Date().toISOString(),
          });
          state.clearAgentStreaming(sessionId);
          state.setAgentThinking(sessionId, false);
          break;
      }
    };

    // Only set up listener once - the handler uses getState() to access current values
    const setupListener = async () => {
      try {
        const unlisten = await onAiEvent(handleEvent);
        // Only store the unlisten function if we're still mounted
        // This handles the React Strict Mode double-mount where cleanup runs
        // before the async setup completes
        if (isMounted) {
          unlistenRef.current = unlisten;
        } else {
          // We were unmounted before setup completed - clean up immediately
          unlisten();
        }
      } catch {
        // AI backend not yet implemented - this is expected
        console.debug("AI events not available - backend not implemented yet");
      }
    };

    setupListener();

    return () => {
      isMounted = false;
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }
    };
  }, []);
}

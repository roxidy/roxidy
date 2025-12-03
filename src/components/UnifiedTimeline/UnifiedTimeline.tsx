import Ansi from "ansi-to-react";
import { Bot, Loader2, Sparkles, TerminalSquare } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef } from "react";
import { Markdown } from "@/components/Markdown";
import { StreamingThinkingBlock } from "@/components/ThinkingBlock";
import { ToolGroup, ToolItem } from "@/components/ToolCallDisplay";
import { stripOscSequences } from "@/lib/ansi";
import { groupConsecutiveTools } from "@/lib/toolGrouping";
import {
  useIsAgentThinking,
  usePendingCommand,
  useSessionTimeline,
  useStreamingBlocks,
  useThinkingContent,
} from "@/store";
import { UnifiedBlock } from "./UnifiedBlock";

interface UnifiedTimelineProps {
  sessionId: string;
}

export function UnifiedTimeline({ sessionId }: UnifiedTimelineProps) {
  const timeline = useSessionTimeline(sessionId);
  const streamingBlocks = useStreamingBlocks(sessionId);
  const pendingCommand = usePendingCommand(sessionId);
  const isAgentThinking = useIsAgentThinking(sessionId);
  const thinkingContent = useThinkingContent(sessionId);
  const containerRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  // Strip OSC sequences from pending output for display
  const pendingOutput = useMemo(
    () => (pendingCommand?.output ? stripOscSequences(pendingCommand.output) : ""),
    [pendingCommand?.output]
  );

  // Group consecutive tool calls for cleaner display
  const groupedBlocks = useMemo(() => groupConsecutiveTools(streamingBlocks), [streamingBlocks]);

  // Debounced scroll to avoid thrashing on every character delta
  const scrollTimeoutRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const scrollToBottom = useCallback(() => {
    if (scrollTimeoutRef.current) {
      clearTimeout(scrollTimeoutRef.current);
    }
    scrollTimeoutRef.current = setTimeout(() => {
      bottomRef.current?.scrollIntoView({ behavior: "smooth" });
    }, 50); // Debounce by 50ms to batch updates
  }, []);

  // Auto-scroll to bottom when new content arrives
  // biome-ignore lint/correctness/useExhaustiveDependencies: intentional triggers for auto-scroll
  useEffect(() => {
    scrollToBottom();
  }, [scrollToBottom, timeline.length, streamingBlocks.length, pendingOutput, thinkingContent]);

  // Cleanup timeout on unmount
  useEffect(() => {
    return () => {
      if (scrollTimeoutRef.current) {
        clearTimeout(scrollTimeoutRef.current);
      }
    };
  }, []);

  // Empty state - only show if no timeline, no streaming, no thinking, and no command running
  const hasRunningCommand = pendingCommand?.command;
  if (
    timeline.length === 0 &&
    streamingBlocks.length === 0 &&
    !hasRunningCommand &&
    !isAgentThinking &&
    !thinkingContent
  ) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-[#565f89] p-8">
        <div className="w-16 h-16 rounded-full bg-[#bb9af7]/10 flex items-center justify-center mb-4">
          <Sparkles className="w-8 h-8 text-[#bb9af7]" />
        </div>
        <h3 className="text-lg font-medium text-[#c0caf5] mb-2">Qbit</h3>
        <p className="text-sm text-center max-w-md">
          Run terminal commands or ask the AI assistant for help. Toggle between modes using the
          button in the input bar.
        </p>
      </div>
    );
  }

  return (
    <div ref={containerRef} className="flex-1 min-w-0 overflow-auto p-4 space-y-4">
      {timeline.map((block) => (
        <UnifiedBlock key={block.id} block={block} />
      ))}

      {/* Streaming output for running command - only show when there's an actual command */}
      {pendingCommand?.command && (
        <div className="border-l-2 border-l-[#7aa2f7] mb-2">
          {/* Header */}
          <div className="flex items-center gap-2 px-3 py-2">
            <div className="flex items-center gap-1.5">
              <TerminalSquare className="w-4 h-4 text-[#7aa2f7]" />
              <span className="w-2 h-2 bg-[#7aa2f7] rounded-full animate-pulse" />
            </div>
            <code className="text-[#c0caf5] font-mono text-sm flex-1 truncate">
              {pendingCommand.command || "Running..."}
            </code>
          </div>
          {/* Streaming output */}
          {pendingOutput && (
            <div className="px-3 pb-3 pl-9">
              <div className="ansi-output text-[13px] leading-5 whitespace-pre-wrap break-words bg-[#13131a] rounded-md p-3 border border-[#1f2335] max-h-96 overflow-auto">
                <Ansi useClasses>{pendingOutput}</Ansi>
              </div>
            </div>
          )}
        </div>
      )}

      {/* Thinking indicator - shown while waiting for first content (when no thinking content yet) */}
      {isAgentThinking && streamingBlocks.length === 0 && !thinkingContent && (
        <div className="flex gap-3">
          <div className="w-8 h-8 rounded-full bg-[#bb9af7]/20 flex items-center justify-center flex-shrink-0">
            <Bot className="w-4 h-4 text-[#bb9af7]" />
          </div>
          <div className="flex-1 max-w-[85%] min-w-0 bg-[#1f2335] border border-[#27293d] rounded-lg p-3">
            <div className="flex items-center gap-2 text-sm text-[#a9b1d6]">
              <Loader2 className="w-4 h-4 animate-spin text-[#bb9af7]" />
              <span>Thinking...</span>
            </div>
          </div>
        </div>
      )}

      {/* Agent response card - contains thinking (if any) and streaming content */}
      {(thinkingContent || streamingBlocks.length > 0) && (
        <div className="flex gap-3">
          <div className="w-8 h-8 rounded-full bg-[#bb9af7]/20 flex items-center justify-center flex-shrink-0">
            <Bot className="w-4 h-4 text-[#bb9af7]" />
          </div>
          <div className="flex-1 max-w-[85%] min-w-0 overflow-hidden bg-[#1f2335] border border-[#27293d] rounded-lg p-3 space-y-3">
            {/* Extended thinking block inside the card */}
            {thinkingContent && <StreamingThinkingBlock sessionId={sessionId} />}

            {/* Streaming text and tool calls (grouped for cleaner display) */}
            {groupedBlocks.map((block, blockIndex) => {
              if (block.type === "text") {
                const isLast = blockIndex === groupedBlocks.length - 1;
                return (
                  // biome-ignore lint/suspicious/noArrayIndexKey: blocks are appended and never reordered
                  <div key={`text-${blockIndex}`}>
                    <Markdown content={block.content} className="text-sm" streaming />
                    {isLast && (
                      <span className="inline-block w-2 h-4 bg-[#bb9af7] animate-pulse ml-0.5 align-middle" />
                    )}
                  </div>
                );
              }
              if (block.type === "tool_group") {
                return <ToolGroup key={`group-${block.tools[0].id}`} group={block} />;
              }
              // Single tool - show with inline name
              return <ToolItem key={block.toolCall.id} tool={block.toolCall} showInlineName />;
            })}
          </div>
        </div>
      )}

      {/* Scroll anchor */}
      <div ref={bottomRef} />
    </div>
  );
}

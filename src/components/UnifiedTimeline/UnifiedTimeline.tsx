import Ansi from "ansi-to-react";
import { Bot, Sparkles, TerminalSquare } from "lucide-react";
import { useEffect, useMemo, useRef } from "react";
import { Markdown } from "@/components/Markdown";
import { stripOscSequences } from "@/lib/ansi";
import { useSessionTimeline, useAgentStreaming, usePendingCommand } from "@/store";
import { UnifiedBlock } from "./UnifiedBlock";

interface UnifiedTimelineProps {
  sessionId: string;
}

export function UnifiedTimeline({ sessionId }: UnifiedTimelineProps) {
  const timeline = useSessionTimeline(sessionId);
  const streaming = useAgentStreaming(sessionId);
  const pendingCommand = usePendingCommand(sessionId);
  const containerRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  // Strip OSC sequences from pending output for display
  const pendingOutput = useMemo(
    () => (pendingCommand?.output ? stripOscSequences(pendingCommand.output) : ""),
    [pendingCommand?.output]
  );

  // Auto-scroll to bottom when new blocks arrive or streaming updates
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [timeline.length, streaming, pendingOutput]);

  // Empty state - only show if no timeline, no streaming, and no command running
  const hasRunningCommand = pendingCommand && pendingCommand.command;
  if (timeline.length === 0 && !streaming && !hasRunningCommand) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-[#565f89] p-8">
        <div className="w-16 h-16 rounded-full bg-[#bb9af7]/10 flex items-center justify-center mb-4">
          <Sparkles className="w-8 h-8 text-[#bb9af7]" />
        </div>
        <h3 className="text-lg font-medium text-[#c0caf5] mb-2">Roxidy</h3>
        <p className="text-sm text-center max-w-md">
          Run terminal commands or ask the AI assistant for help.
          Toggle between modes using the button in the input bar.
        </p>
        <div className="mt-6 flex flex-wrap gap-2 justify-center">
          {[
            "ls -la",
            "git status",
            "Explain this codebase",
            "Find TODO comments",
          ].map((suggestion) => (
            <button
              type="button"
              key={suggestion}
              className="px-3 py-1.5 text-xs bg-[#1f2335] hover:bg-[#292e42] text-[#7aa2f7] rounded-full transition-colors border border-[#3b4261]"
              onClick={() => {
                // TODO: Fill input with suggestion
              }}
            >
              {suggestion}
            </button>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div ref={containerRef} className="flex-1 overflow-auto p-4 space-y-4">
      {timeline.map((block) => (
        <UnifiedBlock key={block.id} block={block} />
      ))}

      {/* Streaming output for running command - only show when there's an actual command */}
      {pendingCommand && pendingCommand.command && (
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
              <div
                className="ansi-output font-mono text-[13px] leading-5 whitespace-pre-wrap break-words bg-[#13131a] rounded-md p-3 border border-[#1f2335] max-h-96 overflow-auto"
                style={{ fontFamily: "JetBrains Mono, Menlo, Monaco, Consolas, monospace" }}
              >
                <Ansi useClasses>{pendingOutput}</Ansi>
              </div>
            </div>
          )}
        </div>
      )}

      {/* Streaming indicator for agent responses */}
      {streaming && (
        <div className="flex gap-3">
          <div className="w-8 h-8 rounded-full bg-[#bb9af7]/20 flex items-center justify-center flex-shrink-0">
            <Bot className="w-4 h-4 text-[#bb9af7]" />
          </div>
          <div className="flex-1 max-w-[85%] bg-[#1f2335] border border-[#27293d] rounded-lg p-3">
            <Markdown content={streaming} className="text-sm" />
            <span className="inline-block w-2 h-4 bg-[#bb9af7] animate-pulse ml-0.5 align-middle" />
          </div>
        </div>
      )}

      {/* Scroll anchor */}
      <div ref={bottomRef} />
    </div>
  );
}

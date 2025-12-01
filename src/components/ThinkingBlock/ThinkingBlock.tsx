import { Brain, ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import { cn } from "@/lib/utils";
import { useIsAgentThinking, useIsThinkingExpanded, useStore, useThinkingContent } from "@/store";

interface ThinkingBlockUIProps {
  content: string;
  isExpanded: boolean;
  isThinking: boolean;
  onToggle: () => void;
}

/**
 * Shared UI component for thinking block display.
 */
function ThinkingBlockUI({ content, isExpanded, isThinking, onToggle }: ThinkingBlockUIProps) {
  return (
    <div className="rounded-md bg-[#16161e] overflow-hidden">
      {/* Header - always visible */}
      <button
        type="button"
        onClick={onToggle}
        className="w-full flex items-center gap-2 px-2.5 py-1.5 hover:bg-[#1a1b26] transition-colors text-left"
      >
        <div className="flex items-center gap-2 flex-1">
          <Brain
            className={cn(
              "w-3.5 h-3.5",
              isThinking ? "text-[#bb9af7] animate-pulse" : "text-[#7dcfff]"
            )}
          />
          <span className="text-xs font-medium text-[#787c99]">
            {isThinking ? "Thinking..." : "Thinking"}
          </span>
          <span className="text-xs text-[#565f89]">
            ({content.length.toLocaleString()} chars)
          </span>
        </div>
        {isExpanded ? (
          <ChevronDown className="w-3.5 h-3.5 text-[#565f89]" />
        ) : (
          <ChevronRight className="w-3.5 h-3.5 text-[#565f89]" />
        )}
      </button>

      {/* Content - collapsible */}
      {isExpanded && (
        <div className="px-2.5 pb-2.5 border-t border-[#1f2335]">
          <div className="mt-2 max-h-48 overflow-y-auto">
            <pre className="text-xs text-[#565f89] whitespace-pre-wrap break-words font-mono leading-relaxed">
              {content}
              {isThinking && (
                <span className="inline-block w-1.5 h-3 bg-[#bb9af7] animate-pulse ml-0.5 align-middle" />
              )}
            </pre>
          </div>
        </div>
      )}
    </div>
  );
}

interface StreamingThinkingBlockProps {
  sessionId: string;
}

/**
 * StreamingThinkingBlock - Displays live thinking content from the store.
 * Use this in the UnifiedTimeline for active streaming.
 */
export function StreamingThinkingBlock({ sessionId }: StreamingThinkingBlockProps) {
  const content = useThinkingContent(sessionId);
  const isExpanded = useIsThinkingExpanded(sessionId);
  const isThinking = useIsAgentThinking(sessionId);
  const setThinkingExpanded = useStore((state) => state.setThinkingExpanded);

  if (!content) {
    return null;
  }

  return (
    <ThinkingBlockUI
      content={content}
      isExpanded={isExpanded}
      isThinking={isThinking}
      onToggle={() => setThinkingExpanded(sessionId, !isExpanded)}
    />
  );
}

interface StaticThinkingBlockProps {
  content: string;
}

/**
 * StaticThinkingBlock - Displays finalized thinking content.
 * Use this in AgentMessage for persisted messages.
 */
export function StaticThinkingBlock({ content }: StaticThinkingBlockProps) {
  const [isExpanded, setIsExpanded] = useState(false);

  if (!content) {
    return null;
  }

  return (
    <ThinkingBlockUI
      content={content}
      isExpanded={isExpanded}
      isThinking={false}
      onToggle={() => setIsExpanded(!isExpanded)}
    />
  );
}

interface ThinkingBlockProps {
  /** Session ID for live streaming mode */
  sessionId?: string;
  /** Static thinking content for finalized messages */
  content?: string;
}

/**
 * ThinkingBlock - Facade that selects the appropriate implementation.
 *
 * @deprecated Prefer using StreamingThinkingBlock or StaticThinkingBlock directly.
 */
export function ThinkingBlock({ sessionId, content }: ThinkingBlockProps) {
  if (sessionId !== undefined) {
    return <StreamingThinkingBlock sessionId={sessionId} />;
  }
  if (content !== undefined) {
    return <StaticThinkingBlock content={content} />;
  }
  return null;
}

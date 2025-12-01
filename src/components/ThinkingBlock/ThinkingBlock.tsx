import { Brain, ChevronDown, ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";
import { useIsAgentThinking, useIsThinkingExpanded, useStore, useThinkingContent } from "@/store";

interface ThinkingBlockProps {
  sessionId: string;
}

export function ThinkingBlock({ sessionId }: ThinkingBlockProps) {
  const thinkingContent = useThinkingContent(sessionId);
  const isExpanded = useIsThinkingExpanded(sessionId);
  const isThinking = useIsAgentThinking(sessionId);
  const setThinkingExpanded = useStore((state) => state.setThinkingExpanded);

  // Don't render if no thinking content
  if (!thinkingContent) {
    return null;
  }

  const toggleExpanded = () => {
    setThinkingExpanded(sessionId, !isExpanded);
  };

  return (
    <div className="rounded-md bg-[#16161e] overflow-hidden">
      {/* Header - always visible */}
      <button
        type="button"
        onClick={toggleExpanded}
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
            ({thinkingContent.length.toLocaleString()} chars)
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
              {thinkingContent}
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

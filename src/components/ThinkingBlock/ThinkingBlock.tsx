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
    <div className="border border-[#3b4261] rounded-lg bg-[#1a1b26]/50 overflow-hidden">
      {/* Header - always visible */}
      <button
        type="button"
        onClick={toggleExpanded}
        className="w-full flex items-center gap-2 px-3 py-2 hover:bg-[#1f2335]/50 transition-colors text-left"
      >
        <div className="flex items-center gap-2 flex-1">
          <div
            className={cn(
              "w-6 h-6 rounded-full flex items-center justify-center",
              isThinking ? "bg-[#bb9af7]/30" : "bg-[#7dcfff]/20"
            )}
          >
            <Brain
              className={cn(
                "w-3.5 h-3.5",
                isThinking ? "text-[#bb9af7] animate-pulse" : "text-[#7dcfff]"
              )}
            />
          </div>
          <span className="text-xs font-medium text-[#a9b1d6]">
            {isThinking ? "Thinking..." : "Extended Thinking"}
          </span>
          <span className="text-xs text-[#565f89]">
            ({thinkingContent.length.toLocaleString()} chars)
          </span>
        </div>
        {isExpanded ? (
          <ChevronDown className="w-4 h-4 text-[#565f89]" />
        ) : (
          <ChevronRight className="w-4 h-4 text-[#565f89]" />
        )}
      </button>

      {/* Content - collapsible */}
      {isExpanded && (
        <div className="px-3 pb-3 border-t border-[#3b4261]">
          <div className="mt-2 max-h-64 overflow-y-auto">
            <pre className="text-xs text-[#787c99] whitespace-pre-wrap break-words font-mono leading-relaxed">
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

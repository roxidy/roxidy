import { Bot, Brain, ChevronDown, ChevronRight, User } from "lucide-react";
import { useState } from "react";
import { Markdown } from "@/components/Markdown";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import type { AgentMessage as AgentMessageType } from "@/store";
import { ToolCallCard } from "./ToolCallCard";

interface AgentMessageProps {
  message: AgentMessageType;
}

export function AgentMessage({ message }: AgentMessageProps) {
  const isUser = message.role === "user";
  const isSystem = message.role === "system";
  const [isThinkingExpanded, setIsThinkingExpanded] = useState(false);

  // Use streamingHistory if available (interleaved text + tool calls), otherwise fallback to legacy
  const hasStreamingHistory = message.streamingHistory && message.streamingHistory.length > 0;

  return (
    <div className={cn("flex gap-3 min-w-0", isUser && "flex-row-reverse")}>
      {/* Avatar */}
      <div
        className={cn(
          "w-8 h-8 rounded-full flex items-center justify-center flex-shrink-0",
          isUser ? "bg-[#7aa2f7]/20" : isSystem ? "bg-[#e0af68]/20" : "bg-[#bb9af7]/20"
        )}
      >
        {isUser ? (
          <User className="w-4 h-4 text-[#7aa2f7]" />
        ) : (
          <Bot className={cn("w-4 h-4", isSystem ? "text-[#e0af68]" : "text-[#bb9af7]")} />
        )}
      </div>

      {/* Content */}
      <Card
        className={cn(
          "flex-1 max-w-[85%] min-w-0 overflow-hidden",
          isUser
            ? "bg-[#7aa2f7]/10 border-[#7aa2f7]/20"
            : isSystem
              ? "bg-[#e0af68]/10 border-[#e0af68]/20"
              : "bg-[#1f2335] border-[#27293d]"
        )}
      >
        <CardContent className="p-3 space-y-3">
          {/* Role label for system messages */}
          {isSystem && (
            <Badge
              variant="outline"
              className="mb-2 bg-[#e0af68]/20 text-[#e0af68] border-[#e0af68]/30 text-xs"
            >
              System
            </Badge>
          )}

          {/* Thinking content (collapsible) */}
          {message.thinkingContent && (
            <div className="rounded-md bg-[#16161e] overflow-hidden">
              <button
                type="button"
                onClick={() => setIsThinkingExpanded(!isThinkingExpanded)}
                className="w-full flex items-center gap-2 px-2.5 py-1.5 hover:bg-[#1a1b26] transition-colors text-left"
              >
                <div className="flex items-center gap-2 flex-1">
                  <Brain className="w-3.5 h-3.5 text-[#7dcfff]" />
                  <span className="text-xs font-medium text-[#787c99]">Thinking</span>
                  <span className="text-xs text-[#565f89]">
                    ({message.thinkingContent.length.toLocaleString()} chars)
                  </span>
                </div>
                {isThinkingExpanded ? (
                  <ChevronDown className="w-3.5 h-3.5 text-[#565f89]" />
                ) : (
                  <ChevronRight className="w-3.5 h-3.5 text-[#565f89]" />
                )}
              </button>
              {isThinkingExpanded && (
                <div className="px-2.5 pb-2.5 border-t border-[#1f2335]">
                  <div className="mt-2 max-h-48 overflow-y-auto">
                    <pre className="text-xs text-[#565f89] whitespace-pre-wrap break-words font-mono leading-relaxed">
                      {message.thinkingContent}
                    </pre>
                  </div>
                </div>
              )}
            </div>
          )}

          {/* Render interleaved streaming history if available */}
          {hasStreamingHistory ? (
            <div className="space-y-2">
              {message.streamingHistory?.map((block, blockIndex) => {
                if (block.type === "text") {
                  // Use content hash + index for stable key since text blocks don't have IDs
                  const textKey = `text-${blockIndex}-${block.content.slice(0, 20)}`;
                  return (
                    <div key={textKey}>
                      <Markdown content={block.content} className="text-sm" />
                    </div>
                  );
                }
                return <ToolCallCard key={block.toolCall.id} tool={block.toolCall} />;
              })}
            </div>
          ) : (
            <>
              {/* Legacy: Message content */}
              {isUser ? (
                <p className="text-sm text-[#c0caf5] whitespace-pre-wrap break-words">
                  {message.content}
                </p>
              ) : (
                <Markdown content={message.content} className="text-sm" />
              )}

              {/* Legacy: Tool calls */}
              {message.toolCalls && message.toolCalls.length > 0 && (
                <div className="mt-3 space-y-2">
                  {message.toolCalls.map((tool) => (
                    <ToolCallCard key={tool.id} tool={tool} />
                  ))}
                </div>
              )}
            </>
          )}

          {/* Timestamp */}
          <div className="mt-2 text-[10px] text-[#565f89]">
            {new Date(message.timestamp).toLocaleTimeString([], {
              hour: "2-digit",
              minute: "2-digit",
            })}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

import { Bot, User } from "lucide-react";
import { memo, useMemo } from "react";
import { Markdown } from "@/components/Markdown";
import { StaticThinkingBlock } from "@/components/ThinkingBlock";
import { ToolGroup, ToolItem } from "@/components/ToolCallDisplay";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { groupConsecutiveTools } from "@/lib/toolGrouping";
import { cn } from "@/lib/utils";
import type { AgentMessage as AgentMessageType } from "@/store";

interface AgentMessageProps {
  message: AgentMessageType;
}

export const AgentMessage = memo(function AgentMessage({ message }: AgentMessageProps) {
  const isUser = message.role === "user";
  const isSystem = message.role === "system";

  // Use streamingHistory if available (interleaved text + tool calls), otherwise fallback to legacy
  const hasStreamingHistory = message.streamingHistory && message.streamingHistory.length > 0;

  // Group consecutive tool calls for cleaner display
  const groupedHistory = useMemo(
    () => (message.streamingHistory ? groupConsecutiveTools(message.streamingHistory) : []),
    [message.streamingHistory]
  );

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
          {message.thinkingContent && <StaticThinkingBlock content={message.thinkingContent} />}

          {/* Render interleaved streaming history if available (grouped for cleaner display) */}
          {hasStreamingHistory ? (
            <div className="space-y-2">
              {groupedHistory.map((block, blockIndex) => {
                if (block.type === "text") {
                  return (
                    // biome-ignore lint/suspicious/noArrayIndexKey: blocks are in fixed order and never reordered
                    <div key={`text-${blockIndex}`}>
                      <Markdown content={block.content} className="text-sm" />
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
                    <ToolItem key={tool.id} tool={tool} />
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
});

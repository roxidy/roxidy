import { User, Bot } from "lucide-react";
import { cn } from "@/lib/utils";
import type { AgentMessage as AgentMessageType } from "@/store";
import { ToolCallCard } from "./ToolCallCard";

interface AgentMessageProps {
  message: AgentMessageType;
}

export function AgentMessage({ message }: AgentMessageProps) {
  const isUser = message.role === "user";
  const isSystem = message.role === "system";

  return (
    <div className={cn("flex gap-3", isUser && "flex-row-reverse")}>
      {/* Avatar */}
      <div
        className={cn(
          "w-8 h-8 rounded-full flex items-center justify-center flex-shrink-0",
          isUser
            ? "bg-[#7aa2f7]/20"
            : isSystem
              ? "bg-[#e0af68]/20"
              : "bg-[#bb9af7]/20"
        )}
      >
        {isUser ? (
          <User className="w-4 h-4 text-[#7aa2f7]" />
        ) : (
          <Bot
            className={cn(
              "w-4 h-4",
              isSystem ? "text-[#e0af68]" : "text-[#bb9af7]"
            )}
          />
        )}
      </div>

      {/* Content */}
      <div
        className={cn(
          "flex-1 max-w-[85%] rounded-lg p-3",
          isUser
            ? "bg-[#7aa2f7]/10 border border-[#7aa2f7]/20"
            : isSystem
              ? "bg-[#e0af68]/10 border border-[#e0af68]/20"
              : "bg-[#1f2335] border border-[#27293d]"
        )}
      >
        {/* Role label for system messages */}
        {isSystem && (
          <div className="text-xs text-[#e0af68] font-medium mb-1">System</div>
        )}

        {/* Message content */}
        <p className="text-sm text-[#c0caf5] whitespace-pre-wrap break-words">
          {message.content}
        </p>

        {/* Tool calls */}
        {message.toolCalls && message.toolCalls.length > 0 && (
          <div className="mt-3 space-y-2">
            {message.toolCalls.map((tool) => (
              <ToolCallCard key={tool.id} tool={tool} />
            ))}
          </div>
        )}

        {/* Timestamp */}
        <div className="mt-2 text-[10px] text-[#565f89]">
          {new Date(message.timestamp).toLocaleTimeString([], {
            hour: "2-digit",
            minute: "2-digit",
          })}
        </div>
      </div>
    </div>
  );
}

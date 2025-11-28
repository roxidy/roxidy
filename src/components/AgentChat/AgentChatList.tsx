import { Bot, Sparkles } from "lucide-react";
import { useEffect, useRef } from "react";
import { useAgentMessages, useAgentStreaming } from "@/store";
import { AgentMessage } from "./AgentMessage";

interface AgentChatListProps {
  sessionId: string;
}

export function AgentChatList({ sessionId }: AgentChatListProps) {
  const messages = useAgentMessages(sessionId);
  const streaming = useAgentStreaming(sessionId);
  const containerRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, []);

  if (messages.length === 0 && !streaming) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-[#565f89] p-8">
        <div className="w-16 h-16 rounded-full bg-[#bb9af7]/10 flex items-center justify-center mb-4">
          <Sparkles className="w-8 h-8 text-[#bb9af7]" />
        </div>
        <h3 className="text-lg font-medium text-[#c0caf5] mb-2">AI Assistant</h3>
        <p className="text-sm text-center max-w-md">
          Ask questions about your code, request changes, or get help with terminal commands. The AI
          can read files, search code, and execute commands in your workspace.
        </p>
        <div className="mt-6 flex flex-wrap gap-2 justify-center">
          {[
            "Explain this codebase",
            "Find all TODO comments",
            "Help me debug an error",
            "Write a unit test",
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
      {messages.map((message) => (
        <AgentMessage key={message.id} message={message} />
      ))}

      {/* Streaming indicator */}
      {streaming && (
        <div className="flex gap-3">
          <div className="w-8 h-8 rounded-full bg-[#bb9af7]/20 flex items-center justify-center flex-shrink-0">
            <Bot className="w-4 h-4 text-[#bb9af7]" />
          </div>
          <div className="flex-1 max-w-[85%] bg-[#1f2335] border border-[#27293d] rounded-lg p-3">
            <p className="text-sm text-[#c0caf5] whitespace-pre-wrap">
              {streaming}
              <span className="inline-block w-2 h-4 bg-[#bb9af7] animate-pulse ml-0.5 align-middle" />
            </p>
          </div>
        </div>
      )}

      {/* Scroll anchor */}
      <div ref={bottomRef} />
    </div>
  );
}

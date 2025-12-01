import {
  Bot,
  CheckCircle,
  ChevronDown,
  ChevronRight,
  Loader2,
  Terminal,
  Wrench,
  XCircle,
} from "lucide-react";
import { useState } from "react";
import { TruncatedOutput } from "@/components/TruncatedOutput";
import { formatToolName, formatToolResult, isAgentTerminalCommand } from "@/lib/tools";
import { cn } from "@/lib/utils";
import type { ActiveToolCall } from "@/store";

interface ToolCallDisplayProps {
  toolCalls: ActiveToolCall[];
}

/** Single tool call item with collapsible details */
function ToolCallItem({ tool }: { tool: ActiveToolCall }) {
  const [expanded, setExpanded] = useState(false);
  const isTerminalCmd = isAgentTerminalCommand(tool);

  const statusIcon = {
    running: <Loader2 className="w-3.5 h-3.5 text-[#7aa2f7] animate-spin" />,
    completed: <CheckCircle className="w-3.5 h-3.5 text-[#9ece6a]" />,
    error: <XCircle className="w-3.5 h-3.5 text-[#f7768e]" />,
  };

  const statusColor = {
    running: "border-l-[#7aa2f7]",
    completed: "border-l-[#9ece6a]",
    error: "border-l-[#f7768e]",
  };

  return (
    <div
      className={cn(
        "border-l-2 bg-[#1a1b26] rounded-r-md overflow-hidden",
        // Use purple border for agent terminal commands
        isTerminalCmd ? "border-l-[#bb9af7]" : statusColor[tool.status]
      )}
    >
      <button
        type="button"
        onClick={() => !isTerminalCmd && setExpanded(!expanded)}
        className={cn(
          "w-full flex items-center gap-2 px-2 py-1.5 transition-colors text-left",
          isTerminalCmd ? "cursor-default" : "hover:bg-[#1f2335]"
        )}
      >
        {!isTerminalCmd &&
          (expanded ? (
            <ChevronDown className="w-3 h-3 text-[#565f89]" />
          ) : (
            <ChevronRight className="w-3 h-3 text-[#565f89]" />
          ))}
        {isTerminalCmd ? (
          <Terminal className="w-3 h-3 text-[#bb9af7]" />
        ) : (
          <Wrench className="w-3 h-3 text-[#bb9af7]" />
        )}
        <span className="text-xs font-medium text-[#c0caf5] flex-1 truncate">
          {formatToolName(tool.name)}
        </span>
        {isTerminalCmd && <Bot className="w-3 h-3 text-[#bb9af7]" />}
        {statusIcon[tool.status]}
      </button>

      {/* For agent terminal commands, show truncated output */}
      {isTerminalCmd ? (
        <div className="px-3 pb-2">
          {tool.result !== undefined && tool.status !== "running" ? (
            <TruncatedOutput content={formatToolResult(tool.result)} maxLines={10} />
          ) : (
            <span className="text-[10px] text-[#565f89] italic">
              {tool.status === "running" ? "Running..." : "Awaiting output"}
            </span>
          )}
        </div>
      ) : (
        expanded && (
          <div className="px-3 pb-2 space-y-2">
            {/* Arguments */}
            {Object.keys(tool.args).length > 0 && (
              <div>
                <span className="text-[10px] uppercase text-[#565f89] font-medium">Arguments</span>
                <pre className="mt-0.5 text-[11px] text-[#9aa5ce] bg-[#13131a] rounded p-2 overflow-auto max-h-32 whitespace-pre-wrap break-all">
                  {JSON.stringify(tool.args, null, 2)}
                </pre>
              </div>
            )}

            {/* Result (only if completed or error) */}
            {tool.result !== undefined && tool.status !== "running" && (
              <div>
                <span className="text-[10px] uppercase text-[#565f89] font-medium">
                  {tool.status === "error" ? "Error" : "Result"}
                </span>
                <pre
                  className={cn(
                    "mt-0.5 text-[11px] bg-[#13131a] rounded p-2 overflow-auto max-h-40 whitespace-pre-wrap break-all",
                    tool.status === "error" ? "text-[#f7768e]" : "text-[#9aa5ce]"
                  )}
                >
                  {formatToolResult(tool.result)}
                </pre>
              </div>
            )}
          </div>
        )
      )}
    </div>
  );
}

/** Display a list of tool calls with their status */
export function ToolCallDisplay({ toolCalls }: ToolCallDisplayProps) {
  if (toolCalls.length === 0) return null;

  return (
    <div className="space-y-1.5 my-2">
      {toolCalls.map((tool) => (
        <ToolCallItem key={tool.id} tool={tool} />
      ))}
    </div>
  );
}

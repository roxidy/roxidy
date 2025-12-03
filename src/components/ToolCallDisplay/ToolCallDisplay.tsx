import {
  Bot,
  CheckCircle,
  ChevronRight,
  Edit,
  FileCode,
  FileText,
  FolderOpen,
  Globe,
  Loader2,
  Search,
  Terminal,
  XCircle,
} from "lucide-react";
import { memo, useState } from "react";
import { TruncatedOutput } from "@/components/TruncatedOutput";
import { Badge } from "@/components/ui/badge";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { formatPrimaryArg } from "@/lib/toolGrouping";
import { formatToolResult, isAgentTerminalCommand } from "@/lib/tools";
import { cn } from "@/lib/utils";
import type { ActiveToolCall, ToolCall } from "@/store";

/** Union type for both ToolCall and ActiveToolCall */
type AnyToolCall = ToolCall | ActiveToolCall;

/** Props for a single tool item */
interface ToolItemProps {
  tool: AnyToolCall;
  /** Compact mode uses less visual weight, suitable for inline display */
  compact?: boolean;
  /** Show primary argument inline with tool name (e.g., "read_file: utils.ts") */
  showInlineName?: boolean;
}

/** Tool name to icon mapping */
const toolIcons: Record<string, typeof FileText> = {
  read_file: FileText,
  write_file: Edit,
  edit_file: Edit,
  list_files: FolderOpen,
  grep_file: Search,
  run_pty_cmd: Terminal,
  shell: Terminal,
  web_fetch: Globe,
  web_search: Globe,
  web_search_answer: Globe,
  apply_patch: FileCode,
};

/** Status configuration for badges and icons */
const statusConfig: Record<
  AnyToolCall["status"],
  {
    icon: typeof CheckCircle;
    borderColor: string;
    badgeClass: string;
    label: string;
    animate?: boolean;
  }
> = {
  pending: {
    icon: Loader2,
    borderColor: "border-l-[#e0af68]",
    badgeClass: "bg-[#e0af68]/20 text-[#e0af68] hover:bg-[#e0af68]/30",
    label: "Pending",
  },
  approved: {
    icon: CheckCircle,
    borderColor: "border-l-[#9ece6a]",
    badgeClass: "bg-[#9ece6a]/20 text-[#9ece6a] hover:bg-[#9ece6a]/30",
    label: "Approved",
  },
  denied: {
    icon: XCircle,
    borderColor: "border-l-[#f7768e]",
    badgeClass: "bg-[#f7768e]/20 text-[#f7768e] hover:bg-[#f7768e]/30",
    label: "Denied",
  },
  running: {
    icon: Loader2,
    borderColor: "border-l-[#7aa2f7]",
    badgeClass: "bg-[#7aa2f7]/20 text-[#7aa2f7] border-[#7aa2f7]/30",
    label: "Running",
    animate: true,
  },
  completed: {
    icon: CheckCircle,
    borderColor: "border-l-[#9ece6a]",
    badgeClass: "bg-[#9ece6a]/20 text-[#9ece6a] hover:bg-[#9ece6a]/30",
    label: "Completed",
  },
  error: {
    icon: XCircle,
    borderColor: "border-l-[#f7768e]",
    badgeClass: "bg-[#f7768e]/20 text-[#f7768e] hover:bg-[#f7768e]/30",
    label: "Error",
  },
};

/** Single tool call item with collapsible details */
export const ToolItem = memo(function ToolItem({
  tool,
  compact = false,
  showInlineName = false,
}: ToolItemProps) {
  const [isOpen, setIsOpen] = useState(false);
  const Icon = toolIcons[tool.name] || Terminal;
  const status = statusConfig[tool.status];
  const StatusIcon = status.icon;
  const isTerminalCmd = isAgentTerminalCommand(tool);
  const hasArgs = Object.keys(tool.args).length > 0;
  const primaryArg = showInlineName ? formatPrimaryArg(tool) : null;

  // For terminal commands, always show output (non-collapsible header behavior)
  // For other tools, make the header clickable to expand
  const canExpand = !isTerminalCmd;

  return (
    <Collapsible open={isOpen} onOpenChange={canExpand ? setIsOpen : undefined}>
      <div
        className={cn(
          "border-l-2 overflow-hidden rounded-r-md",
          compact ? "bg-[#1a1b26]" : "bg-[#1f2335]/50",
          isTerminalCmd ? "border-l-[#bb9af7]" : status.borderColor
        )}
      >
        <CollapsibleTrigger asChild disabled={!canExpand}>
          <div
            className={cn(
              "flex items-center justify-between p-2 transition-colors",
              canExpand && "cursor-pointer hover:bg-[#1f2335]/80"
            )}
          >
            <div className="flex items-center gap-2">
              {canExpand && (
                <ChevronRight
                  className={cn(
                    "w-3 h-3 text-[#565f89] transition-transform",
                    isOpen && "rotate-90"
                  )}
                />
              )}
              <Icon
                className={cn(
                  compact ? "w-3 h-3" : "w-4 h-4",
                  isTerminalCmd ? "text-[#bb9af7]" : "text-[#7aa2f7]"
                )}
              />
              <span className={cn("font-mono text-[#c0caf5]", compact ? "text-xs" : "text-sm")}>
                {tool.name}
                {primaryArg && (
                  <span className="text-[#565f89]">
                    : <span className="text-[#9aa5ce]">{primaryArg}</span>
                  </span>
                )}
              </span>
              {isTerminalCmd && (
                <Bot className={cn("text-[#bb9af7]", compact ? "w-3 h-3" : "w-3.5 h-3.5")} />
              )}
            </div>
            <Badge variant="outline" className={cn("gap-1 flex items-center", status.badgeClass)}>
              <StatusIcon className={cn("w-3 h-3", status.animate && "animate-spin")} />
              {!compact && status.label}
            </Badge>
          </div>
        </CollapsibleTrigger>

        {/* For terminal commands, show output directly (not collapsible) */}
        {isTerminalCmd && (
          <div className="px-3 pb-2">
            {tool.result !== undefined && tool.status !== "running" ? (
              <TruncatedOutput content={formatToolResult(tool.result)} maxLines={10} />
            ) : (
              <span className="text-[10px] text-[#565f89] italic">
                {tool.status === "running" ? "Running..." : "Awaiting output"}
              </span>
            )}
          </div>
        )}

        {/* For non-terminal tools, show collapsible args/result */}
        <CollapsibleContent>
          <div className="px-3 pb-2 space-y-2">
            {/* Arguments */}
            {hasArgs && (
              <div>
                <span className="text-[10px] uppercase text-[#565f89] font-medium">Arguments</span>
                <pre className="mt-0.5 text-[11px] text-[#9aa5ce] bg-[#13131a] rounded p-2 overflow-auto max-h-32 whitespace-pre-wrap break-all">
                  {JSON.stringify(tool.args, null, 2)}
                </pre>
              </div>
            )}

            {/* Result */}
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
        </CollapsibleContent>
      </div>
    </Collapsible>
  );
});

/** Props for the tool call list display */
interface ToolCallDisplayProps {
  toolCalls: AnyToolCall[];
  /** Compact mode uses less visual weight */
  compact?: boolean;
}

/** Display a list of tool calls with their status */
export function ToolCallDisplay({ toolCalls, compact = false }: ToolCallDisplayProps) {
  if (toolCalls.length === 0) return null;

  return (
    <div className="space-y-1.5 my-2">
      {toolCalls.map((tool) => (
        <ToolItem key={tool.id} tool={tool} compact={compact} />
      ))}
    </div>
  );
}

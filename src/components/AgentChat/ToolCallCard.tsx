import {
  AlertCircle,
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
import { useState } from "react";
import { TruncatedOutput } from "@/components/TruncatedOutput";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { formatToolResult, isAgentTerminalCommand } from "@/lib/tools";
import { cn } from "@/lib/utils";
import type { ToolCall } from "@/store";

interface ToolCallCardProps {
  tool: ToolCall;
}

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

const statusConfig: Record<
  ToolCall["status"],
  {
    icon: typeof AlertCircle;
    variant: "default" | "secondary" | "destructive" | "outline";
    label: string;
    animate?: boolean;
  }
> = {
  pending: {
    icon: AlertCircle,
    variant: "secondary",
    label: "Pending",
  },
  approved: {
    icon: CheckCircle,
    variant: "default",
    label: "Approved",
  },
  denied: {
    icon: XCircle,
    variant: "destructive",
    label: "Denied",
  },
  running: {
    icon: Loader2,
    variant: "outline",
    label: "Running",
    animate: true,
  },
  completed: {
    icon: CheckCircle,
    variant: "default",
    label: "Completed",
  },
  error: {
    icon: XCircle,
    variant: "destructive",
    label: "Error",
  },
};

export function ToolCallCard({ tool }: ToolCallCardProps) {
  const [isOpen, setIsOpen] = useState(false);
  const Icon = toolIcons[tool.name] || Terminal;
  const status = statusConfig[tool.status];
  const StatusIcon = status.icon;
  const isTerminalCmd = isAgentTerminalCommand(tool);

  return (
    <Collapsible open={isOpen} onOpenChange={setIsOpen}>
      <Card
        className={cn(
          "bg-[#1f2335]/50",
          // Purple border for agent-executed terminal commands to differentiate
          isTerminalCmd ? "border-[#bb9af7]/40" : "border-[#27293d]"
        )}
      >
        <CollapsibleTrigger asChild>
          <div className="flex items-center justify-between p-3 cursor-pointer hover:bg-[#1f2335]/80 transition-colors">
            <div className="flex items-center gap-2">
              <ChevronRight
                className={cn(
                  "w-3.5 h-3.5 text-[#565f89] transition-transform",
                  isOpen && "rotate-90"
                )}
              />
              <Icon
                className={cn("w-4 h-4", isTerminalCmd ? "text-[#bb9af7]" : "text-[#7aa2f7]")}
              />
              <span className="text-sm font-mono text-[#c0caf5]">{tool.name}</span>
              {/* Show bot indicator for agent-executed commands */}
              {isTerminalCmd && <Bot className="w-3 h-3 text-[#bb9af7]" />}
            </div>
            <Badge
              variant={status.variant}
              className={cn(
                "gap-1 flex items-center",
                status.variant === "default" &&
                  "bg-[#9ece6a]/20 text-[#9ece6a] hover:bg-[#9ece6a]/30",
                status.variant === "secondary" &&
                  "bg-[#e0af68]/20 text-[#e0af68] hover:bg-[#e0af68]/30",
                status.variant === "destructive" &&
                  "bg-[#f7768e]/20 text-[#f7768e] hover:bg-[#f7768e]/30",
                status.variant === "outline" && "bg-[#7aa2f7]/20 text-[#7aa2f7] border-[#7aa2f7]/30"
              )}
            >
              <StatusIcon className={cn("w-3 h-3", status.animate && "animate-spin")} />
              {status.label}
            </Badge>
          </div>
        </CollapsibleTrigger>

        <CollapsibleContent>
          <CardContent className="p-3 pt-0 space-y-2">
            {/* For agent terminal commands, show truncated output */}
            {isTerminalCmd ? (
              tool.result !== undefined ? (
                <TruncatedOutput content={formatToolResult(tool.result)} maxLines={10} />
              ) : (
                <p className="text-xs text-[#565f89] italic">No output captured</p>
              )
            ) : (
              <>
                {/* Arguments */}
                {Object.keys(tool.args).length > 0 && (
                  <div>
                    <span className="text-xs text-[#565f89]">Arguments</span>
                    <pre className="mt-1 text-xs text-[#a9b1d6] bg-[#1a1b26] p-2 rounded overflow-x-auto max-h-32 scrollbar-thin whitespace-pre-wrap break-all">
                      {JSON.stringify(tool.args, null, 2)}
                    </pre>
                  </div>
                )}

                {/* Result */}
                {tool.result !== undefined && (
                  <div>
                    <span className="text-xs text-[#565f89]">Result</span>
                    <pre className="mt-1 text-xs text-[#a9b1d6] bg-[#1a1b26] p-2 rounded overflow-x-auto max-h-40 scrollbar-thin whitespace-pre-wrap break-all">
                      {formatToolResult(tool.result)}
                    </pre>
                  </div>
                )}
              </>
            )}
          </CardContent>
        </CollapsibleContent>
      </Card>
    </Collapsible>
  );
}

import {
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
import { Badge } from "@/components/ui/badge";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import {
  type AnyToolCall,
  formatPrimaryArg,
  getGroupStatus,
  type ToolGroup as ToolGroupType,
} from "@/lib/toolGrouping";
import { cn } from "@/lib/utils";

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

interface ToolGroupProps {
  group: ToolGroupType;
  compact?: boolean;
}

/** Displays a group of consecutive tool calls of the same type */
export const ToolGroup = memo(function ToolGroup({ group, compact = false }: ToolGroupProps) {
  const groupStatus = getGroupStatus(group.tools);

  // Auto-expand if any tool is running or errored
  const shouldAutoExpand = groupStatus === "running" || groupStatus === "error";
  const [isOpen, setIsOpen] = useState(shouldAutoExpand);

  const Icon = toolIcons[group.toolName] || Terminal;
  const status = statusConfig[groupStatus];
  const StatusIcon = status.icon;

  // Build preview text from primary arguments
  const previewItems = group.tools
    .map((tool) => formatPrimaryArg(tool))
    .filter((arg): arg is string => arg !== null);

  const maxPreviewItems = 3;
  const visiblePreview = previewItems.slice(0, maxPreviewItems);
  const hiddenCount = previewItems.length - visiblePreview.length;

  return (
    <Collapsible open={isOpen} onOpenChange={setIsOpen}>
      <div
        className={cn(
          "border-l-2 overflow-hidden rounded-r-md",
          compact ? "bg-[#1a1b26]" : "bg-[#1f2335]/50",
          status.borderColor
        )}
      >
        <CollapsibleTrigger asChild>
          <div className="cursor-pointer hover:bg-[#1f2335]/80 transition-colors">
            {/* Header row */}
            <div className="flex items-center justify-between p-2">
              <div className="flex items-center gap-2">
                <ChevronRight
                  className={cn(
                    "w-3 h-3 text-[#565f89] transition-transform",
                    isOpen && "rotate-90"
                  )}
                />
                <Icon className={cn(compact ? "w-3 h-3" : "w-4 h-4", "text-[#7aa2f7]")} />
                <span className={cn("font-mono text-[#c0caf5]", compact ? "text-xs" : "text-sm")}>
                  {group.toolName}
                </span>
                <Badge
                  variant="outline"
                  className="bg-[#7aa2f7]/10 text-[#7aa2f7] border-[#7aa2f7]/30 text-[10px] px-1.5 py-0"
                >
                  ×{group.tools.length}
                </Badge>
              </div>
              <Badge variant="outline" className={cn("gap-1 flex items-center", status.badgeClass)}>
                <StatusIcon className={cn("w-3 h-3", status.animate && "animate-spin")} />
                {!compact && status.label}
              </Badge>
            </div>

            {/* Preview line (only when collapsed) */}
            {!isOpen && visiblePreview.length > 0 && (
              <div className="px-2 pb-2 -mt-1">
                <span className="text-[11px] text-[#565f89] font-mono">
                  {visiblePreview.join(", ")}
                  {hiddenCount > 0 && (
                    <span className="text-[#7aa2f7]">{` +${hiddenCount} more`}</span>
                  )}
                </span>
              </div>
            )}
          </div>
        </CollapsibleTrigger>

        {/* Expanded content - list of individual tools */}
        <CollapsibleContent>
          <div className="px-2 pb-2 space-y-0.5">
            {group.tools.map((tool) => (
              <ToolGroupItem key={tool.id} tool={tool} compact={compact} />
            ))}
          </div>
        </CollapsibleContent>
      </div>
    </Collapsible>
  );
});

/** Individual item within a tool group (simplified display) */
const ToolGroupItem = memo(function ToolGroupItem({
  tool,
  compact,
}: {
  tool: AnyToolCall;
  compact?: boolean;
}) {
  const Icon = toolIcons[tool.name] || Terminal;
  const status = statusConfig[tool.status];
  const StatusIcon = status.icon;
  const primaryArg = formatPrimaryArg(tool);

  return (
    <div
      className={cn(
        "flex items-center justify-between py-1 px-2 rounded",
        "bg-[#1a1b26]/50 hover:bg-[#1a1b26]"
      )}
    >
      <div className="flex items-center gap-2 min-w-0">
        <Icon className={cn(compact ? "w-3 h-3" : "w-3.5 h-3.5", "text-[#7aa2f7] shrink-0")} />
        {primaryArg ? (
          <span
            className={cn(
              "font-mono text-[#9aa5ce] truncate",
              compact ? "text-[10px]" : "text-[11px]"
            )}
          >
            {primaryArg}
          </span>
        ) : (
          <span
            className={cn(
              "font-mono text-[#565f89] italic truncate",
              compact ? "text-[10px]" : "text-[11px]"
            )}
          >
            {tool.name}
          </span>
        )}
      </div>
      <StatusIcon
        className={cn(
          "w-3 h-3 shrink-0",
          status.animate && "animate-spin",
          tool.status === "completed" && "text-[#9ece6a]",
          tool.status === "running" && "text-[#7aa2f7]",
          tool.status === "error" && "text-[#f7768e]",
          tool.status === "pending" && "text-[#e0af68]"
        )}
      />
    </div>
  );
});

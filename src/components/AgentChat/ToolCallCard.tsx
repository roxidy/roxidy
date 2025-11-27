import {
  FileText,
  Terminal,
  Search,
  Globe,
  CheckCircle,
  XCircle,
  Loader2,
  AlertCircle,
  FolderOpen,
  Edit,
  FileCode,
} from "lucide-react";
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
  apply_patch: FileCode,
};

const statusConfig: Record<
  ToolCall["status"],
  {
    icon: typeof AlertCircle;
    color: string;
    bg: string;
    label: string;
    animate?: boolean;
  }
> = {
  pending: {
    icon: AlertCircle,
    color: "text-[#e0af68]",
    bg: "bg-[#e0af68]/10",
    label: "Pending approval",
  },
  approved: {
    icon: CheckCircle,
    color: "text-[#9ece6a]",
    bg: "bg-[#9ece6a]/10",
    label: "Approved",
  },
  denied: {
    icon: XCircle,
    color: "text-[#f7768e]",
    bg: "bg-[#f7768e]/10",
    label: "Denied",
  },
  running: {
    icon: Loader2,
    color: "text-[#7aa2f7]",
    bg: "bg-[#7aa2f7]/10",
    label: "Running",
    animate: true,
  },
  completed: {
    icon: CheckCircle,
    color: "text-[#9ece6a]",
    bg: "bg-[#9ece6a]/10",
    label: "Completed",
  },
  error: {
    icon: XCircle,
    color: "text-[#f7768e]",
    bg: "bg-[#f7768e]/10",
    label: "Error",
  },
};

export function ToolCallCard({ tool }: ToolCallCardProps) {
  const Icon = toolIcons[tool.name] || Terminal;
  const status = statusConfig[tool.status];
  const StatusIcon = status.icon;

  return (
    <div className={cn("rounded-md p-3 border border-[#27293d]", status.bg)}>
      {/* Header */}
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          <Icon className="w-4 h-4 text-[#7aa2f7]" />
          <span className="text-sm font-mono text-[#c0caf5]">{tool.name}</span>
        </div>
        <div className={cn("flex items-center gap-1", status.color)}>
          <StatusIcon
            className={cn("w-3 h-3", status.animate && "animate-spin")}
          />
          <span className="text-xs">{status.label}</span>
        </div>
      </div>

      {/* Arguments (collapsed by default) */}
      {Object.keys(tool.args).length > 0 && (
        <details className="mt-2 group">
          <summary className="text-xs text-[#565f89] cursor-pointer hover:text-[#7aa2f7] select-none">
            <span className="ml-1">Arguments</span>
          </summary>
          <pre className="mt-1 text-xs text-[#a9b1d6] bg-[#1a1b26] p-2 rounded overflow-x-auto max-h-32 scrollbar-thin">
            {JSON.stringify(tool.args, null, 2)}
          </pre>
        </details>
      )}

      {/* Result */}
      {tool.result !== undefined && (
        <details className="mt-2" open={tool.status === "completed"}>
          <summary className="text-xs text-[#565f89] cursor-pointer hover:text-[#7aa2f7] select-none">
            <span className="ml-1">Result</span>
          </summary>
          <pre className="mt-1 text-xs text-[#a9b1d6] bg-[#1a1b26] p-2 rounded overflow-x-auto max-h-40 scrollbar-thin">
            {typeof tool.result === "string"
              ? tool.result
              : JSON.stringify(tool.result, null, 2)}
          </pre>
        </details>
      )}
    </div>
  );
}

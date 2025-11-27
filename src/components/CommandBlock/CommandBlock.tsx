import { useMemo } from "react";
import { ChevronDown, ChevronRight, Check, X, Clock } from "lucide-react";
import Ansi from "ansi-to-react";
import { cn } from "@/lib/utils";
import { stripOscSequences } from "@/lib/ansi";
import type { CommandBlock as CommandBlockType } from "@/store";

interface CommandBlockProps {
  block: CommandBlockType;
  onToggleCollapse: (blockId: string) => void;
}

function formatDuration(ms: number | null): string {
  if (ms === null) return "";
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  const minutes = Math.floor(ms / 60000);
  const seconds = ((ms % 60000) / 1000).toFixed(0);
  return `${minutes}m ${seconds}s`;
}

export function CommandBlock({ block, onToggleCollapse }: CommandBlockProps) {
  const isSuccess = block.exitCode === 0;

  // Strip OSC sequences but keep ANSI color codes for rendering
  const cleanOutput = useMemo(() => stripOscSequences(block.output), [block.output]);
  const hasOutput = cleanOutput.trim().length > 0;

  return (
    <div
      className={cn(
        "border-l-2 mb-2 transition-colors hover:bg-[#1f2335]",
        isSuccess ? "border-l-[#9ece6a]" : "border-l-[#f7768e]"
      )}
    >
      {/* Header */}
      <div
        className="flex items-center gap-2 px-3 py-2 cursor-pointer select-none"
        onClick={() => hasOutput && onToggleCollapse(block.id)}
      >
        {/* Collapse indicator */}
        {hasOutput ? (
          block.isCollapsed ? (
            <ChevronRight className="w-4 h-4 text-[#565f89] flex-shrink-0" />
          ) : (
            <ChevronDown className="w-4 h-4 text-[#565f89] flex-shrink-0" />
          )
        ) : (
          <div className="w-4 h-4 flex-shrink-0" />
        )}

        {/* Exit code indicator */}
        {block.exitCode !== null && (
          isSuccess ? (
            <Check className="w-4 h-4 text-[#9ece6a] flex-shrink-0" />
          ) : (
            <X className="w-4 h-4 text-[#f7768e] flex-shrink-0" />
          )
        )}

        {/* Command */}
        <code className="text-[#c0caf5] font-mono text-sm flex-1 truncate">
          {block.command || "(empty command)"}
        </code>

        {/* Metadata */}
        <div className="flex items-center gap-3 text-xs text-[#565f89] flex-shrink-0">
          {block.durationMs !== null && (
            <span className="flex items-center gap-1">
              <Clock className="w-3 h-3" />
              {formatDuration(block.durationMs)}
            </span>
          )}
          {block.exitCode !== null && block.exitCode !== 0 && (
            <span className="text-[#f7768e]">
              exit {block.exitCode}
            </span>
          )}
        </div>
      </div>

      {/* Output */}
      {hasOutput && !block.isCollapsed && (
        <div className="px-3 pb-3 pl-9">
          <div
            className="ansi-output font-mono text-[13px] leading-5 whitespace-pre-wrap break-words bg-[#13131a] rounded-md p-3 border border-[#1f2335]"
            style={{ fontFamily: 'JetBrains Mono, Menlo, Monaco, Consolas, monospace' }}
          >
            <Ansi useClasses>{cleanOutput}</Ansi>
          </div>
        </div>
      )}
    </div>
  );
}

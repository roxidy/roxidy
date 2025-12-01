import Ansi from "ansi-to-react";
import { Check, Clock, X } from "lucide-react";
import { useMemo } from "react";
import { Badge } from "@/components/ui/badge";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { stripOscSequences } from "@/lib/ansi";
import { cn } from "@/lib/utils";
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
    <Collapsible
      open={hasOutput && !block.isCollapsed}
      onOpenChange={() => hasOutput && onToggleCollapse(block.id)}
      className={cn(
        "border-l-2 mb-2 transition-colors hover:bg-[#1f2335]",
        isSuccess ? "border-l-[#9ece6a]" : "border-l-[#f7768e]"
      )}
    >
      {/* Header */}
      <CollapsibleTrigger
        className="flex items-center gap-2 px-3 py-2 w-full text-left select-none"
        disabled={!hasOutput}
      >
        {/* Exit code badge */}
        {block.exitCode !== null && (
          <Badge
            variant={isSuccess ? "default" : "destructive"}
            className={cn(
              "h-5 px-1.5 gap-1",
              isSuccess
                ? "bg-[#9ece6a]/20 text-[#9ece6a] hover:bg-[#9ece6a]/30"
                : "bg-[#f7768e]/20 text-[#f7768e] hover:bg-[#f7768e]/30"
            )}
          >
            {isSuccess ? <Check className="w-3 h-3" /> : <X className="w-3 h-3" />}
            {!isSuccess && block.exitCode}
          </Badge>
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
          {hasOutput && (
            <span className="text-[10px] uppercase tracking-wide">
              {block.isCollapsed ? "Show" : "Hide"}
            </span>
          )}
        </div>
      </CollapsibleTrigger>

      {/* Output */}
      <CollapsibleContent>
        <div className="px-3 pb-3 pl-9">
          <div className="ansi-output text-[13px] leading-5 whitespace-pre-wrap break-words bg-[#13131a] rounded-md p-3 border border-[#1f2335]">
            <Ansi useClasses>{cleanOutput}</Ansi>
          </div>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

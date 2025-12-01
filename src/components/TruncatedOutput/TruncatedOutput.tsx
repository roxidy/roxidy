import Ansi from "ansi-to-react";
import { ChevronDown, ChevronUp } from "lucide-react";
import { useMemo, useState } from "react";
import { stripOscSequences } from "@/lib/ansi";
import { truncateByLines } from "@/lib/text";
import { cn } from "@/lib/utils";

interface TruncatedOutputProps {
  content: string;
  maxLines?: number;
  className?: string;
}

export function TruncatedOutput({ content, maxLines = 10, className }: TruncatedOutputProps) {
  const [isExpanded, setIsExpanded] = useState(false);

  // Clean and truncate output
  const { cleanContent, truncation } = useMemo(() => {
    const cleaned = stripOscSequences(content);
    return {
      cleanContent: cleaned,
      truncation: truncateByLines(cleaned, maxLines),
    };
  }, [content, maxLines]);

  const displayContent = isExpanded ? cleanContent : truncation.truncatedContent;

  if (!cleanContent.trim()) {
    return <span className="text-[10px] text-[#565f89] italic">No output</span>;
  }

  return (
    <div className={cn("space-y-1", className)}>
      <pre
        className={cn(
          "ansi-output text-[11px] text-[#9aa5ce] bg-[#13131a] rounded p-2",
          "whitespace-pre-wrap break-all",
          "overflow-x-auto"
        )}
      >
        <Ansi useClasses>{displayContent}</Ansi>
      </pre>

      {truncation.isTruncated && (
        <button
          type="button"
          onClick={() => setIsExpanded(!isExpanded)}
          className={cn(
            "flex items-center gap-1 text-[10px] text-[#7aa2f7]",
            "hover:text-[#89b4fa] transition-colors"
          )}
        >
          {isExpanded ? (
            <>
              <ChevronUp className="w-3 h-3" />
              <span>Show less</span>
            </>
          ) : (
            <>
              <ChevronDown className="w-3 h-3" />
              <span>... {truncation.hiddenLines} more lines</span>
            </>
          )}
        </button>
      )}
    </div>
  );
}

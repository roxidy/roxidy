import { Bot, Terminal } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { SessionMode } from "@/store";
import { useSessionMode, useStore } from "@/store";

interface ModeToggleProps {
  sessionId: string;
}

const options: { value: SessionMode; label: string; icon: typeof Terminal }[] = [
  { value: "terminal", label: "Terminal", icon: Terminal },
  { value: "agent", label: "Agent", icon: Bot },
];

export function ModeToggle({ sessionId }: ModeToggleProps) {
  const mode = useSessionMode(sessionId);
  const setSessionMode = useStore((state) => state.setSessionMode);

  return (
    <div className="flex items-center bg-[#1f2335] rounded-lg p-0.5">
      {options.map(({ value, label, icon: Icon }) => (
        <Button
          key={value}
          variant="ghost"
          size="sm"
          onClick={() => setSessionMode(sessionId, value)}
          className={cn(
            "gap-1.5 h-7 px-3 text-xs font-medium",
            mode === value
              ? value === "terminal"
                ? "bg-[#7aa2f7] text-[#1a1b26] hover:bg-[#7aa2f7]/90 hover:text-[#1a1b26]"
                : "bg-[#bb9af7] text-[#1a1b26] hover:bg-[#bb9af7]/90 hover:text-[#1a1b26]"
              : "text-[#565f89] hover:text-[#c0caf5] hover:bg-transparent"
          )}
        >
          <Icon className="w-3.5 h-3.5" />
          {label}
        </Button>
      ))}
    </div>
  );
}

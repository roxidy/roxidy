import { Terminal, Bot } from "lucide-react";
import { cn } from "@/lib/utils";
import { useStore, useSessionMode } from "@/store";
import type { SessionMode } from "@/store";

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
        <button
          key={value}
          onClick={() => setSessionMode(sessionId, value)}
          className={cn(
            "flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-medium transition-all",
            mode === value
              ? value === "terminal"
                ? "bg-[#7aa2f7] text-[#1a1b26]"
                : "bg-[#bb9af7] text-[#1a1b26]"
              : "text-[#565f89] hover:text-[#c0caf5]"
          )}
        >
          <Icon className="w-3.5 h-3.5" />
          {label}
        </button>
      ))}
    </div>
  );
}

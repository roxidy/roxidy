import { Plus, X, Terminal, Bot } from "lucide-react";
import { cn } from "@/lib/utils";
import { useStore, type Session } from "@/store";
import { ptyDestroy } from "@/lib/tauri";

interface TabBarProps {
  onNewTab: () => void;
}

export function TabBar({ onNewTab }: TabBarProps) {
  const sessions = useStore((state) => state.sessions);
  const activeSessionId = useStore((state) => state.activeSessionId);
  const setActiveSession = useStore((state) => state.setActiveSession);
  const removeSession = useStore((state) => state.removeSession);

  const sessionList = Object.values(sessions);

  const handleCloseTab = async (e: React.MouseEvent, sessionId: string) => {
    e.stopPropagation();
    try {
      await ptyDestroy(sessionId);
    } catch (err) {
      console.error("Failed to destroy PTY:", err);
    }
    removeSession(sessionId);
  };

  return (
    <div className="flex items-center h-9 bg-[#16161e] border-b border-[#27293d] px-1 gap-1">
      {/* Tabs */}
      {sessionList.map((session, index) => (
        <Tab
          key={session.id}
          session={session}
          index={index + 1}
          isActive={session.id === activeSessionId}
          onSelect={() => setActiveSession(session.id)}
          onClose={(e) => handleCloseTab(e, session.id)}
          canClose={sessionList.length > 1}
        />
      ))}

      {/* New tab button */}
      <button
        onClick={onNewTab}
        className="flex items-center justify-center w-7 h-7 rounded hover:bg-[#1f2335] text-[#565f89] hover:text-[#c0caf5] transition-colors"
        title="New tab (Cmd+T)"
      >
        <Plus className="w-4 h-4" />
      </button>
    </div>
  );
}

interface TabProps {
  session: Session;
  index: number;
  isActive: boolean;
  onSelect: () => void;
  onClose: (e: React.MouseEvent) => void;
  canClose: boolean;
}

function Tab({
  session,
  index: _index,
  isActive,
  onSelect,
  onClose,
  canClose,
}: TabProps) {
  // Get short name from working directory
  const dirName = session.workingDirectory.split("/").pop() || "Terminal";

  const ModeIcon = session.mode === "agent" ? Bot : Terminal;
  const modeColor = session.mode === "agent" ? "text-[#bb9af7]" : "text-[#7aa2f7]";

  return (
    <div
      onClick={onSelect}
      className={cn(
        "group flex items-center gap-2 px-3 h-7 rounded cursor-pointer transition-colors min-w-0 max-w-[200px]",
        isActive
          ? "bg-[#1a1b26] text-[#c0caf5]"
          : "text-[#565f89] hover:bg-[#1f2335] hover:text-[#a9b1d6]"
      )}
    >
      {/* Mode icon */}
      <ModeIcon
        className={cn(
          "w-3.5 h-3.5 flex-shrink-0",
          isActive ? modeColor : "text-[#565f89]"
        )}
      />

      {/* Tab name */}
      <span className="truncate text-sm">{dirName}</span>

      {/* Close button */}
      {canClose && (
        <button
          onClick={onClose}
          className={cn(
            "flex-shrink-0 p-0.5 rounded opacity-0 group-hover:opacity-100 transition-opacity",
            "hover:bg-[#3b4261] text-[#565f89] hover:text-[#c0caf5]"
          )}
          title="Close tab"
        >
          <X className="w-3 h-3" />
        </button>
      )}
    </div>
  );
}

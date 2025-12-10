import { getCurrentWindow } from "@tauri-apps/api/window";
import { Bot, Plus, Settings, Terminal, X } from "lucide-react";
import React from "react";
import { Button } from "@/components/ui/button";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { ptyDestroy } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { type Session, useStore } from "@/store";

const startDrag = async (e: React.MouseEvent) => {
  e.preventDefault();
  try {
    await getCurrentWindow().startDragging();
  } catch (err) {
    console.error("Failed to start dragging:", err);
  }
};

interface TabBarProps {
  onNewTab: () => void;
  onOpenSettings?: () => void;
}

export function TabBar({ onNewTab, onOpenSettings }: TabBarProps) {
  const sessions = useStore((state) => state.sessions);
  const activeSessionId = useStore((state) => state.activeSessionId);
  const setActiveSession = useStore((state) => state.setActiveSession);
  const removeSession = useStore((state) => state.removeSession);

  const sessionList = Object.values(sessions);

  const handleCloseTab = React.useCallback(
    async (e: React.MouseEvent, sessionId: string) => {
      e.stopPropagation();
      try {
        await ptyDestroy(sessionId);
      } catch (err) {
        console.error("Failed to destroy PTY:", err);
      }
      removeSession(sessionId);
    },
    [removeSession]
  );

  return (
    <TooltipProvider delayDuration={300}>
      {/* biome-ignore lint/a11y/noStaticElementInteractions: div is used for window drag region */}
      <div
        className="flex items-center h-9 bg-accent/2 backdrop-blur-sm border-b border-border/50 pl-[78px] pr-1 gap-1"
        onMouseDown={startDrag}
      >
        <Tabs
          value={activeSessionId || undefined}
          onValueChange={setActiveSession}
          className="min-w-0"
          onMouseDown={(e) => e.stopPropagation()}
        >
          <TabsList className="h-7 bg-transparent p-0 gap-1 w-full justify-start">
            {sessionList.map((session) => (
              <TabItem
                key={session.id}
                session={session}
                isActive={session.id === activeSessionId}
                onClose={(e) => handleCloseTab(e, session.id)}
                canClose={sessionList.length > 1}
              />
            ))}
          </TabsList>
        </Tabs>

        {/* New tab button */}
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              onClick={onNewTab}
              onMouseDown={(e) => e.stopPropagation()}
              className="h-7 w-7 text-muted-foreground hover:text-foreground hover:bg-card"
            >
              <Plus className="w-4 h-4" />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="bottom">
            <p>New tab (⌘T)</p>
          </TooltipContent>
        </Tooltip>

        {/* Drag region - empty space extends to fill remaining width */}
        <div className="flex-1 h-full min-w-[100px]" />

        {/* Settings button */}
        {onOpenSettings && (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                onClick={onOpenSettings}
                onMouseDown={(e) => e.stopPropagation()}
                className="h-7 w-7 text-[#565f89] hover:text-[#c0caf5] hover:bg-[#1f2335]"
              >
                <Settings className="w-4 h-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent side="bottom">
              <p>Settings (⌘,)</p>
            </TooltipContent>
          </Tooltip>
        )}
      </div>
    </TooltipProvider>
  );
}

interface TabItemProps {
  session: Session;
  isActive: boolean;
  onClose: (e: React.MouseEvent) => void;
  canClose: boolean;
}

const TabItem = React.memo(function TabItem({ session, isActive, onClose, canClose }: TabItemProps) {
  const setCustomTabName = useStore((state) => state.setCustomTabName);
  const [isEditing, setIsEditing] = React.useState(false);
  const [editValue, setEditValue] = React.useState("");
  const inputRef = React.useRef<HTMLInputElement>(null);

  // Determine display name: custom name > process name > directory name
  const { displayName, dirName, isCustomName, isProcessName } = React.useMemo(() => {
    const dir = session.workingDirectory.split(/[/\\]/).pop() || "Terminal";
    const name = session.customName || session.processName || dir;
    return {
      displayName: name,
      dirName: dir,
      isCustomName: !!session.customName,
      isProcessName: !session.customName && !!session.processName,
    };
  }, [session.customName, session.processName, session.workingDirectory]);

  // Focus input when entering edit mode
  React.useEffect(() => {
    if (isEditing && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [isEditing]);

  const handleDoubleClick = React.useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setIsEditing(true);
      setEditValue(session.customName || dirName);
    },
    [session.customName, dirName]
  );

  const handleSave = React.useCallback(() => {
    const trimmed = editValue.trim();
    setCustomTabName(session.id, trimmed || null);
    setIsEditing(false);
  }, [editValue, session.id, setCustomTabName]);

  const handleKeyDown = React.useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        handleSave();
      } else if (e.key === "Escape") {
        e.preventDefault();
        setIsEditing(false);
      }
    },
    [handleSave]
  );

  const ModeIcon = session.mode === "agent" ? Bot : Terminal;
  const modeColor =
    session.mode === "agent" ? "text-[var(--ansi-magenta)]" : "text-[var(--ansi-blue)]";

  // Generate tooltip text showing full context
  const tooltipText = React.useMemo(() => {
    if (isCustomName) return `Custom name: ${displayName}\nDirectory: ${session.workingDirectory}`;
    if (isProcessName) return `Running: ${displayName}\nDirectory: ${session.workingDirectory}`;
    return session.workingDirectory;
  }, [isCustomName, isProcessName, displayName, session.workingDirectory]);

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div className="group relative flex items-center">
          <TabsTrigger
            value={session.id}
            className={cn(
              "relative flex items-center gap-2 px-3 h-5 rounded-md min-w-0 max-w-[200px]",
              "data-[state=active]:bg-accent data-[state=active]:text-foreground data-[state=active]:shadow-none",
              "data-[state=inactive]:text-muted-foreground data-[state=inactive]:hover:bg-card data-[state=inactive]:hover:text-foreground",
              "border-none focus-visible:ring-0 focus-visible:ring-offset-0",
              canClose && "pr-7" // Add padding for close button
            )}
          >
            {/* Mode icon */}
            <ModeIcon
              className={cn(
                "w-3.5 h-3.5 flex-shrink-0",
                isActive ? modeColor : "text-muted-foreground"
              )}
            />

            {/* Tab name or edit input */}
            {isEditing ? (
              <input
                ref={inputRef}
                type="text"
                value={editValue}
                onChange={(e) => setEditValue(e.target.value)}
                onBlur={handleSave}
                onKeyDown={handleKeyDown}
                onClick={(e) => e.stopPropagation()}
                className={cn(
                  "truncate text-xs bg-transparent border-none outline-none",
                  "focus:ring-1 focus:ring-primary rounded px-1 min-w-[60px] max-w-[140px]"
                )}
              />
            ) : (
              <span
                className={cn(
                  "truncate text-xs",
                  isProcessName && "text-[var(--ansi-yellow)]"
                )}
                onDoubleClick={handleDoubleClick}
              >
                {displayName}
              </span>
            )}
          </TabsTrigger>

          {/* Close button - positioned outside TabsTrigger to avoid nested buttons */}
          {canClose && (
            <button
              type="button"
              onClick={onClose}
              className={cn(
                "absolute right-1 p-0.5 rounded opacity-0 group-hover:opacity-100 transition-opacity",
                "hover:bg-primary/20 text-muted-foreground hover:text-foreground",
                "z-10"
              )}
              title="Close tab"
            >
              <X className="w-3 h-3" />
            </button>
          )}
        </div>
      </TooltipTrigger>
      <TooltipContent side="bottom" className="whitespace-pre-wrap">
        <p className="text-xs">{tooltipText}</p>
      </TooltipContent>
    </Tooltip>
  );
});

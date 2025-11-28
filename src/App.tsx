import { useCallback, useEffect, useState } from "react";
import { Toaster, toast } from "sonner";
import { ToolApprovalDialog } from "./components/AgentChat";
import { CommandPalette, type PageRoute } from "./components/CommandPalette";
import { TabBar } from "./components/TabBar";
import { UnifiedInput } from "./components/UnifiedInput";
import { UnifiedTimeline } from "./components/UnifiedTimeline";
import { Skeleton } from "./components/ui/skeleton";
import { useAiEvents } from "./hooks/useAiEvents";
import { useTauriEvents } from "./hooks/useTauriEvents";
import { initVertexClaudeOpus, isAiInitialized } from "./lib/ai";
import { ptyCreate, shellIntegrationInstall, shellIntegrationStatus } from "./lib/tauri";
import { ComponentTestbed } from "./pages/ComponentTestbed";
import { useStore } from "./store";

// ContentArea now just renders the unified timeline
function ContentArea({ sessionId }: { sessionId: string }) {
  return <UnifiedTimeline sessionId={sessionId} />;
}

function App() {
  const { addSession, activeSessionId, sessions, setInputMode } = useStore();
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);
  const [currentPage, setCurrentPage] = useState<PageRoute>("main");

  // Get current session's working directory
  const activeSession = activeSessionId ? sessions[activeSessionId] : null;
  const workingDirectory = activeSession?.workingDirectory;

  // Connect Tauri events to store
  useTauriEvents();

  // Subscribe to AI events for agent mode
  useAiEvents();

  // Create a new terminal tab
  const handleNewTab = useCallback(async () => {
    try {
      const session = await ptyCreate();
      addSession({
        id: session.id,
        name: "Terminal",
        workingDirectory: session.working_directory,
        createdAt: new Date().toISOString(),
        mode: "terminal",
      });
    } catch (e) {
      console.error("Failed to create new tab:", e);
      toast.error("Failed to create new tab");
    }
  }, [addSession]);

  useEffect(() => {
    async function init() {
      try {
        // Check and install shell integration if needed
        const status = await shellIntegrationStatus();
        if (status.type === "NotInstalled") {
          toast.info("Installing shell integration...");
          await shellIntegrationInstall();
          toast.success("Shell integration installed! Restart your shell for full features.");
        } else if (status.type === "Outdated") {
          toast.info("Updating shell integration...");
          await shellIntegrationInstall();
          toast.success("Shell integration updated!");
        }

        // Create initial terminal session
        const session = await ptyCreate();
        addSession({
          id: session.id,
          name: "Terminal",
          workingDirectory: session.working_directory,
          createdAt: new Date().toISOString(),
          mode: "terminal",
        });

        // Initialize AI agent with Vertex AI Claude Opus 4.5
        // Uses the user's GCP service account credentials
        try {
          const alreadyInitialized = await isAiInitialized();
          if (!alreadyInitialized) {
            await initVertexClaudeOpus(
              session.working_directory,
              "/Users/xlyk/.keys/vertex-ai.json",
              "futurhealth",
              "us-east5"
            );
            toast.success("AI agent initialized (Claude Opus 4.5 on Vertex AI)");
          }
        } catch (aiError) {
          console.error("Failed to initialize AI agent:", aiError);
          toast.warning("AI agent not available - agent mode may not work");
        }

        setIsLoading(false);
      } catch (e) {
        console.error("Failed to initialize:", e);
        setError(e instanceof Error ? e.message : String(e));
        setIsLoading(false);
      }
    }

    init();
  }, [addSession]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Cmd+K for command palette
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setCommandPaletteOpen(true);
        return;
      }

      // Cmd+T for new tab
      if ((e.metaKey || e.ctrlKey) && e.key === "t") {
        e.preventDefault();
        handleNewTab();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleNewTab]);

  // Handle input mode change from command palette
  // NOTE: This must be defined before any early returns to maintain hook order
  const handleSetMode = useCallback(
    (newMode: "terminal" | "agent") => {
      if (activeSessionId) {
        setInputMode(activeSessionId, newMode);
      }
    },
    [activeSessionId, setInputMode]
  );

  if (isLoading) {
    return (
      <div className="h-screen w-screen bg-[#1a1b26] flex flex-col overflow-hidden">
        {/* Skeleton tab bar */}
        <div className="flex items-center h-9 bg-[#16161e] border-b border-[#27293d] px-2 gap-2">
          <Skeleton className="h-6 w-24 bg-[#1f2335]" />
          <Skeleton className="h-6 w-6 rounded bg-[#1f2335]" />
        </div>

        {/* Skeleton content area */}
        <div className="flex-1 p-4 space-y-3">
          <Skeleton className="h-16 w-full bg-[#1f2335]" />
          <Skeleton className="h-16 w-3/4 bg-[#1f2335]" />
          <Skeleton className="h-16 w-5/6 bg-[#1f2335]" />
        </div>

        {/* Skeleton input area */}
        <div className="bg-[#1a1b26] border-t border-[#1f2335] px-4 py-3 space-y-2">
          <div className="flex items-center justify-between">
            <Skeleton className="h-4 w-32 bg-[#1f2335]" />
            <Skeleton className="h-7 w-40 rounded-lg bg-[#1f2335]" />
          </div>
          <Skeleton className="h-8 w-full bg-[#1f2335]" />
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-screen bg-[#1a1b26]">
        <div className="text-[#f7768e] text-lg">Error: {error}</div>
      </div>
    );
  }

  // Render component testbed page
  if (currentPage === "testbed") {
    return (
      <>
        <ComponentTestbed />
        <CommandPalette
          open={commandPaletteOpen}
          onOpenChange={setCommandPaletteOpen}
          currentPage={currentPage}
          onNavigate={setCurrentPage}
          activeSessionId={activeSessionId}
          onNewTab={handleNewTab}
          onSetMode={handleSetMode}
        />
        <Toaster
          position="bottom-right"
          theme="dark"
          toastOptions={{
            style: {
              background: "#1f2335",
              border: "1px solid #3b4261",
              color: "#c0caf5",
            },
          }}
        />
      </>
    );
  }

  return (
    <div className="h-screen w-screen bg-[#1a1b26] flex flex-col overflow-hidden">
      {/* Tab bar */}
      <TabBar onNewTab={handleNewTab} />

      {/* Main content area */}
      <div className="flex-1 min-h-0 flex flex-col">
        {activeSessionId ? (
          <>
            {/* Scrollable content area - auto-scroll handled in UnifiedTimeline */}
            <div className="flex-1 overflow-auto bg-[#1a1b26]">
              <ContentArea sessionId={activeSessionId} />
            </div>

            {/* Unified input at bottom */}
            <UnifiedInput sessionId={activeSessionId} workingDirectory={workingDirectory} />

            {/* Tool approval dialog */}
            <ToolApprovalDialog sessionId={activeSessionId} />
          </>
        ) : (
          <div className="flex items-center justify-center h-full">
            <span className="text-[#565f89]">No active session</span>
          </div>
        )}
      </div>

      {/* Command Palette */}
      <CommandPalette
        open={commandPaletteOpen}
        onOpenChange={setCommandPaletteOpen}
        currentPage={currentPage}
        onNavigate={setCurrentPage}
        activeSessionId={activeSessionId}
        onNewTab={handleNewTab}
        onSetMode={handleSetMode}
      />

      <Toaster
        position="bottom-right"
        theme="dark"
        toastOptions={{
          style: {
            background: "#1f2335",
            border: "1px solid #3b4261",
            color: "#c0caf5",
          },
        }}
      />
    </div>
  );
}

export default App;

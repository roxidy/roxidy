import { useEffect, useState, useCallback, useRef } from "react";
import { useStore, useSessionBlocks, useSessionMode } from "./store";
import { useTauriEvents } from "./hooks/useTauriEvents";
import {
  ptyCreate,
  shellIntegrationStatus,
  shellIntegrationInstall,
} from "./lib/tauri";
import { CommandBlockList } from "./components/CommandBlock";
import { AgentChatList, ToolApprovalDialog } from "./components/AgentChat";
import { UnifiedInput } from "./components/UnifiedInput";
import { TabBar } from "./components/TabBar";
import { Toaster, toast } from "sonner";

function TerminalPlaceholder() {
  return (
    <div className="flex items-center justify-center h-full text-[#565f89] text-sm">
      <div className="text-center">
        <p>No command blocks yet</p>
        <p className="text-xs mt-1">Run commands in the terminal below</p>
      </div>
    </div>
  );
}

function ContentArea({ sessionId }: { sessionId: string }) {
  const mode = useSessionMode(sessionId);
  const blocks = useSessionBlocks(sessionId);

  if (mode === "terminal") {
    if (blocks.length === 0) {
      return <TerminalPlaceholder />;
    }
    return <CommandBlockList sessionId={sessionId} />;
  }

  // Agent mode
  return <AgentChatList sessionId={sessionId} />;
}

function App() {
  const { addSession, activeSessionId, sessions, setSessionMode } = useStore();
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const blocksContainerRef = useRef<HTMLDivElement>(null);

  // Get current session's working directory and blocks
  const activeSession = activeSessionId ? sessions[activeSessionId] : null;
  const workingDirectory = activeSession?.workingDirectory;
  const blocks = useSessionBlocks(activeSessionId || "");

  // Auto-scroll to bottom when new blocks are added (terminal mode only)
  const mode = useSessionMode(activeSessionId || "");
  useEffect(() => {
    if (blocksContainerRef.current && blocks.length > 0 && mode === "terminal") {
      blocksContainerRef.current.scrollTop =
        blocksContainerRef.current.scrollHeight;
    }
  }, [blocks.length, mode]);

  // Connect Tauri events to store
  useTauriEvents();

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
          toast.success(
            "Shell integration installed! Restart your shell for full features."
          );
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
      // Cmd+T for new tab
      if ((e.metaKey || e.ctrlKey) && e.key === "t") {
        e.preventDefault();
        handleNewTab();
      }

      // Cmd+1 for terminal mode, Cmd+2 for agent mode
      if ((e.metaKey || e.ctrlKey) && activeSessionId) {
        if (e.key === "1") {
          e.preventDefault();
          setSessionMode(activeSessionId, "terminal");
        } else if (e.key === "2") {
          e.preventDefault();
          setSessionMode(activeSessionId, "agent");
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleNewTab, activeSessionId, setSessionMode]);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-screen bg-[#1a1b26]">
        <div className="text-[#c0caf5] text-lg">Loading...</div>
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

  return (
    <div className="h-screen w-screen bg-[#1a1b26] flex flex-col overflow-hidden">
      {/* Tab bar */}
      <TabBar onNewTab={handleNewTab} />

      {/* Main content area */}
      <div className="flex-1 min-h-0 flex flex-col">
        {activeSessionId ? (
          <>
            {/* Scrollable content area */}
            <div
              ref={blocksContainerRef}
              className="flex-1 overflow-auto bg-[#1a1b26]"
            >
              <ContentArea sessionId={activeSessionId} />
            </div>

            {/* Unified input at bottom */}
            <UnifiedInput
              sessionId={activeSessionId}
              workingDirectory={workingDirectory}
            />

            {/* Tool approval dialog */}
            <ToolApprovalDialog sessionId={activeSessionId} />
          </>
        ) : (
          <div className="flex items-center justify-center h-full">
            <span className="text-[#565f89]">No active session</span>
          </div>
        )}
      </div>

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

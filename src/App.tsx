import { useCallback, useEffect, useRef, useState } from "react";
import { Toaster, toast } from "sonner";
import { ToolApprovalDialog } from "./components/AgentChat";
import { CommandPalette, type PageRoute } from "./components/CommandPalette";
import { MockDevTools } from "./components/MockDevTools";
import { SessionBrowser } from "./components/SessionBrowser";
import { SettingsDialog } from "./components/Settings";
import { Sidebar } from "./components/Sidebar";
import { ContextPanel, SidecarNotifications, SidecarPanel } from "./components/Sidecar";
import { StatusBar } from "./components/StatusBar";
import { TabBar } from "./components/TabBar";
import { UnifiedInput } from "./components/UnifiedInput";
import { UnifiedTimeline } from "./components/UnifiedTimeline";
import { Skeleton } from "./components/ui/skeleton";
import { useAiEvents } from "./hooks/useAiEvents";
import { useTauriEvents } from "./hooks/useTauriEvents";
import { ThemeProvider } from "./hooks/useTheme";
import {
  getVertexAiConfig,
  initVertexClaudeOpus,
  isAiInitialized,
  updateAiWorkspace,
  VERTEX_AI_MODELS,
} from "./lib/ai";
import {
  getIndexedFileCount,
  indexDirectory,
  initIndexer,
  isIndexerInitialized,
} from "./lib/indexer";
import { ptyCreate, shellIntegrationInstall, shellIntegrationStatus } from "./lib/tauri";
import { ComponentTestbed } from "./pages/ComponentTestbed";
import { clearConversation, restoreSession, useStore } from "./store";

// Check if running in browser mode (mocks are active)
// The __MOCK_BROWSER_MODE__ flag is set by setupMocks() BEFORE mockWindows() creates __TAURI_INTERNALS__
// This allows us to correctly detect browser mode even after mocks are initialized
declare global {
  interface Window {
    __MOCK_BROWSER_MODE__?: boolean;
  }
}
const isBrowserMode = typeof window !== "undefined" && window.__MOCK_BROWSER_MODE__ === true;

function App() {
  const { addSession, activeSessionId, sessions, setInputMode, setAiConfig } = useStore();
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);
  const [sessionBrowserOpen, setSessionBrowserOpen] = useState(false);
  const [contextPanelOpen, setContextPanelOpen] = useState(false);
  const [sidecarPanelOpen, setSidecarPanelOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [currentPage, setCurrentPage] = useState<PageRoute>("main");
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const initializingRef = useRef(false);

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

      // Sync AI workspace with the new tab's working directory
      try {
        const initialized = await isAiInitialized();
        if (initialized && session.working_directory) {
          await updateAiWorkspace(session.working_directory);
        }
      } catch {
        // Silently ignore - AI sync is best-effort
      }
    } catch (e) {
      console.error("Failed to create new tab:", e);
      toast.error("Failed to create new tab");
    }
  }, [addSession]);

  useEffect(() => {
    async function init() {
      try {
        // Prevent double-initialization from React StrictMode in development
        if (initializingRef.current) {
          return;
        }
        initializingRef.current = true;

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

        // Initialize code indexer for the workspace (without auto-indexing)
        // Users can manually trigger indexing via command palette or sidebar
        try {
          const indexerInitialized = await isIndexerInitialized();
          if (!indexerInitialized && session.working_directory) {
            await initIndexer(session.working_directory);
          }

          // Check if workspace has been indexed, prompt user if not
          const fileCount = await getIndexedFileCount();
          if (fileCount === 0 && session.working_directory) {
            toast("Index your workspace?", {
              description: "Enable code search and symbol navigation",
              action: {
                label: "Index Now",
                onClick: async () => {
                  toast.promise(indexDirectory(session.working_directory), {
                    loading: "Indexing workspace...",
                    success: (result) => `Indexed ${result.files_indexed} files`,
                    error: (err) => `Indexing failed: ${err}`,
                  });
                },
              },
              duration: 10000,
            });
          }
        } catch (indexerError) {
          console.warn("Failed to initialize code indexer:", indexerError);
          // Non-fatal - indexer is optional
        }

        // Initialize AI agent with Vertex AI Claude Opus 4.5
        // Uses environment variables for configuration
        try {
          const envConfig = await getVertexAiConfig();

          // Check if required env vars are set
          if (!envConfig.credentials_path) {
            throw new Error(
              "Vertex AI credentials not configured. Set VERTEX_AI_CREDENTIALS_PATH or GOOGLE_APPLICATION_CREDENTIALS environment variable."
            );
          }
          if (!envConfig.project_id) {
            throw new Error(
              "Vertex AI project ID not configured. Set VERTEX_AI_PROJECT_ID or GOOGLE_CLOUD_PROJECT environment variable."
            );
          }

          const vertexConfig = {
            workspace: session.working_directory,
            credentialsPath: envConfig.credentials_path,
            projectId: envConfig.project_id,
            location: envConfig.location || "us-east5",
          };

          const alreadyInitialized = await isAiInitialized();
          if (!alreadyInitialized) {
            setAiConfig({
              provider: "anthropic_vertex",
              model: VERTEX_AI_MODELS.CLAUDE_OPUS_4_5,
              status: "initializing",
              vertexConfig,
            });
            await initVertexClaudeOpus(
              vertexConfig.workspace,
              vertexConfig.credentialsPath,
              vertexConfig.projectId,
              vertexConfig.location
            );
            setAiConfig({ status: "ready" });

            // Sync AI workspace with the session's current working directory
            // The shell may have already reported a directory change before AI initialized
            const currentSession = useStore.getState().sessions[session.id];
            if (
              currentSession?.workingDirectory &&
              currentSession.workingDirectory !== vertexConfig.workspace
            ) {
              await updateAiWorkspace(currentSession.workingDirectory);
            }
          } else {
            // Already initialized from previous session
            setAiConfig({
              provider: "anthropic_vertex",
              model: VERTEX_AI_MODELS.CLAUDE_OPUS_4_5,
              status: "ready",
              vertexConfig,
            });
          }
        } catch (aiError) {
          console.error("Failed to initialize AI agent:", aiError);
          setAiConfig({
            provider: "anthropic_vertex",
            model: "",
            status: "error",
            errorMessage: aiError instanceof Error ? aiError.message : "Unknown error",
          });
        }

        setIsLoading(false);
      } catch (e) {
        console.error("Failed to initialize:", e);
        setError(e instanceof Error ? e.message : String(e));
        setIsLoading(false);
      }
    }

    init();
  }, [addSession, setAiConfig]);

  // Handle toggle mode from command palette (switches between terminal and agent)
  // NOTE: This must be defined before the keyboard shortcut useEffect that uses it
  const handleToggleMode = useCallback(() => {
    if (activeSessionId) {
      const currentSession = sessions[activeSessionId];
      const newMode = currentSession?.mode === "agent" ? "terminal" : "agent";
      setInputMode(activeSessionId, newMode);
    }
  }, [activeSessionId, sessions, setInputMode]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Cmd+, for settings
      if ((e.metaKey || e.ctrlKey) && e.key === ",") {
        e.preventDefault();
        setSettingsOpen(true);
        return;
      }
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
        return;
      }

      // Cmd+B for sidebar toggle
      if ((e.metaKey || e.ctrlKey) && e.key === "b") {
        e.preventDefault();
        setSidebarOpen((prev) => !prev);
        return;
      }

      // Cmd+H for session browser
      if ((e.metaKey || e.ctrlKey) && e.key === "h") {
        e.preventDefault();
        setSessionBrowserOpen(true);
        return;
      }

      // Cmd+I for toggle mode
      if ((e.metaKey || e.ctrlKey) && e.key === "i") {
        e.preventDefault();
        handleToggleMode();
        return;
      }

      // Cmd+Shift+C for context panel
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key === "c") {
        e.preventDefault();
        setContextPanelOpen(true);
        return;
      }

      // Cmd+Shift+P for sidecar panel (patches/artifacts)
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key === "p") {
        e.preventDefault();
        setSidecarPanelOpen(true);
        return;
      }

      // Cmd+, for settings
      if ((e.metaKey || e.ctrlKey) && e.key === ",") {
        e.preventDefault();
        setSettingsOpen(true);
        return;
      }

      // Ctrl+] for next tab
      if (e.ctrlKey && e.key === "]") {
        e.preventDefault();
        const sIds = Object.keys(sessions);
        if (activeSessionId && sIds.length > 1) {
          const idx = sIds.indexOf(activeSessionId);
          useStore.getState().setActiveSession(sIds[(idx + 1) % sIds.length]);
        }
        return;
      }

      // Ctrl+[ for previous tab
      if (e.ctrlKey && e.key === "[") {
        e.preventDefault();
        const sIds = Object.keys(sessions);
        if (activeSessionId && sIds.length > 1) {
          const idx = sIds.indexOf(activeSessionId);
          useStore.getState().setActiveSession(sIds[(idx - 1 + sIds.length) % sIds.length]);
        }
        return;
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleNewTab, handleToggleMode, sessions, activeSessionId]);

  // Handle clear conversation from command palette
  const handleClearConversation = useCallback(async () => {
    if (activeSessionId) {
      await clearConversation(activeSessionId);
      toast.success("Conversation cleared");
    }
  }, [activeSessionId]);

  // Handle session restore from session browser
  const handleRestoreSession = useCallback(
    async (identifier: string) => {
      if (!activeSessionId) {
        toast.error("No active session to restore into");
        return;
      }
      try {
        await restoreSession(activeSessionId, identifier);
        toast.success("Session restored");
      } catch (error) {
        toast.error(`Failed to restore session: ${error}`);
      }
    },
    [activeSessionId]
  );

  if (isLoading) {
    return (
      <div className="h-screen w-screen bg-[#1a1b26] flex flex-col overflow-hidden">
        {/* Skeleton tab bar */}
        <div className="flex items-center h-9 bg-[#1a1b26] pl-[78px] pr-2 gap-2 titlebar-drag">
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

        {/* Mock Dev Tools - available during loading in browser mode */}
        {isBrowserMode && <MockDevTools />}
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-screen bg-[#1a1b26]">
        <div className="text-[#f7768e] text-lg">Error: {error}</div>
        {/* Mock Dev Tools - available on error in browser mode */}
        {isBrowserMode && <MockDevTools />}
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
          onToggleMode={handleToggleMode}
          onClearConversation={handleClearConversation}
          onOpenSessionBrowser={() => setSessionBrowserOpen(true)}
          onOpenSettings={() => setSettingsOpen(true)}
        />
        <SessionBrowser
          open={sessionBrowserOpen}
          onOpenChange={setSessionBrowserOpen}
          onSessionRestore={handleRestoreSession}
        />
        <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
        <Toaster
          position="top-right"
          theme="dark"
          closeButton
          toastOptions={{
            style: {
              background: "var(--card)",
              border: "1px solid var(--border)",
              color: "var(--foreground)",
            },
          }}
        />
        {/* Mock Dev Tools - available on testbed in browser mode */}
        {isBrowserMode && <MockDevTools />}
      </>
    );
  }

  return (
    <div className="h-screen w-screen bg-background flex flex-col overflow-hidden app-bg-layered">
      {/* Tab bar */}
      <TabBar onNewTab={handleNewTab} onOpenSettings={() => setSettingsOpen(true)} />

      {/* Main content area with sidebar */}
      <div className="flex-1 min-h-0 min-w-0 flex overflow-hidden">
        {/* Sidebar */}
        <Sidebar
          workingDirectory={workingDirectory}
          isOpen={sidebarOpen}
          onToggle={() => setSidebarOpen(false)}
          onFileSelect={(_filePath, _line) => {
            // File selection is handled by Sidebar internally for now
          }}
        />

        {/* Main content */}
        <div className="flex-1 min-h-0 min-w-0 flex flex-col overflow-hidden">
          {activeSessionId ? (
            <>
              {/* Scrollable content area - auto-scroll handled in UnifiedTimeline */}
              <div className="flex-1 min-w-0 overflow-auto">
                <UnifiedTimeline sessionId={activeSessionId} />
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

        {/* Context Panel - integrated side panel, uses sidecar's current session */}
        <ContextPanel open={contextPanelOpen} onOpenChange={setContextPanelOpen} />
      </div>

      {/* Status bar at the very bottom */}
      <StatusBar sessionId={activeSessionId} />

      {/* Command Palette */}
      <CommandPalette
        open={commandPaletteOpen}
        onOpenChange={setCommandPaletteOpen}
        currentPage={currentPage}
        onNavigate={setCurrentPage}
        activeSessionId={activeSessionId}
        onNewTab={handleNewTab}
        onToggleMode={handleToggleMode}
        onClearConversation={handleClearConversation}
        onToggleSidebar={() => setSidebarOpen((prev) => !prev)}
        workingDirectory={workingDirectory}
        onOpenSessionBrowser={() => setSessionBrowserOpen(true)}
        onOpenContextPanel={() => setContextPanelOpen(true)}
        onOpenSettings={() => setSettingsOpen(true)}
      />

      {/* Sidecar Panel (Patches & Artifacts) */}
      <SidecarPanel open={sidecarPanelOpen} onOpenChange={setSidecarPanelOpen} />

      {/* Session Browser */}
      <SessionBrowser
        open={sessionBrowserOpen}
        onOpenChange={setSessionBrowserOpen}
        onSessionRestore={handleRestoreSession}
      />

      {/* Settings Dialog */}
      <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />

      {/* Sidecar event notifications */}
      <SidecarNotifications />

      <Toaster
        position="top-right"
        theme="dark"
        closeButton
        toastOptions={{
          style: {
            background: "var(--card)",
            border: "1px solid var(--border)",
            color: "var(--foreground)",
          },
        }}
      />

      {/* Mock Dev Tools - only in browser mode */}
      {isBrowserMode && <MockDevTools />}
    </div>
  );
}

function AppWithTheme() {
  return (
    <ThemeProvider defaultThemeId="qbit">
      <App />
    </ThemeProvider>
  );
}

export default AppWithTheme;

import { listen } from "@tauri-apps/api/event";
import { FileText, RefreshCw, ScrollText, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Markdown } from "@/components/Markdown/Markdown";
import { Button } from "@/components/ui/button";
import {
  getCurrentSession,
  getSessionLog,
  getSessionState,
  type SidecarEventType,
} from "@/lib/sidecar";
import { cn } from "@/lib/utils";

interface ContextPanelProps {
  /** Session ID to show context for (uses current session if not provided) */
  sessionId?: string;
  /** Whether the panel is open */
  open: boolean;
  /** Callback when panel should close */
  onOpenChange: (open: boolean) => void;
}

type TabId = "state" | "log";

/**
 * Side panel showing the current session's markdown state and log.
 * Displays the state.md (LLM-managed session context) and log.md (event history).
 * Renders inline as part of the flex layout (not a modal overlay).
 */
export function ContextPanel({ sessionId, open, onOpenChange }: ContextPanelProps) {
  const [activeTab, setActiveTab] = useState<TabId>("state");
  const [stateContent, setStateContent] = useState<string>("");
  const [logContent, setLogContent] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [resolvedSessionId, setResolvedSessionId] = useState<string | null>(null);

  // Fetch content for the current (or specified) session
  const fetchContent = useCallback(async () => {
    setLoading(true);
    setError(null);

    try {
      // Resolve session ID
      let sid: string | undefined = sessionId;
      if (!sid) {
        sid = (await getCurrentSession()) ?? undefined;
      }

      if (!sid) {
        setError(null);
        setStateContent(
          "No active capture session.\n\nSend a message to the AI to start context capture."
        );
        setLogContent(
          "No active capture session.\n\nSend a message to the AI to start context capture."
        );
        setResolvedSessionId(null);
        return;
      }

      setResolvedSessionId(sid);

      // Fetch both state and log
      const [state, log] = await Promise.all([
        getSessionState(sid).catch(() => ""),
        getSessionLog(sid).catch(() => ""),
      ]);

      setStateContent(state || "(empty)");
      setLogContent(log || "(empty)");
    } catch (e) {
      // Tauri errors may be strings, not Error objects
      const message =
        e instanceof Error
          ? e.message
          : typeof e === "string"
            ? e
            : "Failed to fetch session content";
      setError(message);
    } finally {
      setLoading(false);
    }
  }, [sessionId]);

  // Fetch content when panel opens
  useEffect(() => {
    if (!open) return;
    fetchContent();
  }, [open, fetchContent]);

  // Subscribe to sidecar events for auto-refresh
  useEffect(() => {
    if (!open) return;

    const unlisten = listen<SidecarEventType>("sidecar-event", (event) => {
      const eventType = event.payload.event_type;
      // Auto-refresh when a new session starts or ends
      if (eventType === "session_started" || eventType === "session_ended") {
        fetchContent();
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [open, fetchContent]);

  if (!open) return null;

  return (
    <div className="w-[400px] min-w-[300px] max-w-[50vw] bg-card border-l border-border flex flex-col">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border">
        <div className="flex items-center gap-2 min-w-0">
          <FileText className="w-4 h-4 text-muted-foreground shrink-0" />
          <h2 className="text-sm font-medium truncate">Session Context</h2>
          {resolvedSessionId && (
            <span className="text-xs text-muted-foreground font-mono shrink-0">
              {resolvedSessionId.slice(0, 8)}...
            </span>
          )}
        </div>
        <div className="flex items-center gap-1 shrink-0">
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6"
            onClick={fetchContent}
            disabled={loading}
          >
            <RefreshCw className={cn("w-3.5 h-3.5", loading && "animate-spin")} />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6"
            onClick={() => onOpenChange(false)}
          >
            <X className="w-3.5 h-3.5" />
          </Button>
        </div>
      </div>

      {/* Tabs */}
      <div className="flex border-b border-border">
        <button
          type="button"
          onClick={() => setActiveTab("state")}
          className={cn(
            "flex-1 px-3 py-1.5 text-xs font-medium transition-colors",
            activeTab === "state"
              ? "text-foreground border-b-2 border-[var(--ansi-blue)]"
              : "text-muted-foreground hover:text-foreground"
          )}
        >
          <FileText className="w-3.5 h-3.5 inline mr-1" />
          State
        </button>
        <button
          type="button"
          onClick={() => setActiveTab("log")}
          className={cn(
            "flex-1 px-3 py-1.5 text-xs font-medium transition-colors",
            activeTab === "log"
              ? "text-foreground border-b-2 border-[var(--ansi-blue)]"
              : "text-muted-foreground hover:text-foreground"
          )}
        >
          <ScrollText className="w-3.5 h-3.5 inline mr-1" />
          Log
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-3">
        {error ? (
          <div className="text-[var(--ansi-red)] text-xs">{error}</div>
        ) : loading ? (
          <div className="text-muted-foreground text-xs animate-pulse">Loading...</div>
        ) : (
          <div className="text-xs [&_h1]:text-base [&_h2]:text-sm [&_h3]:text-xs [&_p]:text-xs [&_li]:text-xs [&_code]:text-[10px] [&_pre]:text-[10px]">
            <Markdown content={activeTab === "state" ? stateContent : logContent} />
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="px-3 py-1.5 border-t border-border text-[10px] text-muted-foreground">
        {activeTab === "state"
          ? "LLM-managed session state (state.md)"
          : "Append-only event log (log.md)"}
      </div>
    </div>
  );
}

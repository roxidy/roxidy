import { FileText, RefreshCw, ScrollText, X } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { getCurrentSession, getSessionLog, getSessionState } from "@/lib/sidecar";
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
 * Slide-over panel showing the current session's markdown state and log.
 * Displays the state.md (LLM-managed session context) and log.md (event history).
 */
export function ContextPanel({ sessionId, open, onOpenChange }: ContextPanelProps) {
  const [activeTab, setActiveTab] = useState<TabId>("state");
  const [stateContent, setStateContent] = useState<string>("");
  const [logContent, setLogContent] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [resolvedSessionId, setResolvedSessionId] = useState<string | null>(null);

  // Resolve session ID and fetch content when panel opens
  useEffect(() => {
    if (!open) return;

    async function fetchContent() {
      setLoading(true);
      setError(null);

      try {
        // Resolve session ID
        let sid: string | undefined = sessionId;
        if (!sid) {
          sid = (await getCurrentSession()) ?? undefined;
        }

        if (!sid) {
          setError("No active session");
          setStateContent("");
          setLogContent("");
          setResolvedSessionId(null);
          setLoading(false);
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
        setError(e instanceof Error ? e.message : "Failed to fetch session content");
      } finally {
        setLoading(false);
      }
    }

    fetchContent();
  }, [open, sessionId]);

  const handleRefresh = async () => {
    if (!resolvedSessionId) return;

    setLoading(true);
    try {
      if (activeTab === "state") {
        const state = await getSessionState(resolvedSessionId);
        setStateContent(state || "(empty)");
      } else {
        const log = await getSessionLog(resolvedSessionId);
        setLogContent(log || "(empty)");
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to refresh");
    } finally {
      setLoading(false);
    }
  };

  if (!open) return null;

  return (
    <>
      {/* Backdrop */}
      <button
        type="button"
        aria-label="Close panel"
        className="fixed inset-0 bg-black/50 z-40 cursor-default"
        onClick={() => onOpenChange(false)}
        onKeyDown={(e) => e.key === "Escape" && onOpenChange(false)}
      />

      {/* Panel */}
      <div className="fixed right-0 top-0 bottom-0 w-[500px] max-w-[90vw] bg-card border-l border-border z-50 flex flex-col shadow-xl">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-border">
          <div className="flex items-center gap-2">
            <FileText className="w-4 h-4 text-muted-foreground" />
            <h2 className="text-sm font-medium">Session Context</h2>
            {resolvedSessionId && (
              <span className="text-xs text-muted-foreground font-mono">
                {resolvedSessionId.slice(0, 8)}...
              </span>
            )}
          </div>
          <div className="flex items-center gap-1">
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={handleRefresh}
              disabled={loading}
            >
              <RefreshCw className={cn("w-4 h-4", loading && "animate-spin")} />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={() => onOpenChange(false)}
            >
              <X className="w-4 h-4" />
            </Button>
          </div>
        </div>

        {/* Tabs */}
        <div className="flex border-b border-border">
          <button
            type="button"
            onClick={() => setActiveTab("state")}
            className={cn(
              "flex-1 px-4 py-2 text-sm font-medium transition-colors",
              activeTab === "state"
                ? "text-foreground border-b-2 border-[var(--ansi-blue)]"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            <FileText className="w-4 h-4 inline mr-1.5" />
            State
          </button>
          <button
            type="button"
            onClick={() => setActiveTab("log")}
            className={cn(
              "flex-1 px-4 py-2 text-sm font-medium transition-colors",
              activeTab === "log"
                ? "text-foreground border-b-2 border-[var(--ansi-blue)]"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            <ScrollText className="w-4 h-4 inline mr-1.5" />
            Log
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-auto p-4">
          {error ? (
            <div className="text-[var(--ansi-red)] text-sm">{error}</div>
          ) : loading ? (
            <div className="text-muted-foreground text-sm animate-pulse">Loading...</div>
          ) : (
            <pre className="text-xs font-mono whitespace-pre-wrap text-foreground/90 leading-relaxed">
              {activeTab === "state" ? stateContent : logContent}
            </pre>
          )}
        </div>

        {/* Footer */}
        <div className="px-4 py-2 border-t border-border text-xs text-muted-foreground">
          {activeTab === "state"
            ? "LLM-managed session state (state.md)"
            : "Append-only event log (log.md)"}
        </div>
      </div>
    </>
  );
}

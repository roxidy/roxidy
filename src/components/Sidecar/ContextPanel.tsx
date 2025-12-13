import { listen } from "@tauri-apps/api/event";
import {
  ChevronDown,
  ChevronRight,
  FileCode,
  FileText,
  GitCommit,
  Package,
  RefreshCw,
  ScrollText,
  X,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Markdown } from "@/components/Markdown/Markdown";
import { Button } from "@/components/ui/button";
import {
  type Artifact,
  getAppliedPatches,
  getCurrentSession,
  getPendingArtifacts,
  getSessionLog,
  getSessionState,
  getStagedPatches,
  previewArtifact,
  type SidecarEventType,
  type StagedPatch,
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

type TabId = "state" | "log" | "patches" | "artifacts";

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

  // Patches state
  const [stagedPatches, setStagedPatches] = useState<StagedPatch[]>([]);
  const [appliedPatches, setAppliedPatches] = useState<StagedPatch[]>([]);
  const [expandedPatch, setExpandedPatch] = useState<number | null>(null);

  // Artifacts state
  const [pendingArtifacts, setPendingArtifacts] = useState<Artifact[]>([]);
  const [expandedArtifact, setExpandedArtifact] = useState<string | null>(null);
  const [artifactPreview, setArtifactPreview] = useState<string | null>(null);

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
        setStagedPatches([]);
        setAppliedPatches([]);
        setPendingArtifacts([]);
        setResolvedSessionId(null);
        return;
      }

      setResolvedSessionId(sid);

      // Fetch all data in parallel
      const [state, log, staged, applied, artifacts] = await Promise.all([
        getSessionState(sid).catch(() => ""),
        getSessionLog(sid).catch(() => ""),
        getStagedPatches(sid).catch(() => []),
        getAppliedPatches(sid).catch(() => []),
        getPendingArtifacts(sid).catch(() => []),
      ]);

      setStateContent(state || "(empty)");
      setLogContent(log || "(empty)");
      setStagedPatches(staged);
      setAppliedPatches(applied);
      setPendingArtifacts(artifacts);
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
      // Auto-refresh on session and patch/artifact events
      if (
        eventType === "session_started" ||
        eventType === "session_ended" ||
        eventType === "patch_created" ||
        eventType === "patch_applied" ||
        eventType === "patch_discarded" ||
        eventType === "artifact_created" ||
        eventType === "artifact_applied" ||
        eventType === "artifact_discarded"
      ) {
        fetchContent();
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [open, fetchContent]);

  // Handle artifact preview expansion
  const handlePreviewArtifact = useCallback(
    async (filename: string) => {
      if (!resolvedSessionId) return;

      if (expandedArtifact === filename) {
        setExpandedArtifact(null);
        setArtifactPreview(null);
        return;
      }

      setExpandedArtifact(filename);
      setArtifactPreview(null);

      try {
        const preview = await previewArtifact(resolvedSessionId, filename);
        setArtifactPreview(preview);
      } catch {
        setArtifactPreview("Failed to load preview");
      }
    },
    [resolvedSessionId, expandedArtifact]
  );

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
        <button
          type="button"
          onClick={() => setActiveTab("patches")}
          className={cn(
            "flex-1 px-3 py-1.5 text-xs font-medium transition-colors",
            activeTab === "patches"
              ? "text-foreground border-b-2 border-[var(--ansi-blue)]"
              : "text-muted-foreground hover:text-foreground"
          )}
        >
          <GitCommit className="w-3.5 h-3.5 inline mr-1" />
          Patches
          {stagedPatches.length + appliedPatches.length > 0 && (
            <span className="ml-1 text-[10px] bg-muted px-1 rounded">
              {stagedPatches.length + appliedPatches.length}
            </span>
          )}
        </button>
        <button
          type="button"
          onClick={() => setActiveTab("artifacts")}
          className={cn(
            "flex-1 px-3 py-1.5 text-xs font-medium transition-colors",
            activeTab === "artifacts"
              ? "text-foreground border-b-2 border-[var(--ansi-blue)]"
              : "text-muted-foreground hover:text-foreground"
          )}
        >
          <Package className="w-3.5 h-3.5 inline mr-1" />
          Artifacts
          {pendingArtifacts.length > 0 && (
            <span className="ml-1 text-[10px] bg-muted px-1 rounded">
              {pendingArtifacts.length}
            </span>
          )}
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-3">
        {error ? (
          <div className="text-[var(--ansi-red)] text-xs">{error}</div>
        ) : loading ? (
          <div className="text-muted-foreground text-xs animate-pulse">Loading...</div>
        ) : activeTab === "state" ? (
          <div className="text-xs [&_h1]:text-base [&_h2]:text-sm [&_h3]:text-xs [&_p]:text-xs [&_li]:text-xs [&_code]:text-[10px] [&_pre]:text-[10px]">
            <Markdown content={stateContent} />
          </div>
        ) : activeTab === "log" ? (
          <div className="text-xs [&_h1]:text-base [&_h2]:text-sm [&_h3]:text-xs [&_p]:text-xs [&_li]:text-xs [&_code]:text-[10px] [&_pre]:text-[10px]">
            <Markdown content={logContent} />
          </div>
        ) : activeTab === "patches" ? (
          <PatchesView
            staged={stagedPatches}
            applied={appliedPatches}
            expandedPatch={expandedPatch}
            onToggleExpand={(id) => setExpandedPatch(expandedPatch === id ? null : id)}
          />
        ) : (
          <ArtifactsView
            pending={pendingArtifacts}
            expandedArtifact={expandedArtifact}
            artifactPreview={artifactPreview}
            onToggleExpand={handlePreviewArtifact}
          />
        )}
      </div>

      {/* Footer */}
      <div className="px-3 py-1.5 border-t border-border text-[10px] text-muted-foreground">
        {activeTab === "state"
          ? "LLM-managed session state (state.md)"
          : activeTab === "log"
            ? "Append-only event log (log.md)"
            : activeTab === "patches"
              ? "Git patches from this session (staged & applied)"
              : "Generated documentation artifacts"}
      </div>
    </div>
  );
}

// ============================================================================
// PatchesView Component
// ============================================================================

interface PatchesViewProps {
  staged: StagedPatch[];
  applied: StagedPatch[];
  expandedPatch: number | null;
  onToggleExpand: (id: number) => void;
}

function PatchesView({ staged, applied, expandedPatch, onToggleExpand }: PatchesViewProps) {
  if (staged.length === 0 && applied.length === 0) {
    return <p className="text-xs text-muted-foreground">No patches generated yet.</p>;
  }

  return (
    <div className="space-y-2">
      {staged.length > 0 && (
        <>
          <h4 className="text-xs font-medium text-muted-foreground">Staged</h4>
          {staged.map((patch) => (
            <ReadOnlyPatchCard
              key={patch.meta.id}
              patch={patch}
              expanded={expandedPatch === patch.meta.id}
              onToggle={() => onToggleExpand(patch.meta.id)}
              status="staged"
            />
          ))}
        </>
      )}
      {applied.length > 0 && (
        <>
          <h4 className="text-xs font-medium text-muted-foreground mt-3">Applied</h4>
          {applied.map((patch) => (
            <ReadOnlyPatchCard
              key={patch.meta.id}
              patch={patch}
              expanded={expandedPatch === patch.meta.id}
              onToggle={() => onToggleExpand(patch.meta.id)}
              status="applied"
            />
          ))}
        </>
      )}
    </div>
  );
}

// ============================================================================
// ReadOnlyPatchCard Component
// ============================================================================

interface ReadOnlyPatchCardProps {
  patch: StagedPatch;
  expanded: boolean;
  onToggle: () => void;
  status: "staged" | "applied";
}

function ReadOnlyPatchCard({ patch, expanded, onToggle, status }: ReadOnlyPatchCardProps) {
  return (
    <div className="rounded border border-border bg-background/50">
      <button
        type="button"
        onClick={onToggle}
        className="w-full p-2 flex items-center gap-2 text-left"
      >
        {expanded ? (
          <ChevronDown className="w-3 h-3 text-muted-foreground shrink-0" />
        ) : (
          <ChevronRight className="w-3 h-3 text-muted-foreground shrink-0" />
        )}
        <GitCommit
          className={cn(
            "w-3 h-3 shrink-0",
            status === "applied" ? "text-[var(--ansi-green)]" : "text-[var(--ansi-yellow)]"
          )}
        />
        <div className="flex-1 min-w-0">
          <p className="text-xs font-mono truncate">{patch.subject}</p>
          <p className="text-[10px] text-muted-foreground">
            {patch.files.length} file{patch.files.length !== 1 ? "s" : ""} •{" "}
            {new Date(patch.meta.created_at).toLocaleTimeString()}
            {status === "applied" && patch.meta.applied_sha && (
              <span className="ml-1 font-mono">{patch.meta.applied_sha.slice(0, 7)}</span>
            )}
          </p>
        </div>
      </button>
      {expanded && (
        <div className="border-t border-border p-2 space-y-2">
          <div>
            <p className="text-[10px] text-muted-foreground mb-1">Files:</p>
            <div className="flex flex-wrap gap-1">
              {patch.files.map((file) => (
                <span
                  key={file}
                  className="text-[10px] font-mono bg-muted px-1 py-0.5 rounded flex items-center gap-1"
                >
                  <FileCode className="w-2.5 h-2.5" />
                  {file.split("/").pop()}
                </span>
              ))}
            </div>
          </div>
          {patch.message !== patch.subject && (
            <div>
              <p className="text-[10px] text-muted-foreground mb-1">Message:</p>
              <pre className="text-[10px] font-mono whitespace-pre-wrap bg-muted p-1.5 rounded max-h-32 overflow-auto">
                {patch.message}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ============================================================================
// ArtifactsView Component
// ============================================================================

interface ArtifactsViewProps {
  pending: Artifact[];
  expandedArtifact: string | null;
  artifactPreview: string | null;
  onToggleExpand: (filename: string) => void;
}

function ArtifactsView({
  pending,
  expandedArtifact,
  artifactPreview,
  onToggleExpand,
}: ArtifactsViewProps) {
  if (pending.length === 0) {
    return <p className="text-xs text-muted-foreground">No artifacts generated yet.</p>;
  }

  return (
    <div className="space-y-2">
      <h4 className="text-xs font-medium text-muted-foreground">Pending</h4>
      {pending.map((artifact) => (
        <ReadOnlyArtifactCard
          key={artifact.filename}
          artifact={artifact}
          expanded={expandedArtifact === artifact.filename}
          preview={expandedArtifact === artifact.filename ? artifactPreview : null}
          onToggle={() => onToggleExpand(artifact.filename)}
        />
      ))}
    </div>
  );
}

// ============================================================================
// ReadOnlyArtifactCard Component
// ============================================================================

interface ReadOnlyArtifactCardProps {
  artifact: Artifact;
  expanded: boolean;
  preview: string | null;
  onToggle: () => void;
}

function ReadOnlyArtifactCard({
  artifact,
  expanded,
  preview,
  onToggle,
}: ReadOnlyArtifactCardProps) {
  return (
    <div className="rounded border border-border bg-background/50">
      <button
        type="button"
        onClick={onToggle}
        className="w-full p-2 flex items-center gap-2 text-left"
      >
        {expanded ? (
          <ChevronDown className="w-3 h-3 text-muted-foreground shrink-0" />
        ) : (
          <ChevronRight className="w-3 h-3 text-muted-foreground shrink-0" />
        )}
        <Package className="w-3 h-3 text-[var(--ansi-cyan)] shrink-0" />
        <div className="flex-1 min-w-0">
          <p className="text-xs font-mono">{artifact.filename}</p>
          <p className="text-[10px] text-muted-foreground truncate">→ {artifact.meta.target}</p>
        </div>
      </button>
      {expanded && (
        <div className="border-t border-border p-2">
          <p className="text-[10px] text-muted-foreground mb-2">
            {artifact.meta.reason} • Based on {artifact.meta.based_on_patches.length} patch(es)
          </p>
          {preview ? (
            <pre className="text-[10px] font-mono whitespace-pre-wrap bg-muted p-1.5 rounded max-h-48 overflow-auto">
              {preview}
            </pre>
          ) : (
            <p className="text-[10px] text-muted-foreground animate-pulse">Loading preview...</p>
          )}
        </div>
      )}
    </div>
  );
}

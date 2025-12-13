import {
  Check,
  ChevronDown,
  ChevronRight,
  FileCode,
  FileText,
  GitCommit,
  Package,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { useSidecarEvents } from "@/hooks/useSidecarEvents";
import {
  type Artifact,
  applyAllArtifacts,
  applyAllPatches,
  applyArtifact,
  applyPatch,
  discardArtifact,
  discardPatch,
  getAppliedPatches,
  getCurrentSession,
  getPendingArtifacts,
  getStagedPatches,
  previewArtifact,
  type SidecarEventType,
  type StagedPatch,
} from "@/lib/sidecar";
import { cn } from "@/lib/utils";

interface SidecarPanelProps {
  /** Whether the panel is open */
  open: boolean;
  /** Callback when panel should close */
  onOpenChange: (open: boolean) => void;
}

type TabId = "patches" | "artifacts";

/**
 * Panel for managing sidecar patches and artifacts.
 * Allows viewing, applying, and discarding staged commits and generated files.
 */
export function SidecarPanel({ open, onOpenChange }: SidecarPanelProps) {
  const [activeTab, setActiveTab] = useState<TabId>("patches");
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Patches state
  const [stagedPatches, setStagedPatches] = useState<StagedPatch[]>([]);
  const [appliedPatches, setAppliedPatches] = useState<StagedPatch[]>([]);
  const [expandedPatches, setExpandedPatches] = useState<Set<number>>(new Set());

  // Artifacts state
  const [pendingArtifacts, setPendingArtifacts] = useState<Artifact[]>([]);
  const [expandedArtifacts, setExpandedArtifacts] = useState<Set<string>>(new Set());
  const [artifactPreviews, setArtifactPreviews] = useState<Map<string, string>>(new Map());

  // Fetch data when panel opens
  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);

    try {
      const sid = await getCurrentSession();
      setSessionId(sid);

      if (!sid) {
        setStagedPatches([]);
        setAppliedPatches([]);
        setPendingArtifacts([]);
        setLoading(false);
        return;
      }

      const [staged, applied, artifacts] = await Promise.all([
        getStagedPatches(sid).catch(() => []),
        getAppliedPatches(sid).catch(() => []),
        getPendingArtifacts(sid).catch(() => []),
      ]);

      setStagedPatches(staged);
      setAppliedPatches(applied);
      setPendingArtifacts(artifacts);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to fetch data");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (open) {
      fetchData();
    }
  }, [open, fetchData]);

  // Listen for sidecar events to auto-refresh
  const handleSidecarEvent = useCallback(
    (event: SidecarEventType) => {
      if (!open) return;

      // Refresh on relevant events
      if (
        event.event_type === "patch_created" ||
        event.event_type === "patch_applied" ||
        event.event_type === "patch_discarded" ||
        event.event_type === "artifact_created" ||
        event.event_type === "artifact_applied" ||
        event.event_type === "artifact_discarded"
      ) {
        fetchData();
      }
    },
    [open, fetchData]
  );

  useSidecarEvents(handleSidecarEvent);

  // Patch actions
  const handleApplyPatch = async (patchId: number) => {
    if (!sessionId) return;
    try {
      await applyPatch(sessionId, patchId);
      toast.success("Patch applied successfully");
      fetchData();
    } catch (e) {
      toast.error(`Failed to apply patch: ${e}`);
    }
  };

  const handleDiscardPatch = async (patchId: number) => {
    if (!sessionId) return;
    try {
      await discardPatch(sessionId, patchId);
      toast.success("Patch discarded");
      fetchData();
    } catch (e) {
      toast.error(`Failed to discard patch: ${e}`);
    }
  };

  const handleApplyAllPatches = async () => {
    if (!sessionId || stagedPatches.length === 0) return;
    try {
      const results = await applyAllPatches(sessionId);
      toast.success(`Applied ${results.length} patches`);
      fetchData();
    } catch (e) {
      toast.error(`Failed to apply patches: ${e}`);
    }
  };

  // Artifact actions
  const handleApplyArtifact = async (filename: string) => {
    if (!sessionId) return;
    try {
      await applyArtifact(sessionId, filename);
      toast.success(`Applied ${filename}`);
      fetchData();
    } catch (e) {
      toast.error(`Failed to apply artifact: ${e}`);
    }
  };

  const handleDiscardArtifact = async (filename: string) => {
    if (!sessionId) return;
    try {
      await discardArtifact(sessionId, filename);
      toast.success(`Discarded ${filename}`);
      fetchData();
    } catch (e) {
      toast.error(`Failed to discard artifact: ${e}`);
    }
  };

  const handleApplyAllArtifacts = async () => {
    if (!sessionId || pendingArtifacts.length === 0) return;
    try {
      const results = await applyAllArtifacts(sessionId);
      toast.success(`Applied ${results.length} artifacts`);
      fetchData();
    } catch (e) {
      toast.error(`Failed to apply artifacts: ${e}`);
    }
  };

  const handleTogglePatch = (id: number) => {
    setExpandedPatches((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const handleToggleArtifact = async (filename: string) => {
    if (!sessionId) return;

    // If already expanded, just collapse it
    if (expandedArtifacts.has(filename)) {
      setExpandedArtifacts((prev) => {
        const next = new Set(prev);
        next.delete(filename);
        return next;
      });
      return;
    }

    // Expand and fetch preview if not already cached
    try {
      if (!artifactPreviews.has(filename)) {
        const preview = await previewArtifact(sessionId, filename);
        setArtifactPreviews((prev) => new Map(prev).set(filename, preview));
      }
      setExpandedArtifacts((prev) => new Set(prev).add(filename));
    } catch (e) {
      toast.error(`Failed to preview: ${e}`);
    }
  };

  if (!open) return null;

  const panelContent = (
    <>
      {/* Backdrop */}
      <button
        type="button"
        aria-label="Close panel"
        className="fixed inset-0 bg-black/50 z-[9998] cursor-default"
        onClick={() => onOpenChange(false)}
        onKeyDown={(e) => e.key === "Escape" && onOpenChange(false)}
      />

      {/* Panel */}
      <div className="fixed right-0 top-0 bottom-0 w-[600px] max-w-[90vw] bg-card border-l border-border z-[9999] flex flex-col shadow-xl">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-border">
          <div className="flex items-center gap-2">
            <Package className="w-4 h-4 text-muted-foreground" />
            <h2 className="text-sm font-medium">Sidecar Manager</h2>
            {sessionId && (
              <span className="text-xs text-muted-foreground font-mono">
                {sessionId.slice(0, 8)}...
              </span>
            )}
          </div>
          <div className="flex items-center gap-1">
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={fetchData}
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
            onClick={() => setActiveTab("patches")}
            className={cn(
              "flex-1 px-4 py-2 text-sm font-medium transition-colors flex items-center justify-center gap-2",
              activeTab === "patches"
                ? "text-foreground border-b-2 border-[var(--ansi-blue)]"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            <GitCommit className="w-4 h-4" />
            Patches
            {stagedPatches.length > 0 && (
              <span className="bg-[var(--ansi-blue)] text-white text-xs px-1.5 py-0.5 rounded-full">
                {stagedPatches.length}
              </span>
            )}
          </button>
          <button
            type="button"
            onClick={() => setActiveTab("artifacts")}
            className={cn(
              "flex-1 px-4 py-2 text-sm font-medium transition-colors flex items-center justify-center gap-2",
              activeTab === "artifacts"
                ? "text-foreground border-b-2 border-[var(--ansi-blue)]"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            <FileText className="w-4 h-4" />
            Artifacts
            {pendingArtifacts.length > 0 && (
              <span className="bg-[var(--ansi-green)] text-white text-xs px-1.5 py-0.5 rounded-full">
                {pendingArtifacts.length}
              </span>
            )}
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-auto">
          {error ? (
            <div className="p-4 text-[var(--ansi-red)] text-sm">{error}</div>
          ) : loading ? (
            <div className="p-4 text-muted-foreground text-sm animate-pulse">Loading...</div>
          ) : !sessionId ? (
            <div className="p-4 text-muted-foreground text-sm">No active session</div>
          ) : activeTab === "patches" ? (
            <PatchesTab
              staged={stagedPatches}
              applied={appliedPatches}
              expandedPatches={expandedPatches}
              onToggleExpand={handleTogglePatch}
              onApply={handleApplyPatch}
              onDiscard={handleDiscardPatch}
              onApplyAll={handleApplyAllPatches}
            />
          ) : (
            <ArtifactsTab
              pending={pendingArtifacts}
              expandedArtifacts={expandedArtifacts}
              artifactPreviews={artifactPreviews}
              onToggleExpand={handleToggleArtifact}
              onApply={handleApplyArtifact}
              onDiscard={handleDiscardArtifact}
              onApplyAll={handleApplyAllArtifacts}
            />
          )}
        </div>
      </div>
    </>
  );

  // Use portal to render at document root, avoiding CSS containment issues
  return createPortal(panelContent, document.body);
}

// Patches Tab Component
interface PatchesTabProps {
  staged: StagedPatch[];
  applied: StagedPatch[];
  expandedPatches: Set<number>;
  onToggleExpand: (id: number) => void;
  onApply: (id: number) => void;
  onDiscard: (id: number) => void;
  onApplyAll: () => void;
}

function PatchesTab({
  staged,
  applied,
  expandedPatches,
  onToggleExpand,
  onApply,
  onDiscard,
  onApplyAll,
}: PatchesTabProps) {
  return (
    <div className="p-4 space-y-4">
      {/* Staged Patches */}
      <div>
        <div className="flex items-center justify-between mb-2">
          <h3 className="text-sm font-medium text-foreground">Staged Patches</h3>
          {staged.length > 0 && (
            <Button size="sm" variant="outline" onClick={onApplyAll}>
              <Check className="w-3 h-3 mr-1" />
              Apply All
            </Button>
          )}
        </div>
        {staged.length === 0 ? (
          <p className="text-sm text-muted-foreground">No staged patches</p>
        ) : (
          <div className="space-y-2">
            {staged.map((patch) => (
              <PatchCard
                key={patch.meta.id}
                patch={patch}
                expanded={expandedPatches.has(patch.meta.id)}
                onToggleExpand={() => onToggleExpand(patch.meta.id)}
                onApply={() => onApply(patch.meta.id)}
                onDiscard={() => onDiscard(patch.meta.id)}
              />
            ))}
          </div>
        )}
      </div>

      {/* Applied Patches */}
      {applied.length > 0 && (
        <div>
          <h3 className="text-sm font-medium text-muted-foreground mb-2">Applied Patches</h3>
          <div className="space-y-2 opacity-60">
            {applied.map((patch) => (
              <div
                key={patch.meta.id}
                className="p-3 rounded-md bg-muted/30 border border-border/50"
              >
                <div className="flex items-center gap-2">
                  <Check className="w-4 h-4 text-[var(--ansi-green)]" />
                  <span className="text-sm font-mono">{patch.subject}</span>
                  {patch.meta.applied_sha && (
                    <span className="text-xs text-muted-foreground font-mono ml-auto">
                      {patch.meta.applied_sha.slice(0, 7)}
                    </span>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// Patch Card Component
interface PatchCardProps {
  patch: StagedPatch;
  expanded: boolean;
  onToggleExpand: () => void;
  onApply: () => void;
  onDiscard: () => void;
}

function PatchCard({ patch, expanded, onToggleExpand, onApply, onDiscard }: PatchCardProps) {
  return (
    <div className="rounded-md border border-border bg-background/50">
      <div className="p-3 flex items-center gap-2">
        <button
          type="button"
          onClick={onToggleExpand}
          className="text-muted-foreground hover:text-foreground"
        >
          {expanded ? <ChevronDown className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
        </button>
        <GitCommit className="w-4 h-4 text-[var(--ansi-yellow)]" />
        <div className="flex-1 min-w-0">
          <p className="text-sm font-mono truncate">{patch.subject}</p>
          <p className="text-xs text-muted-foreground">
            {patch.files.length} file{patch.files.length !== 1 ? "s" : ""} •{" "}
            {new Date(patch.meta.created_at).toLocaleTimeString()}
          </p>
        </div>
        <div className="flex items-center gap-1">
          <Button size="icon" variant="ghost" className="h-7 w-7" onClick={onApply} title="Apply">
            <Check className="w-4 h-4 text-[var(--ansi-green)]" />
          </Button>
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7"
            onClick={onDiscard}
            title="Discard"
          >
            <Trash2 className="w-4 h-4 text-[var(--ansi-red)]" />
          </Button>
        </div>
      </div>
      {expanded && (
        <div className="border-t border-border p-3 space-y-2">
          <div>
            <p className="text-xs text-muted-foreground mb-1">Files:</p>
            <div className="flex flex-wrap gap-1">
              {patch.files.map((file) => (
                <span
                  key={file}
                  className="text-xs font-mono bg-muted px-1.5 py-0.5 rounded flex items-center gap-1"
                >
                  <FileCode className="w-3 h-3" />
                  {file.split("/").pop()}
                </span>
              ))}
            </div>
          </div>
          {patch.message !== patch.subject && (
            <div>
              <p className="text-xs text-muted-foreground mb-1">Message:</p>
              <pre className="text-xs font-mono whitespace-pre-wrap bg-muted p-2 rounded">
                {patch.message}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// Artifacts Tab Component
interface ArtifactsTabProps {
  pending: Artifact[];
  expandedArtifacts: Set<string>;
  artifactPreviews: Map<string, string>;
  onToggleExpand: (filename: string) => void;
  onApply: (filename: string) => void;
  onDiscard: (filename: string) => void;
  onApplyAll: () => void;
}

function ArtifactsTab({
  pending,
  expandedArtifacts,
  artifactPreviews,
  onToggleExpand,
  onApply,
  onDiscard,
  onApplyAll,
}: ArtifactsTabProps) {
  return (
    <div className="p-4 space-y-4">
      <div className="flex items-center justify-between mb-2">
        <h3 className="text-sm font-medium text-foreground">Pending Artifacts</h3>
        {pending.length > 0 && (
          <Button size="sm" variant="outline" onClick={onApplyAll}>
            <Check className="w-3 h-3 mr-1" />
            Apply All
          </Button>
        )}
      </div>
      {pending.length === 0 ? (
        <p className="text-sm text-muted-foreground">No pending artifacts</p>
      ) : (
        <div className="space-y-2">
          {pending.map((artifact) => (
            <ArtifactCard
              key={artifact.filename}
              artifact={artifact}
              expanded={expandedArtifacts.has(artifact.filename)}
              preview={artifactPreviews.get(artifact.filename) ?? null}
              onToggleExpand={() => onToggleExpand(artifact.filename)}
              onApply={() => onApply(artifact.filename)}
              onDiscard={() => onDiscard(artifact.filename)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

// Artifact Card Component
interface ArtifactCardProps {
  artifact: Artifact;
  expanded: boolean;
  preview: string | null;
  onToggleExpand: () => void;
  onApply: () => void;
  onDiscard: () => void;
}

function ArtifactCard({
  artifact,
  expanded,
  preview,
  onToggleExpand,
  onApply,
  onDiscard,
}: ArtifactCardProps) {
  return (
    <div className="rounded-md border border-border bg-background/50">
      <div className="p-3 flex items-center gap-2">
        <button
          type="button"
          onClick={onToggleExpand}
          className="text-muted-foreground hover:text-foreground"
        >
          {expanded ? <ChevronDown className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
        </button>
        <FileText className="w-4 h-4 text-[var(--ansi-cyan)]" />
        <div className="flex-1 min-w-0">
          <p className="text-sm font-mono">{artifact.filename}</p>
          <p className="text-xs text-muted-foreground truncate">→ {artifact.meta.target}</p>
        </div>
        <div className="flex items-center gap-1">
          <Button size="icon" variant="ghost" className="h-7 w-7" onClick={onApply} title="Apply">
            <Check className="w-4 h-4 text-[var(--ansi-green)]" />
          </Button>
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7"
            onClick={onDiscard}
            title="Discard"
          >
            <Trash2 className="w-4 h-4 text-[var(--ansi-red)]" />
          </Button>
        </div>
      </div>
      {expanded && (
        <div className="border-t border-border p-3">
          <p className="text-xs text-muted-foreground mb-2">
            {artifact.meta.reason} • Based on {artifact.meta.based_on_patches.length} patch(es)
          </p>
          {preview ? (
            <pre className="text-xs font-mono whitespace-pre-wrap bg-muted p-2 rounded max-h-64 overflow-auto">
              {preview}
            </pre>
          ) : (
            <p className="text-xs text-muted-foreground">Loading preview...</p>
          )}
        </div>
      )}
    </div>
  );
}

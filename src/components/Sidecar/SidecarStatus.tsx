import { Database, Download, HardDrive, Loader2, Zap } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import {
  downloadModels,
  getModelsStatus,
  getSidecarStatus,
  type ModelsStatus,
  type SidecarStatus as SidecarStatusType,
} from "@/lib/sidecar";
import { cn } from "@/lib/utils";

interface SidecarStatusProps {
  className?: string;
  showDetails?: boolean;
}

export function SidecarStatus({ className, showDetails = false }: SidecarStatusProps) {
  const [status, setStatus] = useState<SidecarStatusType | null>(null);
  const [modelsStatus, setModelsStatus] = useState<ModelsStatus | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Poll for status updates
  useEffect(() => {
    let mounted = true;

    const fetchStatus = async () => {
      try {
        const [sidecar, models] = await Promise.all([getSidecarStatus(), getModelsStatus()]);
        if (mounted) {
          setStatus(sidecar);
          setModelsStatus(models);
          setError(null);
        }
      } catch (e) {
        if (mounted) {
          setError(e instanceof Error ? e.message : "Failed to get sidecar status");
        }
      }
    };

    fetchStatus();
    const interval = setInterval(fetchStatus, 5000); // Poll every 5 seconds

    return () => {
      mounted = false;
      clearInterval(interval);
    };
  }, []);

  const handleDownloadModels = async () => {
    setDownloading(true);
    try {
      await downloadModels();
      // Refresh status after download
      const [sidecar, models] = await Promise.all([getSidecarStatus(), getModelsStatus()]);
      setStatus(sidecar);
      setModelsStatus(models);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to download models");
    } finally {
      setDownloading(false);
    }
  };

  if (error && !status) {
    return <div className={cn("text-[#565f89] text-xs", className)}>Sidecar unavailable</div>;
  }

  if (!status) {
    return <div className={cn("text-[#565f89] text-xs animate-pulse", className)}>Loading...</div>;
  }

  const isReady = status.storage_ready;

  // Compact view for status bar
  if (!showDetails) {
    return (
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <div
              className={cn(
                "flex items-center gap-1.5 h-7 px-2 rounded-md cursor-default",
                isReady ? "bg-[#9ece6a]/10 text-[#9ece6a]" : "bg-[#565f89]/10 text-[#565f89]",
                className
              )}
            >
              <Database className="w-3.5 h-3.5" />
              {status.active_session && (
                <span className="w-1.5 h-1.5 rounded-full bg-[#9ece6a] animate-pulse" />
              )}
            </div>
          </TooltipTrigger>
          <TooltipContent side="top" className="bg-[#1f2335] border-[#3b4261] text-[#c0caf5]">
            <div className="text-xs space-y-1">
              <div>Sidecar: {isReady ? "Ready" : "Not initialized"}</div>
              {status.active_session && <div>Session: {status.event_count} events captured</div>}
              {modelsStatus && (
                <div>
                  Models: {modelsStatus.embedding_available ? "Embeddings " : ""}
                  {modelsStatus.llm_available ? "LLM" : ""}
                  {!modelsStatus.embedding_available && !modelsStatus.llm_available && "None"}
                </div>
              )}
            </div>
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    );
  }

  // Detailed view for sidebar/panel
  return (
    <div className={cn("p-3 space-y-3 text-sm", className)}>
      {/* Status header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Database className={cn("w-4 h-4", isReady ? "text-[#9ece6a]" : "text-[#565f89]")} />
          <span className="text-[#c0caf5] font-medium">Context Capture</span>
        </div>
        <span
          className={cn(
            "text-xs px-2 py-0.5 rounded",
            isReady ? "bg-[#9ece6a]/10 text-[#9ece6a]" : "bg-[#565f89]/10 text-[#565f89]"
          )}
        >
          {isReady ? "Active" : "Inactive"}
        </span>
      </div>

      {/* Session info */}
      {status.active_session && (
        <div className="bg-[#1f2335] rounded-md p-2 space-y-1">
          <div className="flex items-center gap-2 text-[#bb9af7]">
            <Zap className="w-3.5 h-3.5" />
            <span>Active Session</span>
          </div>
          <div className="text-xs text-[#565f89] pl-5">
            {status.event_count} events captured
            {status.buffer_size > 0 && ` (${status.buffer_size} buffered)`}
          </div>
        </div>
      )}

      {/* Storage info */}
      {isReady && (
        <div className="flex items-center gap-2 text-xs text-[#565f89]">
          <HardDrive className="w-3.5 h-3.5" />
          <span>Storage: {status.workspace_path ? "Initialized" : "Pending"}</span>
        </div>
      )}

      {/* Models status */}
      <div className="space-y-2">
        <div className="text-xs text-[#565f89]">AI Models</div>
        <div className="grid grid-cols-2 gap-2">
          <div
            className={cn(
              "text-xs px-2 py-1.5 rounded bg-[#1f2335] flex items-center gap-1.5",
              modelsStatus?.embedding_available ? "text-[#9ece6a]" : "text-[#565f89]"
            )}
          >
            <span
              className={cn(
                "w-1.5 h-1.5 rounded-full",
                modelsStatus?.embedding_available ? "bg-[#9ece6a]" : "bg-[#565f89]"
              )}
            />
            Embeddings
          </div>
          <div
            className={cn(
              "text-xs px-2 py-1.5 rounded bg-[#1f2335] flex items-center gap-1.5",
              modelsStatus?.llm_available ? "text-[#9ece6a]" : "text-[#565f89]"
            )}
          >
            <span
              className={cn(
                "w-1.5 h-1.5 rounded-full",
                modelsStatus?.llm_available ? "bg-[#9ece6a]" : "bg-[#565f89]"
              )}
            />
            LLM
          </div>
        </div>

        {/* Download button if embeddings missing (needed for semantic search) */}
        {modelsStatus && !modelsStatus.embedding_available && (
          <Button
            variant="outline"
            size="sm"
            onClick={handleDownloadModels}
            disabled={downloading}
            className="w-full h-8 text-xs bg-[#1f2335] border-[#3b4261] hover:bg-[#292e42] text-[#c0caf5]"
          >
            {downloading ? (
              <>
                <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                Downloading...
              </>
            ) : (
              <>
                <Download className="w-3.5 h-3.5 mr-1.5" />
                Download Embedding Model (~90MB)
              </>
            )}
          </Button>
        )}
      </div>

      {/* Error display */}
      {error && <div className="text-xs text-[#f7768e] bg-[#f7768e]/10 rounded p-2">{error}</div>}
    </div>
  );
}

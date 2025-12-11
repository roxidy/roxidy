import { Database } from "lucide-react";
import { useEffect, useState } from "react";
import { getSidecarStatus, type SidecarStatus as SidecarStatusType } from "@/lib/sidecar";
import { cn } from "@/lib/utils";

/**
 * Minimal status indicator for the sidecar context capture system.
 * Shows whether context capture is active for the current session.
 */
export function SidecarStatus() {
  const [status, setStatus] = useState<SidecarStatusType | null>(null);

  useEffect(() => {
    // Initial fetch
    getSidecarStatus()
      .then(setStatus)
      .catch((e) => console.warn("Failed to get sidecar status:", e));

    // Poll every 5 seconds for status updates
    const interval = setInterval(() => {
      getSidecarStatus()
        .then(setStatus)
        .catch((e) => console.warn("Failed to get sidecar status:", e));
    }, 5000);

    return () => clearInterval(interval);
  }, []);

  if (!status) {
    return null;
  }

  const isCapturing = status.enabled && status.active_session;

  return (
    <div
      className={cn(
        "h-6 px-2 gap-1.5 text-xs font-normal rounded-md flex items-center",
        isCapturing
          ? "bg-[var(--ansi-green)]/10 text-[var(--ansi-green)]"
          : "bg-muted-foreground/10 text-muted-foreground"
      )}
      title={
        isCapturing
          ? `Capturing session: ${status.session_id}`
          : status.enabled
            ? "Capture ready"
            : "Capture disabled"
      }
    >
      <Database className={cn("w-3.5 h-3.5", isCapturing && "animate-pulse")} />
      <span className="hidden sm:inline">{isCapturing ? "Capturing" : "Capture"}</span>
    </div>
  );
}

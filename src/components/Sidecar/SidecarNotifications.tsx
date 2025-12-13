import { useCallback } from "react";
import { toast } from "sonner";
import { useSidecarEvents } from "@/hooks/useSidecarEvents";
import type { SidecarEventType } from "@/lib/sidecar";

/**
 * Component that subscribes to sidecar events and displays toast notifications.
 * This component renders nothing - it only handles event subscriptions.
 */
export function SidecarNotifications() {
  const handleEvent = useCallback((event: SidecarEventType) => {
    switch (event.event_type) {
      // Session events
      case "session_started":
        toast.info("Sidecar session started", {
          description: `Session: ${event.session_id.slice(0, 8)}...`,
        });
        break;

      case "session_ended":
        toast.info("Sidecar session ended", {
          description: `Session: ${event.session_id.slice(0, 8)}...`,
        });
        break;

      // Patch events
      case "patch_created":
        toast.success("Patch created", {
          description: event.subject,
        });
        break;

      case "patch_applied":
        toast.success("Patch applied", {
          description: `Commit: ${event.commit_sha.slice(0, 7)}`,
        });
        break;

      case "patch_discarded":
        toast.info("Patch discarded", {
          description: `Patch #${event.patch_id}`,
        });
        break;

      case "patch_message_updated":
        toast.info("Patch message updated", {
          description: event.new_subject,
        });
        break;

      // Artifact events
      case "artifact_created":
        toast.success("Artifact generated", {
          description: `${event.filename} → ${event.target}`,
        });
        break;

      case "artifact_applied":
        toast.success("Artifact applied", {
          description: `${event.filename} written to ${event.target}`,
        });
        break;

      case "artifact_discarded":
        toast.info("Artifact discarded", {
          description: event.filename,
        });
        break;

      // State events
      case "state_updated":
        toast.success("Session state updated", {
          description: `state.md synthesized via ${event.backend}`,
        });
        break;

      default: {
        // TypeScript exhaustiveness check
        const _exhaustive: never = event;
        console.warn("Unknown sidecar event:", _exhaustive);
      }
    }
  }, []);

  useSidecarEvents(handleEvent);

  // This component renders nothing
  return null;
}

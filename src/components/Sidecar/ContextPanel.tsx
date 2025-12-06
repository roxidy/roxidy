import { Database } from "lucide-react";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { CommitDraft } from "./CommitDraft";
import { SessionHistory } from "./SessionHistory";
import { SidecarStatus } from "./SidecarStatus";

interface ContextPanelProps {
  sessionId?: string;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}

export function ContextPanel({ sessionId, open, onOpenChange }: ContextPanelProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="!max-w-none w-[1100px] max-w-[95vw] h-[85vh] flex flex-col bg-[#1a1b26] border-[#3b4261] p-0">
        <DialogHeader className="px-4 py-3 border-b border-[#3b4261] flex-shrink-0">
          <DialogTitle className="flex items-center gap-2 text-[#c0caf5]">
            <Database className="w-4 h-4 text-[#bb9af7]" />
            Context Capture
          </DialogTitle>
        </DialogHeader>

        <div className="flex-1 overflow-hidden flex flex-col min-h-0">
          {/* Status section */}
          <div className="px-4 py-3 border-b border-[#3b4261] flex-shrink-0">
            <SidecarStatus showDetails />
          </div>

          {/* Commit Draft section */}
          <div className="px-4 py-3 border-b border-[#3b4261] flex-shrink-0">
            <CommitDraft sessionId={sessionId} />
          </div>

          {/* Session History - takes remaining space */}
          <div className="flex-1 min-h-0 overflow-hidden">
            <SessionHistory className="h-full" />
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

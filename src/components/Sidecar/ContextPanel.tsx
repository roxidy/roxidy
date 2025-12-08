import { Activity, Database, GitCommit, History, X } from "lucide-react";
import { useState } from "react";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import { CommitDraft } from "./CommitDraft";
import { SessionHistory } from "./SessionHistory";
import { SidecarStatus } from "./SidecarStatus";

interface ContextPanelProps {
  sessionId?: string;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}

type PanelSection = "status" | "commit" | "history";

interface NavItem {
  id: PanelSection;
  label: string;
  icon: React.ReactNode;
  description: string;
}

const NAV_ITEMS: NavItem[] = [
  {
    id: "status",
    label: "Status",
    icon: <Activity className="w-4 h-4" />,
    description: "Capture status and models",
  },
  {
    id: "commit",
    label: "Commit Draft",
    icon: <GitCommit className="w-4 h-4" />,
    description: "Generate commit messages",
  },
  {
    id: "history",
    label: "Session History",
    icon: <History className="w-4 h-4" />,
    description: "Browse past sessions",
  },
];

export function ContextPanel({ sessionId, open, onOpenChange }: ContextPanelProps) {
  const [activeSection, setActiveSection] = useState<PanelSection>("commit");

  const handleClose = () => {
    onOpenChange?.(false);
  };

  const renderContent = () => {
    switch (activeSection) {
      case "status":
        return <SidecarStatus showDetails />;
      case "commit":
        return <CommitDraft sessionId={sessionId} />;
      case "history":
        return <SessionHistory className="h-full" />;
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={false}
        className="!max-w-none !top-0 !left-0 !right-0 !bottom-0 !translate-x-0 !translate-y-0 !w-full !h-full p-0 bg-[#1a1b26] border-0 rounded-none text-[#c0caf5] flex flex-col"
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-[#3b4261] flex-shrink-0">
          <div className="flex items-center gap-2">
            <Database className="w-5 h-5 text-[#bb9af7]" />
            <h2 className="text-lg font-semibold text-[#c0caf5]">Context Capture</h2>
          </div>
          <button
            type="button"
            onClick={handleClose}
            className="p-1.5 rounded-md hover:bg-[#292e42] text-[#565f89] hover:text-[#c0caf5] transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        <div className="flex-1 flex min-h-0">
          {/* Sidebar Navigation */}
          <nav className="w-64 border-r border-[#3b4261] flex flex-col flex-shrink-0">
            <div className="flex-1 py-2">
              {NAV_ITEMS.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => setActiveSection(item.id)}
                  className={cn(
                    "w-full flex items-start gap-3 px-4 py-3 text-left transition-colors",
                    activeSection === item.id
                      ? "bg-[#292e42] text-[#c0caf5] border-l-2 border-[#bb9af7]"
                      : "text-[#565f89] hover:bg-[#1f2335] hover:text-[#c0caf5] border-l-2 border-transparent"
                  )}
                >
                  <span className={cn("mt-0.5", activeSection === item.id ? "text-[#bb9af7]" : "")}>
                    {item.icon}
                  </span>
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium">{item.label}</div>
                    <div className="text-xs text-[#565f89] mt-0.5">{item.description}</div>
                  </div>
                </button>
              ))}
            </div>
          </nav>

          {/* Main Content */}
          <div className="flex-1 flex flex-col min-w-0">
            <ScrollArea className="flex-1">
              <div className="p-6">{renderContent()}</div>
            </ScrollArea>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

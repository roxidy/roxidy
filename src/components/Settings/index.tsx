import { Bot, Cog, Loader2, Shield, Terminal, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { getSettings, type QbitSettings, updateSettings } from "@/lib/settings";
import { cn } from "@/lib/utils";
import { AdvancedSettings } from "./AdvancedSettings";
import { AgentSettings } from "./AgentSettings";
import { AiSettings } from "./AiSettings";
import { TerminalSettings } from "./TerminalSettings";

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

type SettingsSection = "ai" | "terminal" | "agent" | "advanced";

interface NavItem {
  id: SettingsSection;
  label: string;
  icon: React.ReactNode;
  description: string;
}

const NAV_ITEMS: NavItem[] = [
  {
    id: "ai",
    label: "AI & Providers",
    icon: <Bot className="w-4 h-4" />,
    description: "Configure AI providers and synthesis",
  },
  {
    id: "terminal",
    label: "Terminal",
    icon: <Terminal className="w-4 h-4" />,
    description: "Shell and display settings",
  },
  {
    id: "agent",
    label: "Agent",
    icon: <Cog className="w-4 h-4" />,
    description: "Session and approval settings",
  },
  {
    id: "advanced",
    label: "Advanced",
    icon: <Shield className="w-4 h-4" />,
    description: "Privacy and debug options",
  },
];

export function SettingsDialog({ open, onOpenChange }: SettingsDialogProps) {
  const [settings, setSettings] = useState<QbitSettings | null>(null);
  const [activeSection, setActiveSection] = useState<SettingsSection>("ai");
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

  // Load settings when dialog opens
  useEffect(() => {
    if (open) {
      setIsLoading(true);
      getSettings()
        .then(setSettings)
        .catch((err) => {
          console.error("Failed to load settings:", err);
          toast.error("Failed to load settings");
        })
        .finally(() => setIsLoading(false));
    }
  }, [open]);

  const handleSave = useCallback(async () => {
    if (!settings) return;

    setIsSaving(true);
    try {
      await updateSettings(settings);
      toast.success("Settings saved");
      onOpenChange(false);
    } catch (err) {
      console.error("Failed to save settings:", err);
      toast.error("Failed to save settings");
    } finally {
      setIsSaving(false);
    }
  }, [settings, onOpenChange]);

  const handleCancel = useCallback(() => {
    onOpenChange(false);
  }, [onOpenChange]);

  // Handler to update a specific section of settings
  const updateSection = useCallback(
    <K extends keyof QbitSettings>(section: K, value: QbitSettings[K]) => {
      setSettings((prev) => (prev ? { ...prev, [section]: value } : null));
    },
    []
  );

  const renderContent = () => {
    if (!settings) return null;

    switch (activeSection) {
      case "ai":
        return (
          <AiSettings
            settings={settings.ai}
            apiKeys={settings.api_keys}
            sidecarSettings={settings.sidecar}
            onChange={(ai) => updateSection("ai", ai)}
            onApiKeysChange={(keys) => updateSection("api_keys", keys)}
            onSidecarChange={(sidecar) => updateSection("sidecar", sidecar)}
          />
        );
      case "terminal":
        return (
          <TerminalSettings
            settings={settings.terminal}
            onChange={(terminal) => updateSection("terminal", terminal)}
          />
        );
      case "agent":
        return (
          <AgentSettings
            settings={settings.agent}
            onChange={(agent) => updateSection("agent", agent)}
          />
        );
      case "advanced":
        return (
          <AdvancedSettings
            settings={settings.advanced}
            privacy={settings.privacy}
            onChange={(advanced) => updateSection("advanced", advanced)}
            onPrivacyChange={(privacy) => updateSection("privacy", privacy)}
          />
        );
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={false}
        className="!max-w-none !inset-0 !translate-x-0 !translate-y-0 !w-screen !h-screen p-0 bg-[#1a1b26] border-0 rounded-none text-[#c0caf5] flex flex-col overflow-hidden"
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-[#3b4261] flex-shrink-0">
          <h2 className="text-lg font-semibold text-[#c0caf5]">Settings</h2>
          <button
            type="button"
            onClick={handleCancel}
            className="p-1.5 rounded-md hover:bg-[#292e42] text-[#565f89] hover:text-[#c0caf5] transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {isLoading ? (
          <div className="flex-1 flex items-center justify-center">
            <Loader2 className="w-6 h-6 text-[#565f89] animate-spin" />
          </div>
        ) : settings ? (
          <div className="flex-1 flex min-h-0 overflow-hidden">
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
                        ? "bg-[#292e42] text-[#c0caf5] border-l-2 border-[#7aa2f7]"
                        : "text-[#565f89] hover:bg-[#1f2335] hover:text-[#c0caf5] border-l-2 border-transparent"
                    )}
                  >
                    <span
                      className={cn("mt-0.5", activeSection === item.id ? "text-[#7aa2f7]" : "")}
                    >
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
            <div className="flex-1 flex flex-col min-w-0 min-h-0 overflow-hidden">
              <ScrollArea className="h-full">
                <div className="p-6 max-w-3xl">{renderContent()}</div>
              </ScrollArea>
            </div>
          </div>
        ) : (
          <div className="flex-1 flex items-center justify-center">
            <span className="text-[#f7768e]">Failed to load settings</span>
          </div>
        )}

        {/* Footer */}
        <div className="flex items-center justify-end gap-3 px-6 py-4 border-t border-[#3b4261] flex-shrink-0">
          <Button
            variant="outline"
            onClick={handleCancel}
            className="bg-transparent border-[#3b4261] text-[#c0caf5] hover:bg-[#1f2335]"
          >
            Cancel
          </Button>
          <Button
            onClick={handleSave}
            disabled={!settings || isSaving}
            className="bg-[#7aa2f7] text-[#1a1b26] hover:bg-[#7aa2f7]/80"
          >
            {isSaving ? "Saving..." : "Save Changes"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

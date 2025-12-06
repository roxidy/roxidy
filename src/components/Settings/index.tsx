import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { getSettings, type QbitSettings, updateSettings } from "@/lib/settings";
import { AdvancedSettings } from "./AdvancedSettings";
import { AgentSettings } from "./AgentSettings";
import { AiSettings } from "./AiSettings";
import { TerminalSettings } from "./TerminalSettings";

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function SettingsDialog({ open, onOpenChange }: SettingsDialogProps) {
  const [settings, setSettings] = useState<QbitSettings | null>(null);
  const [activeTab, setActiveTab] = useState("ai");
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

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl h-[85vh] flex flex-col bg-[#1a1b26] border-[#3b4261] text-[#c0caf5]">
        <DialogHeader className="flex-shrink-0">
          <DialogTitle className="text-[#c0caf5]">Settings</DialogTitle>
        </DialogHeader>

        {isLoading ? (
          <div className="flex-1 flex items-center justify-center">
            <span className="text-[#565f89]">Loading settings...</span>
          </div>
        ) : settings ? (
          <Tabs
            value={activeTab}
            onValueChange={setActiveTab}
            className="flex-1 flex flex-col min-h-0"
          >
            <TabsList className="flex-shrink-0 grid w-full grid-cols-4 bg-[#1f2335]">
              <TabsTrigger
                value="ai"
                className="data-[state=active]:bg-[#3b4261] data-[state=active]:text-[#c0caf5]"
              >
                AI
              </TabsTrigger>
              <TabsTrigger
                value="terminal"
                className="data-[state=active]:bg-[#3b4261] data-[state=active]:text-[#c0caf5]"
              >
                Terminal
              </TabsTrigger>
              <TabsTrigger
                value="agent"
                className="data-[state=active]:bg-[#3b4261] data-[state=active]:text-[#c0caf5]"
              >
                Agent
              </TabsTrigger>
              <TabsTrigger
                value="advanced"
                className="data-[state=active]:bg-[#3b4261] data-[state=active]:text-[#c0caf5]"
              >
                Advanced
              </TabsTrigger>
            </TabsList>

            <div className="flex-1 min-h-0 mt-4 overflow-hidden">
              <ScrollArea className="h-full pr-4">
                <TabsContent value="ai" className="mt-0">
                  <AiSettings
                    settings={settings.ai}
                    apiKeys={settings.api_keys}
                    onChange={(ai) => updateSection("ai", ai)}
                    onApiKeysChange={(keys) => updateSection("api_keys", keys)}
                  />
                </TabsContent>

                <TabsContent value="terminal" className="mt-0">
                  <TerminalSettings
                    settings={settings.terminal}
                    onChange={(terminal) => updateSection("terminal", terminal)}
                  />
                </TabsContent>

                <TabsContent value="agent" className="mt-0">
                  <AgentSettings
                    settings={settings.agent}
                    onChange={(agent) => updateSection("agent", agent)}
                  />
                </TabsContent>

                <TabsContent value="advanced" className="mt-0">
                  <AdvancedSettings
                    settings={settings.advanced}
                    privacy={settings.privacy}
                    onChange={(advanced) => updateSection("advanced", advanced)}
                    onPrivacyChange={(privacy) => updateSection("privacy", privacy)}
                  />
                </TabsContent>
              </ScrollArea>
            </div>
          </Tabs>
        ) : (
          <div className="flex-1 flex items-center justify-center">
            <span className="text-[#f7768e]">Failed to load settings</span>
          </div>
        )}

        <DialogFooter className="flex-shrink-0 gap-2 pt-4 border-t border-[#3b4261]">
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
            {isSaving ? "Saving..." : "Save"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

import { Input } from "@/components/ui/input";
import type { TerminalSettings as TerminalSettingsType } from "@/lib/settings";
import { ThemePicker } from "./ThemePicker";

interface TerminalSettingsProps {
  settings: TerminalSettingsType;
  onChange: (settings: TerminalSettingsType) => void;
}

export function TerminalSettings({ settings, onChange }: TerminalSettingsProps) {
  const updateField = <K extends keyof TerminalSettingsType>(
    key: K,
    value: TerminalSettingsType[K]
  ) => {
    onChange({ ...settings, [key]: value });
  };

  return (
    <div className="space-y-6">
      {/* Theme */}
      <div className="space-y-2">
        <h3 className="text-sm font-medium text-foreground mb-4">Theme</h3>
        <ThemePicker />
      </div>

      {/* Divider */}
      <div className="border-t border-border" />

      {/* Shell */}
      <div className="space-y-2">
        <label htmlFor="terminal-shell" className="text-sm font-medium text-foreground">
          Shell
        </label>
        <Input
          id="terminal-shell"
          value={settings.shell || ""}
          onChange={(e) => updateField("shell", e.target.value || null)}
          placeholder="Auto-detect from environment"
        />
        <p className="text-xs text-muted-foreground">
          Override the default shell. Leave empty to auto-detect.
        </p>
      </div>

      {/* Font Family */}
      <div className="space-y-2">
        <label htmlFor="terminal-font-family" className="text-sm font-medium text-foreground">
          Font Family
        </label>
        <Input
          id="terminal-font-family"
          value={settings.font_family}
          onChange={(e) => updateField("font_family", e.target.value)}
          placeholder="JetBrains Mono"
        />
        <p className="text-xs text-muted-foreground">Monospace font for the terminal</p>
      </div>

      {/* Font Size */}
      <div className="space-y-2">
        <label htmlFor="terminal-font-size" className="text-sm font-medium text-foreground">
          Font Size
        </label>
        <Input
          id="terminal-font-size"
          type="number"
          min={8}
          max={32}
          value={settings.font_size}
          onChange={(e) => updateField("font_size", parseInt(e.target.value, 10) || 14)}
          className="w-24"
        />
        <p className="text-xs text-muted-foreground">Font size in pixels (8-32)</p>
      </div>

      {/* Scrollback */}
      <div className="space-y-2">
        <label htmlFor="terminal-scrollback" className="text-sm font-medium text-foreground">
          Scrollback Lines
        </label>
        <Input
          id="terminal-scrollback"
          type="number"
          min={1000}
          max={100000}
          step={1000}
          value={settings.scrollback}
          onChange={(e) => updateField("scrollback", parseInt(e.target.value, 10) || 10000)}
          className="w-32"
        />
        <p className="text-xs text-muted-foreground">
          Number of lines to keep in scrollback buffer
        </p>
      </div>
    </div>
  );
}

import { Input } from "@/components/ui/input";
import type { TerminalSettings as TerminalSettingsType } from "@/lib/settings";

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
      {/* Shell */}
      <div className="space-y-2">
        <label htmlFor="terminal-shell" className="text-sm font-medium text-[#c0caf5]">
          Shell
        </label>
        <Input
          id="terminal-shell"
          value={settings.shell || ""}
          onChange={(e) => updateField("shell", e.target.value || null)}
          placeholder="Auto-detect from environment"
          className="bg-[#1f2335] border-[#3b4261] text-[#c0caf5]"
        />
        <p className="text-xs text-[#565f89]">
          Override the default shell. Leave empty to auto-detect.
        </p>
      </div>

      {/* Font Family */}
      <div className="space-y-2">
        <label htmlFor="terminal-font-family" className="text-sm font-medium text-[#c0caf5]">
          Font Family
        </label>
        <Input
          id="terminal-font-family"
          value={settings.font_family}
          onChange={(e) => updateField("font_family", e.target.value)}
          placeholder="JetBrains Mono"
          className="bg-[#1f2335] border-[#3b4261] text-[#c0caf5]"
        />
        <p className="text-xs text-[#565f89]">Monospace font for the terminal</p>
      </div>

      {/* Font Size */}
      <div className="space-y-2">
        <label htmlFor="terminal-font-size" className="text-sm font-medium text-[#c0caf5]">
          Font Size
        </label>
        <Input
          id="terminal-font-size"
          type="number"
          min={8}
          max={32}
          value={settings.font_size}
          onChange={(e) => updateField("font_size", parseInt(e.target.value, 10) || 14)}
          className="bg-[#1f2335] border-[#3b4261] text-[#c0caf5] w-24"
        />
        <p className="text-xs text-[#565f89]">Font size in pixels (8-32)</p>
      </div>

      {/* Scrollback */}
      <div className="space-y-2">
        <label htmlFor="terminal-scrollback" className="text-sm font-medium text-[#c0caf5]">
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
          className="bg-[#1f2335] border-[#3b4261] text-[#c0caf5] w-32"
        />
        <p className="text-xs text-[#565f89]">Number of lines to keep in scrollback buffer</p>
      </div>
    </div>
  );
}

import { Switch } from "@/components/ui/switch";
import type { AdvancedSettings as AdvancedSettingsType, PrivacySettings } from "@/lib/settings";

interface AdvancedSettingsProps {
  settings: AdvancedSettingsType;
  privacy: PrivacySettings;
  onChange: (settings: AdvancedSettingsType) => void;
  onPrivacyChange: (privacy: PrivacySettings) => void;
}

function SimpleSelect({
  id,
  value,
  onValueChange,
  options,
}: {
  id?: string;
  value: string;
  onValueChange: (value: string) => void;
  options: { value: string; label: string }[];
}) {
  return (
    <select
      id={id}
      value={value}
      onChange={(e) => onValueChange(e.target.value)}
      className="w-full h-9 rounded-md border border-input bg-card px-3 py-1 text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-ring cursor-pointer appearance-none"
      style={{
        backgroundImage:
          "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%23565f89' stroke-width='2'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E\")",
        backgroundRepeat: "no-repeat",
        backgroundPosition: "right 12px center",
      }}
    >
      {options.map((opt) => (
        <option key={opt.value} value={opt.value} className="bg-card">
          {opt.label}
        </option>
      ))}
    </select>
  );
}

export function AdvancedSettings({
  settings,
  privacy,
  onChange,
  onPrivacyChange,
}: AdvancedSettingsProps) {
  const logLevelOptions = [
    { value: "error", label: "Error" },
    { value: "warn", label: "Warn" },
    { value: "info", label: "Info" },
    { value: "debug", label: "Debug" },
    { value: "trace", label: "Trace" },
  ];

  return (
    <div className="space-y-6">
      {/* Log Level */}
      <div className="space-y-2">
        <label htmlFor="advanced-log-level" className="text-sm font-medium text-foreground">
          Log Level
        </label>
        <SimpleSelect
          id="advanced-log-level"
          value={settings.log_level}
          onValueChange={(value) =>
            onChange({ ...settings, log_level: value as AdvancedSettingsType["log_level"] })
          }
          options={logLevelOptions}
        />
        <p className="text-xs text-muted-foreground">Verbosity of debug logging</p>
      </div>

      {/* Experimental Features */}
      <div className="flex items-center justify-between">
        <div className="space-y-1">
          <label htmlFor="advanced-experimental" className="text-sm font-medium text-foreground">
            Experimental Features
          </label>
          <p className="text-xs text-muted-foreground">Enable experimental functionality</p>
        </div>
        <Switch
          id="advanced-experimental"
          checked={settings.enable_experimental}
          onCheckedChange={(checked) => onChange({ ...settings, enable_experimental: checked })}
        />
      </div>

      {/* Privacy Section */}
      <div className="space-y-4 p-4 rounded-lg bg-card border border-border">
        <h4 className="text-sm font-medium text-[var(--ansi-blue)]">Privacy</h4>

        {/* Usage Statistics */}
        <div className="flex items-center justify-between">
          <div className="space-y-1">
            <label htmlFor="privacy-usage-stats" className="text-sm text-foreground">
              Usage Statistics
            </label>
            <p className="text-xs text-muted-foreground">Send anonymous usage data</p>
          </div>
          <Switch
            id="privacy-usage-stats"
            checked={privacy.usage_statistics}
            onCheckedChange={(checked) =>
              onPrivacyChange({ ...privacy, usage_statistics: checked })
            }
          />
        </div>

        {/* Log Prompts */}
        <div className="flex items-center justify-between">
          <div className="space-y-1">
            <label htmlFor="privacy-log-prompts" className="text-sm text-foreground">
              Log Prompts
            </label>
            <p className="text-xs text-muted-foreground">Save prompts locally for debugging</p>
          </div>
          <Switch
            id="privacy-log-prompts"
            checked={privacy.log_prompts}
            onCheckedChange={(checked) => onPrivacyChange({ ...privacy, log_prompts: checked })}
          />
        </div>
      </div>
    </div>
  );
}

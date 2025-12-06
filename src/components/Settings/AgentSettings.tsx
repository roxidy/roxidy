import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import type { AgentSettings as AgentSettingsType } from "@/lib/settings";

interface AgentSettingsProps {
  settings: AgentSettingsType;
  onChange: (settings: AgentSettingsType) => void;
}

export function AgentSettings({ settings, onChange }: AgentSettingsProps) {
  const updateField = <K extends keyof AgentSettingsType>(key: K, value: AgentSettingsType[K]) => {
    onChange({ ...settings, [key]: value });
  };

  return (
    <div className="space-y-6">
      {/* Session Persistence */}
      <div className="flex items-center justify-between">
        <div className="space-y-1">
          <label htmlFor="agent-session-persistence" className="text-sm font-medium text-[#c0caf5]">
            Session Persistence
          </label>
          <p className="text-xs text-[#565f89]">Auto-save conversations to disk</p>
        </div>
        <Switch
          id="agent-session-persistence"
          checked={settings.session_persistence}
          onCheckedChange={(checked) => updateField("session_persistence", checked)}
        />
      </div>

      {/* Session Retention */}
      <div className="space-y-2">
        <label htmlFor="agent-session-retention" className="text-sm font-medium text-[#c0caf5]">
          Session Retention (days)
        </label>
        <Input
          id="agent-session-retention"
          type="number"
          min={0}
          max={365}
          value={settings.session_retention_days}
          onChange={(e) => updateField("session_retention_days", parseInt(e.target.value, 10) || 0)}
          className="bg-[#1f2335] border-[#3b4261] text-[#c0caf5] w-24"
        />
        <p className="text-xs text-[#565f89]">How long to keep saved sessions (0 = forever)</p>
      </div>

      {/* Pattern Learning */}
      <div className="flex items-center justify-between">
        <div className="space-y-1">
          <label htmlFor="agent-pattern-learning" className="text-sm font-medium text-[#c0caf5]">
            Pattern Learning
          </label>
          <p className="text-xs text-[#565f89]">Learn from approvals for auto-approval</p>
        </div>
        <Switch
          id="agent-pattern-learning"
          checked={settings.pattern_learning}
          onCheckedChange={(checked) => updateField("pattern_learning", checked)}
        />
      </div>

      {/* Min Approvals */}
      <div className="space-y-2">
        <label htmlFor="agent-min-approvals" className="text-sm font-medium text-[#c0caf5]">
          Minimum Approvals
        </label>
        <Input
          id="agent-min-approvals"
          type="number"
          min={1}
          max={10}
          value={settings.min_approvals_for_auto}
          onChange={(e) => updateField("min_approvals_for_auto", parseInt(e.target.value, 10) || 3)}
          className="bg-[#1f2335] border-[#3b4261] text-[#c0caf5] w-24"
        />
        <p className="text-xs text-[#565f89]">
          Minimum approvals before a tool can be auto-approved
        </p>
      </div>

      {/* Approval Threshold */}
      <div className="space-y-2">
        <label htmlFor="agent-approval-threshold" className="text-sm font-medium text-[#c0caf5]">
          Approval Threshold: {(settings.approval_threshold * 100).toFixed(0)}%
        </label>
        <input
          id="agent-approval-threshold"
          type="range"
          min={0}
          max={100}
          value={settings.approval_threshold * 100}
          onChange={(e) => updateField("approval_threshold", parseInt(e.target.value, 10) / 100)}
          className="w-full h-2 bg-[#1f2335] rounded-lg appearance-none cursor-pointer accent-[#7aa2f7]"
        />
        <p className="text-xs text-[#565f89]">Required approval rate for auto-approval</p>
      </div>
    </div>
  );
}

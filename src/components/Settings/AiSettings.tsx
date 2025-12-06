import { Input } from "@/components/ui/input";
import type { AiSettings as AiSettingsType, ApiKeysSettings } from "@/lib/settings";

interface AiSettingsProps {
  settings: AiSettingsType;
  apiKeys: ApiKeysSettings;
  onChange: (settings: AiSettingsType) => void;
  onApiKeysChange: (keys: ApiKeysSettings) => void;
}

// Simple Select component using native select for now
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
      className="w-full h-9 rounded-md border border-[#3b4261] bg-[#1f2335] px-3 py-1 text-sm text-[#c0caf5] focus:outline-none focus:ring-1 focus:ring-[#7aa2f7] cursor-pointer appearance-none"
      style={{
        backgroundImage:
          "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%23565f89' stroke-width='2'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E\")",
        backgroundRepeat: "no-repeat",
        backgroundPosition: "right 12px center",
      }}
    >
      {options.map((opt) => (
        <option key={opt.value} value={opt.value} className="bg-[#1f2335]">
          {opt.label}
        </option>
      ))}
    </select>
  );
}

export function AiSettings({ settings, apiKeys, onChange, onApiKeysChange }: AiSettingsProps) {
  const updateField = <K extends keyof AiSettingsType>(key: K, value: AiSettingsType[K]) => {
    onChange({ ...settings, [key]: value });
  };

  const updateVertexAi = (field: string, value: string | null) => {
    onChange({
      ...settings,
      vertex_ai: { ...settings.vertex_ai, [field]: value || null },
    });
  };

  const updateOpenRouter = (field: string, value: string | null) => {
    onChange({
      ...settings,
      openrouter: { ...settings.openrouter, [field]: value || null },
    });
  };

  const providerOptions = [
    { value: "vertex_ai", label: "Vertex AI (Anthropic)" },
    { value: "openrouter", label: "OpenRouter" },
    { value: "anthropic", label: "Anthropic" },
    { value: "openai", label: "OpenAI" },
    { value: "ollama", label: "Ollama (Local)" },
  ];

  return (
    <div className="space-y-6">
      {/* Default Provider */}
      <div className="space-y-2">
        <label htmlFor="ai-default-provider" className="text-sm font-medium text-[#c0caf5]">
          Default Provider
        </label>
        <SimpleSelect
          id="ai-default-provider"
          value={settings.default_provider}
          onValueChange={(value) =>
            updateField("default_provider", value as AiSettingsType["default_provider"])
          }
          options={providerOptions}
        />
        <p className="text-xs text-[#565f89]">The AI provider to use for conversations</p>
      </div>

      {/* Default Model */}
      <div className="space-y-2">
        <label htmlFor="ai-default-model" className="text-sm font-medium text-[#c0caf5]">
          Default Model
        </label>
        <Input
          id="ai-default-model"
          value={settings.default_model}
          onChange={(e) => updateField("default_model", e.target.value)}
          placeholder="claude-opus-4-5@20251101"
          className="bg-[#1f2335] border-[#3b4261] text-[#c0caf5]"
        />
        <p className="text-xs text-[#565f89]">Model identifier for the selected provider</p>
      </div>

      {/* Vertex AI Settings */}
      {settings.default_provider === "vertex_ai" && (
        <div className="space-y-4 p-4 rounded-lg bg-[#1f2335] border border-[#3b4261]">
          <h4 className="text-sm font-medium text-[#7aa2f7]">Vertex AI Configuration</h4>

          <div className="space-y-2">
            <label htmlFor="vertex-credentials-path" className="text-sm text-[#c0caf5]">
              Credentials Path
            </label>
            <Input
              id="vertex-credentials-path"
              value={settings.vertex_ai.credentials_path || ""}
              onChange={(e) => updateVertexAi("credentials_path", e.target.value)}
              placeholder="/path/to/service-account.json"
              className="bg-[#1a1b26] border-[#3b4261] text-[#c0caf5]"
            />
          </div>

          <div className="space-y-2">
            <label htmlFor="vertex-project-id" className="text-sm text-[#c0caf5]">
              Project ID
            </label>
            <Input
              id="vertex-project-id"
              value={settings.vertex_ai.project_id || ""}
              onChange={(e) => updateVertexAi("project_id", e.target.value)}
              placeholder="your-project-id"
              className="bg-[#1a1b26] border-[#3b4261] text-[#c0caf5]"
            />
          </div>

          <div className="space-y-2">
            <label htmlFor="vertex-location" className="text-sm text-[#c0caf5]">
              Location
            </label>
            <Input
              id="vertex-location"
              value={settings.vertex_ai.location || ""}
              onChange={(e) => updateVertexAi("location", e.target.value)}
              placeholder="us-east5"
              className="bg-[#1a1b26] border-[#3b4261] text-[#c0caf5]"
            />
          </div>
        </div>
      )}

      {/* OpenRouter Settings */}
      {settings.default_provider === "openrouter" && (
        <div className="space-y-4 p-4 rounded-lg bg-[#1f2335] border border-[#3b4261]">
          <h4 className="text-sm font-medium text-[#7aa2f7]">OpenRouter Configuration</h4>

          <div className="space-y-2">
            <label htmlFor="openrouter-api-key" className="text-sm text-[#c0caf5]">
              API Key
            </label>
            <Input
              id="openrouter-api-key"
              type="password"
              value={settings.openrouter.api_key || ""}
              onChange={(e) => updateOpenRouter("api_key", e.target.value)}
              placeholder="sk-or-..."
              className="bg-[#1a1b26] border-[#3b4261] text-[#c0caf5]"
            />
            <p className="text-xs text-[#565f89]">
              Use $OPENROUTER_API_KEY to reference an environment variable
            </p>
          </div>
        </div>
      )}

      {/* API Keys */}
      <div className="space-y-4 p-4 rounded-lg bg-[#1f2335] border border-[#3b4261]">
        <h4 className="text-sm font-medium text-[#7aa2f7]">API Keys</h4>

        <div className="space-y-2">
          <label htmlFor="api-key-tavily" className="text-sm text-[#c0caf5]">
            Tavily (Web Search)
          </label>
          <Input
            id="api-key-tavily"
            type="password"
            value={apiKeys.tavily || ""}
            onChange={(e) => onApiKeysChange({ ...apiKeys, tavily: e.target.value || null })}
            placeholder="tvly-..."
            className="bg-[#1a1b26] border-[#3b4261] text-[#c0caf5]"
          />
          <p className="text-xs text-[#565f89]">
            Use $TAVILY_API_KEY to reference an environment variable
          </p>
        </div>
      </div>
    </div>
  );
}

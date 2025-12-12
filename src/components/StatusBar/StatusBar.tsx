import { Bot, ChevronDown, Cloud, Cpu, Terminal } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { SidecarStatus } from "@/components/Sidecar";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { getOpenRouterApiKey, initAiAgent, initVertexAiAgent, VERTEX_AI_MODELS } from "@/lib/ai";
import { cn } from "@/lib/utils";
import { useAiConfig, useInputMode, useStore } from "../../store";

// Available Vertex AI models
const VERTEX_MODELS = [
  { id: VERTEX_AI_MODELS.CLAUDE_OPUS_4_5, name: "Claude Opus 4.5", provider: "vertex" as const },
  {
    id: VERTEX_AI_MODELS.CLAUDE_SONNET_4_5,
    name: "Claude Sonnet 4.5",
    provider: "vertex" as const,
  },
  { id: VERTEX_AI_MODELS.CLAUDE_HAIKU_4_5, name: "Claude Haiku 4.5", provider: "vertex" as const },
];

// Available OpenRouter models (fixed list per spec)
const OPENROUTER_MODELS = [
  { id: "mistralai/devstral-2512", name: "Devstral 2512", provider: "openrouter" as const },
  { id: "deepseek/deepseek-v3.2", name: "Deepseek v3.2", provider: "openrouter" as const },
  { id: "z-ai/glm-4.6", name: "GLM 4.6", provider: "openrouter" as const },
];

function formatModel(model: string): string {
  if (!model) return "No Model";

  // Check Vertex AI models
  if (model.includes("claude-opus-4")) return "Claude Opus 4.5";
  if (model.includes("claude-sonnet-4-5")) return "Claude Sonnet 4.5";
  if (model.includes("claude-haiku-4-5")) return "Claude Haiku 4.5";

  // Check OpenRouter models
  const openRouterModel = OPENROUTER_MODELS.find((m) => m.id === model);
  if (openRouterModel) return openRouterModel.name;

  return model;
}

function formatProvider(provider: string): string {
  switch (provider) {
    case "anthropic_vertex":
      return "Vertex AI";
    case "openrouter":
      return "OpenRouter";
    case "openai":
      return "OpenAI";
    case "anthropic":
      return "Anthropic";
    case "gemini":
      return "Gemini";
    default:
      return provider || "None";
  }
}

interface StatusBarProps {
  sessionId: string | null;
}

export function StatusBar({ sessionId }: StatusBarProps) {
  const aiConfig = useAiConfig();
  const { provider, model, status, errorMessage } = aiConfig;
  const inputMode = useInputMode(sessionId ?? "");
  const setInputMode = useStore((state) => state.setInputMode);
  const setAiConfig = useStore((state) => state.setAiConfig);

  // Track OpenRouter availability
  const [openRouterEnabled, setOpenRouterEnabled] = useState(false);
  const [openRouterApiKey, setOpenRouterApiKey] = useState<string | null>(null);

  // Check for OpenRouter API key on mount and when dropdown opens
  const refreshOpenRouterKey = useCallback(async () => {
    try {
      const key = await getOpenRouterApiKey();
      setOpenRouterApiKey(key);
      setOpenRouterEnabled(!!key);
    } catch (e) {
      console.warn("Failed to get OpenRouter API key:", e);
      setOpenRouterEnabled(false);
    }
  }, []);

  useEffect(() => {
    refreshOpenRouterKey();
  }, [refreshOpenRouterKey]);

  const handleModelSelect = async (modelId: string, modelProvider: "vertex" | "openrouter") => {
    // Don't switch if already on this model
    if (
      model === modelId &&
      provider === (modelProvider === "vertex" ? "anthropic_vertex" : "openrouter")
    ) {
      return;
    }

    const allModels = [...VERTEX_MODELS, ...OPENROUTER_MODELS];
    const modelName = allModels.find((m) => m.id === modelId)?.name ?? modelId;

    try {
      setAiConfig({ status: "initializing", model: modelId });

      if (modelProvider === "vertex") {
        // Vertex AI model switch
        if (!aiConfig.vertexConfig) {
          throw new Error("Vertex AI configuration not available");
        }
        const { vertexConfig } = aiConfig;
        await initVertexAiAgent({
          workspace: vertexConfig.workspace,
          credentialsPath: vertexConfig.credentialsPath,
          projectId: vertexConfig.projectId,
          location: vertexConfig.location,
          model: modelId,
        });
        setAiConfig({ status: "ready", provider: "anthropic_vertex" });
      } else {
        // OpenRouter model switch
        const apiKey = openRouterApiKey ?? (await getOpenRouterApiKey());
        if (!apiKey) {
          throw new Error("OpenRouter API key not configured");
        }
        // Get workspace from vertexConfig or use current directory
        const workspace = aiConfig.vertexConfig?.workspace ?? ".";
        await initAiAgent({
          workspace,
          provider: "openrouter",
          model: modelId,
          apiKey,
        });
        setAiConfig({ status: "ready", provider: "openrouter" });
      }

      toast.success(`Switched to ${modelName}`, {
        style: {
          background: "var(--card)",
          color: "var(--ansi-magenta)",
          border: "1px solid var(--border)",
        },
      });
    } catch (error) {
      console.error("Failed to switch model:", error);
      setAiConfig({
        status: "error",
        errorMessage: error instanceof Error ? error.message : "Failed to switch model",
      });
      toast.error(`Failed to switch to ${modelName}`);
    }
  };

  return (
    <div className="h-9 bg-muted/20 backdrop-blur-sm border-t border-border/50 flex items-center justify-between px-3 text-xs text-muted-foreground relative z-10">
      {/* Left side */}
      <div className="flex items-center gap-3">
        {/* Mode segmented control - icons only */}
        <div className="flex items-center h-6 rounded-md bg-card p-1 border border-border">
          <button
            type="button"
            onClick={() => sessionId && setInputMode(sessionId, "terminal")}
            disabled={!sessionId}
            className={cn(
              "h-5 w-7 flex items-center justify-center rounded transition-colors",
              inputMode === "terminal"
                ? "bg-[var(--ansi-blue)] text-background"
                : "text-muted-foreground hover:text-[var(--ansi-blue)]"
            )}
          >
            <Terminal className="w-4 h-4" />
          </button>
          <button
            type="button"
            onClick={() => sessionId && setInputMode(sessionId, "agent")}
            disabled={!sessionId}
            className={cn(
              "h-5 w-7 flex items-center justify-center rounded transition-colors",
              inputMode === "agent"
                ? "bg-[var(--ansi-magenta)] text-background"
                : "text-muted-foreground hover:text-[var(--ansi-magenta)]"
            )}
          >
            <Bot className="w-4 h-4" />
          </button>
        </div>

        {/* Model selector badge or Terminal Mode indicator */}
        {inputMode === "terminal" ? (
          <div className="h-6 px-2.5 gap-1.5 text-xs font-normal rounded-md bg-[var(--ansi-blue)]/10 text-[var(--ansi-blue)] flex items-center">
            <Terminal className="w-4 h-4" />
            <span>Terminal Mode</span>
          </div>
        ) : status === "disconnected" ? (
          <div className="h-6 px-2.5 gap-1.5 text-xs font-normal rounded-md bg-muted-foreground/10 text-muted-foreground flex items-center">
            <Cpu className="w-4 h-4" />
            <span>AI Disconnected</span>
          </div>
        ) : status === "error" ? (
          <div className="h-6 px-2.5 gap-1.5 text-xs font-normal rounded-md bg-[var(--ansi-red)]/10 text-[var(--ansi-red)] flex items-center">
            <Cpu className="w-4 h-4" />
            <span>AI Error</span>
          </div>
        ) : status === "initializing" ? (
          <div className="h-6 px-2.5 gap-1.5 text-xs font-normal rounded-md bg-[var(--ansi-yellow)]/10 text-[var(--ansi-yellow)] flex items-center">
            <Cpu className="w-4 h-4 animate-pulse" />
            <span>Initializing...</span>
          </div>
        ) : (
          <DropdownMenu onOpenChange={(open) => open && refreshOpenRouterKey()}>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="sm"
                className="h-6 px-2.5 gap-1.5 text-xs font-normal rounded-md bg-[var(--ansi-magenta)]/10 text-[var(--ansi-magenta)] hover:bg-[var(--ansi-magenta)]/20 hover:text-[var(--ansi-magenta)]"
              >
                <Cpu className="w-4 h-4" />
                <span>{formatModel(model)}</span>
                <ChevronDown className="w-4 h-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="bg-card border-border min-w-[200px]">
              {/* Vertex AI Models */}
              <div className="px-2 py-1 text-[10px] text-muted-foreground uppercase tracking-wide">
                Vertex AI
              </div>
              {VERTEX_MODELS.map((m) => (
                <DropdownMenuItem
                  key={m.id}
                  onClick={() => handleModelSelect(m.id, "vertex")}
                  disabled={!aiConfig.vertexConfig}
                  className={cn(
                    "text-xs cursor-pointer",
                    model === m.id && provider === "anthropic_vertex"
                      ? "text-[var(--ansi-magenta)] bg-[var(--ansi-magenta)]/10"
                      : "text-foreground hover:text-[var(--ansi-magenta)]"
                  )}
                >
                  {m.name}
                </DropdownMenuItem>
              ))}

              {/* OpenRouter Models (only shown when API key is configured) */}
              {openRouterEnabled && (
                <>
                  <DropdownMenuSeparator />
                  <div className="px-2 py-1 text-[10px] text-muted-foreground uppercase tracking-wide">
                    OpenRouter
                  </div>
                  {OPENROUTER_MODELS.map((m) => (
                    <DropdownMenuItem
                      key={m.id}
                      onClick={() => handleModelSelect(m.id, "openrouter")}
                      className={cn(
                        "text-xs cursor-pointer",
                        model === m.id && provider === "openrouter"
                          ? "text-[var(--ansi-magenta)] bg-[var(--ansi-magenta)]/10"
                          : "text-foreground hover:text-[var(--ansi-magenta)]"
                      )}
                    >
                      {m.name}
                    </DropdownMenuItem>
                  ))}
                </>
              )}
            </DropdownMenuContent>
          </DropdownMenu>
        )}
      </div>

      {/* Right side - Sidecar + Provider */}
      <div className="flex items-center gap-2">
        {status === "error" && errorMessage && (
          <span className="text-[var(--ansi-red)] truncate max-w-[200px]">({errorMessage})</span>
        )}

        {/* Sidecar status indicator */}
        <SidecarStatus />

        <div
          className={cn(
            "h-6 px-2.5 gap-1.5 text-xs font-normal rounded-md flex items-center",
            "bg-card text-foreground"
          )}
        >
          <Cloud className="w-4 h-4 text-muted-foreground" />
          <span>{formatProvider(provider)}</span>
        </div>
      </div>
    </div>
  );
}

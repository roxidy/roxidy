import { Bot, ChevronDown, Cloud, Cpu, Terminal } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { initVertexAiAgent, VERTEX_AI_MODELS } from "@/lib/ai";
import { cn } from "@/lib/utils";
import { useAiConfig, useInputMode, useStore } from "../../store";

// Available models for the dropdown
const AVAILABLE_MODELS = [
  { id: VERTEX_AI_MODELS.CLAUDE_OPUS_4_5, name: "Claude Opus 4.5" },
  { id: VERTEX_AI_MODELS.CLAUDE_SONNET_4_5, name: "Claude Sonnet 4.5" },
  { id: VERTEX_AI_MODELS.CLAUDE_HAIKU_4_5, name: "Claude Haiku 4.5" },
];

function formatModel(model: string): string {
  // Simplify Vertex AI model names
  if (model.includes("claude-opus-4")) return "Claude Opus 4.5";
  if (model.includes("claude-sonnet-4-5")) return "Claude Sonnet 4.5";
  if (model.includes("claude-haiku-4-5")) return "Claude Haiku 4.5";
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

  const handleModelSelect = async (modelId: string) => {
    // Don't switch if already on this model or no vertex config
    if (model === modelId || !aiConfig.vertexConfig) {
      return;
    }

    const { vertexConfig } = aiConfig;
    const modelName = AVAILABLE_MODELS.find((m) => m.id === modelId)?.name ?? modelId;

    try {
      setAiConfig({ status: "initializing", model: modelId });

      await initVertexAiAgent({
        workspace: vertexConfig.workspace,
        credentialsPath: vertexConfig.credentialsPath,
        projectId: vertexConfig.projectId,
        location: vertexConfig.location,
        model: modelId,
      });

      setAiConfig({ status: "ready" });
      toast.success(`Switched to ${modelName}`, {
        style: {
          background: "#1f2335",
          color: "#bb9af7",
          border: "1px solid #3b4261",
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
    <div className="h-9 bg-[#16161e] border-t border-[#27293d] flex items-center justify-between px-3 text-sm text-[#565f89] relative z-10">
      {/* Left side */}
      <div className="flex items-center gap-3">
        {/* Mode segmented control - icons only */}
        <div className="flex items-center h-7 rounded-md bg-[#1f2335] p-1 border border-[#3b4261]">
          <button
            type="button"
            onClick={() => sessionId && setInputMode(sessionId, "terminal")}
            disabled={!sessionId}
            className={cn(
              "h-5 w-7 flex items-center justify-center rounded transition-colors",
              inputMode === "terminal"
                ? "bg-[#7aa2f7] text-[#1a1b26]"
                : "text-[#565f89] hover:text-[#7aa2f7]"
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
                ? "bg-[#bb9af7] text-[#1a1b26]"
                : "text-[#565f89] hover:text-[#bb9af7]"
            )}
          >
            <Bot className="w-4 h-4" />
          </button>
        </div>

        {/* Model selector badge */}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="sm"
              className="h-7 px-2.5 gap-1.5 text-sm font-normal rounded-md bg-[#bb9af7]/10 text-[#bb9af7] hover:bg-[#bb9af7]/20 hover:text-[#bb9af7]"
            >
              <Cpu className="w-4 h-4" />
              <span>{formatModel(model)}</span>
              <ChevronDown className="w-4 h-4" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent
            align="start"
            className="bg-[#1f2335] border-[#3b4261] min-w-[180px]"
          >
            {AVAILABLE_MODELS.map((m) => (
              <DropdownMenuItem
                key={m.id}
                onClick={() => handleModelSelect(m.id)}
                className={cn(
                  "text-sm cursor-pointer",
                  model === m.id
                    ? "text-[#bb9af7] bg-[#bb9af7]/10"
                    : "text-[#c0caf5] hover:text-[#bb9af7]"
                )}
              >
                {m.name}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {/* Right side - Provider */}
      <div className="flex items-center gap-2">
        {status === "error" && errorMessage && (
          <span className="text-[#f7768e] truncate max-w-[200px]">({errorMessage})</span>
        )}

        <div
          className={cn(
            "h-7 px-2.5 gap-1.5 text-sm font-normal rounded-md flex items-center",
            "bg-[#1f2335] text-[#c0caf5]"
          )}
        >
          <Cloud className="w-4 h-4 text-[#565f89]" />
          <span>{formatProvider(provider)}</span>
        </div>
      </div>
    </div>
  );
}

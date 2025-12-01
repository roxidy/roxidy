import {
  AlertTriangle,
  CheckCircle,
  ChevronDown,
  ChevronUp,
  Info,
  Shield,
  ShieldCheck,
  Terminal,
  XCircle,
  Zap,
} from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { type ApprovalPattern, calculateApprovalRate, respondToToolApproval } from "@/lib/ai";
import { getRiskLevel, isDangerousTool } from "@/lib/tools";
import type { RiskLevel } from "@/store";
import { cn } from "@/lib/utils";
import { usePendingToolApproval, useStore } from "@/store";

interface ToolApprovalDialogProps {
  sessionId: string;
}

// Risk level styling
const RISK_STYLES: Record<RiskLevel, { color: string; bg: string; icon: typeof Shield }> = {
  low: { color: "text-[#9ece6a]", bg: "bg-[#9ece6a]/10", icon: ShieldCheck },
  medium: { color: "text-[#7aa2f7]", bg: "bg-[#7aa2f7]/10", icon: Shield },
  high: { color: "text-[#e0af68]", bg: "bg-[#e0af68]/10", icon: AlertTriangle },
  critical: { color: "text-[#f7768e]", bg: "bg-[#f7768e]/10", icon: AlertTriangle },
};

function ApprovalStats({ stats }: { stats: ApprovalPattern }) {
  const rate = calculateApprovalRate(stats);
  const ratePercent = Math.round(rate * 100);

  return (
    <div className="flex items-center gap-3 text-xs text-[#565f89]">
      <div className="flex items-center gap-1">
        <CheckCircle className="w-3 h-3 text-[#9ece6a]" />
        <span>{stats.approvals}</span>
      </div>
      <div className="flex items-center gap-1">
        <XCircle className="w-3 h-3 text-[#f7768e]" />
        <span>{stats.denials}</span>
      </div>
      <div className="h-3 w-px bg-[#27293d]" />
      <span className={rate >= 0.8 ? "text-[#9ece6a]" : ""}>{ratePercent}% approval rate</span>
    </div>
  );
}

export function ToolApprovalDialog({ sessionId }: ToolApprovalDialogProps) {
  const tool = usePendingToolApproval(sessionId);
  const setPendingToolApproval = useStore((state) => state.setPendingToolApproval);
  const updateToolCallStatus = useStore((state) => state.updateToolCallStatus);
  const markToolRequestProcessed = useStore((state) => state.markToolRequestProcessed);

  const [alwaysAllow, setAlwaysAllow] = useState(false);
  const [showArgs, setShowArgs] = useState(true);

  if (!tool) return null;

  // Get risk level - use provided or calculate from tool name
  const riskLevel = tool.riskLevel ?? getRiskLevel(tool.name);
  const { stats, suggestion } = tool;
  const canLearn = tool.canLearn ?? true;
  const isDangerous = isDangerousTool(tool.name, riskLevel);
  const RiskIcon = RISK_STYLES[riskLevel].icon;

  const handleApprove = async () => {
    // Mark as processed before clearing to prevent duplicate events from re-showing dialog
    markToolRequestProcessed(tool.id);
    setPendingToolApproval(sessionId, null);
    updateToolCallStatus(sessionId, tool.id, "running");

    // Send approval decision to backend - the backend will execute the tool
    // and emit a tool_result event when complete. We don't execute here to avoid
    // double execution since execute_with_hitl is waiting for this approval.
    try {
      await respondToToolApproval({
        request_id: tool.id,
        approved: true,
        remember: true,
        always_allow: alwaysAllow,
      });
    } catch (error) {
      console.warn("Failed to send approval:", error);
      toast.error("Failed to approve tool execution");
      updateToolCallStatus(sessionId, tool.id, "error", "Failed to send approval");
    }
  };

  const handleDeny = async () => {
    // Mark as processed before clearing to prevent duplicate events from re-showing dialog
    markToolRequestProcessed(tool.id);
    setPendingToolApproval(sessionId, null);
    updateToolCallStatus(sessionId, tool.id, "denied");

    // Send denial decision to backend for pattern learning
    try {
      await respondToToolApproval({
        request_id: tool.id,
        approved: false,
        remember: true,
        always_allow: false,
      });
    } catch (error) {
      console.warn("Failed to record denial:", error);
    }
  };

  const argsString = JSON.stringify(tool.args, null, 2);
  const hasArgs = Object.keys(tool.args).length > 0;

  return (
    <Dialog open={true} onOpenChange={() => handleDeny()}>
      <DialogContent className="bg-[#1f2335] border-[#27293d] text-[#c0caf5] max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-[#c0caf5]">
            <RiskIcon className={cn("w-5 h-5", RISK_STYLES[riskLevel].color)} />
            Tool Approval Required
          </DialogTitle>
          <DialogDescription className="text-[#565f89]">
            The AI assistant wants to execute the following tool. Review the details before
            approving.
          </DialogDescription>
        </DialogHeader>

        <div className="bg-[#16161e] rounded-md p-4 border border-[#27293d]">
          {/* Tool name and risk badge */}
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-2">
              <Terminal className="w-4 h-4 text-[#7aa2f7]" />
              <span className="font-mono text-sm text-[#c0caf5] font-medium">{tool.name}</span>
            </div>
            <span
              className={cn(
                "text-xs px-2 py-0.5 rounded-full capitalize",
                RISK_STYLES[riskLevel].bg,
                RISK_STYLES[riskLevel].color
              )}
            >
              {riskLevel}
            </span>
          </div>

          {/* Approval stats (if available) */}
          {stats && stats.total_requests > 0 && (
            <div className="mb-3 pb-3 border-b border-[#27293d]">
              <ApprovalStats stats={stats} />
            </div>
          )}

          {/* Arguments (collapsible) */}
          {hasArgs && (
            <>
              <button
                type="button"
                onClick={() => setShowArgs(!showArgs)}
                className="flex items-center gap-1 text-xs text-[#565f89] mb-2 hover:text-[#7aa2f7] transition-colors"
              >
                {showArgs ? <ChevronUp className="w-3 h-3" /> : <ChevronDown className="w-3 h-3" />}
                Arguments
              </button>
              {showArgs && (
                <pre className="text-xs text-[#a9b1d6] bg-[#1a1b26] p-3 rounded overflow-auto max-h-64 scrollbar-thin whitespace-pre-wrap break-all">
                  {argsString}
                </pre>
              )}
            </>
          )}
        </div>

        {/* Suggestion for auto-approve */}
        {suggestion && (
          <div className="flex items-start gap-2 p-3 bg-[#7aa2f7]/10 rounded-md border border-[#7aa2f7]/30">
            <Zap className="w-4 h-4 text-[#7aa2f7] flex-shrink-0 mt-0.5" />
            <p className="text-xs text-[#7aa2f7]">{suggestion}</p>
          </div>
        )}

        {/* Warning for dangerous tools */}
        {isDangerous && (
          <div
            className={cn(
              "flex items-start gap-2 p-3 rounded-md border",
              RISK_STYLES[riskLevel].bg,
              `border-${RISK_STYLES[riskLevel].color.replace("text-", "")}/30`
            )}
          >
            <RiskIcon
              className={cn("w-4 h-4 flex-shrink-0 mt-0.5", RISK_STYLES[riskLevel].color)}
            />
            <p className={cn("text-xs", RISK_STYLES[riskLevel].color)}>
              {riskLevel === "critical"
                ? "This is a critical operation that may cause irreversible changes. Review carefully."
                : "This tool can modify files or execute commands on your system. Review the arguments carefully."}
            </p>
          </div>
        )}

        {/* Always allow checkbox */}
        {canLearn && (
          <div className="flex items-center gap-2">
            <Checkbox
              id="always-allow"
              checked={alwaysAllow}
              onCheckedChange={(checked: boolean | "indeterminate") =>
                setAlwaysAllow(checked === true)
              }
              className="border-[#3b4261] data-[state=checked]:bg-[#7aa2f7] data-[state=checked]:border-[#7aa2f7]"
            />
            <label
              htmlFor="always-allow"
              className="text-xs text-[#565f89] cursor-pointer flex items-center gap-1"
            >
              <Info className="w-3 h-3" />
              Always allow this tool (skip future approvals)
            </label>
          </div>
        )}

        <DialogFooter className="gap-2 sm:gap-2">
          <Button
            variant="outline"
            onClick={handleDeny}
            className="border-[#3b4261] bg-transparent text-[#c0caf5] hover:bg-[#3b4261] hover:text-[#c0caf5]"
          >
            <XCircle className="w-4 h-4 mr-2" />
            Deny
          </Button>
          <Button
            onClick={handleApprove}
            className={cn(
              riskLevel === "critical"
                ? "bg-[#f7768e] hover:bg-[#f7768e]/80"
                : riskLevel === "high"
                  ? "bg-[#e0af68] hover:bg-[#e0af68]/80"
                  : "bg-[#7aa2f7] hover:bg-[#7aa2f7]/80",
              "text-[#1a1b26]"
            )}
          >
            <CheckCircle className="w-4 h-4 mr-2" />
            {alwaysAllow ? "Approve & Remember" : "Approve"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

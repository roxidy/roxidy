import {
  AlertTriangle,
  Terminal,
  CheckCircle,
  XCircle,
} from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { useStore, usePendingToolApproval } from "@/store";
import { cn } from "@/lib/utils";

interface ToolApprovalDialogProps {
  sessionId: string;
}

// Tools that can modify files or execute code
const DANGEROUS_TOOLS = [
  "write_file",
  "edit_file",
  "apply_patch",
  "run_pty_cmd",
  "shell",
  "execute_code",
];

export function ToolApprovalDialog({ sessionId }: ToolApprovalDialogProps) {
  const tool = usePendingToolApproval(sessionId);
  const setPendingToolApproval = useStore(
    (state) => state.setPendingToolApproval
  );
  const updateToolCallStatus = useStore((state) => state.updateToolCallStatus);

  if (!tool) return null;

  const isDangerous = DANGEROUS_TOOLS.includes(tool.name);

  const handleApprove = async () => {
    setPendingToolApproval(sessionId, null);
    updateToolCallStatus(sessionId, tool.id, "running");

    // TODO: Actually execute the tool via Tauri
    // For now, simulate completion
    setTimeout(() => {
      updateToolCallStatus(
        sessionId,
        tool.id,
        "completed",
        "Tool executed successfully (simulated)"
      );
    }, 1000);
  };

  const handleDeny = () => {
    setPendingToolApproval(sessionId, null);
    updateToolCallStatus(sessionId, tool.id, "denied");
  };

  return (
    <Dialog open={true} onOpenChange={() => handleDeny()}>
      <DialogContent className="bg-[#1f2335] border-[#27293d] text-[#c0caf5] max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-[#c0caf5]">
            {isDangerous && (
              <AlertTriangle className="w-5 h-5 text-[#e0af68]" />
            )}
            Tool Approval Required
          </DialogTitle>
          <DialogDescription className="text-[#565f89]">
            The AI assistant wants to execute the following tool. Review the
            details before approving.
          </DialogDescription>
        </DialogHeader>

        <div className="bg-[#16161e] rounded-md p-4 border border-[#27293d]">
          {/* Tool name */}
          <div className="flex items-center gap-2 mb-3">
            <Terminal className="w-4 h-4 text-[#7aa2f7]" />
            <span className="font-mono text-sm text-[#c0caf5] font-medium">
              {tool.name}
            </span>
          </div>

          {/* Arguments */}
          {Object.keys(tool.args).length > 0 && (
            <>
              <div className="text-xs text-[#565f89] mb-2">Arguments:</div>
              <pre className="text-xs text-[#a9b1d6] bg-[#1a1b26] p-3 rounded overflow-x-auto max-h-48 scrollbar-thin">
                {JSON.stringify(tool.args, null, 2)}
              </pre>
            </>
          )}
        </div>

        {/* Warning for dangerous tools */}
        {isDangerous && (
          <div className="flex items-start gap-2 p-3 bg-[#e0af68]/10 rounded-md border border-[#e0af68]/30">
            <AlertTriangle className="w-4 h-4 text-[#e0af68] flex-shrink-0 mt-0.5" />
            <p className="text-xs text-[#e0af68]">
              This tool can modify files or execute commands on your system.
              Review the arguments carefully before approving.
            </p>
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
              isDangerous
                ? "bg-[#e0af68] hover:bg-[#e0af68]/80"
                : "bg-[#7aa2f7] hover:bg-[#7aa2f7]/80",
              "text-[#1a1b26]"
            )}
          >
            <CheckCircle className="w-4 h-4 mr-2" />
            Approve
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

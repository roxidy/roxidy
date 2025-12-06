import { Check, Copy, GitCommit, Loader2, RefreshCw, Sparkles } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Textarea } from "@/components/ui/textarea";
import {
  type CommitDraft as CommitDraftType,
  clearCommitBoundary,
  generateCommit,
  getPendingFiles,
} from "@/lib/sidecar";
import { cn } from "@/lib/utils";

interface CommitDraftProps {
  sessionId?: string;
  className?: string;
  onCommit?: (message: string) => void;
}

export function CommitDraft({ sessionId, className, onCommit }: CommitDraftProps) {
  const [draft, setDraft] = useState<CommitDraftType | null>(null);
  const [pendingFiles, setPendingFiles] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editedMessage, setEditedMessage] = useState("");
  const [copied, setCopied] = useState(false);

  const generateDraft = async () => {
    setLoading(true);
    setError(null);
    try {
      const [commitDraft, files] = await Promise.all([
        generateCommit(sessionId),
        getPendingFiles(),
      ]);
      setDraft(commitDraft);
      setPendingFiles(files);
      setEditedMessage(commitDraft.message);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to generate commit");
    } finally {
      setLoading(false);
    }
  };

  const copyMessage = async () => {
    try {
      await navigator.clipboard.writeText(editedMessage);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (e) {
      console.error("Failed to copy:", e);
    }
  };

  const handleCommit = async () => {
    if (onCommit) {
      onCommit(editedMessage);
      await clearCommitBoundary();
      setDraft(null);
      setEditedMessage("");
    }
  };

  // Initial state - prompt to generate
  if (!draft && !loading) {
    return (
      <div className={cn("p-4 space-y-4", className)}>
        <div className="flex items-center gap-2 text-[#c0caf5]">
          <GitCommit className="w-5 h-5 text-[#bb9af7]" />
          <span className="font-medium">Generate Commit Message</span>
        </div>

        <p className="text-sm text-[#565f89]">
          Generate a commit message based on your session activity. The AI will analyze your file
          changes and reasoning to create a meaningful commit message.
        </p>

        <Button
          onClick={generateDraft}
          className="w-full bg-[#bb9af7] hover:bg-[#bb9af7]/80 text-[#1a1b26]"
        >
          <Sparkles className="w-4 h-4 mr-2" />
          Generate from Session
        </Button>

        {error && <div className="text-xs text-[#f7768e] bg-[#f7768e]/10 rounded p-2">{error}</div>}
      </div>
    );
  }

  // Loading state
  if (loading) {
    return (
      <div className={cn("p-4 flex items-center justify-center", className)}>
        <div className="text-center space-y-2">
          <Loader2 className="w-6 h-6 text-[#bb9af7] animate-spin mx-auto" />
          <p className="text-sm text-[#565f89]">Analyzing session...</p>
        </div>
      </div>
    );
  }

  // Draft view
  return (
    <div className={cn("flex flex-col h-full", className)}>
      {/* Header */}
      <div className="p-3 border-b border-[#3b4261] flex items-center justify-between">
        <div className="flex items-center gap-2">
          <GitCommit className="w-4 h-4 text-[#bb9af7]" />
          <span className="text-sm font-medium text-[#c0caf5]">Commit Draft</span>
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={generateDraft}
          disabled={loading}
          className="h-7 px-2 text-[#565f89] hover:text-[#c0caf5]"
        >
          <RefreshCw className="w-3.5 h-3.5" />
        </Button>
      </div>

      <ScrollArea className="flex-1">
        <div className="p-3 space-y-4">
          {/* Scope badge */}
          {draft?.scope && (
            <div className="inline-flex items-center gap-1.5 px-2 py-1 rounded-md bg-[#7aa2f7]/10 text-[#7aa2f7] text-xs">
              <span className="font-medium">Scope:</span>
              {draft.scope}
            </div>
          )}

          {/* Editable message */}
          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <span className="text-xs text-[#565f89]">Commit Message</span>
              <Button
                variant="ghost"
                size="sm"
                onClick={copyMessage}
                className="h-6 px-2 text-[#565f89] hover:text-[#c0caf5]"
              >
                {copied ? (
                  <Check className="w-3.5 h-3.5 text-[#9ece6a]" />
                ) : (
                  <Copy className="w-3.5 h-3.5" />
                )}
              </Button>
            </div>
            <Textarea
              value={editedMessage}
              onChange={(e) => setEditedMessage(e.target.value)}
              rows={4}
              className="bg-[#1f2335] border-[#3b4261] text-[#c0caf5] text-sm resize-none"
            />
          </div>

          {/* Files to commit */}
          {(draft?.files.length || pendingFiles.length) > 0 && (
            <div className="space-y-1.5">
              <span className="text-xs text-[#565f89]">
                Files ({(draft?.files || pendingFiles).length})
              </span>
              <div className="bg-[#1f2335] rounded-md p-2 space-y-1 max-h-32 overflow-y-auto">
                {(draft?.files || pendingFiles).map((file) => (
                  <div key={file} className="text-xs text-[#c0caf5] font-mono truncate">
                    {file}
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* AI reasoning */}
          {draft?.reasoning && (
            <div className="space-y-1.5">
              <span className="text-xs text-[#565f89]">AI Reasoning</span>
              <div className="bg-[#1f2335] rounded-md p-2">
                <p className="text-xs text-[#565f89] italic">{draft.reasoning}</p>
              </div>
            </div>
          )}
        </div>
      </ScrollArea>

      {/* Actions */}
      <div className="p-3 border-t border-[#3b4261] flex gap-2">
        <Button
          variant="outline"
          onClick={() => {
            setDraft(null);
            setEditedMessage("");
          }}
          className="flex-1 bg-[#1f2335] border-[#3b4261] text-[#c0caf5] hover:bg-[#292e42]"
        >
          Cancel
        </Button>
        {onCommit && (
          <Button
            onClick={handleCommit}
            disabled={!editedMessage.trim()}
            className="flex-1 bg-[#9ece6a] hover:bg-[#9ece6a]/80 text-[#1a1b26]"
          >
            <GitCommit className="w-4 h-4 mr-1.5" />
            Commit
          </Button>
        )}
      </div>
    </div>
  );
}

import { formatDistanceToNow } from "date-fns";
import {
  Bot,
  Calendar,
  Clock,
  Download,
  FileText,
  Folder,
  MessageSquare,
  Search,
  Wrench,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  exportAiSessionTranscript,
  listAiSessions,
  loadAiSession,
  type SessionListingInfo,
  type SessionSnapshot,
} from "@/lib/ai";

interface SessionBrowserProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSessionRestore?: (identifier: string) => void;
}

export function SessionBrowser({ open, onOpenChange, onSessionRestore }: SessionBrowserProps) {
  const [sessions, setSessions] = useState<SessionListingInfo[]>([]);
  const [filteredSessions, setFilteredSessions] = useState<SessionListingInfo[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [selectedSession, setSelectedSession] = useState<SessionListingInfo | null>(null);
  const [sessionDetail, setSessionDetail] = useState<SessionSnapshot | null>(null);
  const [isLoadingDetail, setIsLoadingDetail] = useState(false);

  const loadSessions = useCallback(async () => {
    setIsLoading(true);
    try {
      const result = await listAiSessions(50);
      setSessions(result);
      setFilteredSessions(result);
    } catch (error) {
      toast.error(`Failed to load sessions: ${error}`);
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Load sessions when dialog opens
  useEffect(() => {
    if (open) {
      loadSessions();
    } else {
      // Reset state when closing
      setSelectedSession(null);
      setSessionDetail(null);
      setSearchQuery("");
    }
  }, [open, loadSessions]);

  // Filter sessions based on search query
  useEffect(() => {
    if (!searchQuery.trim()) {
      setFilteredSessions(sessions);
      return;
    }

    const query = searchQuery.toLowerCase();
    const filtered = sessions.filter(
      (session) =>
        session.workspace_label.toLowerCase().includes(query) ||
        session.model.toLowerCase().includes(query) ||
        session.first_prompt_preview?.toLowerCase().includes(query) ||
        session.first_reply_preview?.toLowerCase().includes(query)
    );
    setFilteredSessions(filtered);
  }, [searchQuery, sessions]);

  const handleSelectSession = useCallback(async (session: SessionListingInfo) => {
    setSelectedSession(session);
    setIsLoadingDetail(true);
    try {
      const detail = await loadAiSession(session.identifier);
      setSessionDetail(detail);
    } catch (error) {
      toast.error(`Failed to load session details: ${error}`);
    } finally {
      setIsLoadingDetail(false);
    }
  }, []);

  const handleExportSession = useCallback(
    async (session: SessionListingInfo, e: React.MouseEvent) => {
      e.stopPropagation();
      try {
        // Use Downloads folder with session identifier
        const outputPath = `${session.workspace_path}/session-${session.identifier}.md`;
        await exportAiSessionTranscript(session.identifier, outputPath);
        toast.success(`Exported to ${outputPath}`);
      } catch (error) {
        toast.error(`Failed to export: ${error}`);
      }
    },
    []
  );

  const handleLoadSession = useCallback(() => {
    if (selectedSession && onSessionRestore) {
      onSessionRestore(selectedSession.identifier);
      onOpenChange(false);
    }
  }, [selectedSession, onSessionRestore, onOpenChange]);

  const formatDate = (dateStr: string) => {
    try {
      const date = new Date(dateStr);
      return formatDistanceToNow(date, { addSuffix: true });
    } catch {
      return dateStr;
    }
  };

  const formatDuration = (startedAt: string, endedAt: string) => {
    try {
      const start = new Date(startedAt);
      const end = new Date(endedAt);
      const durationMs = end.getTime() - start.getTime();
      const minutes = Math.floor(durationMs / 60000);
      const seconds = Math.floor((durationMs % 60000) / 1000);
      if (minutes > 0) {
        return `${minutes}m ${seconds}s`;
      }
      return `${seconds}s`;
    } catch {
      return "—";
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="h-[85vh] p-0 gap-0 bg-[#1a1b26] border-[#3b4261] flex flex-col"
        style={{ maxWidth: "90vw", width: "90vw" }}
      >
        <DialogHeader className="px-4 py-3 border-b border-[#3b4261] shrink-0">
          <DialogTitle className="text-[#c0caf5] flex items-center gap-2">
            <Clock className="h-5 w-5 text-[#7aa2f7]" />
            Session History
          </DialogTitle>
          <DialogDescription className="text-[#565f89]">
            Browse and restore previous AI conversations
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-1 min-h-0 overflow-hidden">
          {/* Session List */}
          <div className="w-[380px] shrink-0 border-r border-[#3b4261] flex flex-col min-h-0">
            {/* Search */}
            <div className="p-3 border-b border-[#3b4261] shrink-0">
              <div className="relative">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-[#565f89]" />
                <Input
                  placeholder="Search sessions..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="pl-9 bg-[#1f2335] border-[#3b4261] text-[#c0caf5] placeholder:text-[#565f89]"
                />
              </div>
            </div>

            {/* Session List */}
            <ScrollArea className="flex-1 min-h-0">
              {isLoading ? (
                <div className="p-4 text-center text-[#565f89]">Loading sessions...</div>
              ) : filteredSessions.length === 0 ? (
                <div className="p-4 text-center text-[#565f89]">
                  {sessions.length === 0 ? "No sessions found" : "No matching sessions"}
                </div>
              ) : (
                <div className="p-2">
                  {filteredSessions.map((session) => (
                    <button
                      type="button"
                      key={session.identifier}
                      onClick={() => handleSelectSession(session)}
                      className={`w-full text-left p-3 rounded-lg mb-1 transition-colors ${
                        selectedSession?.identifier === session.identifier
                          ? "bg-[#3b4261] border border-[#7aa2f7]"
                          : "hover:bg-[#1f2335] border border-transparent"
                      }`}
                    >
                      <div className="flex items-start justify-between gap-2">
                        <div className="flex-1 min-w-0">
                          <div className="flex items-center gap-2 mb-1">
                            <Folder className="h-3.5 w-3.5 text-[#7aa2f7] shrink-0" />
                            <span className="text-sm font-medium text-[#c0caf5] truncate">
                              {session.workspace_label}
                            </span>
                          </div>
                          {session.first_prompt_preview && (
                            <p className="text-xs text-[#a9b1d6] truncate mb-1">
                              {session.first_prompt_preview}
                            </p>
                          )}
                          <div className="flex items-center gap-3 text-xs text-[#565f89]">
                            <span className="flex items-center gap-1">
                              <MessageSquare className="h-3 w-3" />
                              {session.total_messages}
                            </span>
                            <span>{formatDate(session.ended_at)}</span>
                          </div>
                        </div>
                        <button
                          type="button"
                          onClick={(e) => handleExportSession(session, e)}
                          className="p-1.5 rounded hover:bg-[#1f2335] text-[#565f89] hover:text-[#7aa2f7] transition-colors"
                          title="Export transcript"
                        >
                          <Download className="h-4 w-4" />
                        </button>
                      </div>
                    </button>
                  ))}
                </div>
              )}
            </ScrollArea>
          </div>

          {/* Session Detail */}
          <div className="flex-1 flex flex-col min-w-0 min-h-0">
            {selectedSession ? (
              <>
                {/* Session Header */}
                <div className="p-4 border-b border-[#3b4261] shrink-0">
                  <div className="flex items-start justify-between">
                    <div>
                      <h3 className="text-lg font-medium text-[#c0caf5] mb-2">
                        {selectedSession.workspace_label}
                      </h3>
                      <div className="flex flex-wrap gap-4 text-sm text-[#a9b1d6]">
                        <span className="flex items-center gap-1.5">
                          <Bot className="h-4 w-4 text-[#bb9af7]" />
                          {selectedSession.model}
                        </span>
                        <span className="flex items-center gap-1.5">
                          <Calendar className="h-4 w-4 text-[#7dcfff]" />
                          {formatDate(selectedSession.started_at)}
                        </span>
                        <span className="flex items-center gap-1.5">
                          <Clock className="h-4 w-4 text-[#9ece6a]" />
                          {formatDuration(selectedSession.started_at, selectedSession.ended_at)}
                        </span>
                      </div>
                      {selectedSession.distinct_tools.length > 0 && (
                        <div className="flex items-center gap-2 mt-2">
                          <Wrench className="h-4 w-4 text-[#e0af68]" />
                          <div className="flex flex-wrap gap-1">
                            {selectedSession.distinct_tools.slice(0, 5).map((tool) => (
                              <span
                                key={tool}
                                className="px-2 py-0.5 text-xs bg-[#1f2335] text-[#a9b1d6] rounded"
                              >
                                {tool}
                              </span>
                            ))}
                            {selectedSession.distinct_tools.length > 5 && (
                              <span className="px-2 py-0.5 text-xs bg-[#1f2335] text-[#565f89] rounded">
                                +{selectedSession.distinct_tools.length - 5} more
                              </span>
                            )}
                          </div>
                        </div>
                      )}
                    </div>
                    {onSessionRestore && (
                      <button
                        type="button"
                        onClick={handleLoadSession}
                        disabled={!selectedSession}
                        className="px-4 py-2 bg-[#7aa2f7] text-[#1a1b26] rounded-lg font-medium hover:bg-[#89b4fa] disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                      >
                        Load Session
                      </button>
                    )}
                  </div>
                </div>

                {/* Messages Preview */}
                <ScrollArea className="flex-1 min-h-0">
                  {isLoadingDetail ? (
                    <div className="p-4 text-center text-[#565f89] py-8">Loading messages...</div>
                  ) : sessionDetail ? (
                    <div className="p-4 space-y-4">
                      {sessionDetail.messages.map((msg, index) => (
                        <div
                          key={`${msg.role}-${index}-${msg.content.slice(0, 20)}`}
                          className={`p-3 rounded-lg ${
                            msg.role === "user"
                              ? "bg-[#1f2335] border-l-2 border-[#7aa2f7]"
                              : msg.role === "assistant"
                                ? "bg-[#1f2335] border-l-2 border-[#9ece6a]"
                                : msg.role === "tool"
                                  ? "bg-[#1f2335] border-l-2 border-[#e0af68]"
                                  : "bg-[#1f2335] border-l-2 border-[#565f89]"
                          }`}
                        >
                          <div className="flex items-center gap-2 mb-2">
                            {msg.role === "user" && (
                              <span className="text-xs font-medium text-[#7aa2f7]">User</span>
                            )}
                            {msg.role === "assistant" && (
                              <span className="text-xs font-medium text-[#9ece6a]">Assistant</span>
                            )}
                            {msg.role === "tool" && (
                              <span className="text-xs font-medium text-[#e0af68]">
                                Tool: {msg.tool_name || "unknown"}
                              </span>
                            )}
                            {msg.role === "system" && (
                              <span className="text-xs font-medium text-[#565f89]">System</span>
                            )}
                          </div>
                          <p className="text-sm text-[#c0caf5] whitespace-pre-wrap break-words">
                            {msg.content.length > 500
                              ? `${msg.content.slice(0, 500)}...`
                              : msg.content}
                          </p>
                        </div>
                      ))}
                    </div>
                  ) : (
                    <div className="p-4 text-center text-[#565f89] py-8">
                      Failed to load session details
                    </div>
                  )}
                </ScrollArea>
              </>
            ) : (
              <div className="flex-1 flex items-center justify-center text-[#565f89]">
                <div className="text-center">
                  <FileText className="h-12 w-12 mx-auto mb-3 opacity-50" />
                  <p>Select a session to view details</p>
                </div>
              </div>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

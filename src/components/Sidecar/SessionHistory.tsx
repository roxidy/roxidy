import {
  Bookmark,
  Calendar,
  ChevronRight,
  Clock,
  File,
  FolderOpen,
  Hammer,
  Loader2,
  MessageSquare,
  RefreshCw,
  Search,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  getSidecarStatus,
  type HistoryResponse,
  listSessions,
  queryHistory,
  type SessionEvent,
  type SidecarSession,
  searchEvents,
} from "@/lib/sidecar";
import { cn } from "@/lib/utils";

/** Extract a clean title from initial_request, stripping XML-like context tags */
function extractSessionTitle(initialRequest: string | undefined): string {
  if (!initialRequest) return "Session";

  // Remove XML-like tags and their content
  let cleaned = initialRequest
    .replace(/<context>[\s\S]*?<\/context>/gi, "")
    .replace(/<cwd>[\s\S]*?<\/cwd>/gi, "")
    .replace(/<session_id>[\s\S]*?<\/session_id>/gi, "")
    .replace(/<[^>]+>/g, "") // Remove any remaining tags
    .trim();

  // If nothing left, try to extract just the user query part
  if (!cleaned) {
    // Look for content after closing tags
    const match = initialRequest.match(/>\s*([^<]+)\s*$/);
    if (match) {
      cleaned = match[1].trim();
    }
  }

  // Still nothing? Use a generic title with timestamp hint
  if (!cleaned) {
    return "AI Session";
  }

  // Truncate if too long
  if (cleaned.length > 80) {
    return `${cleaned.slice(0, 77)}...`;
  }

  return cleaned;
}

interface SessionHistoryProps {
  className?: string;
  onSelectSession?: (session: SidecarSession) => void;
}

export function SessionHistory({ className, onSelectSession }: SessionHistoryProps) {
  const [sessions, setSessions] = useState<SidecarSession[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<SessionEvent[] | null>(null);
  const [historyAnswer, setHistoryAnswer] = useState<HistoryResponse | null>(null);
  const [searching, setSearching] = useState(false);

  // Track previous session state to detect when sessions end
  const lastSessionId = useRef<string | null>(null);

  const loadSessions = useCallback(async () => {
    try {
      setLoading(true);
      const data = await listSessions();
      setSessions(data);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load sessions");
    } finally {
      setLoading(false);
    }
  }, []);

  // Load sessions on mount
  useEffect(() => {
    loadSessions();
  }, [loadSessions]);

  // Poll for session status changes and auto-refresh when sessions end
  useEffect(() => {
    const pollInterval = setInterval(async () => {
      try {
        const status = await getSidecarStatus();
        const currentSessionId = status.session_id;

        // If we had an active session that just ended, refresh the sessions list
        if (lastSessionId.current && !currentSessionId) {
          // Session just ended - wait a moment for storage to complete, then refresh
          setTimeout(() => {
            loadSessions();
          }, 500);
        }

        lastSessionId.current = currentSessionId;
      } catch {
        // Ignore polling errors
      }
    }, 2000); // Poll every 2 seconds

    return () => clearInterval(pollInterval);
  }, [loadSessions]);

  const handleSearch = async () => {
    if (!searchQuery.trim()) {
      setSearchResults(null);
      setHistoryAnswer(null);
      return;
    }

    setSearching(true);
    try {
      // Try semantic query first
      const [events, answer] = await Promise.all([
        searchEvents(searchQuery, 10),
        queryHistory(searchQuery, 5),
      ]);
      setSearchResults(events);
      setHistoryAnswer(answer);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Search failed");
    } finally {
      setSearching(false);
    }
  };

  const formatDate = (dateStr: string) => {
    const date = new Date(dateStr);
    const now = new Date();
    const diff = now.getTime() - date.getTime();
    const days = Math.floor(diff / (1000 * 60 * 60 * 24));

    if (days === 0) return "Today";
    if (days === 1) return "Yesterday";
    if (days < 7) return `${days} days ago`;
    return date.toLocaleDateString();
  };

  const formatTime = (dateStr: string) => {
    return new Date(dateStr).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
    });
  };

  const formatDuration = (startStr: string, endStr: string | null) => {
    if (!endStr) return "Active";
    const start = new Date(startStr).getTime();
    const end = new Date(endStr).getTime();
    const diffMs = end - start;
    const diffSecs = Math.floor(diffMs / 1000);
    const diffMins = Math.floor(diffSecs / 60);
    const diffHours = Math.floor(diffMins / 60);

    if (diffHours > 0) return `${diffHours}h ${diffMins % 60}m`;
    if (diffMins > 0) return `${diffMins}m ${diffSecs % 60}s`;
    return `${diffSecs}s`;
  };

  const truncatePath = (path: string, maxLength: number = 30) => {
    if (path.length <= maxLength) return path;
    const parts = path.split("/");
    if (parts.length <= 2) return `...${path.slice(-maxLength)}`;
    // Keep the last two parts of the path
    return `.../${parts.slice(-2).join("/")}`;
  };

  const getEventIcon = (event: SessionEvent) => {
    const type = event.event_type.type;
    switch (type) {
      case "user_prompt":
        return <MessageSquare className="w-3 h-3 text-[#7aa2f7]" />;
      case "file_edit":
        return <File className="w-3 h-3 text-[#9ece6a]" />;
      case "tool_call":
        return <ChevronRight className="w-3 h-3 text-[#bb9af7]" />;
      default:
        return <ChevronRight className="w-3 h-3 text-[#565f89]" />;
    }
  };

  if (loading) {
    return (
      <div className={cn("flex items-center justify-center p-8", className)}>
        <Loader2 className="w-5 h-5 text-[#565f89] animate-spin" />
      </div>
    );
  }

  return (
    <div className={cn("flex flex-col h-full", className)}>
      {/* Search bar */}
      <div className="p-3 border-b border-[#3b4261]">
        <div className="flex gap-2">
          <div className="relative flex-1">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-4 h-4 text-[#565f89]" />
            <Input
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleSearch()}
              placeholder="Search history or ask a question..."
              className="pl-9 h-8 bg-[#1f2335] border-[#3b4261] text-[#c0caf5] placeholder:text-[#565f89]"
            />
          </div>
          <Button
            size="sm"
            onClick={handleSearch}
            disabled={searching}
            className="h-8 bg-[#bb9af7] hover:bg-[#bb9af7]/80 text-[#1a1b26]"
          >
            {searching ? <Loader2 className="w-4 h-4 animate-spin" /> : "Search"}
          </Button>
        </div>
      </div>

      <ScrollArea className="flex-1">
        {/* AI Answer */}
        {historyAnswer?.answer && (
          <div className="p-3 border-b border-[#3b4261] bg-[#1f2335]/50">
            <div className="text-xs text-[#bb9af7] mb-1.5 flex items-center gap-1">
              <MessageSquare className="w-3 h-3" />
              AI Answer
            </div>
            <p className="text-sm text-[#c0caf5]">{historyAnswer.answer}</p>
            {historyAnswer.confidence < 0.5 && (
              <p className="text-xs text-[#565f89] mt-1">
                (Low confidence - results may not be accurate)
              </p>
            )}
          </div>
        )}

        {/* Search Results */}
        {searchResults && searchResults.length > 0 && (
          <div className="p-3 border-b border-[#3b4261]">
            <div className="text-xs text-[#565f89] mb-2">
              Found {searchResults.length} matching events
            </div>
            <div className="space-y-2">
              {searchResults.map((event) => (
                <div key={event.id} className="p-2 rounded bg-[#1f2335] text-sm">
                  <div className="flex items-center gap-2">
                    {getEventIcon(event)}
                    <span className="text-[#c0caf5] truncate flex-1">{event.content}</span>
                    <span className="text-xs text-[#565f89]">{formatTime(event.timestamp)}</span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Sessions list */}
        <div className="p-3">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs text-[#565f89]">
              {sessions.length} session{sessions.length !== 1 ? "s" : ""}
            </span>
            <button
              type="button"
              onClick={loadSessions}
              disabled={loading}
              className="p-1 rounded hover:bg-[#292e42] transition-colors"
              title="Refresh sessions"
            >
              <RefreshCw className={cn("w-3 h-3 text-[#565f89]", loading && "animate-spin")} />
            </button>
          </div>

          {error && <div className="text-xs text-[#f7768e] mb-2">{error}</div>}

          <div className="space-y-2">
            {sessions.map((session) => (
              <button
                type="button"
                key={session.id}
                onClick={() => onSelectSession?.(session)}
                className="w-full p-3 rounded-md bg-[#1f2335] hover:bg-[#292e42] transition-colors text-left group"
              >
                {/* Session title and status */}
                <div className="flex items-start justify-between gap-2 mb-2">
                  <div className="flex-1 min-w-0">
                    <p className="text-sm text-[#c0caf5] truncate font-medium">
                      {extractSessionTitle(session.initial_request)}
                    </p>
                  </div>
                  <div
                    className={cn(
                      "text-xs px-1.5 py-0.5 rounded",
                      session.ended_at
                        ? "bg-[#565f89]/20 text-[#565f89]"
                        : "bg-[#9ece6a]/20 text-[#9ece6a]"
                    )}
                  >
                    {session.ended_at ? "Completed" : "Active"}
                  </div>
                </div>

                {/* Workspace path */}
                {session.workspace_path && (
                  <div className="flex items-center gap-1.5 text-xs text-[#565f89] mb-2">
                    <FolderOpen className="w-3 h-3 shrink-0" />
                    <span className="truncate font-mono" title={session.workspace_path}>
                      {truncatePath(session.workspace_path)}
                    </span>
                  </div>
                )}

                {/* Stats row */}
                <div className="flex items-center gap-3 text-xs text-[#565f89] flex-wrap">
                  <span className="flex items-center gap-1">
                    <Calendar className="w-3 h-3" />
                    {formatDate(session.started_at)}
                  </span>
                  <span className="flex items-center gap-1">
                    <Clock className="w-3 h-3" />
                    {formatTime(session.started_at)}
                  </span>
                  <span className="flex items-center gap-1 text-[#7aa2f7]">
                    <Clock className="w-3 h-3" />
                    {formatDuration(session.started_at, session.ended_at)}
                  </span>
                </div>

                {/* Metrics row */}
                <div className="flex items-center gap-3 mt-2 text-xs">
                  <span className="flex items-center gap-1 text-[#bb9af7]">
                    <Hammer className="w-3 h-3" />
                    {session.event_count} events
                  </span>
                  <span className="flex items-center gap-1 text-[#9ece6a]">
                    <File className="w-3 h-3" />
                    {session.files_touched.length} files
                  </span>
                  <span className="flex items-center gap-1 text-[#7dcfff]">
                    <Bookmark className="w-3 h-3" />
                    {session.checkpoint_count} checkpoints
                  </span>
                </div>

                {/* Files touched preview (collapsed by default, show on hover) */}
                {session.files_touched.length > 0 && (
                  <div className="mt-2 pt-2 border-t border-[#3b4261]/50 opacity-0 group-hover:opacity-100 transition-opacity">
                    <div className="text-xs text-[#565f89] mb-1">Files touched:</div>
                    <div className="flex flex-wrap gap-1">
                      {session.files_touched.slice(0, 5).map((file) => (
                        <span
                          key={file}
                          className="px-1.5 py-0.5 rounded bg-[#292e42] text-[#c0caf5] text-xs font-mono truncate max-w-[150px]"
                          title={file}
                        >
                          {file.split("/").pop()}
                        </span>
                      ))}
                      {session.files_touched.length > 5 && (
                        <span className="px-1.5 py-0.5 rounded bg-[#292e42] text-[#565f89] text-xs">
                          +{session.files_touched.length - 5} more
                        </span>
                      )}
                    </div>
                  </div>
                )}
              </button>
            ))}

            {sessions.length === 0 && !error && (
              <div className="text-center py-8 text-[#565f89] text-sm">
                No sessions captured yet.
                <br />
                Sessions are recorded during AI interactions.
              </div>
            )}
          </div>
        </div>
      </ScrollArea>
    </div>
  );
}

import {
  ArrowLeftRight,
  Clock,
  FileSearch,
  FileText,
  FolderTree,
  Keyboard,
  Palette,
  Plus,
  RefreshCw,
  Search,
  Settings,
  Terminal,
  Trash2,
} from "lucide-react";
import { useCallback, useState } from "react";
import { toast } from "sonner";
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
  CommandShortcut,
} from "@/components/ui/command";
import { indexDirectory, isIndexerInitialized, searchCode, searchFiles } from "@/lib/indexer";

export type PageRoute = "main" | "testbed";

interface CommandPaletteProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  currentPage: PageRoute;
  onNavigate: (page: PageRoute) => void;
  activeSessionId: string | null;
  onNewTab: () => void;
  onToggleMode: () => void;
  onClearConversation: () => void;
  onToggleSidebar?: () => void;
  workingDirectory?: string;
  onShowSearchResults?: (results: SearchResult[]) => void;
  onOpenSessionBrowser?: () => void;
}

// Types for search results
export interface SearchResult {
  file_path: string;
  line_number: number;
  line_content: string;
  matches: string[];
}

export interface SymbolResult {
  name: string;
  kind: string;
  line: number;
  column: number;
  scope: string | null;
  signature: string | null;
  documentation: string | null;
}

export function CommandPalette({
  open,
  onOpenChange,
  currentPage,
  onNavigate,
  activeSessionId,
  onNewTab,
  onToggleMode,
  onClearConversation,
  onToggleSidebar,
  workingDirectory,
  onShowSearchResults,
  onOpenSessionBrowser,
}: CommandPaletteProps) {
  const [searchQuery, setSearchQuery] = useState("");
  const [isSearching, setIsSearching] = useState(false);

  // Handle command selection
  const runCommand = useCallback(
    (command: () => void) => {
      onOpenChange(false);
      command();
    },
    [onOpenChange]
  );

  // Re-index workspace
  const handleReindex = useCallback(async () => {
    if (!workingDirectory) {
      toast.error("No workspace directory available");
      return;
    }
    try {
      const initialized = await isIndexerInitialized();
      if (!initialized) {
        toast.error("Indexer not initialized");
        return;
      }
      toast.info("Re-indexing workspace...");
      await indexDirectory(workingDirectory);
      toast.success("Workspace re-indexed successfully");
    } catch (error) {
      toast.error(`Failed to re-index: ${error}`);
    }
  }, [workingDirectory]);

  // Search code in workspace
  const handleSearchCode = useCallback(async () => {
    if (!searchQuery.trim()) {
      toast.error("Enter a search query first");
      return;
    }
    try {
      setIsSearching(true);
      const results = await searchCode(searchQuery);
      if (results.length === 0) {
        toast.info("No matches found");
      } else {
        toast.success(`Found ${results.length} matches`);
        onShowSearchResults?.(results);
      }
    } catch (error) {
      toast.error(`Search failed: ${error}`);
    } finally {
      setIsSearching(false);
    }
  }, [searchQuery, onShowSearchResults]);

  // Search files by name
  const handleSearchFiles = useCallback(async () => {
    if (!searchQuery.trim()) {
      toast.error("Enter a file name pattern first");
      return;
    }
    try {
      setIsSearching(true);
      const files = await searchFiles(searchQuery);
      if (files.length === 0) {
        toast.info("No files found");
      } else {
        toast.success(`Found ${files.length} files`);
        // Convert to search results format for display
        const results: SearchResult[] = files.map((f) => ({
          file_path: f,
          line_number: 0,
          line_content: "",
          matches: [],
        }));
        onShowSearchResults?.(results);
      }
    } catch (error) {
      toast.error(`File search failed: ${error}`);
    } finally {
      setIsSearching(false);
    }
  }, [searchQuery, onShowSearchResults]);

  return (
    <CommandDialog open={open} onOpenChange={onOpenChange}>
      <CommandInput
        placeholder="Type a command or search..."
        value={searchQuery}
        onValueChange={setSearchQuery}
      />
      <CommandList>
        <CommandEmpty>No results found.</CommandEmpty>

        {/* Navigation */}
        <CommandGroup heading="Navigation">
          <CommandItem
            onSelect={() => runCommand(() => onNavigate("main"))}
            disabled={currentPage === "main"}
          >
            <Terminal className="mr-2 h-4 w-4" />
            <span>Main App</span>
            {currentPage === "main" && (
              <span className="ml-auto text-xs text-[#565f89]">Current</span>
            )}
          </CommandItem>
          <CommandItem
            onSelect={() => runCommand(() => onNavigate("testbed"))}
            disabled={currentPage === "testbed"}
          >
            <Palette className="mr-2 h-4 w-4" />
            <span>Component Testbed</span>
            {currentPage === "testbed" && (
              <span className="ml-auto text-xs text-[#565f89]">Current</span>
            )}
          </CommandItem>
          {onToggleSidebar && (
            <CommandItem onSelect={() => runCommand(onToggleSidebar)}>
              <FolderTree className="mr-2 h-4 w-4" />
              <span>Toggle Sidebar</span>
              <CommandShortcut>⌘B</CommandShortcut>
            </CommandItem>
          )}
        </CommandGroup>

        <CommandSeparator />

        {/* Session Actions */}
        <CommandGroup heading="Session">
          <CommandItem onSelect={() => runCommand(onNewTab)}>
            <Plus className="mr-2 h-4 w-4" />
            <span>New Tab</span>
            <CommandShortcut>⌘T</CommandShortcut>
          </CommandItem>
          <CommandItem onSelect={() => runCommand(onToggleMode)}>
            <ArrowLeftRight className="mr-2 h-4 w-4" />
            <span>Toggle Mode</span>
            <CommandShortcut>⌘I</CommandShortcut>
          </CommandItem>
          {activeSessionId && (
            <CommandItem onSelect={() => runCommand(onClearConversation)}>
              <Trash2 className="mr-2 h-4 w-4" />
              <span>Clear Conversation</span>
              <CommandShortcut>⌘K</CommandShortcut>
            </CommandItem>
          )}
          {onOpenSessionBrowser && (
            <CommandItem onSelect={() => runCommand(onOpenSessionBrowser)}>
              <Clock className="mr-2 h-4 w-4" />
              <span>Browse Session History</span>
              <CommandShortcut>⌘H</CommandShortcut>
            </CommandItem>
          )}
        </CommandGroup>

        <CommandSeparator />

        {/* Code Search & Analysis */}
        <CommandGroup heading="Code Search">
          <CommandItem onSelect={() => runCommand(handleSearchCode)} disabled={isSearching}>
            <Search className="mr-2 h-4 w-4" />
            <span>Search Code</span>
            <span className="ml-auto text-xs text-[#565f89]">regex</span>
          </CommandItem>
          <CommandItem onSelect={() => runCommand(handleSearchFiles)} disabled={isSearching}>
            <FileSearch className="mr-2 h-4 w-4" />
            <span>Find Files</span>
            <span className="ml-auto text-xs text-[#565f89]">pattern</span>
          </CommandItem>
          <CommandItem onSelect={() => runCommand(handleReindex)} disabled={!workingDirectory}>
            <RefreshCw className="mr-2 h-4 w-4" />
            <span>Re-index Workspace</span>
          </CommandItem>
        </CommandGroup>

        <CommandSeparator />

        {/* Help */}
        <CommandGroup heading="Help">
          <CommandItem disabled>
            <Keyboard className="mr-2 h-4 w-4" />
            <span>Keyboard Shortcuts</span>
          </CommandItem>
          <CommandItem disabled>
            <FileText className="mr-2 h-4 w-4" />
            <span>Documentation</span>
          </CommandItem>
          <CommandItem disabled>
            <Settings className="mr-2 h-4 w-4" />
            <span>Settings</span>
          </CommandItem>
        </CommandGroup>
      </CommandList>
    </CommandDialog>
  );
}

// Hook to manage command palette state
export function useCommandPalette() {
  return {
    // Can be extended with more functionality
  };
}

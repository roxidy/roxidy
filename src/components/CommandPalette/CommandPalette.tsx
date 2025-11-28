import { Bot, FileText, Keyboard, Palette, Plus, Settings, Terminal } from "lucide-react";
import { useCallback } from "react";
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

export type PageRoute = "main" | "testbed";

interface CommandPaletteProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  currentPage: PageRoute;
  onNavigate: (page: PageRoute) => void;
  activeSessionId: string | null;
  onNewTab: () => void;
  onSetMode: (mode: "terminal" | "agent") => void;
}

export function CommandPalette({
  open,
  onOpenChange,
  currentPage,
  onNavigate,
  activeSessionId,
  onNewTab,
  onSetMode,
}: CommandPaletteProps) {
  // Handle command selection
  const runCommand = useCallback(
    (command: () => void) => {
      onOpenChange(false);
      command();
    },
    [onOpenChange]
  );

  return (
    <CommandDialog open={open} onOpenChange={onOpenChange}>
      <CommandInput placeholder="Type a command or search..." />
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
        </CommandGroup>

        <CommandSeparator />

        {/* Session Actions */}
        <CommandGroup heading="Session">
          <CommandItem onSelect={() => runCommand(onNewTab)}>
            <Plus className="mr-2 h-4 w-4" />
            <span>New Tab</span>
            <CommandShortcut>⌘T</CommandShortcut>
          </CommandItem>
          {activeSessionId && (
            <>
              <CommandItem onSelect={() => runCommand(() => onSetMode("terminal"))}>
                <Terminal className="mr-2 h-4 w-4" />
                <span>Switch to Terminal Mode</span>
                <CommandShortcut>⌘1</CommandShortcut>
              </CommandItem>
              <CommandItem onSelect={() => runCommand(() => onSetMode("agent"))}>
                <Bot className="mr-2 h-4 w-4" />
                <span>Switch to Agent Mode</span>
                <CommandShortcut>⌘2</CommandShortcut>
              </CommandItem>
            </>
          )}
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

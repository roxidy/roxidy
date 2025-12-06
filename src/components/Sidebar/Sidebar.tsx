import {
  ChevronDown,
  ChevronRight,
  Code,
  File,
  FileCode,
  Folder,
  FolderOpen,
  GripVertical,
  Hash,
  Loader2,
  Search,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  extractSymbols,
  isIndexerInitialized,
  type SymbolResult,
  searchFiles,
} from "@/lib/indexer";

interface SidebarProps {
  workingDirectory?: string;
  onFileSelect?: (filePath: string, line?: number) => void;
  isOpen: boolean;
  onToggle: () => void;
}

interface FileNode {
  name: string;
  path: string;
  type: "file" | "directory";
  children?: FileNode[];
}

interface SymbolGroup {
  file: string;
  symbols: SymbolResult[];
}

// Icon for symbol kinds
function SymbolIcon({ kind }: { kind: string }) {
  switch (kind.toLowerCase()) {
    case "function":
    case "method":
      return <Code className="h-3.5 w-3.5 text-[#7aa2f7]" />;
    case "class":
    case "struct":
      return <FileCode className="h-3.5 w-3.5 text-[#bb9af7]" />;
    case "variable":
    case "constant":
      return <Hash className="h-3.5 w-3.5 text-[#9ece6a]" />;
    default:
      return <Code className="h-3.5 w-3.5 text-[#565f89]" />;
  }
}

// File tree node component
function FileTreeNode({
  node,
  depth,
  onSelect,
  expandedPaths,
  onToggleExpand,
}: {
  node: FileNode;
  depth: number;
  onSelect: (path: string) => void;
  expandedPaths: Set<string>;
  onToggleExpand: (path: string) => void;
}) {
  const isExpanded = expandedPaths.has(node.path);
  const paddingLeft = 12 + depth * 16;

  if (node.type === "directory") {
    return (
      <Collapsible open={isExpanded}>
        <CollapsibleTrigger asChild>
          <button
            type="button"
            className="flex w-full items-center gap-1.5 py-1 px-2 text-sm text-[#a9b1d6] hover:bg-[#1f2335] transition-colors"
            style={{ paddingLeft }}
            onClick={() => onToggleExpand(node.path)}
          >
            {isExpanded ? (
              <>
                <ChevronDown className="h-3.5 w-3.5 flex-shrink-0" />
                <FolderOpen className="h-3.5 w-3.5 flex-shrink-0 text-[#e0af68]" />
              </>
            ) : (
              <>
                <ChevronRight className="h-3.5 w-3.5 flex-shrink-0" />
                <Folder className="h-3.5 w-3.5 flex-shrink-0 text-[#e0af68]" />
              </>
            )}
            <span className="truncate">{node.name}</span>
          </button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          {node.children?.map((child) => (
            <FileTreeNode
              key={child.path}
              node={child}
              depth={depth + 1}
              onSelect={onSelect}
              expandedPaths={expandedPaths}
              onToggleExpand={onToggleExpand}
            />
          ))}
        </CollapsibleContent>
      </Collapsible>
    );
  }

  return (
    <button
      type="button"
      className="flex w-full items-center gap-1.5 py-1 px-2 text-sm text-[#a9b1d6] hover:bg-[#1f2335] transition-colors"
      style={{ paddingLeft }}
      onClick={() => onSelect(node.path)}
    >
      <File className="h-3.5 w-3.5 flex-shrink-0 text-[#7dcfff]" />
      <span className="truncate">{node.name}</span>
    </button>
  );
}

// Build file tree from flat file list
function buildFileTree(files: string[], workingDir: string): FileNode[] {
  // Use a map to track all nodes by their full path
  const nodeMap = new Map<string, FileNode>();
  const rootChildren: FileNode[] = [];

  for (const filePath of files) {
    // Make path relative to working directory
    const relativePath = filePath.startsWith(workingDir)
      ? filePath.slice(workingDir.length + 1)
      : filePath;

    if (!relativePath) continue;

    const parts = relativePath.split("/").filter(Boolean);
    let currentPath = workingDir;

    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      currentPath = `${currentPath}/${part}`;
      const isFile = i === parts.length - 1;

      // Skip if we already have this node
      if (nodeMap.has(currentPath)) {
        continue;
      }

      const node: FileNode = {
        name: part,
        path: currentPath,
        type: isFile ? "file" : "directory",
        children: isFile ? undefined : [],
      };
      nodeMap.set(currentPath, node);

      // Add to parent or root
      if (i === 0) {
        rootChildren.push(node);
      } else {
        const parentPath = `${workingDir}/${parts.slice(0, i).join("/")}`;
        const parent = nodeMap.get(parentPath);
        if (parent?.children) {
          parent.children.push(node);
        }
      }
    }
  }

  // Sort nodes recursively
  const sortNodes = (nodes: FileNode[]): FileNode[] => {
    return nodes
      .sort((a, b) => {
        // Directories first
        if (a.type !== b.type) {
          return a.type === "directory" ? -1 : 1;
        }
        return a.name.localeCompare(b.name);
      })
      .map((node) => {
        if (node.children) {
          node.children = sortNodes(node.children);
        }
        return node;
      });
  };

  return sortNodes(rootChildren);
}

const MIN_WIDTH = 200;
const MAX_WIDTH = 600;
const DEFAULT_WIDTH = 256;

export function Sidebar({ workingDirectory, onFileSelect, isOpen, onToggle }: SidebarProps) {
  const [searchQuery, setSearchQuery] = useState("");
  const [files, setFiles] = useState<string[]>([]);
  const [symbols, setSymbols] = useState<SymbolGroup[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [activeTab, setActiveTab] = useState<"files" | "symbols">("files");
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const [expandedSymbolFiles, setExpandedSymbolFiles] = useState<Set<string>>(new Set());

  // Resize state
  const [width, setWidth] = useState(DEFAULT_WIDTH);
  const isResizing = useRef(false);
  const sidebarRef = useRef<HTMLDivElement>(null);

  // Handle resize
  const startResizing = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    isResizing.current = true;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  }, []);

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!isResizing.current) return;

      const newWidth = e.clientX;
      if (newWidth >= MIN_WIDTH && newWidth <= MAX_WIDTH) {
        setWidth(newWidth);
      }
    };

    const handleMouseUp = () => {
      if (isResizing.current) {
        isResizing.current = false;
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      }
    };

    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);

    return () => {
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
    };
  }, []);

  // Load initial file list
  const loadFiles = useCallback(async () => {
    try {
      const initialized = await isIndexerInitialized();
      if (!initialized) return;

      setIsLoading(true);
      const results = await searchFiles(".*");
      setFiles(results);
    } catch (error) {
      console.error("Failed to load files:", error);
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Search files
  const handleSearch = useCallback(async () => {
    if (!searchQuery.trim()) {
      await loadFiles();
      return;
    }

    try {
      setIsLoading(true);
      const results = await searchFiles(searchQuery);
      setFiles(results);
    } catch (error) {
      toast.error(`Search failed: ${error}`);
    } finally {
      setIsLoading(false);
    }
  }, [searchQuery, loadFiles]);

  // Extract symbols from a file
  const loadSymbols = useCallback(async (filePath: string) => {
    try {
      const symbols = await extractSymbols(filePath);
      if (symbols.length > 0) {
        setSymbols((prev) => {
          // Replace existing or add new
          const existing = prev.filter((g) => g.file !== filePath);
          return [...existing, { file: filePath, symbols }];
        });
      }
    } catch (error) {
      console.error("Failed to extract symbols:", error);
    }
  }, []);

  // Load files on mount
  useEffect(() => {
    if (isOpen) {
      loadFiles();
    }
  }, [isOpen, loadFiles]);

  // Debounce search
  useEffect(() => {
    const timer = setTimeout(() => {
      handleSearch();
    }, 300);
    return () => clearTimeout(timer);
  }, [handleSearch]);

  // Build file tree
  const fileTree = useMemo(() => {
    if (!workingDirectory) return [];
    return buildFileTree(files, workingDirectory);
  }, [files, workingDirectory]);

  // Handle file selection
  const handleFileSelect = useCallback(
    (filePath: string, line?: number) => {
      onFileSelect?.(filePath, line);
      // Also load symbols for the selected file
      loadSymbols(filePath);
    },
    [onFileSelect, loadSymbols]
  );

  // Toggle folder expansion
  const toggleExpand = useCallback((path: string) => {
    setExpandedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);

  // Toggle symbol file expansion
  const toggleSymbolFile = useCallback((file: string) => {
    setExpandedSymbolFiles((prev) => {
      const next = new Set(prev);
      if (next.has(file)) {
        next.delete(file);
      } else {
        next.add(file);
      }
      return next;
    });
  }, []);

  if (!isOpen) {
    return null;
  }

  return (
    <div
      ref={sidebarRef}
      className="h-full bg-[#1a1b26] border-r border-[#1f2335] flex flex-col overflow-hidden relative"
      style={{ width: `${width}px`, minWidth: `${MIN_WIDTH}px`, maxWidth: `${MAX_WIDTH}px` }}
    >
      {/* Resize handle */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: resize handle is mouse-only */}
      <div
        className="absolute top-0 right-0 w-1 h-full cursor-col-resize hover:bg-[#7aa2f7] transition-colors z-10 group"
        onMouseDown={startResizing}
      >
        <div className="absolute top-1/2 right-0 -translate-y-1/2 opacity-0 group-hover:opacity-100 transition-opacity">
          <GripVertical className="w-3 h-3 text-[#565f89]" />
        </div>
      </div>
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-[#1f2335]">
        <span className="text-sm font-medium text-[#c0caf5]">Explorer</span>
        <Button variant="ghost" size="icon" className="h-6 w-6" onClick={onToggle}>
          <X className="h-4 w-4" />
        </Button>
      </div>

      {/* Search */}
      <div className="px-2 py-2 border-b border-[#1f2335]">
        <div className="relative">
          <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-[#565f89]" />
          <Input
            placeholder="Search files..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="h-7 pl-7 text-sm bg-[#1f2335] border-[#3b4261] text-[#c0caf5] placeholder:text-[#565f89]"
          />
        </div>
      </div>

      {/* Tabs */}
      <div className="flex border-b border-[#1f2335]">
        <button
          type="button"
          className={`flex-1 py-1.5 text-xs font-medium transition-colors ${
            activeTab === "files"
              ? "text-[#7aa2f7] border-b-2 border-[#7aa2f7]"
              : "text-[#565f89] hover:text-[#a9b1d6]"
          }`}
          onClick={() => setActiveTab("files")}
        >
          Files
        </button>
        <button
          type="button"
          className={`flex-1 py-1.5 text-xs font-medium transition-colors ${
            activeTab === "symbols"
              ? "text-[#7aa2f7] border-b-2 border-[#7aa2f7]"
              : "text-[#565f89] hover:text-[#a9b1d6]"
          }`}
          onClick={() => setActiveTab("symbols")}
        >
          Symbols
        </button>
      </div>

      {/* Content */}
      <ScrollArea className="flex-1 min-h-0">
        {isLoading ? (
          <div className="flex items-center justify-center py-8">
            <Loader2 className="h-5 w-5 animate-spin text-[#565f89]" />
          </div>
        ) : activeTab === "files" ? (
          <div className="py-1">
            {fileTree.length === 0 ? (
              <div className="px-3 py-4 text-sm text-[#565f89] text-center">
                No files indexed yet
              </div>
            ) : (
              fileTree.map((node) => (
                <FileTreeNode
                  key={node.path}
                  node={node}
                  depth={0}
                  onSelect={handleFileSelect}
                  expandedPaths={expandedPaths}
                  onToggleExpand={toggleExpand}
                />
              ))
            )}
          </div>
        ) : (
          <div className="py-1">
            {symbols.length === 0 ? (
              <div className="px-3 py-4 text-sm text-[#565f89] text-center">
                Select a file to view symbols
              </div>
            ) : (
              symbols.map((group) => {
                const fileName = group.file.split("/").pop() || group.file;
                const isExpanded = expandedSymbolFiles.has(group.file);

                return (
                  <Collapsible key={group.file} open={isExpanded}>
                    <CollapsibleTrigger asChild>
                      <button
                        type="button"
                        className="flex w-full items-center gap-1.5 py-1.5 px-3 text-sm text-[#a9b1d6] hover:bg-[#1f2335] transition-colors"
                        onClick={() => toggleSymbolFile(group.file)}
                      >
                        {isExpanded ? (
                          <ChevronDown className="h-3.5 w-3.5" />
                        ) : (
                          <ChevronRight className="h-3.5 w-3.5" />
                        )}
                        <FileCode className="h-3.5 w-3.5 text-[#7dcfff]" />
                        <span className="truncate">{fileName}</span>
                        <span className="ml-auto text-xs text-[#565f89]">
                          {group.symbols.length}
                        </span>
                      </button>
                    </CollapsibleTrigger>
                    <CollapsibleContent>
                      {group.symbols.map((symbol, idx) => (
                        <button
                          key={`${symbol.name}-${idx}`}
                          type="button"
                          className="flex w-full items-center gap-1.5 py-1 px-3 pl-8 text-sm text-[#a9b1d6] hover:bg-[#1f2335] transition-colors"
                          onClick={() => handleFileSelect(group.file, symbol.line)}
                        >
                          <SymbolIcon kind={symbol.kind} />
                          <span className="truncate">{symbol.name}</span>
                          <span className="ml-auto text-xs text-[#565f89]">:{symbol.line}</span>
                        </button>
                      ))}
                    </CollapsibleContent>
                  </Collapsible>
                );
              })
            )}
          </div>
        )}
      </ScrollArea>
    </div>
  );
}

import { useEffect, useRef } from "react";
import { Popover, PopoverAnchor, PopoverContent } from "@/components/ui/popover";
import type { FileInfo } from "@/lib/tauri";
import { cn } from "@/lib/utils";

interface FileCommandPopupProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Already-filtered files to display */
  files: FileInfo[];
  selectedIndex: number;
  onSelect: (file: FileInfo) => void;
  children: React.ReactNode;
}

export function FileCommandPopup({
  open,
  onOpenChange,
  files,
  selectedIndex,
  onSelect,
  children,
}: FileCommandPopupProps) {
  const listRef = useRef<HTMLDivElement>(null);

  // Scroll selected item into view
  useEffect(() => {
    if (open && listRef.current) {
      const selectedElement = listRef.current.querySelector(`[data-index="${selectedIndex}"]`);
      selectedElement?.scrollIntoView({ block: "nearest" });
    }
  }, [selectedIndex, open]);

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverAnchor asChild>{children}</PopoverAnchor>
      <PopoverContent
        className="w-[400px] p-0"
        side="top"
        align="start"
        sideOffset={8}
        onOpenAutoFocus={(e) => e.preventDefault()}
      >
        <div
          ref={listRef}
          className="bg-[#1a1b26] border border-[#1f2335] rounded-md overflow-hidden"
        >
          {files.length === 0 ? (
            <div className="py-3 text-center text-sm text-[#565f89]">No files found</div>
          ) : (
            <div className="max-h-[200px] overflow-y-auto py-1" role="listbox">
              {files.map((file, index) => (
                <div
                  key={file.relative_path}
                  role="option"
                  aria-selected={index === selectedIndex}
                  tabIndex={0}
                  data-index={index}
                  onClick={() => onSelect(file)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      onSelect(file);
                    }
                  }}
                  className={cn(
                    "flex flex-col gap-0.5 px-3 py-2",
                    "cursor-pointer transition-colors",
                    index === selectedIndex ? "bg-[#292e42]" : "hover:bg-[#1f2335]"
                  )}
                >
                  <span className="font-mono text-sm text-[#c0caf5]">{file.name}</span>
                  <span className="font-mono text-xs text-[#565f89] truncate">
                    {file.relative_path}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}

// Export helper to filter files by query
export function filterFiles(files: FileInfo[], query: string): FileInfo[] {
  if (!query) return files;
  const lowerQuery = query.toLowerCase();
  return files.filter(
    (file) =>
      file.name.toLowerCase().includes(lowerQuery) ||
      file.relative_path.toLowerCase().includes(lowerQuery)
  );
}

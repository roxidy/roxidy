import { Palette, Pencil, Trash2, Upload } from "lucide-react";
import { useRef, useState } from "react";
import { toast } from "sonner";
import { useTheme } from "../../hooks/useTheme";
import { loadThemeFromDirectory, loadThemeFromFile } from "../../lib/theme/ThemeLoader";
import { Button } from "../ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../ui/dialog";

interface ThemePickerProps {
  onEditTheme?: (themeId: string) => void;
}

export function ThemePicker({ onEditTheme }: ThemePickerProps) {
  const { currentTheme, currentThemeId, availableThemes, setTheme, deleteTheme } = useTheme();
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [themeToDelete, setThemeToDelete] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const handleThemeSelect = async (themeId: string) => {
    const success = await setTheme(themeId);
    if (success) {
      toast.success(`Applied theme: ${availableThemes.find((t) => t.id === themeId)?.name}`);
    } else {
      toast.error("Failed to apply theme");
    }
  };

  const handleFileImport = async (files: FileList | null) => {
    if (!files || files.length === 0) return;

    try {
      // Check if it's a directory import (multiple files with webkitRelativePath)
      const isDirectory = files.length > 1 || files[0]?.webkitRelativePath;

      if (isDirectory) {
        await toast.promise(loadThemeFromDirectory(files), {
          loading: "Importing theme directory...",
          success: () => {
            // Clear the file input after successful import
            if (fileInputRef.current) {
              fileInputRef.current.value = "";
            }
            return `Theme applied: ${currentTheme?.name ?? "Custom Theme"}`;
          },
          error: (err) => `Import failed: ${err instanceof Error ? err.message : String(err)}`,
        });
      } else {
        await toast.promise(loadThemeFromFile(files[0]), {
          loading: "Importing theme...",
          success: () => {
            // Clear the file input after successful import
            if (fileInputRef.current) {
              fileInputRef.current.value = "";
            }
            return `Theme applied: ${currentTheme?.name ?? "Custom Theme"}`;
          },
          error: (err) => `Import failed: ${err instanceof Error ? err.message : String(err)}`,
        });
      }
    } catch (err) {
      console.error("Failed to load theme", err);
    }
  };

  const handleDeleteClick = (themeId: string) => {
    const theme = availableThemes.find((t) => t.id === themeId);
    if (!theme) return;

    if (theme.builtin) {
      toast.error("Cannot delete builtin themes");
      return;
    }

    setThemeToDelete(themeId);
    setDeleteDialogOpen(true);
  };

  const handleConfirmDelete = async () => {
    if (!themeToDelete) return;

    const theme = availableThemes.find((t) => t.id === themeToDelete);
    if (!theme) return;

    try {
      const success = await deleteTheme(themeToDelete);

      if (!success) {
        toast.error("Failed to delete theme");
        return;
      }

      toast.success(`Deleted theme: ${theme.name}`);

      // If we deleted the current theme, switch to first available theme
      if (themeToDelete === currentThemeId) {
        const remainingThemes = availableThemes.filter((t) => t.id !== themeToDelete);
        if (remainingThemes.length > 0) {
          await setTheme(remainingThemes[0].id);
        }
      }
    } catch (err) {
      console.error("Delete failed", err);
      toast.error("Failed to delete theme");
    } finally {
      setDeleteDialogOpen(false);
      setThemeToDelete(null);
    }
  };

  return (
    <>
      <div className="space-y-6">
        {/* Theme List */}
        <div className="space-y-2">
          <div className="flex items-center gap-2 text-sm font-medium">
            <Palette className="w-4 h-4" />
            Themes
          </div>
          <div className="space-y-1 border rounded-md p-2 max-h-64 overflow-y-auto">
            {availableThemes.map((theme) => {
              const isActive = theme.id === currentThemeId;
              return (
                <button
                  key={theme.id}
                  type="button"
                  onClick={() => handleThemeSelect(theme.id)}
                  className={`w-full flex items-center justify-between p-2 rounded hover:bg-accent group transition-colors ${
                    isActive ? "bg-accent" : ""
                  }`}
                >
                  <span className="flex-1 text-left text-sm">
                    {theme.name}
                    {isActive && (
                      <span className="text-xs text-primary ml-2">● Active</span>
                    )}
                  </span>
                  <div className="flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
                    {onEditTheme && (
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity"
                        onClick={(e) => {
                          e.stopPropagation();
                          onEditTheme(theme.id);
                        }}
                        title="Edit theme"
                      >
                        <Pencil className="w-3 h-3" />
                      </Button>
                    )}
                    {!theme.builtin && (
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity"
                        onClick={(e) => {
                          e.stopPropagation();
                          handleDeleteClick(theme.id);
                        }}
                        title="Delete theme"
                      >
                        <Trash2 className="w-3 h-3 text-destructive" />
                      </Button>
                    )}
                  </div>
                </button>
              );
            })}
          </div>
        </div>

        {/* Import from File or Directory */}
        <div className="space-y-2">
          <label htmlFor="theme-file" className="flex items-center gap-2 text-sm font-medium">
            <Upload className="w-4 h-4" />
            Import Theme
          </label>
          <input
            id="theme-file"
            ref={fileInputRef}
            type="file"
            accept="application/json,.json"
            // @ts-expect-error - webkitdirectory is not in the types but works in browsers
            webkitdirectory=""
            directory=""
            multiple
            onChange={(e) => handleFileImport(e.target.files)}
            className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
          />
          <p className="text-xs text-muted-foreground">
            Select a theme directory containing theme.json and assets folder
          </p>
        </div>
      </div>

      {/* Delete Confirmation Dialog */}
      <Dialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete Theme</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete "
              {availableThemes.find((t) => t.id === themeToDelete)?.name}"? This action cannot be
              undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteDialogOpen(false)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleConfirmDelete}>
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

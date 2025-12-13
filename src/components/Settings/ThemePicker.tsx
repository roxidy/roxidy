import { Copy, Palette, Pencil, Trash2, Upload } from "lucide-react";
import { useRef, useState } from "react";
import { toast } from "sonner";
import { useTheme } from "../../hooks/useTheme";
import { ThemeManager } from "../../lib/theme/ThemeManager";
import { getUniqueThemeName } from "../../lib/theme/themeNameUtils";
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
  const { currentThemeId, availableThemes, setTheme, deleteTheme } = useTheme();
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [themeToDelete, setThemeToDelete] = useState<string | null>(null);
  const [importConflictDialogOpen, setImportConflictDialogOpen] = useState(false);
  const [pendingImport, setPendingImport] = useState<{
    // biome-ignore lint/suspicious/noExplicitAny: theme structure is dynamic and comes from external files
    theme: any;
    assets?: Array<[string, Uint8Array]>;
    existingThemeId: string;
  } | null>(null);
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
      const isDirectory = files.length > 1 || !!files[0]?.webkitRelativePath;

      // Parse the theme to check for conflicts
      const { loadThemeData } = await import("../../lib/theme/ThemeLoader");
      const { theme, assets } = await loadThemeData(files, isDirectory);

      // Check if a theme with this name already exists
      const existingTheme = availableThemes.find(
        (t) => t.name.toLowerCase() === theme.name.toLowerCase()
      );

      if (existingTheme) {
        // Show conflict dialog
        setPendingImport({ theme, assets, existingThemeId: existingTheme.id });
        setImportConflictDialogOpen(true);
        // Clear the file input
        if (fileInputRef.current) {
          fileInputRef.current.value = "";
        }
        return;
      }

      // No conflict, import directly
      await toast.promise(ThemeManager.loadThemeFromObject(theme, assets), {
        loading: "Importing theme...",
        success: () => `Theme imported: ${theme.name}`,
        error: (err) => `Import failed: ${err instanceof Error ? err.message : String(err)}`,
      });

      // Clear the file input
      if (fileInputRef.current) {
        fileInputRef.current.value = "";
      }
    } catch (err) {
      console.error("Failed to load theme", err);
      toast.error(`Import failed: ${err instanceof Error ? err.message : String(err)}`);
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

  const handleImportOverwrite = async () => {
    if (!pendingImport) return;

    try {
      await toast.promise(
        ThemeManager.loadThemeFromObject(
          pendingImport.theme,
          pendingImport.assets,
          pendingImport.existingThemeId
        ),
        {
          loading: "Overwriting theme...",
          success: () => `Theme overwritten: ${pendingImport.theme.name}`,
          error: (err) =>
            `Failed to overwrite: ${err instanceof Error ? err.message : String(err)}`,
        }
      );
    } catch (err) {
      console.error("Failed to overwrite theme", err);
    } finally {
      setImportConflictDialogOpen(false);
      setPendingImport(null);
    }
  };

  const handleImportRename = async () => {
    if (!pendingImport) return;

    try {
      // Generate unique name
      const uniqueName = getUniqueThemeName(pendingImport.theme.name, availableThemes);
      const renamedTheme = { ...pendingImport.theme, name: uniqueName };

      await toast.promise(ThemeManager.loadThemeFromObject(renamedTheme, pendingImport.assets), {
        loading: "Importing theme...",
        success: () => `Theme imported as: ${uniqueName}`,
        error: (err) => `Failed to import: ${err instanceof Error ? err.message : String(err)}`,
      });
    } catch (err) {
      console.error("Failed to import theme", err);
    } finally {
      setImportConflictDialogOpen(false);
      setPendingImport(null);
    }
  };

  const handleCloneClick = async (themeId: string) => {
    const theme = availableThemes.find((t) => t.id === themeId);
    if (!theme || !("theme" in theme)) return;

    try {
      // Generate unique name for the cloned theme
      const uniqueName = getUniqueThemeName(theme.name, availableThemes);

      // Clone the theme with the new name
      const clonedTheme = { ...theme.theme, name: uniqueName };

      // Save the cloned theme (let it generate a new ID)
      await ThemeManager.loadThemeFromObject(clonedTheme);

      toast.success(`Cloned theme: ${uniqueName}`);
    } catch (err) {
      console.error("Clone failed", err);
      toast.error("Failed to clone theme");
    }
  };

  const handleConfirmDelete = async () => {
    if (!themeToDelete) return;

    const theme = availableThemes.find((t) => t.id === themeToDelete);
    if (!theme) return;

    try {
      const success = await deleteTheme(themeToDelete);

      if (!success) {
        toast.error(`Failed to delete "${theme.name}". It may have already been deleted.`);
        // Force refresh the available themes list
        window.location.reload();
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
      toast.error(`Failed to delete theme: ${err instanceof Error ? err.message : String(err)}`);
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
                    {isActive && <span className="text-xs text-primary ml-2">● Active</span>}
                  </span>
                  {/* biome-ignore lint/a11y/useKeyWithClickEvents: event propagation control for nested buttons */}
                  {/* biome-ignore lint/a11y/noStaticElementInteractions: event propagation control for nested buttons */}
                  <div className="flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
                    {onEditTheme && (
                      <>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity"
                          onClick={(e) => {
                            e.stopPropagation();
                            handleCloneClick(theme.id);
                          }}
                          title="Clone theme"
                        >
                          <Copy className="w-3 h-3" />
                        </Button>
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
                      </>
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

      {/* Import Conflict Dialog */}
      <Dialog open={importConflictDialogOpen} onOpenChange={setImportConflictDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Theme Already Exists</DialogTitle>
            <DialogDescription>
              A theme named "{pendingImport?.theme.name}" already exists. How would you like to
              proceed?
            </DialogDescription>
          </DialogHeader>
          <DialogFooter className="flex gap-2">
            <Button
              variant="outline"
              onClick={() => {
                setImportConflictDialogOpen(false);
                setPendingImport(null);
              }}
            >
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleImportOverwrite}>
              Overwrite
            </Button>
            <Button onClick={handleImportRename}>
              Rename to "
              {pendingImport && getUniqueThemeName(pendingImport.theme.name, availableThemes)}"
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

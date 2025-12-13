import { zodResolver } from "@hookform/resolvers/zod";
import { Pencil, Save, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ChromePicker } from "react-color";
import { useForm } from "react-hook-form";
import { toast } from "sonner";
import { z } from "zod";
import googleFonts from "@/assets/google-fonts.json";
import { useTheme } from "@/hooks/useTheme";
import { ThemeRegistry } from "@/lib/theme/registry";
import { ThemeManager } from "@/lib/theme/ThemeManager";
import { getUniqueThemeName, validateThemeName } from "@/lib/theme/themeNameUtils";
import type { QbitTheme } from "@/lib/theme/types";
import { Button } from "../ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../ui/dialog";
import { Input } from "../ui/input";
import { Label } from "../ui/label";
import { ScrollArea } from "../ui/scroll-area";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";

interface ThemeDesignerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  editThemeId?: string | null;
}

// Popular monospace fonts for terminal
const popularMonospaceFonts = [
  "JetBrains Mono",
  "Fira Code",
  "Source Code Pro",
  "Roboto Mono",
  "IBM Plex Mono",
  "Inconsolata",
  "Ubuntu Mono",
  "Cascadia Code",
  "Courier Prime",
  "Anonymous Pro",
];

// Popular UI fonts
const popularUIFonts = [
  "Inter",
  "Roboto",
  "Open Sans",
  "Lato",
  "Montserrat",
  "Poppins",
  "Raleway",
  "Nunito",
  "Work Sans",
  "DM Sans",
];

// Extract and cache popular fonts from Google Fonts JSON (top 300 by popularity)
const allFonts = googleFonts.familyMetadataList
  .sort((a, b) => (a.popularity || 999) - (b.popularity || 999))
  .slice(0, 300)
  .map((f) => f.family);

// Combine popular fonts with the full list, removing duplicates, then sort alphabetically
const fontFamilies = [
  ...new Set([...popularMonospaceFonts, ...popularUIFonts, ...allFonts]),
].sort();

// Helper to load Google Fonts dynamically
function loadGoogleFont(fontFamily: string) {
  // Check if font is already loaded
  const existingLink = document.querySelector(
    `link[href*="family=${encodeURIComponent(fontFamily)}"]`
  );
  if (existingLink) return;

  // Create and append font link
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = `https://fonts.googleapis.com/css2?family=${encodeURIComponent(
    fontFamily
  )}:wght@300;400;500;600;700&display=swap`;
  document.head.appendChild(link);
}

export function ThemeDesigner({ open, onOpenChange, editThemeId }: ThemeDesignerProps) {
  const { availableThemes, currentThemeId } = useTheme();

  const [theme, setTheme] = useState<QbitTheme | null>(null);
  const [originalThemeId, setOriginalThemeId] = useState<string | null>(null);
  const [isOriginalBuiltin, setIsOriginalBuiltin] = useState(false);
  const originalThemeIdRef = useRef<string | null>(null);
  const justSavedRef = useRef<boolean>(false);
  const [backgroundFile, setBackgroundFile] = useState<File | null>(null);
  const [activeColorPicker, setActiveColorPicker] = useState<string | null>(null);
  const previewTimerRef = useRef<NodeJS.Timeout | null>(null);
  const loadedFontsRef = useRef<Set<string>>(new Set());

  // Create Zod schema with custom validation
  const themeFormSchema = z.object({
    name: z
      .string()
      .min(1, "Theme name cannot be empty")
      .refine(
        (name) => {
          const error = validateThemeName(name, availableThemes, originalThemeId || undefined);
          return error === null;
        },
        {
          message: "A theme with this name already exists",
        }
      ),
  });

  type ThemeFormData = z.infer<typeof themeFormSchema>;

  const {
    register,
    handleSubmit,
    formState: { errors },
    setValue,
    trigger,
  } = useForm<ThemeFormData>({
    resolver: zodResolver(themeFormSchema),
    mode: "onChange",
  });

  // Load fonts when theme changes
  useEffect(() => {
    if (!theme) return;

    const fontsToLoad: string[] = [];

    if (theme.typography?.ui?.fontFamily) {
      const uiFont = theme.typography.ui.fontFamily.split(",")[0].trim().replace(/['"]/g, "");
      if (!loadedFontsRef.current.has(uiFont)) {
        fontsToLoad.push(uiFont);
        loadedFontsRef.current.add(uiFont);
      }
    }

    if (theme.typography?.terminal?.fontFamily) {
      const terminalFont = theme.typography.terminal.fontFamily
        .split(",")[0]
        .trim()
        .replace(/['"]/g, "");
      if (!loadedFontsRef.current.has(terminalFont)) {
        fontsToLoad.push(terminalFont);
        loadedFontsRef.current.add(terminalFont);
      }
    }

    // Load fonts
    fontsToLoad.forEach((font) => {
      loadGoogleFont(font);
    });
  }, [theme?.typography?.ui?.fontFamily, theme?.typography?.terminal?.fontFamily, theme]);

  // Memoize font options to avoid re-rendering
  const fontOptions = useMemo(
    () =>
      fontFamilies.map((font) => (
        <SelectItem key={font} value={font}>
          {font}
        </SelectItem>
      )),
    []
  );

  // Initialize theme when dialog opens
  useEffect(() => {
    if (open) {
      // Store the current theme ID at dialog open time
      originalThemeIdRef.current = currentThemeId;
      justSavedRef.current = false; // Reset save flag

      if (editThemeId) {
        // Editing existing theme
        const existingTheme = availableThemes.find((t) => t.id === editThemeId);
        if (existingTheme && "theme" in existingTheme) {
          const themeClone = JSON.parse(JSON.stringify(existingTheme.theme)); // Deep clone
          setTheme(themeClone);
          setValue("name", existingTheme.name);
          setOriginalThemeId(editThemeId);
          setIsOriginalBuiltin(existingTheme.builtin);
          // Apply preview immediately
          ThemeManager.applyThemePreview(themeClone).catch(console.error);
        }
      } else {
        // Creating new theme - start with builtin qbit theme as base
        const qbitTheme = ThemeRegistry.get("qbit");
        const uniqueName = getUniqueThemeName("My Theme", availableThemes);

        if (qbitTheme) {
          const themeClone = JSON.parse(JSON.stringify(qbitTheme)); // Deep clone
          themeClone.name = uniqueName;
          setTheme(themeClone);
          setValue("name", uniqueName);
          // Apply preview immediately
          ThemeManager.applyThemePreview(themeClone).catch(console.error);
        } else {
          // Fallback to a default theme structure if qbit theme not found
          const defaultTheme = createDefaultTheme();
          defaultTheme.name = uniqueName;
          setTheme(defaultTheme);
          setValue("name", uniqueName);
          // Apply preview immediately
          ThemeManager.applyThemePreview(defaultTheme).catch(console.error);
        }
        setOriginalThemeId(null);
        setIsOriginalBuiltin(false);
      }
    }
    // Only run when dialog opens or editThemeId changes, not when themes update
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, editThemeId, availableThemes, currentThemeId, setValue]);

  // Apply preview with debouncing
  const applyPreview = useCallback((updatedTheme: QbitTheme) => {
    if (previewTimerRef.current) {
      clearTimeout(previewTimerRef.current);
    }

    previewTimerRef.current = setTimeout(() => {
      ThemeManager.applyThemePreview(updatedTheme).catch(console.error);
    }, 300);
  }, []);

  const updateThemeField = <K extends keyof QbitTheme>(key: K, value: QbitTheme[K]) => {
    setTheme((prev) => {
      if (!prev) return null;
      const updated = { ...prev, [key]: value };
      applyPreview(updated);
      return updated;
    });
  };

  const updateColorField = (path: string, value: string) => {
    setTheme((prev) => {
      if (!prev) return null;
      const newTheme = JSON.parse(JSON.stringify(prev));
      const keys = path.split(".");
      // biome-ignore lint/suspicious/noExplicitAny: dynamic theme object navigation requires any
      let current: any = newTheme;
      for (let i = 0; i < keys.length - 1; i++) {
        current = current[keys[i]];
      }
      current[keys[keys.length - 1]] = value;
      applyPreview(newTheme);
      return newTheme;
    });
  };

  // Extract first font name from a font-family string
  const getFirstFont = (fontFamily: string | undefined): string => {
    if (!fontFamily) return "";
    // Remove quotes and get first font name
    return fontFamily.split(",")[0].trim().replace(/['"]/g, "");
  };

  const handleSaveClick = async () => {
    if (!theme) return;

    // Validate form
    const isValid = await trigger();
    if (!isValid) {
      return; // Don't save if validation fails
    }

    // Save directly
    handleSubmit(onSaveSubmit)();
  };

  const onSaveSubmit = async (data: ThemeFormData) => {
    if (!theme) return;

    try {
      const themeToSave: QbitTheme = { ...theme, name: data.name };

      // Handle background file if selected
      const assets: Array<[string, Uint8Array]> = [];
      if (backgroundFile) {
        const buffer = await backgroundFile.arrayBuffer();
        const uint8Array = new Uint8Array(buffer);
        assets.push([`assets/${backgroundFile.name}`, uint8Array]);

        // Update theme to reference the asset
        themeToSave.background = {
          ...themeToSave.background,
          image: `assets/${backgroundFile.name}`,
        };
      }

      // For editing existing custom themes, use the original ID to overwrite
      // For new themes or builtin themes, let it generate a new ID
      const themeIdToUse = originalThemeId && !isOriginalBuiltin ? originalThemeId : undefined;

      await ThemeManager.loadThemeFromObject(themeToSave, assets, themeIdToUse);
      toast.success(`Theme saved: ${data.name}`);
      justSavedRef.current = true;
      onOpenChange(false);
    } catch (err) {
      console.error("Failed to save theme:", err);
      toast.error("Failed to save theme");
    }
  };

  const handleBackgroundFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      setBackgroundFile(file);
    }
  };

  // Restore original theme when dialog closes
  const handleClose = useCallback(
    (open: boolean) => {
      if (!open) {
        // Dialog is closing - restore original theme only if we didn't just save
        if (previewTimerRef.current) {
          clearTimeout(previewTimerRef.current);
        }
        if (!justSavedRef.current && originalThemeIdRef.current) {
          // Restore by the original theme ID from when dialog opened
          ThemeManager.applyThemeById(originalThemeIdRef.current).catch(console.error);
        }
        // Reset the flag for next time
        justSavedRef.current = false;
      }
      onOpenChange(open);
    },
    [onOpenChange]
  );

  if (!theme) return null;

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="max-w-5xl h-[90vh] p-0 flex flex-col">
        <DialogHeader className="px-6 py-4 border-b">
          <DialogTitle className="flex items-center gap-2">
            <Pencil className="w-5 h-5" />
            {editThemeId ? "Edit Theme" : "Create Theme"}
          </DialogTitle>
          <DialogDescription>Customize your theme and see changes in real-time</DialogDescription>
        </DialogHeader>

        <div className="flex-1 overflow-hidden">
          <ScrollArea className="h-full">
            <div className="p-6 space-y-6">
              {/* Theme Name */}
              <div className="space-y-2">
                <Label htmlFor="theme-name">Theme Name</Label>
                <Input
                  id="theme-name"
                  {...register("name")}
                  placeholder="Enter theme name"
                  className={errors.name ? "border-destructive" : ""}
                />
                {errors.name && <p className="text-sm text-destructive">{errors.name.message}</p>}
              </div>

              {/* Typography */}
              <div className="space-y-4">
                <h3 className="text-lg font-semibold">Typography</h3>

                <div className="grid grid-cols-2 gap-4">
                  <div className="space-y-2">
                    <Label>UI Font Family</Label>
                    <Select
                      value={getFirstFont(theme.typography?.ui?.fontFamily)}
                      onValueChange={(value: string) =>
                        updateThemeField("typography", {
                          ...theme.typography,
                          ui: { ...theme.typography?.ui, fontFamily: value },
                        })
                      }
                    >
                      <SelectTrigger>
                        <SelectValue placeholder="Select font" />
                      </SelectTrigger>
                      <SelectContent className="max-h-60">{fontOptions}</SelectContent>
                    </Select>
                  </div>

                  <div className="space-y-2">
                    <Label>Terminal Font Family</Label>
                    <Select
                      value={getFirstFont(theme.typography?.terminal?.fontFamily)}
                      onValueChange={(value: string) =>
                        updateThemeField("typography", {
                          ...theme.typography,
                          terminal: { ...theme.typography?.terminal, fontFamily: value },
                        })
                      }
                    >
                      <SelectTrigger>
                        <SelectValue placeholder="Select font" />
                      </SelectTrigger>
                      <SelectContent className="max-h-60">{fontOptions}</SelectContent>
                    </Select>
                  </div>
                </div>

                <div className="space-y-2">
                  <Label>Terminal Font Size</Label>
                  <Input
                    type="number"
                    min={8}
                    max={32}
                    value={theme.typography?.terminal?.fontSize || 14}
                    onChange={(e) =>
                      updateThemeField("typography", {
                        ...theme.typography,
                        terminal: {
                          ...theme.typography?.terminal,
                          fontSize: parseInt(e.target.value, 10) || 14,
                        },
                      })
                    }
                  />
                </div>
              </div>

              {/* Background */}
              <div className="space-y-4">
                <h3 className="text-lg font-semibold">Background</h3>

                <div className="space-y-2">
                  <Label>Background Image</Label>
                  <input
                    type="file"
                    accept="image/*"
                    onChange={handleBackgroundFileSelect}
                    className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                  />
                  {backgroundFile && (
                    <p className="text-xs text-muted-foreground">Selected: {backgroundFile.name}</p>
                  )}
                </div>

                <div className="space-y-2">
                  <Label>Background Opacity</Label>
                  <Input
                    type="number"
                    min={0}
                    max={1}
                    step={0.05}
                    value={theme.background?.opacity || 1}
                    onChange={(e) =>
                      updateThemeField("background", {
                        ...theme.background,
                        opacity: parseFloat(e.target.value) || 1,
                      })
                    }
                  />
                </div>
              </div>

              {/* UI Colors */}
              <div className="space-y-4">
                <h3 className="text-lg font-semibold">UI Colors</h3>

                <div className="grid grid-cols-2 gap-4">
                  {renderColorPicker(
                    "Background",
                    "colors.ui.background",
                    theme.colors.ui.background
                  )}
                  {renderColorPicker(
                    "Foreground",
                    "colors.ui.foreground",
                    theme.colors.ui.foreground
                  )}
                  {renderColorPicker("Primary", "colors.ui.primary", theme.colors.ui.primary)}
                  {renderColorPicker("Secondary", "colors.ui.secondary", theme.colors.ui.secondary)}
                  {renderColorPicker("Accent", "colors.ui.accent", theme.colors.ui.accent)}
                  {renderColorPicker("Muted", "colors.ui.muted", theme.colors.ui.muted)}
                  {renderColorPicker("Border", "colors.ui.border", theme.colors.ui.border)}
                  {renderColorPicker("Card", "colors.ui.card", theme.colors.ui.card)}
                </div>
              </div>

              {/* ANSI Colors */}
              <div className="space-y-4">
                <h3 className="text-lg font-semibold">Terminal ANSI Colors</h3>

                <div className="grid grid-cols-4 gap-4">
                  {renderColorPicker("Black", "colors.ansi.black", theme.colors.ansi.black)}
                  {renderColorPicker("Red", "colors.ansi.red", theme.colors.ansi.red)}
                  {renderColorPicker("Green", "colors.ansi.green", theme.colors.ansi.green)}
                  {renderColorPicker("Yellow", "colors.ansi.yellow", theme.colors.ansi.yellow)}
                  {renderColorPicker("Blue", "colors.ansi.blue", theme.colors.ansi.blue)}
                  {renderColorPicker("Magenta", "colors.ansi.magenta", theme.colors.ansi.magenta)}
                  {renderColorPicker("Cyan", "colors.ansi.cyan", theme.colors.ansi.cyan)}
                  {renderColorPicker("White", "colors.ansi.white", theme.colors.ansi.white)}
                </div>
              </div>
            </div>
          </ScrollArea>
        </div>

        <DialogFooter className="px-6 py-4 border-t">
          <Button variant="outline" onClick={() => handleClose(false)}>
            <X className="w-4 h-4 mr-2" />
            Cancel
          </Button>
          <Button onClick={handleSaveClick}>
            <Save className="w-4 h-4 mr-2" />
            Save Theme
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );

  function renderColorPicker(label: string, path: string, color: string) {
    const isActive = activeColorPicker === path;

    return (
      <div className="space-y-2 relative">
        <Label>{label}</Label>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={() => setActiveColorPicker(isActive ? null : path)}
            className="w-12 h-10 rounded border border-input"
            style={{ backgroundColor: color }}
          />
          <Input
            value={color}
            onChange={(e) => updateColorField(path, e.target.value)}
            className="flex-1"
          />
        </div>
        {isActive && (
          <div className="absolute z-50 mt-2">
            {/* biome-ignore lint/a11y/useKeyWithClickEvents: overlay backdrop for color picker dismissal */}
            {/* biome-ignore lint/a11y/noStaticElementInteractions: overlay backdrop for color picker dismissal */}
            <div className="fixed inset-0" onClick={() => setActiveColorPicker(null)} />
            <ChromePicker
              color={color}
              onChange={(c) => updateColorField(path, c.hex)}
              disableAlpha={false}
            />
          </div>
        )}
      </div>
    );
  }
}

function createDefaultTheme(): QbitTheme {
  return {
    schemaVersion: "1.0.0",
    name: "Custom Theme",
    version: "1.0.0",
    colors: {
      ui: {
        background: "#0a0a0a",
        foreground: "#e9ecf5",
        card: "#1a1b26",
        cardForeground: "#e9ecf5",
        popover: "#1a1b26",
        popoverForeground: "#e9ecf5",
        primary: "#bb9af7",
        primaryForeground: "#1a1b26",
        secondary: "#414868",
        secondaryForeground: "#e9ecf5",
        muted: "#414868",
        mutedForeground: "#8e8e8e",
        accent: "#414868",
        accentForeground: "#e9ecf5",
        destructive: "#f7768e",
        border: "#414868",
        input: "#414868",
        ring: "#bb9af7",
        sidebar: "#1a1b26",
        sidebarForeground: "#e9ecf5",
        sidebarPrimary: "#bb9af7",
        sidebarPrimaryForeground: "#1a1b26",
        sidebarAccent: "#414868",
        sidebarAccentForeground: "#e9ecf5",
        sidebarBorder: "#414868",
        sidebarRing: "#bb9af7",
      },
      ansi: {
        black: "#414868",
        red: "#f7768e",
        green: "#9ece6a",
        yellow: "#e0af68",
        blue: "#7aa2f7",
        magenta: "#bb9af7",
        cyan: "#7dcfff",
        white: "#c0caf5",
        brightBlack: "#565f89",
        brightRed: "#ff9e9e",
        brightGreen: "#b9f27c",
        brightYellow: "#ffd07b",
        brightBlue: "#99b4ff",
        brightMagenta: "#d4b8ff",
        brightCyan: "#a6e4ff",
        brightWhite: "#e9ecf5",
        defaultFg: "#c0caf5",
        defaultBg: "#1a1b26",
      },
    },
    typography: {
      ui: {
        fontFamily: "Inter",
      },
      terminal: {
        fontFamily: "JetBrains Mono",
        fontSize: 14,
      },
    },
    radii: {
      base: "0.5rem",
    },
    terminal: {
      cursorStyle: "block",
      cursorBlink: true,
    },
  };
}

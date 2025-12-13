import { convertFileSrc } from "@tauri-apps/api/core";
import type { Terminal as XTerm } from "@xterm/xterm";
import { getThemeAssetPath } from "../themes";
// Import builtin theme assets directly (use ?url to get the asset path)
import obsidianEmberBg from "./builtin/obsidian-ember/assets/background.jpeg?url";
import { ThemeRegistry } from "./registry";
import type { QbitTheme } from "./types";

// Import builtin theme assets
const builtinAssets: Record<string, Record<string, string>> = {
  "obsidian-ember": {
    "assets/background.jpeg": obsidianEmberBg,
  },
};

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

type ThemeListeners = Array<(t: QbitTheme | null) => void>;

class ThemeManagerImpl {
  private currentTheme: QbitTheme | null = null;
  private currentThemeId: string | null = null;
  private listeners: ThemeListeners = [];
  private styleElement: HTMLStyleElement | null = null;

  getTheme() {
    return this.currentTheme;
  }

  getThemeId() {
    return this.currentThemeId;
  }

  onChange(listener: (t: QbitTheme | null) => void) {
    this.listeners.push(listener);
    return () => {
      this.listeners = this.listeners.filter((l) => l !== listener);
    };
  }

  /**
   * Apply a theme from the registry by ID
   */
  async applyThemeById(themeId: string): Promise<boolean> {
    const theme = ThemeRegistry.get(themeId);
    if (!theme) {
      console.warn(`Theme not found in registry: ${themeId}`);
      return false;
    }

    this.currentTheme = theme;
    this.currentThemeId = themeId;
    await this.injectThemeStyles(theme);

    // Load Google Fonts if specified
    if (theme.typography?.ui?.fontFamily) {
      const uiFont = theme.typography.ui.fontFamily.split(",")[0].trim().replace(/['"]/g, "");
      loadGoogleFont(uiFont);
    }
    if (theme.typography?.terminal?.fontFamily) {
      const terminalFont = theme.typography.terminal.fontFamily
        .split(",")[0]
        .trim()
        .replace(/['"]/g, "");
      loadGoogleFont(terminalFont);
    }

    try {
      localStorage.setItem("qbit.currentThemeId", themeId);
    } catch (e) {
      console.warn("Failed to persist theme ID:", e);
    }

    this.emit();
    return true;
  }

  /**
   * Load and apply a custom theme object (for user uploads)
   * @param themeId Optional theme ID to use. If not provided, generates from theme name
   */
  async loadThemeFromObject(
    theme: QbitTheme,
    assets?: Array<[string, Uint8Array]>,
    themeId?: string
  ): Promise<void> {
    // If themeId is explicitly provided, use it as-is (for overwriting)
    // Otherwise, generate a safe theme ID from the theme name
    const customId = themeId || theme.name.toLowerCase().replace(/[^a-z0-9-]/g, "-");

    // Save the theme to filesystem and registry
    await ThemeRegistry.saveTheme(customId, theme, assets);

    // Apply the theme
    await this.applyThemeById(customId);
  }

  /**
   * Apply a theme for preview without saving to registry
   * Used by theme designer for live preview
   */
  async applyThemePreview(theme: QbitTheme): Promise<void> {
    // Apply the theme styles but don't update currentTheme/currentThemeId
    // This prevents the preview from being treated as the "current" theme
    await this.injectThemeStyles(theme);

    // Load Google Fonts if specified
    if (theme.typography?.ui?.fontFamily) {
      const uiFont = theme.typography.ui.fontFamily.split(",")[0].trim().replace(/['"]/g, "");
      loadGoogleFont(uiFont);
    }
    if (theme.typography?.terminal?.fontFamily) {
      const terminalFont = theme.typography.terminal.fontFamily
        .split(",")[0]
        .trim()
        .replace(/['"]/g, "");
      loadGoogleFont(terminalFont);
    }

    // Don't emit theme change - this is just a preview
    // Don't update currentTheme or currentThemeId
  }

  /**
   * Try to restore the last used theme from localStorage
   */
  async tryLoadPersistedTheme(): Promise<boolean> {
    try {
      const themeId = localStorage.getItem("qbit.currentThemeId");
      if (!themeId) return false;

      // Check if theme exists in registry
      if (ThemeRegistry.has(themeId)) {
        return await this.applyThemeById(themeId);
      }

      // Fallback: try loading from old format
      const raw = localStorage.getItem("qbit.theme");
      if (raw) {
        const obj = JSON.parse(raw) as QbitTheme;
        await this.loadThemeFromObject(obj);
        return true;
      }

      return false;
    } catch (e) {
      console.warn("Failed to load persisted theme:", e);
      return false;
    }
  }

  applyToTerminal(term: XTerm) {
    if (!this.currentTheme) return;
    const t = this.currentTheme;
    const ansi = t.colors.ansi;
    const hasBgImage = !!t.background?.image;
    const xtermTheme = {
      background: hasBgImage ? "rgba(0,0,0,0)" : (ansi.defaultBg ?? t.colors.ui.background),
      foreground: ansi.defaultFg ?? t.colors.ui.foreground,
      cursor: "#ff0000",
      cursorAccent: "#ffffff",
      selectionBackground: t.terminal?.selectionBackground ?? ansi.blue,
      black: ansi.black,
      red: ansi.red,
      green: ansi.green,
      yellow: ansi.yellow,
      blue: ansi.blue,
      magenta: ansi.magenta,
      cyan: ansi.cyan,
      white: ansi.white,
      brightBlack: ansi.brightBlack,
      brightRed: ansi.brightRed,
      brightGreen: ansi.brightGreen,
      brightYellow: ansi.brightYellow,
      brightBlue: ansi.brightBlue,
      brightMagenta: ansi.brightMagenta,
      brightCyan: ansi.brightCyan,
      brightWhite: ansi.brightWhite,
    } as const;

    // xterm@5 uses options property - set individual properties to trigger updates
    term.options.theme = xtermTheme;
    if (t.typography?.terminal?.fontFamily) {
      term.options.fontFamily = t.typography.terminal.fontFamily;
    }
    if (t.typography?.terminal?.fontSize) {
      term.options.fontSize = t.typography.terminal.fontSize;
    }
    if (t.terminal?.cursorBlink !== undefined) {
      term.options.cursorBlink = t.terminal.cursorBlink;
    }
    if (t.terminal?.cursorStyle) {
      term.options.cursorStyle = t.terminal.cursorStyle;
    }
  }

  /**
   * Inject theme styles using a style element for better performance
   */
  private async injectThemeStyles(theme: QbitTheme): Promise<void> {
    const root = document.documentElement;

    // Set theme name as data attribute for CSS targeting
    root.setAttribute("data-theme", theme.name);

    // Remove existing theme style element if present
    if (this.styleElement) {
      this.styleElement.remove();
    }

    // Create new style element
    this.styleElement = document.createElement("style");
    this.styleElement.id = "qbit-theme-vars";

    // Build CSS variable declarations
    const cssVars: string[] = [];

    // UI colors
    const ui = theme.colors.ui;
    cssVars.push(`--background: ${ui.background};`);
    cssVars.push(`--foreground: ${ui.foreground};`);
    cssVars.push(`--card: ${ui.card};`);
    cssVars.push(`--card-foreground: ${ui.cardForeground};`);
    cssVars.push(`--popover: ${ui.popover};`);
    cssVars.push(`--popover-foreground: ${ui.popoverForeground};`);
    cssVars.push(`--primary: ${ui.primary};`);
    cssVars.push(`--primary-foreground: ${ui.primaryForeground};`);
    cssVars.push(`--secondary: ${ui.secondary};`);
    cssVars.push(`--secondary-foreground: ${ui.secondaryForeground};`);
    cssVars.push(`--muted: ${ui.muted};`);
    cssVars.push(`--muted-foreground: ${ui.mutedForeground};`);
    cssVars.push(`--accent: ${ui.accent};`);
    cssVars.push(`--accent-foreground: ${ui.accentForeground};`);
    cssVars.push(`--destructive: ${ui.destructive};`);
    cssVars.push(`--border: ${ui.border};`);
    cssVars.push(`--input: ${ui.input};`);
    cssVars.push(`--ring: ${ui.ring};`);

    // Chart colors
    if (ui.chart) {
      cssVars.push(`--chart-1: ${ui.chart.c1};`);
      cssVars.push(`--chart-2: ${ui.chart.c2};`);
      cssVars.push(`--chart-3: ${ui.chart.c3};`);
      cssVars.push(`--chart-4: ${ui.chart.c4};`);
      cssVars.push(`--chart-5: ${ui.chart.c5};`);
    }

    // Sidebar colors
    cssVars.push(`--sidebar: ${ui.sidebar};`);
    cssVars.push(`--sidebar-foreground: ${ui.sidebarForeground};`);
    cssVars.push(`--sidebar-primary: ${ui.sidebarPrimary};`);
    cssVars.push(`--sidebar-primary-foreground: ${ui.sidebarPrimaryForeground};`);
    cssVars.push(`--sidebar-accent: ${ui.sidebarAccent};`);
    cssVars.push(`--sidebar-accent-foreground: ${ui.sidebarAccentForeground};`);
    cssVars.push(`--sidebar-border: ${ui.sidebarBorder};`);
    cssVars.push(`--sidebar-ring: ${ui.sidebarRing};`);

    // ANSI colors
    const ansi = theme.colors.ansi;
    cssVars.push(`--ansi-black: ${ansi.black};`);
    cssVars.push(`--ansi-red: ${ansi.red};`);
    cssVars.push(`--ansi-green: ${ansi.green};`);
    cssVars.push(`--ansi-yellow: ${ansi.yellow};`);
    cssVars.push(`--ansi-blue: ${ansi.blue};`);
    cssVars.push(`--ansi-magenta: ${ansi.magenta};`);
    cssVars.push(`--ansi-cyan: ${ansi.cyan};`);
    cssVars.push(`--ansi-white: ${ansi.white};`);
    cssVars.push(`--ansi-bright-black: ${ansi.brightBlack};`);
    cssVars.push(`--ansi-bright-red: ${ansi.brightRed};`);
    cssVars.push(`--ansi-bright-green: ${ansi.brightGreen};`);
    cssVars.push(`--ansi-bright-yellow: ${ansi.brightYellow};`);
    cssVars.push(`--ansi-bright-blue: ${ansi.brightBlue};`);
    cssVars.push(`--ansi-bright-magenta: ${ansi.brightMagenta};`);
    cssVars.push(`--ansi-bright-cyan: ${ansi.brightCyan};`);
    cssVars.push(`--ansi-bright-white: ${ansi.brightWhite};`);
    cssVars.push(`--ansi-default-fg: ${ansi.defaultFg};`);
    cssVars.push(`--ansi-default-bg: ${ansi.defaultBg};`);

    // Radii
    if (theme.radii?.base) cssVars.push(`--radius: ${theme.radii.base};`);
    if (theme.radii?.sm) cssVars.push(`--radius-sm: ${theme.radii.sm};`);
    if (theme.radii?.md) cssVars.push(`--radius-md: ${theme.radii.md};`);
    if (theme.radii?.lg) cssVars.push(`--radius-lg: ${theme.radii.lg};`);
    if (theme.radii?.xl) cssVars.push(`--radius-xl: ${theme.radii.xl};`);

    // Background settings
    if (theme.background?.image) {
      let src = theme.background.image;

      // Handle theme asset paths
      if (src.startsWith("assets/") && this.currentThemeId) {
        // Check if this is a builtin theme first
        const entry = ThemeRegistry.getEntry(this.currentThemeId);
        if (entry?.builtin && builtinAssets[this.currentThemeId]?.[src]) {
          // Use bundled asset for builtin themes
          src = builtinAssets[this.currentThemeId][src];
          console.log("[ThemeManager] Using builtin asset:", src);
        } else {
          // Get the absolute path from Tauri for user themes
          try {
            const filePath = await getThemeAssetPath(this.currentThemeId, src);
            console.log("[ThemeManager] Got file path from Tauri:", filePath);
            // Convert the file path to a Tauri asset URL
            src = convertFileSrc(filePath);
            console.log("[ThemeManager] Converted to asset URL:", src);
          } catch (error) {
            console.warn(`Failed to resolve theme asset: ${src}`, error);
            // Fallback to direct path
            src = `/${this.currentThemeId}/${src}`;
          }
        }
      } else if (/^\//.test(src) && typeof window !== "undefined") {
        // Ensure absolute URL for Vite/Tauri
        try {
          src = new URL(src, window.location.origin).toString();
        } catch {}
      }

      console.log("[ThemeManager] Final background image URL:", src);
      cssVars.push(`--background-image: ${src};`);
      cssVars.push(`--background-image-url: url(${src});`);
    }
    if (theme.background?.size) {
      cssVars.push(`--background-size: ${theme.background.size};`);
    }
    if (theme.background?.position) {
      cssVars.push(`--background-position: ${theme.background.position};`);
    }
    if (theme.background?.opacity !== undefined) {
      cssVars.push(`--background-opacity: ${theme.background.opacity};`);
    }

    // Typography via CSS variables for better Tailwind integration
    if (theme.typography?.ui?.fontFamily) {
      cssVars.push(`--font-family-ui: ${theme.typography.ui.fontFamily};`);
    }
    if (theme.typography?.ui?.headingFamily) {
      cssVars.push(`--font-family-heading: ${theme.typography.ui.headingFamily};`);
    }

    // Inject the CSS
    const cssContent = `:root { ${cssVars.join(" ")} }`;
    this.styleElement.textContent = cssContent;

    // Add typography rules if specified
    if (theme.typography?.ui?.fontFamily || theme.typography?.ui?.headingFamily) {
      let typographyCss = "";
      if (theme.typography.ui.fontFamily) {
        typographyCss += `body { font-family: var(--font-family-ui) !important; }`;
      }
      if (theme.typography.ui.headingFamily) {
        typographyCss += `h1, h2, h3, h4, h5, h6 { font-family: var(--font-family-heading) !important; }`;
      }
      this.styleElement.textContent += typographyCss;
    }

    document.head.appendChild(this.styleElement);

    // Force a style recalculation to ensure fonts are applied
    // This is needed when reverting themes to ensure the browser picks up the change
    document.body.offsetHeight;
  }

  private emit() {
    for (const l of this.listeners) l(this.currentTheme);
  }
}

export const ThemeManager = new ThemeManagerImpl();

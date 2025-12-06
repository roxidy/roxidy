import Ajv from "ajv";
import schema from "./schema.json" with { type: "json" };
import { ThemeManager } from "./ThemeManager";
import type { QbitTheme } from "./types";

/**
 * Load and validate a theme from a file upload
 */
export async function loadThemeFromFile(file: File): Promise<void> {
  const text = await file.text();
  const raw = JSON.parse(text);
  const theme = normalizeTheme(raw);
  validateTheme(theme);
  await ThemeManager.loadThemeFromObject(theme);
}

/**
 * Load a theme from a directory (with assets)
 */
export async function loadThemeFromDirectory(files: FileList): Promise<void> {
  // Find theme.json or theme.ts file
  let themeFile: File | null = null;
  const assetFiles: Array<[string, File]> = [];

  for (let i = 0; i < files.length; i++) {
    const file = files[i];
    const relativePath = file.webkitRelativePath;
    const pathParts = relativePath.split("/");
    const fileName = pathParts[pathParts.length - 1];

    if (fileName === "theme.json" || fileName === "theme.ts") {
      themeFile = file;
    } else if (relativePath.includes("/assets/")) {
      // Extract asset path relative to theme root (e.g., "assets/background.jpeg")
      const assetIndex = pathParts.indexOf("assets");
      const assetPath = pathParts.slice(assetIndex).join("/");
      assetFiles.push([assetPath, file]);
    }
  }

  if (!themeFile) {
    throw new Error("No theme.json or theme.ts file found in directory");
  }

  // Load and parse the theme
  const text = await themeFile.text();
  let raw: unknown;

  if (themeFile.name.endsWith(".ts")) {
    // Extract the exported object from TypeScript file
    // Strip out import statements and evaluate the rest

    try {
      // Remove all import statements (they won't work in Function constructor anyway)
      const withoutImports = text.replace(/import\s+.*?from\s+['"].*?['"];?\s*/g, "");

      // Find the exported variable name
      const exportMatch = withoutImports.match(/export\s+const\s+(\w+)\s*=/);
      const exportDefaultMatch = withoutImports.match(/export\s+default\s+/);

      if (exportMatch) {
        // export const themeName = { ... }
        const varName = exportMatch[1];
        // Execute the code and return the exported variable
        raw = new Function(`
          ${withoutImports.replace(/export\s+const\s+/, "const ")}
          return ${varName};
        `)();
      } else if (exportDefaultMatch) {
        // export default { ... }
        raw = new Function(`
          ${withoutImports.replace(/export\s+default\s+/, "return ")}
        `)();
      } else {
        throw new Error("No export statement found");
      }
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      throw new Error(`Could not evaluate TypeScript theme: ${errorMsg}`);
    }
  } else {
    raw = JSON.parse(text);
  }

  const theme = normalizeTheme(raw);
  validateTheme(theme);

  // Convert asset files to Uint8Array
  const assets: Array<[string, Uint8Array]> = [];
  for (const [path, file] of assetFiles) {
    const arrayBuffer = await file.arrayBuffer();
    assets.push([path, new Uint8Array(arrayBuffer)]);
  }

  // Load theme with assets
  await ThemeManager.loadThemeFromObject(theme, assets);
}

/**
 * Load and validate a theme from a URL
 */
export async function loadThemeFromUrl(url: string): Promise<void> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`Failed to fetch theme: ${res.status}`);
  const raw = await res.json();
  const theme = normalizeTheme(raw);
  validateTheme(theme);
  await ThemeManager.loadThemeFromObject(theme);
}

/**
 * Apply a theme object directly (for programmatic use)
 */
export async function applyTheme(theme: QbitTheme): Promise<void> {
  await ThemeManager.loadThemeFromObject(theme);
}

/**
 * Validate a theme against the JSON schema
 */
function validateTheme(theme: QbitTheme): void {
  const ajv = new Ajv({ allErrors: true, strict: false });
  const localSchema = { ...schema };
  // biome-ignore lint/suspicious/noExplicitAny: Schema manipulation requires any
  delete (localSchema as any).$schema;
  // biome-ignore lint/suspicious/noExplicitAny: AJV compile requires any
  const validate = ajv.compile(localSchema as any);
  const valid = validate(theme);

  if (!valid) {
    console.warn("Theme validation warnings:", validate.errors);
    // Continue with best-effort apply instead of throwing
  }
}

// Convert known external theme formats into QbitTheme
// biome-ignore lint/suspicious/noExplicitAny: Input theme format is unknown
function normalizeTheme(raw: any): QbitTheme {
  // If already in Qbit format
  if (raw?.colors?.ui && raw?.colors?.ansi) return raw as QbitTheme;

  if (raw?.ui && raw?.ansi) {
    const ui = raw.ui ?? {};
    const ansi = raw.ansi ?? {};
    const term = raw.terminal ?? {};
    const name = raw.name ?? "Custom Theme";

    // Map to QbitTheme structure
    const qbit: QbitTheme = {
      schemaVersion: "1.0.0",
      name,
      version: raw.version,
      colors: {
        ui: {
          background: ui.background,
          foreground: ui.foreground,
          card: ui.card,
          cardForeground: ui.cardForeground,
          popover: ui.popover,
          popoverForeground: ui.popoverForeground,
          primary: ui.primary,
          primaryForeground: ui.primaryForeground,
          secondary: ui.secondary,
          secondaryForeground: ui.secondaryForeground,
          muted: ui.muted,
          mutedForeground: ui.mutedForeground,
          accent: ui.accent,
          accentForeground: ui.accentForeground,
          destructive: ui.destructive,
          border: ui.border,
          input: ui.input,
          ring: ui.ring,
          // charts
          ...(ui.chart
            ? {
                "chart.c1": ui.chart.c1,
                "chart.c2": ui.chart.c2,
                "chart.c3": ui.chart.c3,
                "chart.c4": ui.chart.c4,
                "chart.c5": ui.chart.c5,
              }
            : {}),
          // sidebar (fallback to background/foreground if not present)
          sidebar: ui.sidebar ?? ui.background,
          sidebarForeground: ui.sidebarForeground ?? ui.foreground,
          sidebarPrimary: ui.sidebarPrimary ?? ui.primary,
          sidebarPrimaryForeground: ui.sidebarPrimaryForeground ?? ui.primaryForeground,
          sidebarAccent: ui.sidebarAccent ?? ui.accent,
          sidebarAccentForeground: ui.sidebarAccentForeground ?? ui.accentForeground,
          sidebarBorder: ui.sidebarBorder ?? ui.border,
          sidebarRing: ui.sidebarRing ?? ui.ring,
        },
        ansi: {
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
          defaultFg: term.foreground ?? ui.foreground,
          defaultBg: term.background ?? ui.background,
        },
      },
      typography: {
        terminal: {
          fontFamily: raw.typography?.terminal?.fontFamily,
          fontSize: raw.typography?.terminal?.fontSize,
        },
        ui: {
          fontFamily: raw.typography?.ui?.fontFamily,
          headingFamily: raw.typography?.ui?.headingFamily,
        },
      },
      radii: {
        base: raw.radii?.base,
        sm: raw.radii?.sm,
        md: raw.radii?.md,
        lg: raw.radii?.lg,
        xl: raw.radii?.xl,
      },
      background: {
        image: term.backgroundImage,
        size: term.backgroundSize,
        position: term.backgroundPosition,
        opacity: term.opacity,
      },
      terminal: {
        selectionBackground: term.selection ?? term.selectionBackground,
        cursorBlink: term.cursorBlink,
        cursorStyle: term.cursorStyle,
      },
      effects: {
        cursor: raw.effects?.cursor,
        plugins: raw.effects?.plugins,
      },
    };
    return qbit;
  }

  // Fallback pass-through
  return raw as QbitTheme;
}

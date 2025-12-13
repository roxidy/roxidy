import {
  deleteTheme as deleteTauriTheme,
  listThemes,
  readTheme,
  saveTheme as saveTauriTheme,
} from "../themes";
import { registerBuiltinThemes } from "./builtin";
import type { QbitTheme, ThemeRegistryEntry } from "./types";

/**
 * Central registry for all available themes
 */
class ThemeRegistryClass {
  private themes = new Map<string, ThemeRegistryEntry>();
  private listeners: Array<() => void> = [];
  private initialized = false;

  /**
   * Initialize the registry by loading themes from filesystem
   */
  async initialize(): Promise<void> {
    if (this.initialized) return;

    // Register builtin themes first
    registerBuiltinThemes();

    try {
      // Load user themes from ~/.qbit/themes/
      const userThemes = await listThemes();

      for (const themeInfo of userThemes) {
        try {
          const themeJson = await readTheme(themeInfo.name);
          const theme = JSON.parse(themeJson) as QbitTheme;

          // Don't overwrite builtin themes with user themes
          if (this.themes.has(themeInfo.name)) {
            console.warn(
              `[ThemeRegistry] Skipping user theme '${themeInfo.name}' - conflicts with builtin theme`
            );
            continue;
          }

          // Register as a custom theme (not builtin)
          this.register(themeInfo.name, theme, false);
        } catch (error) {
          console.warn(`Failed to load theme '${themeInfo.name}':`, error);
        }
      }

      this.initialized = true;
    } catch (error) {
      console.warn("Failed to load themes from filesystem:", error);
      // Continue with just builtin themes
      this.initialized = true;
    }
  }

  /**
   * Register a theme in the registry
   */
  register(id: string, theme: QbitTheme, builtin = false): void {
    this.themes.set(id, { id, theme, builtin });
    this.emit();
  }

  /**
   * Unregister a theme (only custom themes can be removed)
   */
  async unregister(id: string): Promise<boolean> {
    const entry = this.themes.get(id);
    if (!entry || entry.builtin) {
      return false;
    }

    try {
      // Delete from filesystem
      await deleteTauriTheme(id);

      // Remove from registry
      this.themes.delete(id);
      this.emit();
      return true;
    } catch (error) {
      console.error(`Failed to delete theme '${id}' from filesystem:`, error);
      return false;
    }
  }

  /**
   * Save a theme to the filesystem and register it
   */
  async saveTheme(
    id: string,
    theme: QbitTheme,
    assets?: Array<[string, Uint8Array]>
  ): Promise<boolean> {
    try {
      // Save to filesystem
      const themeJson = JSON.stringify(theme, null, 2);
      await saveTauriTheme(id, themeJson);

      // Save assets if provided
      if (assets && assets.length > 0) {
        const { saveThemeAsset } = await import("../themes");
        for (const [assetPath, data] of assets) {
          // Extract filename from path (e.g., "assets/background.jpeg" -> "background.jpeg")
          const filename = assetPath.split("/").pop() || assetPath;
          await saveThemeAsset(id, filename, data);
        }
      }

      // Register in memory
      this.register(id, theme, false);

      return true;
    } catch (error) {
      console.error(`Failed to save theme '${id}':`, error);
      return false;
    }
  }

  /**
   * Get a theme by ID
   */
  get(id: string): QbitTheme | null {
    const entry = this.themes.get(id);
    return entry?.theme ?? null;
  }

  /**
   * Get a theme entry (includes metadata) by ID
   */
  getEntry(id: string): ThemeRegistryEntry | null {
    return this.themes.get(id) ?? null;
  }

  /**
   * Get all registered themes
   */
  getAll(): ThemeRegistryEntry[] {
    return Array.from(this.themes.values());
  }

  /**
   * Get all builtin themes
   */
  getBuiltin(): ThemeRegistryEntry[] {
    return this.getAll().filter((t) => t.builtin);
  }

  /**
   * Get all custom themes
   */
  getCustom(): ThemeRegistryEntry[] {
    return this.getAll().filter((t) => !t.builtin);
  }

  /**
   * Check if a theme exists
   */
  has(id: string): boolean {
    return this.themes.has(id);
  }

  /**
   * Generate a unique theme ID by checking for duplicates.
   * If the ID exists, appends " - 1", " - 2", etc.
   * 
   * Examples:
   * - "catherine" -> "catherine" (if doesn't exist)
   * - "catherine" -> "catherine - 1" (if "catherine" exists)
   * - "catherine" -> "catherine - 2" (if "catherine" and "catherine - 1" exist)
   */
  getUniqueThemeId(baseId: string): string {
    // If the base ID doesn't exist, use it as-is
    if (!this.has(baseId)) {
      return baseId;
    }

    // Find the next available number
    let counter = 1;
    let uniqueId = `${baseId} - ${counter}`;
    
    while (this.has(uniqueId)) {
      counter++;
      uniqueId = `${baseId} - ${counter}`;
    }

    return uniqueId;
  }

  /**
   * Subscribe to registry changes
   */
  onChange(callback: () => void): () => void {
    this.listeners.push(callback);
    return () => {
      this.listeners = this.listeners.filter((l) => l !== callback);
    };
  }

  private emit(): void {
    this.listeners.forEach((l) => {
      l();
    });
  }
}

export const ThemeRegistry = new ThemeRegistryClass();

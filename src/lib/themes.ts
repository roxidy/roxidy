import { invoke } from "@tauri-apps/api/core";

export interface ThemeInfo {
  name: string;
  path: string;
}

/**
 * List all available themes in ~/.qbit/themes/
 */
export async function listThemes(): Promise<ThemeInfo[]> {
  return await invoke<ThemeInfo[]>("list_themes");
}

/**
 * Read a theme from ~/.qbit/themes/{theme_name}/theme.json
 */
export async function readTheme(themeName: string): Promise<string> {
  return await invoke<string>("read_theme", { themeName });
}

/**
 * Save a theme to ~/.qbit/themes/{theme_name}/theme.json
 */
export async function saveTheme(themeName: string, themeData: string): Promise<string> {
  return await invoke<string>("save_theme", { themeName, themeData });
}

/**
 * Delete a theme from ~/.qbit/themes/{theme_name}/
 */
export async function deleteTheme(themeName: string): Promise<void> {
  return await invoke<void>("delete_theme", { themeName });
}

/**
 * Save a theme asset (like background images) to ~/.qbit/themes/{theme_name}/assets/{filename}
 */
export async function saveThemeAsset(
  themeName: string,
  filename: string,
  data: Uint8Array
): Promise<string> {
  return await invoke<string>("save_theme_asset", {
    themeName,
    filename,
    data: Array.from(data),
  });
}

/**
 * Get the absolute path to a theme asset
 */
export async function getThemeAssetPath(themeName: string, assetPath: string): Promise<string> {
  return await invoke<string>("get_theme_asset_path", { themeName, assetPath });
}

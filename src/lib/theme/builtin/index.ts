import { ThemeRegistry } from "../registry";
import { obsidianEmber } from "./obsidian-ember/theme";
import { qbitTheme } from "./qbit/theme";

/**
 * Register all builtin themes
 */
export function registerBuiltinThemes(): void {
  ThemeRegistry.register("qbit", qbitTheme, true);
  ThemeRegistry.register("obsidian-ember", obsidianEmber, true);
}

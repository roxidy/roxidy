import { createContext, type ReactNode, useContext, useEffect, useState } from "react";
import { ThemeRegistry } from "../lib/theme/registry";
import { ThemeManager } from "../lib/theme/ThemeManager";
import type { QbitTheme } from "../lib/theme/types";

interface ThemeContextValue {
  currentTheme: QbitTheme | null;
  currentThemeId: string | null;
  availableThemes: Array<{ id: string; name: string; builtin: boolean; theme: QbitTheme }>;
  setTheme: (themeId: string) => Promise<boolean>;
  loadCustomTheme: (theme: QbitTheme) => Promise<void>;
  deleteTheme: (themeId: string) => Promise<boolean>;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

interface ThemeProviderProps {
  children: ReactNode;
  defaultThemeId?: string;
}

export function ThemeProvider({ children, defaultThemeId }: ThemeProviderProps) {
  const [currentTheme, setCurrentTheme] = useState<QbitTheme | null>(null);
  const [currentThemeId, setCurrentThemeId] = useState<string | null>(null);
  const [availableThemes, setAvailableThemes] = useState<
    Array<{ id: string; name: string; builtin: boolean; theme: QbitTheme }>
  >([]);

  // Initialize theme on mount
  useEffect(() => {
    const initTheme = async () => {
      // Initialize the theme registry (loads themes from filesystem)
      await ThemeRegistry.initialize();

      // Try to load persisted theme
      const loaded = await ThemeManager.tryLoadPersistedTheme();

      if (!loaded && defaultThemeId) {
        // Fall back to default theme if available
        await ThemeManager.applyThemeById(defaultThemeId);
      }

      // Update state
      setCurrentTheme(ThemeManager.getTheme());
      setCurrentThemeId(ThemeManager.getThemeId());
    };

    initTheme();
  }, [defaultThemeId]);

  // Subscribe to theme changes
  useEffect(() => {
    // Immediately sync state on mount/remount
    setCurrentTheme(ThemeManager.getTheme());
    setCurrentThemeId(ThemeManager.getThemeId());

    const unsubscribe = ThemeManager.onChange((theme) => {
      setCurrentTheme(theme);
      setCurrentThemeId(ThemeManager.getThemeId());
    });

    return unsubscribe;
  }, []);

  // Subscribe to registry changes
  useEffect(() => {
    const updateAvailableThemes = () => {
      const themes = ThemeRegistry.getAll().map((entry) => ({
        id: entry.id,
        name: entry.theme.name,
        builtin: entry.builtin ?? false,
        theme: entry.theme,
      }));
      setAvailableThemes(themes);
    };

    updateAvailableThemes();
    const unsubscribe = ThemeRegistry.onChange(updateAvailableThemes);

    return unsubscribe;
  }, []);

  const setTheme = async (themeId: string): Promise<boolean> => {
    const success = await ThemeManager.applyThemeById(themeId);
    if (success) {
      // Manually sync state to ensure UI updates even if listeners aren't working
      setCurrentTheme(ThemeManager.getTheme());
      setCurrentThemeId(ThemeManager.getThemeId());
    }
    return success;
  };

  const loadCustomTheme = async (theme: QbitTheme): Promise<void> => {
    await ThemeManager.loadThemeFromObject(theme);
  };

  const deleteTheme = async (themeId: string): Promise<boolean> => {
    return await ThemeRegistry.unregister(themeId);
  };

  const value: ThemeContextValue = {
    currentTheme,
    currentThemeId,
    availableThemes,
    setTheme,
    loadCustomTheme,
    deleteTheme,
  };

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const context = useContext(ThemeContext);
  if (!context) {
    throw new Error("useTheme must be used within a ThemeProvider");
  }
  return context;
}

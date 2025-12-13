/**
 * Utility functions for theme name validation and generation
 */

// Type for themes as returned by useTheme hook
export interface ThemeListItem {
  id: string;
  name: string;
  builtin: boolean;
}

/**
 * Generate a unique theme name by checking for duplicates.
 * Uses "(Copy)" or "(Copy N)" suffix pattern.
 *
 * Examples:
 * - "My Theme" -> "My Theme (Copy)" (if "My Theme" exists)
 * - "My Theme (Copy)" -> "My Theme (Copy 2)" (if "My Theme (Copy)" exists)
 * - "My Theme (Copy 2)" -> "My Theme (Copy 3)" (if "My Theme (Copy 2)" exists)
 */
export function getUniqueThemeName(baseName: string, availableThemes: ThemeListItem[]): string {
  // Check by name (not ID)
  const nameExists = (name: string) =>
    availableThemes.some((t) => t.name.toLowerCase() === name.toLowerCase());

  // If the base name doesn't exist, use it as-is
  if (!nameExists(baseName)) {
    return baseName;
  }

  // Try "Name (Copy)" first
  const copyName = `${baseName} (Copy)`;
  if (!nameExists(copyName)) {
    return copyName;
  }

  // Find the next available copy number
  let counter = 2;
  let uniqueName = `${baseName} (Copy ${counter})`;

  while (nameExists(uniqueName)) {
    counter++;
    uniqueName = `${baseName} (Copy ${counter})`;
  }

  return uniqueName;
}

/**
 * Check if a theme name already exists (case-insensitive)
 * @param name The theme name to check
 * @param availableThemes List of available themes
 * @param excludeId Optional theme ID to exclude from the check (for editing existing themes)
 * @returns true if the name exists
 */
export function themeNameExists(
  name: string,
  availableThemes: ThemeListItem[],
  excludeId?: string
): boolean {
  const trimmedName = name.trim().toLowerCase();
  return availableThemes.some((t) => t.id !== excludeId && t.name.toLowerCase() === trimmedName);
}

/**
 * Validate a theme name
 * @param name The theme name to validate
 * @param availableThemes List of available themes
 * @param excludeId Optional theme ID to exclude from uniqueness check
 * @returns An error message if invalid, or null if valid
 */
export function validateThemeName(
  name: string,
  availableThemes: ThemeListItem[],
  excludeId?: string
): string | null {
  const trimmedName = name.trim();

  if (!trimmedName) {
    return "Theme name cannot be empty";
  }

  if (themeNameExists(trimmedName, availableThemes, excludeId)) {
    return "A theme with this name already exists";
  }

  return null;
}

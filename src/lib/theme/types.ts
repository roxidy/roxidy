/**
 * Theme type definitions for Qbit
 * These types replace the JSON schema for compile-time type safety
 */

export interface QbitThemeMetadata {
  schemaVersion: string;
  name: string;
  version?: string;
  author?: string;
  license?: string;
  homepage?: string;
  tags?: string[];
}

export interface UIColors {
  background: string;
  foreground: string;
  card: string;
  cardForeground: string;
  popover: string;
  popoverForeground: string;
  primary: string;
  primaryForeground: string;
  secondary: string;
  secondaryForeground: string;
  muted: string;
  mutedForeground: string;
  accent: string;
  accentForeground: string;
  destructive: string;
  border: string;
  input: string;
  ring: string;
  chart?: {
    c1: string;
    c2: string;
    c3: string;
    c4: string;
    c5: string;
  };
  sidebar: string;
  sidebarForeground: string;
  sidebarPrimary: string;
  sidebarPrimaryForeground: string;
  sidebarAccent: string;
  sidebarAccentForeground: string;
  sidebarBorder: string;
  sidebarRing: string;
}

export interface AnsiColors {
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
  defaultFg: string;
  defaultBg: string;
}

export interface ThemeColors {
  ui: UIColors;
  ansi: AnsiColors;
}

export interface TerminalTypography {
  fontFamily?: string;
  fontSize?: number;
  lineHeight?: number;
}

export interface UITypography {
  fontFamily?: string;
  headingFamily?: string;
}

export interface ThemeTypography {
  terminal?: TerminalTypography;
  ui?: UITypography;
}

export interface ThemeRadii {
  base?: string;
  sm?: string;
  md?: string;
  lg?: string;
  xl?: string;
}

export type CursorStyle = "block" | "underline" | "bar";

export interface BackgroundSettings {
  image?: string;
  size?: string;
  position?: string;
  opacity?: number;
}

export interface TerminalSettings {
  cursorStyle?: CursorStyle;
  cursorBlink?: boolean;
  selectionBackground?: string;
  webgl?: boolean;
}

export interface CursorEffect {
  style?: string;
  color?: string;
}

export interface ThemePlugin {
  id: string;
  name?: string;
  entry: string;
  config?: Record<string, unknown>;
}

export interface ThemeEffects {
  cursor?: CursorEffect;
  plugins?: ThemePlugin[];
}

export interface QbitTheme extends QbitThemeMetadata {
  colors: ThemeColors;
  typography?: ThemeTypography;
  radii?: ThemeRadii;
  background?: BackgroundSettings;
  terminal?: TerminalSettings;
  effects?: ThemeEffects;
}

/**
 * Theme registry entry
 */
export interface ThemeRegistryEntry {
  id: string;
  theme: QbitTheme;
  builtin?: boolean;
}

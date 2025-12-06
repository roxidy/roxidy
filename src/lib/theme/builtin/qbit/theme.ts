import type { QbitTheme } from "../../types";

// Qbit Color Palette
const palette = {
  // Neutrals - grays from darkest to lightest
  gray950: "oklch(0.145 0 0)", // Darkest - main background
  gray900: "oklch(0.205 0 0)", // Very dark - cards/elevated surfaces
  gray800: "oklch(0.269 0 0)", // Secondary surfaces
  gray700: "oklch(0.556 0 0)", // Ring/focus states
  gray400: "oklch(0.708 0 0)", // Muted text
  gray50: "oklch(0.985 0 0)", // Lightest - primary text

  // Primary brand color
  primary: "oklch(0.922 0 0)", // Light gray primary
  primaryDark: "oklch(0.205 0 0)", // Dark text on primary

  // Destructive/error states
  destructive: "oklch(0.704 0.191 22.216)", // Red

  // Chart colors
  chartPurple: "oklch(0.488 0.243 264.376)",
  chartGreen: "oklch(0.696 0.17 162.48)",
  chartYellow: "oklch(0.769 0.188 70.08)",
  chartMagenta: "oklch(0.627 0.265 303.9)",
  chartOrange: "oklch(0.645 0.246 16.439)",

  // ANSI terminal colors
  ansiBlack: "#414868",
  ansiBlue: "#7aa2f7",
  ansiBrightBlack: "#565f89",
  ansiBrightBlue: "#99b4ff",
  ansiBrightCyan: "#a6e4ff",
  ansiBrightGreen: "#b9f27c",
  ansiBrightMagenta: "#d4b8ff",
  ansiBrightRed: "#ff9e9e",
  ansiBrightWhite: "#e9ecf5",
  ansiBrightYellow: "#ffd07b",
  ansiCyan: "#7dcfff",
  ansiDefaultBg: "#1a1b26",
  ansiDefaultFg: "#c0caf5",
  ansiGreen: "#9ece6a",
  ansiMagenta: "#bb9af7",
  ansiRed: "#f7768e",
  ansiWhite: "#c0caf5",
  ansiYellow: "#e0af68",
};

export const qbitTheme: QbitTheme = {
  author: "Qbit Team",
  license: "MIT",
  name: "Qbit",
  schemaVersion: "1.0.0",
  version: "1.0.0",

  colors: {
    ansi: {
      black: palette.ansiBlack,
      blue: palette.ansiBlue,
      brightBlack: palette.ansiBrightBlack,
      brightBlue: palette.ansiBrightBlue,
      brightCyan: palette.ansiBrightCyan,
      brightGreen: palette.ansiBrightGreen,
      brightMagenta: palette.ansiBrightMagenta,
      brightRed: palette.ansiBrightRed,
      brightWhite: palette.ansiBrightWhite,
      brightYellow: palette.ansiBrightYellow,
      cyan: palette.ansiCyan,
      defaultBg: palette.ansiDefaultBg,
      defaultFg: palette.ansiDefaultFg,
      green: palette.ansiGreen,
      magenta: palette.ansiMagenta,
      red: palette.ansiRed,
      white: palette.ansiWhite,
      yellow: palette.ansiYellow,
    },

    ui: {
      accent: palette.gray800,
      accentForeground: palette.gray50,
      background: palette.gray950,
      border: "oklch(1 0 0 / 10%)",
      card: palette.gray900,
      cardForeground: palette.gray50,

      chart: {
        c1: palette.chartPurple,
        c2: palette.chartGreen,
        c3: palette.chartYellow,
        c4: palette.chartMagenta,
        c5: palette.chartOrange,
      },

      destructive: palette.destructive,
      foreground: palette.gray50,
      input: "oklch(1 0 0 / 15%)",
      muted: palette.gray800,
      mutedForeground: palette.gray400,
      popover: palette.gray900,
      popoverForeground: palette.gray50,
      primary: palette.primary,
      primaryForeground: palette.primaryDark,
      ring: palette.gray700,
      secondary: palette.gray800,
      secondaryForeground: palette.gray50,
      sidebar: palette.gray900,
      sidebarAccent: palette.gray800,
      sidebarAccentForeground: palette.gray50,
      sidebarBorder: "oklch(1 0 0 / 10%)",
      sidebarForeground: palette.gray50,
      sidebarPrimary: palette.chartPurple,
      sidebarPrimaryForeground: palette.gray50,
      sidebarRing: palette.gray700,
    },
  },

  effects: {
    plugins: [],
  },

  radii: {
    base: "0.5rem",
  },

  terminal: {
    cursorBlink: true,
    cursorStyle: "block",
    selectionBackground: palette.gray800,
  },

  typography: {
    terminal: {
      fontFamily: "monospace",
      fontSize: 14,
    },
  },
};

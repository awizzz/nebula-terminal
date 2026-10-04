import type { TabColor } from "./types";

export interface TerminalTheme {
  id: string;
  name: string;
  scheme: "dark" | "light";
  accent: string;
  background: string;
  foreground: string;
  /** ANSI 0–15: black, red, green, yellow, blue, magenta, cyan, white, then the bright variants. */
  ansi: [string, string, string, string, string, string, string, string, string, string, string, string, string, string, string, string];
}

export const themes: TerminalTheme[] = [
  {
    id: "nebula",
    name: "Nebula",
    scheme: "dark",
    accent: "#e8a33d",
    background: "#111214",
    foreground: "#d9dce1",
    ansi: ["#1d1f23", "#ee6f78", "#8ccf87", "#e4bd76", "#6fa6f8", "#c39ce8", "#5fc0cc", "#c7cbd2", "#5d636e", "#ff8a92", "#a6e2a1", "#f1d091", "#8fbcff", "#d5b4f5", "#80d6e0", "#f2f4f7"],
  },
  {
    id: "nebula-light",
    name: "Nebula Light",
    scheme: "light",
    accent: "#b86e0e",
    background: "#fbfaf7",
    foreground: "#2a2c30",
    ansi: ["#2a2c30", "#c23a4c", "#38823b", "#9a6700", "#2c65cf", "#9343b0", "#0d7d8c", "#d6d5d0", "#6a6e77", "#d64b5d", "#47984a", "#b17c0d", "#467de3", "#a95bc6", "#1d93a3", "#f2f1ec"],
  },
  {
    id: "campbell",
    name: "Campbell",
    scheme: "dark",
    accent: "#3b78ff",
    background: "#0c0c0c",
    foreground: "#cccccc",
    ansi: ["#0c0c0c", "#c50f1f", "#13a10e", "#c19c00", "#0037da", "#881798", "#3a96dd", "#cccccc", "#767676", "#e74856", "#16c60c", "#f9f1a5", "#3b78ff", "#b4009e", "#61d6d6", "#f2f2f2"],
  },
  {
    id: "one-half-dark",
    name: "One Half Dark",
    scheme: "dark",
    accent: "#61afef",
    background: "#282c34",
    foreground: "#dcdfe4",
    ansi: ["#282c34", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#dcdfe4", "#5a6374", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#dcdfe4"],
  },
  {
    id: "one-half-light",
    name: "One Half Light",
    scheme: "light",
    accent: "#0184bc",
    background: "#fafafa",
    foreground: "#383a42",
    ansi: ["#383a42", "#e45649", "#50a14f", "#c18401", "#0184bc", "#a626a4", "#0997b3", "#d4d4d6", "#4f525d", "#df6c75", "#5bab5a", "#d19a1e", "#2196d3", "#b13cae", "#22a6c0", "#ffffff"],
  },
  {
    id: "tokyo-night",
    name: "Tokyo Night",
    scheme: "dark",
    accent: "#7aa2f7",
    background: "#1a1b26",
    foreground: "#c0caf5",
    ansi: ["#15161e", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#a9b1d6", "#414868", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#c0caf5"],
  },
  {
    id: "catppuccin-mocha",
    name: "Catppuccin Mocha",
    scheme: "dark",
    accent: "#cba6f7",
    background: "#1e1e2e",
    foreground: "#cdd6f4",
    ansi: ["#45475a", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7", "#94e2d5", "#bac2de", "#585b70", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7", "#94e2d5", "#a6adc8"],
  },
  {
    id: "gruvbox-dark",
    name: "Gruvbox Dark",
    scheme: "dark",
    accent: "#fabd2f",
    background: "#282828",
    foreground: "#ebdbb2",
    ansi: ["#282828", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#a89984", "#928374", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#ebdbb2"],
  },
  {
    id: "dracula",
    name: "Dracula",
    scheme: "dark",
    accent: "#bd93f9",
    background: "#282a36",
    foreground: "#f8f8f2",
    ansi: ["#21222c", "#ff5555", "#50fa7b", "#f1fa8c", "#bd93f9", "#ff79c6", "#8be9fd", "#f8f8f2", "#6272a4", "#ff6e6e", "#69ff94", "#ffffa5", "#d6acff", "#ff92df", "#a4ffff", "#ffffff"],
  },
];

/** Older releases stored these theme ids; map them to their closest replacement. */
const legacyThemeIds: Record<string, string> = {
  carbon: "nebula",
  boreal: "nebula",
  ember: "nebula",
  tide: "nebula",
  paper: "nebula-light",
};

/** A theme id that exists, among the built-in themes and the imported ones. */
export function normalizeThemeId(id: unknown, custom: readonly TerminalTheme[] = []): string | undefined {
  if (typeof id !== "string") return undefined;
  if (themes.some((theme) => theme.id === id) || custom.some((theme) => theme.id === id)) return id;
  return legacyThemeIds[id];
}

export function resolveTheme(id: string, custom: readonly TerminalTheme[] = []): TerminalTheme {
  return themes.find((theme) => theme.id === id) ?? custom.find((theme) => theme.id === id) ?? themes[0]!;
}

export function xtermTheme(theme: TerminalTheme, accent: string) {
  const [black, red, green, yellow, blue, magenta, cyan, white, brightBlack, brightRed, brightGreen, brightYellow, brightBlue, brightMagenta, brightCyan, brightWhite] = theme.ansi;
  return {
    background: theme.background,
    foreground: theme.foreground,
    cursor: accent,
    cursorAccent: theme.background,
    selectionBackground: `${accent}${theme.scheme === "light" ? "40" : "4d"}`,
    scrollbarSliderBackground: `${theme.foreground}1f`,
    scrollbarSliderHoverBackground: `${theme.foreground}33`,
    scrollbarSliderActiveBackground: `${theme.foreground}47`,
    black, red, green, yellow, blue, magenta, cyan, white,
    brightBlack, brightRed, brightGreen, brightYellow, brightBlue, brightMagenta, brightCyan, brightWhite,
  };
}

/** Accent colors offered as one-click swatches in Settings. */
export const accentSwatches = ["#e8a33d", "#e5734a", "#e05d6f", "#c879d8", "#8b8cf0", "#4c8df6", "#2fa8c9", "#3fb27f", "#9aa3ab"];

/** Tab colors. Mid-tone on purpose: they have to read on dark and light themes alike. */
export const tabColors: Array<{ id: TabColor; name: string; value: string }> = [
  { id: "red", name: "Red", value: "#e5534b" },
  { id: "orange", name: "Orange", value: "#e5793a" },
  { id: "yellow", name: "Yellow", value: "#d4a72c" },
  { id: "green", name: "Green", value: "#3fa66b" },
  { id: "teal", name: "Teal", value: "#2a9fa6" },
  { id: "blue", name: "Blue", value: "#4c8df6" },
  { id: "purple", name: "Purple", value: "#8f74e6" },
  { id: "pink", name: "Pink", value: "#d466a5" },
];

export function tabColorValue(id: TabColor | undefined): string | undefined {
  return tabColors.find((color) => color.id === id)?.value;
}

export function isTabColor(value: unknown): value is TabColor {
  return tabColors.some((color) => color.id === value);
}

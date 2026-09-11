import type { AppearancePreferences } from "./types";

export interface ThemePreset {
  id: string;
  name: string;
  accent: string;
  background: string;
  foreground: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
}

export const themePresets: ThemePreset[] = [
  { id: "nebula", name: "Nebula", accent: "#8b7cf6", background: "#0a0b0e", foreground: "#e8e9ed", black: "#17181d", red: "#ef6b73", green: "#8ccf7e", yellow: "#e5c07b", blue: "#7aa2f7", magenta: "#bb9af7", cyan: "#7dcfff", white: "#c7c9d1" },
  { id: "tokyo-night", name: "Tokyo Night", accent: "#7aa2f7", background: "#1a1b26", foreground: "#c0caf5", black: "#15161e", red: "#f7768e", green: "#9ece6a", yellow: "#e0af68", blue: "#7aa2f7", magenta: "#bb9af7", cyan: "#7dcfff", white: "#a9b1d6" },
  { id: "catppuccin", name: "Catppuccin Mocha", accent: "#cba6f7", background: "#1e1e2e", foreground: "#cdd6f4", black: "#181825", red: "#f38ba8", green: "#a6e3a1", yellow: "#f9e2af", blue: "#89b4fa", magenta: "#cba6f7", cyan: "#94e2d5", white: "#bac2de" },
  { id: "rose-pine", name: "Rosé Pine", accent: "#c4a7e7", background: "#191724", foreground: "#e0def4", black: "#26233a", red: "#eb6f92", green: "#9ccfd8", yellow: "#f6c177", blue: "#31748f", magenta: "#c4a7e7", cyan: "#9ccfd8", white: "#e0def4" },
  { id: "nord", name: "Nord", accent: "#88c0d0", background: "#2e3440", foreground: "#eceff4", black: "#3b4252", red: "#bf616a", green: "#a3be8c", yellow: "#ebcb8b", blue: "#81a1c1", magenta: "#b48ead", cyan: "#8fbcbb", white: "#e5e9f0" },
  { id: "gruvbox", name: "Gruvbox Dark", accent: "#fabd2f", background: "#282828", foreground: "#ebdbb2", black: "#1d2021", red: "#fb4934", green: "#b8bb26", yellow: "#fabd2f", blue: "#83a598", magenta: "#d3869b", cyan: "#8ec07c", white: "#ebdbb2" },
];

export const defaultPreferences: AppearancePreferences = {
  accent: "#8b7cf6",
  themeId: "nebula",
  fontFamily: '"Cascadia Mono", "Cascadia Code", Consolas, monospace',
  fontSize: 14,
  lineHeight: 1.25,
  cursorStyle: "bar",
  cursorBlink: true,
  terminalPadding: 14,
  terminalOpacity: 0.96,
  backgroundMode: "mica",
  backgroundImageOpacity: 0.28,
  animationLevel: "full",
  tabDensity: "comfortable",
  defaultProfileId: "nebula",
  restoreSession: true,
  copyOnSelect: false,
  confirmCloseMultipleTabs: true,
  keybindings: {
    newTab: "Ctrl+Shift+T",
    closeTab: "Ctrl+Shift+W",
    commandPalette: "Ctrl+Shift+P",
    settings: "Ctrl+,",
    find: "Ctrl+F",
    splitVertical: "Ctrl+Shift+D",
    splitHorizontal: "Ctrl+Shift+E",
    closePane: "Ctrl+Shift+Q",
  },
};

const STORAGE_KEY = "nebula-terminal.preferences.v2";

export function loadPreferences(): AppearancePreferences {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (!saved) return defaultPreferences;
    const parsed = JSON.parse(saved) as Partial<AppearancePreferences>;
    return {
      ...defaultPreferences,
      ...parsed,
      keybindings: { ...defaultPreferences.keybindings, ...(parsed.keybindings ?? {}) },
    };
  } catch {
    return defaultPreferences;
  }
}

export function savePreferences(value: AppearancePreferences): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
}

export function resolveTheme(themeId: string): ThemePreset {
  return themePresets.find((theme) => theme.id === themeId) ?? themePresets[0]!;
}

export function exportAppearance(preferences: AppearancePreferences): void {
  const blob = new Blob([JSON.stringify({ version: 1, preferences }, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = "nebula-terminal-theme.json";
  anchor.click();
  URL.revokeObjectURL(url);
}

export async function importAppearance(file: File): Promise<AppearancePreferences> {
  const parsed = JSON.parse(await file.text()) as { version?: number; preferences?: Partial<AppearancePreferences> };
  if (parsed.version !== 1 || !parsed.preferences) throw new Error("Unsupported Nebula Terminal theme file.");
  return {
    ...defaultPreferences,
    ...parsed.preferences,
    keybindings: { ...defaultPreferences.keybindings, ...(parsed.preferences.keybindings ?? {}) },
  };
}

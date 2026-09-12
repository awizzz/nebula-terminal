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
  { id: "nebula", name: "Solar Noir", accent: "#f2a93b", background: "#090d0f", foreground: "#eee9df", black: "#151b1d", red: "#e6786f", green: "#7fbd91", yellow: "#efb95c", blue: "#6f9fbd", magenta: "#bd8f9a", cyan: "#65aaa0", white: "#d8d4ca" },
  { id: "carbon", name: "Carbon", accent: "#c4cbd0", background: "#090a0c", foreground: "#e6e8e9", black: "#16181c", red: "#d56f72", green: "#89b27f", yellow: "#c9a967", blue: "#7e9fbb", magenta: "#aa8fb0", cyan: "#79aeb0", white: "#d4d7d9" },
  { id: "boreal", name: "Boreal", accent: "#5aa89c", background: "#071012", foreground: "#dce9e6", black: "#102023", red: "#db766f", green: "#70bd8b", yellow: "#d5b66b", blue: "#6b9dba", magenta: "#a38eac", cyan: "#5eb9ad", white: "#d0dfdc" },
  { id: "ember", name: "Ember", accent: "#ef7e45", background: "#110b09", foreground: "#eee4dc", black: "#211512", red: "#ee6d5d", green: "#92af75", yellow: "#efa34b", blue: "#7999b4", magenta: "#bf8492", cyan: "#70a8a0", white: "#ddcec3" },
  { id: "tide", name: "Tide", accent: "#65a9c6", background: "#081015", foreground: "#dfe9ec", black: "#122028", red: "#dc746f", green: "#7db28b", yellow: "#d7b46c", blue: "#69aacf", magenta: "#9a8db8", cyan: "#69b4bc", white: "#d1dde2" },
  { id: "paper", name: "Paper", accent: "#a76529", background: "#e9e3d7", foreground: "#292824", black: "#cfc7b8", red: "#a9473d", green: "#4f7654", yellow: "#9a6b24", blue: "#476d85", magenta: "#765578", cyan: "#397476", white: "#f6f1e8" },
];

export const defaultPreferences: AppearancePreferences = {
  accent: "#f2a93b",
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
  workingDirectory: "",
  restoreSession: true,
  copyOnSelect: false,
  confirmMultilinePaste: true,
  confirmCloseMultipleTabs: true,
  scrollback: 20_000,
  atmosphere: 0.72,
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

const STORAGE_KEY = "nebula-terminal.preferences.v3";
const LEGACY_STORAGE_KEY = "nebula-terminal.preferences.v2";

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null;
const isHex = (value: unknown): value is string => typeof value === "string" && /^#[0-9a-f]{6}$/i.test(value);
const numberIn = (value: unknown, fallback: number, min: number, max: number) => typeof value === "number" && Number.isFinite(value) ? Math.min(max, Math.max(min, value)) : fallback;
const stringFrom = (value: unknown, fallback: string, max = 240) => typeof value === "string" && value.length <= max ? value : fallback;

type ThemePreferences = Pick<AppearancePreferences,
  | "accent"
  | "themeId"
  | "fontFamily"
  | "fontSize"
  | "lineHeight"
  | "cursorStyle"
  | "cursorBlink"
  | "terminalPadding"
  | "terminalOpacity"
  | "backgroundMode"
  | "backgroundImageOpacity"
  | "animationLevel"
  | "tabDensity"
  | "atmosphere"
>;

function pickThemePreferences(preferences: AppearancePreferences): ThemePreferences {
  return {
    accent: preferences.accent,
    themeId: preferences.themeId,
    fontFamily: preferences.fontFamily,
    fontSize: preferences.fontSize,
    lineHeight: preferences.lineHeight,
    cursorStyle: preferences.cursorStyle,
    cursorBlink: preferences.cursorBlink,
    terminalPadding: preferences.terminalPadding,
    terminalOpacity: preferences.terminalOpacity,
    backgroundMode: preferences.backgroundMode,
    backgroundImageOpacity: preferences.backgroundImageOpacity,
    animationLevel: preferences.animationLevel,
    tabDensity: preferences.tabDensity,
    atmosphere: preferences.atmosphere,
  };
}

function sanitizePreferences(value: unknown): AppearancePreferences {
  if (!isRecord(value)) return { ...defaultPreferences, keybindings: { ...defaultPreferences.keybindings } };
  const bindings = isRecord(value.keybindings) ? value.keybindings : {};
  const themeId = themePresets.some((theme) => theme.id === value.themeId) ? String(value.themeId) : defaultPreferences.themeId;
  const cursorStyle = ["block", "bar", "underline"].includes(String(value.cursorStyle)) ? value.cursorStyle as AppearancePreferences["cursorStyle"] : defaultPreferences.cursorStyle;
  const backgroundMode = ["mica", "solid", "image"].includes(String(value.backgroundMode)) ? value.backgroundMode as AppearancePreferences["backgroundMode"] : defaultPreferences.backgroundMode;
  const animationLevel = ["full", "reduced", "off"].includes(String(value.animationLevel)) ? value.animationLevel as AppearancePreferences["animationLevel"] : defaultPreferences.animationLevel;
  const tabDensity = ["comfortable", "compact"].includes(String(value.tabDensity)) ? value.tabDensity as AppearancePreferences["tabDensity"] : defaultPreferences.tabDensity;
  const cleanBindings = Object.fromEntries(Object.entries(defaultPreferences.keybindings).map(([key, fallback]) => [key, stringFrom(bindings[key], fallback, 48)])) as unknown as AppearancePreferences["keybindings"];
  return {
    ...defaultPreferences,
    accent: isHex(value.accent) ? value.accent : defaultPreferences.accent,
    themeId,
    fontFamily: stringFrom(value.fontFamily, defaultPreferences.fontFamily, 240),
    fontSize: numberIn(value.fontSize, defaultPreferences.fontSize, 8, 32),
    lineHeight: numberIn(value.lineHeight, defaultPreferences.lineHeight, 1, 2),
    cursorStyle,
    cursorBlink: typeof value.cursorBlink === "boolean" ? value.cursorBlink : defaultPreferences.cursorBlink,
    terminalPadding: numberIn(value.terminalPadding, defaultPreferences.terminalPadding, 4, 40),
    terminalOpacity: numberIn(value.terminalOpacity, defaultPreferences.terminalOpacity, 0.65, 1),
    backgroundMode,
    backgroundImage: typeof value.backgroundImage === "string" && value.backgroundImage.startsWith("data:image/") && value.backgroundImage.length <= 7_000_000 ? value.backgroundImage : undefined,
    backgroundImageOpacity: numberIn(value.backgroundImageOpacity, defaultPreferences.backgroundImageOpacity, 0.05, 0.85),
    animationLevel,
    tabDensity,
    defaultProfileId: stringFrom(value.defaultProfileId, defaultPreferences.defaultProfileId, 80),
    workingDirectory: stringFrom(value.workingDirectory, defaultPreferences.workingDirectory, 1024),
    restoreSession: typeof value.restoreSession === "boolean" ? value.restoreSession : defaultPreferences.restoreSession,
    copyOnSelect: typeof value.copyOnSelect === "boolean" ? value.copyOnSelect : defaultPreferences.copyOnSelect,
    confirmMultilinePaste: typeof value.confirmMultilinePaste === "boolean" ? value.confirmMultilinePaste : defaultPreferences.confirmMultilinePaste,
    confirmCloseMultipleTabs: typeof value.confirmCloseMultipleTabs === "boolean" ? value.confirmCloseMultipleTabs : defaultPreferences.confirmCloseMultipleTabs,
    scrollback: Math.round(numberIn(value.scrollback, defaultPreferences.scrollback, 1_000, 100_000)),
    atmosphere: numberIn(value.atmosphere, defaultPreferences.atmosphere, 0, 1),
    keybindings: cleanBindings,
  };
}

export function loadPreferences(): AppearancePreferences {
  try {
    const saved = localStorage.getItem(STORAGE_KEY) ?? localStorage.getItem(LEGACY_STORAGE_KEY);
    if (!saved) return defaultPreferences;
    return sanitizePreferences(JSON.parse(saved));
  } catch {
    return defaultPreferences;
  }
}

export function savePreferences(value: AppearancePreferences): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(sanitizePreferences(value)));
  } catch {
    // A large background image can exceed the browser storage quota. The current
    // settings remain active for this run even when they cannot be persisted.
  }
}

export function resolveTheme(themeId: string): ThemePreset {
  return themePresets.find((theme) => theme.id === themeId) ?? themePresets[0]!;
}

export function exportAppearance(preferences: AppearancePreferences): void {
  const blob = new Blob([JSON.stringify({ version: 2, appearance: pickThemePreferences(preferences) }, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = "nebula-terminal-theme.json";
  anchor.click();
  URL.revokeObjectURL(url);
}

export async function importAppearance(file: File, current: AppearancePreferences): Promise<AppearancePreferences> {
  if (file.size > 1_000_000) throw new Error("This theme file is too large.");
  const parsed = JSON.parse(await file.text()) as { version?: number; appearance?: unknown; preferences?: unknown };
  const imported = parsed.version === 2 ? parsed.appearance : parsed.version === 1 ? parsed.preferences : undefined;
  if (!isRecord(imported)) throw new Error("Unsupported Nebula Terminal theme file.");
  const sanitized = sanitizePreferences({ ...current, ...imported });
  return { ...current, ...pickThemePreferences(sanitized) };
}

import { normalizeThemeId } from "./themes";
import type { AppearancePreferences, KeybindingPreferences } from "./types";

export const defaultKeybindings: KeybindingPreferences = {
  newTab: "Ctrl+Shift+T",
  closeTab: "Ctrl+Shift+W",
  nextTab: "Ctrl+Tab",
  previousTab: "Ctrl+Shift+Tab",
  splitVertical: "Ctrl+Shift+D",
  splitHorizontal: "Ctrl+Shift+E",
  closePane: "Ctrl+Shift+Q",
  find: "Ctrl+Shift+F",
  commandPalette: "Ctrl+Shift+P",
  settings: "Ctrl+,",
  zoomIn: "Ctrl+=",
  zoomOut: "Ctrl+-",
  zoomReset: "Ctrl+0",
};

export const keybindingLabels: Record<keyof KeybindingPreferences, string> = {
  newTab: "New tab",
  closeTab: "Close tab",
  nextTab: "Next tab",
  previousTab: "Previous tab",
  splitVertical: "Split right",
  splitHorizontal: "Split down",
  closePane: "Close pane",
  find: "Find",
  commandPalette: "Command palette",
  settings: "Settings",
  zoomIn: "Zoom in",
  zoomOut: "Zoom out",
  zoomReset: "Reset zoom",
};

export const defaultFontFamily = '"Cascadia Mono", "Cascadia Code", Consolas, monospace';

export const defaultPreferences: AppearancePreferences = {
  accent: "#e8a33d",
  themeId: "nebula",
  fontFamily: defaultFontFamily,
  fontSize: 14,
  lineHeight: 1.2,
  cursorStyle: "bar",
  cursorBlink: true,
  terminalPadding: 12,
  terminalOpacity: 1,
  backgroundMode: "mica",
  backgroundImageOpacity: 0.28,
  animationLevel: "full",
  tabDensity: "comfortable",
  defaultProfileId: "",
  workingDirectory: "",
  restoreSession: true,
  copyOnSelect: false,
  confirmMultilinePaste: true,
  confirmCloseMultipleTabs: true,
  scrollback: 10_000,
  gpuAcceleration: true,
  keybindings: defaultKeybindings,
};

const STORAGE_KEY = "nebula-terminal.preferences.v4";
const LEGACY_STORAGE_KEYS = ["nebula-terminal.preferences.v3", "nebula-terminal.preferences.v2"];

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null;
const isHex = (value: unknown): value is string => typeof value === "string" && /^#[0-9a-f]{6}$/i.test(value);
const numberIn = (value: unknown, fallback: number, min: number, max: number) => typeof value === "number" && Number.isFinite(value) ? Math.min(max, Math.max(min, value)) : fallback;
const stringFrom = (value: unknown, fallback: string, max = 240) => typeof value === "string" && value.length <= max ? value : fallback;
const booleanFrom = (value: unknown, fallback: boolean) => typeof value === "boolean" ? value : fallback;
const oneOf = <T extends string>(value: unknown, options: readonly T[], fallback: T): T => options.includes(value as T) ? value as T : fallback;

const themeKeys = [
  "accent",
  "themeId",
  "fontFamily",
  "fontSize",
  "lineHeight",
  "cursorStyle",
  "cursorBlink",
  "terminalPadding",
  "terminalOpacity",
  "backgroundMode",
  "backgroundImageOpacity",
  "animationLevel",
  "tabDensity",
] as const satisfies ReadonlyArray<keyof AppearancePreferences>;

type ThemePreferences = Pick<AppearancePreferences, typeof themeKeys[number]>;

function pickThemePreferences(preferences: AppearancePreferences): ThemePreferences {
  return Object.fromEntries(themeKeys.map((key) => [key, preferences[key]])) as ThemePreferences;
}

export function sanitizePreferences(value: unknown): AppearancePreferences {
  if (!isRecord(value)) return { ...defaultPreferences, keybindings: { ...defaultKeybindings } };
  const bindings = isRecord(value.keybindings) ? value.keybindings : {};
  const keybindings = Object.fromEntries(
    Object.entries(defaultKeybindings).map(([key, fallback]) => [key, stringFrom(bindings[key], fallback, 48)]),
  ) as unknown as KeybindingPreferences;
  return {
    accent: isHex(value.accent) ? value.accent : defaultPreferences.accent,
    themeId: normalizeThemeId(value.themeId) ?? defaultPreferences.themeId,
    fontFamily: stringFrom(value.fontFamily, defaultPreferences.fontFamily, 240).trim() || defaultFontFamily,
    fontSize: numberIn(value.fontSize, defaultPreferences.fontSize, 8, 32),
    lineHeight: numberIn(value.lineHeight, defaultPreferences.lineHeight, 1, 2),
    cursorStyle: oneOf(value.cursorStyle, ["block", "bar", "underline"], defaultPreferences.cursorStyle),
    cursorBlink: booleanFrom(value.cursorBlink, defaultPreferences.cursorBlink),
    terminalPadding: numberIn(value.terminalPadding, defaultPreferences.terminalPadding, 0, 40),
    terminalOpacity: numberIn(value.terminalOpacity, defaultPreferences.terminalOpacity, 0.6, 1),
    backgroundMode: oneOf(value.backgroundMode, ["mica", "solid", "image"], defaultPreferences.backgroundMode),
    backgroundImage: typeof value.backgroundImage === "string" && value.backgroundImage.startsWith("data:image/") && value.backgroundImage.length <= 3_500_000 ? value.backgroundImage : undefined,
    backgroundImageOpacity: numberIn(value.backgroundImageOpacity, defaultPreferences.backgroundImageOpacity, 0.05, 0.85),
    animationLevel: oneOf(value.animationLevel, ["full", "reduced", "off"], defaultPreferences.animationLevel),
    tabDensity: oneOf(value.tabDensity, ["comfortable", "compact"], defaultPreferences.tabDensity),
    defaultProfileId: stringFrom(value.defaultProfileId, defaultPreferences.defaultProfileId, 80),
    workingDirectory: stringFrom(value.workingDirectory, defaultPreferences.workingDirectory, 1024),
    restoreSession: booleanFrom(value.restoreSession, defaultPreferences.restoreSession),
    copyOnSelect: booleanFrom(value.copyOnSelect, defaultPreferences.copyOnSelect),
    confirmMultilinePaste: booleanFrom(value.confirmMultilinePaste, defaultPreferences.confirmMultilinePaste),
    confirmCloseMultipleTabs: booleanFrom(value.confirmCloseMultipleTabs, defaultPreferences.confirmCloseMultipleTabs),
    scrollback: Math.round(numberIn(value.scrollback, defaultPreferences.scrollback, 1_000, 100_000)),
    gpuAcceleration: booleanFrom(value.gpuAcceleration, defaultPreferences.gpuAcceleration),
    keybindings,
  };
}

/** Preferences written by 0.x builds: drop the Nebula Shell profile and move Find off Ctrl+F, which shells use. */
export function migrateLegacy(value: unknown): unknown {
  if (!isRecord(value)) return value;
  const bindings = isRecord(value.keybindings) ? { ...value.keybindings } : {};
  if (bindings.find === "Ctrl+F") bindings.find = defaultKeybindings.find;
  return {
    ...value,
    defaultProfileId: value.defaultProfileId === "nebula" ? "" : value.defaultProfileId,
    terminalOpacity: value.terminalOpacity === 0.96 ? defaultPreferences.terminalOpacity : value.terminalOpacity,
    terminalPadding: value.terminalPadding === 14 ? defaultPreferences.terminalPadding : value.terminalPadding,
    keybindings: bindings,
  };
}

export function loadPreferences(): AppearancePreferences {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved) return sanitizePreferences(JSON.parse(saved));
    for (const key of LEGACY_STORAGE_KEYS) {
      const legacy = localStorage.getItem(key);
      if (legacy) return sanitizePreferences(migrateLegacy(JSON.parse(legacy)));
    }
  } catch {
    // Corrupt preferences fall back to defaults rather than blocking startup.
  }
  return defaultPreferences;
}

export function savePreferences(value: AppearancePreferences): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(sanitizePreferences(value)));
  } catch {
    // A large background image can exceed the storage quota. The current
    // settings stay active for this run even when they cannot be persisted.
  }
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
  if (!isRecord(imported)) throw new Error("This is not a Nebula Terminal theme file.");
  const sanitized = sanitizePreferences({ ...current, ...imported });
  return { ...current, ...pickThemePreferences(sanitized) };
}

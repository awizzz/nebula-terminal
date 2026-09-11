import type { AppearancePreferences } from "./types";

export const defaultPreferences: AppearancePreferences = {
  accent: "#8b7cf6",
  fontFamily: '"Cascadia Mono", "Cascadia Code", Consolas, monospace',
  fontSize: 14,
  lineHeight: 1.25,
  cursorStyle: "bar",
  cursorBlink: true,
  terminalPadding: 14,
  terminalOpacity: 0.96,
  backgroundMode: "mica",
  animationLevel: "full",
  tabDensity: "comfortable",
};

const STORAGE_KEY = "nebula-terminal.appearance.v1";

export function loadPreferences(): AppearancePreferences {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (!saved) return defaultPreferences;
    return { ...defaultPreferences, ...JSON.parse(saved) } as AppearancePreferences;
  } catch {
    return defaultPreferences;
  }
}

export function savePreferences(value: AppearancePreferences): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
}

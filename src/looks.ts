import { normalizeThemeId, resolveTheme, type TerminalTheme } from "./themes";
import type { AppearancePreferences, ProfileLook } from "./types";

/** The accent of admin windows, so they're never taken for ordinary ones. */
export const ADMIN_ACCENT = "#e5534b";
const MAX_LOOKS = 100;

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null;

/** What a profile changes about the app's look: a theme (and its accent), a font, a size. */
export function lookFor(preferences: AppearancePreferences, profileId: string | undefined): AppearancePreferences {
  const look = profileId && Object.hasOwn(preferences.profileLooks, profileId) ? preferences.profileLooks[profileId] : undefined;
  if (!look) return preferences;
  const theme = look.themeId ? resolveTheme(look.themeId, preferences.customThemes) : undefined;
  return {
    ...preferences,
    themeId: theme?.id ?? preferences.themeId,
    accent: theme?.accent ?? preferences.accent,
    fontFamily: look.fontFamily ?? preferences.fontFamily,
    fontSize: look.fontSize ?? preferences.fontSize,
  };
}

/**
 * The settings a window draws with: an admin window marks itself with a red accent, and
 * with a Windows contrast theme on, every terminal wears a high contrast one.
 */
export function windowLook(preferences: AppearancePreferences, administrator: boolean, contrast?: "dark" | "light"): AppearancePreferences {
  let look = preferences;
  if (contrast && preferences.followContrastThemes) {
    const theme = resolveTheme(contrast === "dark" ? "high-contrast-dark" : "high-contrast-light");
    look = { ...look, themeId: theme.id, accent: theme.accent, profileLooks: {}, terminalOpacity: 1 };
  }
  return administrator && preferences.markAdministratorWindows ? { ...look, accent: ADMIN_ACCENT } : look;
}

/** Sets or clears part of a profile's look. A look with nothing left in it goes. */
export function withLook(looks: Readonly<Record<string, ProfileLook>>, profileId: string, change: ProfileLook): Record<string, ProfileLook> {
  const current = Object.hasOwn(looks, profileId) ? looks[profileId] : undefined;
  const next = Object.fromEntries(
    Object.entries({ ...current, ...change }).filter(([, value]) => value !== undefined && value !== ""),
  ) as ProfileLook;
  const rest = Object.entries(looks).filter(([id]) => id !== profileId);
  return Object.fromEntries(Object.keys(next).length ? [...rest, [profileId, next]] : rest);
}

/** Looks read back from storage: unknown themes and odd values are dropped. */
export function sanitizeLooks(value: unknown, customThemes: readonly TerminalTheme[]): Record<string, ProfileLook> {
  if (!isRecord(value)) return {};
  const entries = Object.entries(value).slice(0, MAX_LOOKS).flatMap(([id, raw]): Array<[string, ProfileLook]> => {
    if (id.length > 120 || !isRecord(raw)) return [];
    const look: ProfileLook = {};
    const themeId = normalizeThemeId(raw.themeId, customThemes);
    if (themeId) look.themeId = themeId;
    if (typeof raw.fontFamily === "string" && raw.fontFamily.trim() && raw.fontFamily.length <= 240) look.fontFamily = raw.fontFamily.trim();
    if (typeof raw.fontSize === "number" && Number.isFinite(raw.fontSize)) look.fontSize = Math.min(32, Math.max(8, Math.round(raw.fontSize)));
    return Object.keys(look).length ? [[id, look]] : [];
  });
  return Object.fromEntries(entries);
}

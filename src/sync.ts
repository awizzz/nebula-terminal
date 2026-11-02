import { sanitizePreferences } from "./preferences";
import type { AppearancePreferences } from "./types";

/**
 * Settings sync through a file the user picks (src-tauri/src/sync.rs). What belongs to
 * one PC stays on it: Explorer and PATH entries, the starting folder, what opens files
 * (a command that runs), the background image (heavy), and the sync file itself.
 */
const LOCAL_ONLY = ["explorerMenu", "pathCommand", "startingFolder", "workingDirectory", "fileOpener", "fileOpenerCommand", "backgroundImage", "syncFile"] as const satisfies ReadonlyArray<keyof AppearancePreferences>;

const STATE_KEY = "nebula-terminal.sync.v1";

export interface SyncState {
  /** When the settings this PC has were saved, here or on another PC. */
  savedAt: number;
  /** The synced settings as they were last read or written, to skip writing them again. */
  digest: string;
  /** The file's modification time when it was last read or written. */
  modified: number;
}

export function syncedPart(preferences: AppearancePreferences): Record<string, unknown> {
  const part: Record<string, unknown> = { ...preferences };
  for (const key of LOCAL_ONLY) delete part[key];
  return part;
}

export function digestOf(preferences: AppearancePreferences): string {
  return JSON.stringify(syncedPart(preferences));
}

export function syncDocument(preferences: AppearancePreferences, savedAt: number): string {
  return `${JSON.stringify({ app: "Nebula Terminal", version: 1, savedAt, preferences: syncedPart(preferences) }, null, 2)}\n`;
}

/** The settings in a sync file, or null when it isn't one. */
export function readSyncDocument(text: string): { savedAt: number; preferences: Record<string, unknown> } | null {
  try {
    const parsed = JSON.parse(text) as { app?: unknown; version?: unknown; savedAt?: unknown; preferences?: unknown };
    if (parsed.app !== "Nebula Terminal" || parsed.version !== 1 || typeof parsed.savedAt !== "number") return null;
    if (typeof parsed.preferences !== "object" || parsed.preferences === null || Array.isArray(parsed.preferences)) return null;
    return { savedAt: parsed.savedAt, preferences: parsed.preferences as Record<string, unknown> };
  } catch {
    return null;
  }
}

/** Another PC's settings, with this PC's own kept, checked like any saved settings. */
export function applySynced(current: AppearancePreferences, synced: Record<string, unknown>): AppearancePreferences {
  const local = Object.fromEntries(LOCAL_ONLY.map((key) => [key, current[key]]));
  return sanitizePreferences({ ...synced, ...local });
}

export function loadSyncState(): SyncState {
  try {
    const parsed = JSON.parse(localStorage.getItem(STATE_KEY) ?? "null") as Partial<SyncState> | null;
    if (parsed && typeof parsed.savedAt === "number" && typeof parsed.digest === "string" && typeof parsed.modified === "number") return parsed as SyncState;
  } catch {
    // A fresh state reads the file again, which is harmless.
  }
  return { savedAt: 0, digest: "", modified: 0 };
}

export function saveSyncState(state: SyncState): void {
  try {
    localStorage.setItem(STATE_KEY, JSON.stringify(state));
  } catch {
    // Without it, the next launch reads the file once more.
  }
}

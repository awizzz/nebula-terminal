import { sanitizePreferences } from "./preferences";
import { sanitizeWorkspaces, type Workspace } from "./workspaces";
import type { AppearancePreferences } from "./types";

/** Every setting and the workspaces, as a file to bring back later or on another PC. */
export function backupDocument(preferences: AppearancePreferences, workspaces: Workspace[], savedAt = Date.now()): string {
  return `${JSON.stringify({ app: "nebula-terminal", kind: "backup", version: 1, savedAt, preferences, workspaces }, null, 2)}\n`;
}

export interface Backup {
  savedAt: number;
  /** Applies the backup to the settings there are now; the sync file stays this PC's. */
  preferences: (current: AppearancePreferences) => AppearancePreferences;
  workspaces: Workspace[];
}

/** A backup file, checked like anything read back from storage. */
export function readBackup(text: string): Backup {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new Error("This file isn't a Nebula Terminal backup.");
  }
  const document = parsed as { app?: unknown; kind?: unknown; savedAt?: unknown; preferences?: unknown; workspaces?: unknown };
  if (typeof parsed !== "object" || parsed === null || document.app !== "nebula-terminal" || document.kind !== "backup") {
    throw new Error("This file isn't a Nebula Terminal backup.");
  }
  const restored = sanitizePreferences(document.preferences);
  return {
    savedAt: typeof document.savedAt === "number" ? document.savedAt : 0,
    preferences: (current) => ({ ...restored, syncFile: current.syncFile }),
    workspaces: sanitizeWorkspaces(document.workspaces),
  };
}

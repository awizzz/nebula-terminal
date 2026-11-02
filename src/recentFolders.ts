/** Folders the shells were in lately (OSC 7), newest first, to open a tab in again. */

const STORAGE_KEY = "nebula-terminal.recent-folders.v1";
export const MAX_RECENT_FOLDERS = 20;

/** Windows paths ignore case; WSL shares don't, but the same folder rarely differs by case. */
const same = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: "accent" }) === 0;

export function rememberFolder(list: readonly string[], folder: string): string[] {
  const trimmed = folder.trim();
  if (!trimmed || trimmed.length > 1024) return [...list];
  if (list[0] !== undefined && same(list[0], trimmed)) return [...list];
  return [trimmed, ...list.filter((entry) => !same(entry, trimmed))].slice(0, MAX_RECENT_FOLDERS);
}

export function loadRecentFolders(): string[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter((entry): entry is string => typeof entry === "string" && entry.length <= 1024).slice(0, MAX_RECENT_FOLDERS) : [];
  } catch {
    return [];
  }
}

export function saveRecentFolders(list: readonly string[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(list));
  } catch {
    // A convenience only.
  }
}

/** "app" and "C:\Users\me\projects" for C:\Users\me\projects\app. */
export function folderParts(path: string): { name: string; parent: string } {
  const parts = path.replace(/[\\/]+$/, "").split(/[\\/]/);
  const name = parts.pop() || path;
  return { name: /^[a-z]:$/i.test(name) ? `${name}\\` : name, parent: parts.join(path.includes("\\") ? "\\" : "/") };
}

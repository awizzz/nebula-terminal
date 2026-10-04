const MAX_FOLDER = 1024;

/**
 * A Windows folder a pane can start in: a drive path (`C:\Users\me`) or a WSL
 * distribution's own share (`\\wsl.localhost\Ubuntu\home\me`). Other network paths
 * are left out, because checking an unreachable server would stall a new tab.
 */
export function isStartableFolder(value: unknown): value is string {
  return typeof value === "string"
    && value.length <= MAX_FOLDER
    && !/[\u0000-\u001f]/.test(value)
    && (/^[a-z]:\\/i.test(value) || /^\\\\(wsl\.localhost|wsl\$)\\[^\\]+/i.test(value));
}

/** The folder in an OSC 7 report (`file://host/C:/Users/me`), or null when it isn't one a tab can open in. */
export function folderFromReport(report: string): string | null {
  const match = /^file:\/\/([^/]*)(\/.*)$/.exec(report.trim());
  if (!match) return null;
  let path: string;
  try {
    path = decodeURIComponent(match[2]!);
  } catch {
    return null;
  }
  const host = match[1]!.toLowerCase();
  const folder = /^\/[a-z]:(\/|$)/i.test(path)
    ? path.slice(1).replace(/\//g, "\\")
    : `\\\\${host}${path.replace(/\//g, "\\")}`;
  const normalized = /^[a-z]:$/i.test(folder) ? `${folder}\\` : folder;
  return isStartableFolder(normalized) ? normalized : null;
}

import { hostName, isWindows } from "./platform";

const MAX_FOLDER = 1024;

/**
 * A folder a pane can start in. On Windows, a drive path (`C:\Users\me`) or a WSL
 * distribution's own share (`\\wsl.localhost\Ubuntu\home\me`); other network paths are
 * left out, because checking an unreachable server would stall a new tab. On Linux and
 * macOS, an absolute path.
 */
export function isStartableFolder(value: unknown, windows = isWindows): value is string {
  return typeof value === "string"
    && value.length <= MAX_FOLDER
    && !/[\u0000-\u001f]/.test(value)
    && (windows
      ? /^[a-z]:\\/i.test(value) || /^\\\\(wsl\.localhost|wsl\$)\\[^\\]+/i.test(value)
      : value.startsWith("/"));
}

/**
 * The folder in an OSC 7 report (`file://host/C:/Users/me`), or null when it isn't one a
 * tab can open in. On Linux and macOS only this machine's folders count, not a server's
 * seen through SSH.
 */
export function folderFromReport(report: string, windows = isWindows, host = hostName): string | null {
  const match = /^file:\/\/([^/]*)(\/.*)$/.exec(report.trim());
  if (!match) return null;
  let path: string;
  try {
    path = decodeURIComponent(match[2]!);
  } catch {
    return null;
  }
  const reporter = match[1]!.toLowerCase();
  if (!windows) {
    const local = reporter === "" || reporter === "localhost" || reporter === host.toLowerCase();
    return local && isStartableFolder(path, false) ? path : null;
  }
  const folder = /^\/[a-z]:(\/|$)/i.test(path)
    ? path.slice(1).replace(/\//g, "\\")
    : `\\\\${reporter}${path.replace(/\//g, "\\")}`;
  const normalized = /^[a-z]:$/i.test(folder) ? `${folder}\\` : folder;
  return isStartableFolder(normalized, true) ? normalized : null;
}

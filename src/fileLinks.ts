import { invoke, isTauri } from "@tauri-apps/api/core";
import type { IBufferLine, ILink, Terminal } from "@xterm/xterm";
import { findPathCandidates, type PathCandidate } from "./filePaths";
import type { AppearancePreferences } from "./types";

interface FoundPath {
  index: number;
  path: string;
  directory: boolean;
}

/** The text of a buffer line, and the cell each of its characters sits in. */
function readLine(line: IBufferLine): { text: string; cells: number[] } {
  let text = "";
  const cells: number[] = [];
  for (let x = 0; x < line.length; x++) {
    const cell = line.getCell(x);
    if (!cell || cell.getWidth() === 0) continue;
    const chars = cell.getChars() || " ";
    for (let index = 0; index < chars.length; index++) cells.push(x);
    text += chars;
  }
  return { text, cells };
}

/** The browser preview has no files; its replayed output pretends its relative paths exist. */
function previewPaths(candidates: PathCandidate[]): FoundPath[] {
  return candidates.flatMap((candidate, index) => /^[\w.][\w./\\-]*$/.test(candidate.path) ? [{ index, path: candidate.path, directory: false }] : []);
}

async function existing(cwd: string | undefined, candidates: PathCandidate[]): Promise<FoundPath[]> {
  if (!isTauri()) return previewPaths(candidates);
  return invoke<FoundPath[]>("find_paths", { cwd: cwd ?? null, candidates: candidates.map((candidate) => candidate.path) });
}

export async function openPath(path: string, line: number | undefined, column: number | undefined, preferences: AppearancePreferences): Promise<void> {
  if (!isTauri()) throw new Error("Files open from the desktop app");
  await invoke("open_path", { path, line: line ?? null, column: column ?? null, opener: preferences.fileOpener, command: preferences.fileOpenerCommand });
}

/**
 * Underlines file paths that exist in the pane's folder; Ctrl+Click opens them. `folder`
 * and `open` are read when needed, so they follow the shell as it moves.
 */
export function registerFileLinks(terminal: Terminal, folder: () => string | undefined, open: (path: string, candidate: PathCandidate) => void) {
  return terminal.registerLinkProvider({
    provideLinks(y, callback) {
      const bufferLine = terminal.buffer.active.getLine(y - 1);
      if (!bufferLine) return callback(undefined);
      const { text, cells } = readLine(bufferLine);
      const candidates = findPathCandidates(text);
      if (candidates.length === 0) return callback(undefined);
      existing(folder(), candidates)
        .then((found) => {
          const links = found.flatMap(({ index, path }): ILink[] => {
            const candidate = candidates[index];
            if (!candidate) return [];
            const first = cells[candidate.start]!;
            const last = cells[candidate.end - 1]!;
            return [{
              range: { start: { x: first + 1, y }, end: { x: last + (bufferLine.getCell(last)?.getWidth() ?? 1), y } },
              text: text.slice(candidate.start, candidate.end),
              decorations: { pointerCursor: true, underline: true },
              activate: (event) => {
                if (event.ctrlKey || !isTauri()) open(path, candidate);
              },
            }];
          });
          callback(links.length ? links : undefined);
        })
        .catch(() => callback(undefined));
    },
  });
}

/**
 * File paths in terminal output, with the line and column compilers and tools print
 * after them: `src/main.rs:42:7`, `app.ts(12,5)`, Python's `File "x.py", line 3`.
 * Only shapes are found here; the backend keeps the paths that exist.
 */

export interface PathCandidate {
  /** Offsets in the line's text, end excluded. */
  start: number;
  end: number;
  path: string;
  line?: number;
  column?: number;
}

const NAME = String.raw`[\w.@%+~#$&\-\[\]{}]`;
const DRIVE = String.raw`[A-Za-z]:[\\/][^\s"'\`<>|?*:()]*`;
const RELATIVE = String.raw`(?:~|\.{1,2})?[\\/]?${NAME}+(?:[\\/]${NAME}+)*[\\/]?`;
const LOCATION = String.raw`(?::(\d+)(?::(\d+))?|\((\d+)(?:,\s?(\d+))?\))?`;
// Not in the middle of a word, a URL or another path.
const BARE = new RegExp(String.raw`(?<![\w/\\.:\-])(${DRIVE}|${RELATIVE})${LOCATION}`, "g");
const QUOTED = /(["'])([^"'\r\n]*[\\/][^"'\r\n]*)\1(?:,\s*line\s+(\d+))?/g;

const MAX_PATH = 512;
const MAX_PER_LINE = 20;

const number = (value: string | undefined) => (value === undefined ? undefined : Number(value));

function trimEnd(path: string): string {
  return path.replace(/[.,;]+$/, "");
}

/** Where paths may be in one line of output, left to right. */
export function findPathCandidates(text: string): PathCandidate[] {
  const found: PathCandidate[] = [];
  const taken: Array<[number, number]> = [];
  const free = (start: number, end: number) => taken.every(([from, to]) => end <= from || start >= to);
  const add = (candidate: PathCandidate) => {
    if (candidate.path.length === 0 || candidate.path.length > MAX_PATH || !free(candidate.start, candidate.end)) return;
    taken.push([candidate.start, candidate.end]);
    found.push(candidate);
  };

  for (const match of text.matchAll(QUOTED)) {
    const start = match.index! + 1;
    add({ start, end: start + match[2]!.length, path: match[2]!, line: number(match[3]) });
  }
  for (const match of text.matchAll(BARE)) {
    const line = number(match[2] ?? match[4]);
    const column = number(match[3] ?? match[5]);
    // Without a location, only something with a slash in it is worth a look.
    const path = line === undefined ? trimEnd(match[1]!) : match[1]!;
    if (line === undefined && !/[\\/]/.test(path)) continue;
    if (/^[\\/]+$/.test(path) || /^\.{1,2}[\\/]?$/.test(path)) continue;
    const end = line === undefined ? match.index! + path.length : match.index! + match[0].length;
    add({ start: match.index!, end, path, line, column });
  }
  return found.sort((a, b) => a.start - b.start).slice(0, MAX_PER_LINE);
}

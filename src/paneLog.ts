import type { IMarker, Terminal } from "@xterm/xterm";

interface LogLine {
  isWrapped: boolean;
  translateToString(trimRight?: boolean): string;
}

/** Lines `start` to `end - 1` as text. A line that wrapped onto the next one gets no line break. */
export function logText(getLine: (index: number) => LogLine | undefined, start: number, end: number): string {
  let text = "";
  for (let index = start; index < end; index++) {
    const line = getLine(index);
    if (!line) continue;
    text += getLine(index + 1)?.isWrapped ? line.translateToString(false) : `${line.translateToString(true).trimEnd()}\n`;
  }
  return text;
}

const FLUSH_MS = 1000;

export interface PaneLog {
  /** Writes what's on screen now, before something wipes it. */
  flush: () => void;
  stop: () => void;
}

/**
 * Writes what a pane shows to a log as plain text, from the line the cursor is on: every
 * second, the lines finished since the last time. xterm.js has already applied colors,
 * carriage returns and cursor moves (ConPTY sends a screen, not a stream), so a progress
 * bar logs as it ended up. Full-screen programs (vim, less) use the other buffer and
 * aren't logged.
 */
export function startLog(terminal: Terminal, append: (text: string) => void): PaneLog {
  const buffer = () => terminal.buffer.normal;
  const cursorLine = () => buffer().baseY + buffer().cursorY;
  const normal = () => terminal.buffer.active.type === "normal";

  // The first line not logged yet. Line numbers move when old lines leave the scrollback
  // and when a resize rewraps them, so two markers keep track: one on the last line
  // logged, and one in the scrollback, which erasing the screen never touches.
  let next: number | undefined;
  let last: IMarker | undefined;
  let anchor: { marker: IMarker; line: number } | undefined;

  const mark = () => {
    last?.dispose();
    anchor?.marker.dispose();
    const cursor = cursorLine();
    last = next ? terminal.registerMarker(next - 1 - cursor) : undefined;
    const line = buffer().baseY - 1;
    anchor = line >= 0 ? { marker: terminal.registerMarker(line - cursor), line } : undefined;
  };

  const locate = (): number => {
    if (next === undefined) return cursorLine();
    if (last && !last.isDisposed) return last.line + 1;
    if (anchor && !anchor.marker.isDisposed) return next - (anchor.line - anchor.marker.line);
    // Both gone: the scrollback was cleared, or more output came than it holds. What's
    // left starts at the top.
    return last || anchor ? 0 : next;
  };

  const writeThrough = (end: number) => {
    if (!normal()) return;
    feeds = 0;
    const start = locate();
    if (end > start) append(logText((index) => buffer().getLine(index), start, end));
    next = Math.max(start, end);
    mark();
  };

  /** Everything down to the last line with text on screen. */
  const screenEnd = () => {
    const { baseY } = buffer();
    let end = baseY + terminal.rows;
    while (end > baseY && !buffer().getLine(end - 1)?.translateToString(true)) end--;
    return end;
  };

  // `clear` and `cls` erase the screen in place: log it first, then carry on from the top.
  const clears = terminal.parser.registerCsiHandler({ final: "J" }, (params) => {
    if (normal() && (params[0] === 2 || params[0] === 3)) {
      writeThrough(screenEnd());
      next = buffer().baseY;
      mark();
    }
    return false;
  });

  // A flood of output could push lines out of the scrollback before the next second:
  // line feeds are counted as they're parsed, and half a scrollback is written at once.
  let feeds = 0;
  const parsed = terminal.onLineFeed(() => {
    if (++feeds > (terminal.options.scrollback ?? 1000) / 2) writeThrough(cursorLine());
  });

  if (normal()) writeThrough(cursorLine());
  const timer = window.setInterval(() => writeThrough(cursorLine()), FLUSH_MS);
  return {
    flush: () => writeThrough(Math.max(cursorLine() + 1, screenEnd())),
    stop: () => {
      window.clearInterval(timer);
      clears.dispose();
      parsed.dispose();
      writeThrough(Math.max(cursorLine() + 1, screenEnd()));
      last?.dispose();
      anchor?.marker.dispose();
    },
  };
}

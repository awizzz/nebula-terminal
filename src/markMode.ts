import type { IDecoration, Terminal } from "@xterm/xterm";

export interface Point {
  x: number;
  y: number;
}

interface Size {
  cols: number;
  rows: number;
  /** Lines in the buffer, scrollback included. */
  lines: number;
}

const isSpace = (char: string | undefined) => char === undefined || /\s/.test(char);

/**
 * Where a key moves the mark: arrows by a cell (a word with Ctrl), Home and End to the
 * ends of the line (of the buffer with Ctrl), Page Up and Down by a screen. Null for any
 * other key.
 */
export function moveMark(point: Point, key: string, ctrl: boolean, size: Size, text: (y: number) => string): Point | null {
  const lastLine = size.lines - 1;
  const lineEnd = (y: number) => Math.max(0, text(y).trimEnd().length - 1);
  const atLine = (y: number) => Math.max(0, Math.min(lastLine, y));
  switch (key) {
    case "ArrowLeft": {
      if (!ctrl) return point.x > 0 ? { ...point, x: point.x - 1 } : point.y > 0 ? { x: size.cols - 1, y: point.y - 1 } : point;
      if (point.x === 0) return point.y > 0 ? { x: lineEnd(point.y - 1), y: point.y - 1 } : point;
      const line = text(point.y);
      let x = point.x - 1;
      while (x > 0 && isSpace(line[x])) x--;
      while (x > 0 && !isSpace(line[x - 1])) x--;
      return { ...point, x };
    }
    case "ArrowRight": {
      if (!ctrl) return point.x < size.cols - 1 ? { ...point, x: point.x + 1 } : point.y < lastLine ? { x: 0, y: point.y + 1 } : point;
      const line = text(point.y);
      let x = point.x;
      while (x < line.length && !isSpace(line[x])) x++;
      while (x < line.length && isSpace(line[x])) x++;
      if (x >= line.trimEnd().length) return point.y < lastLine ? { x: 0, y: point.y + 1 } : { ...point, x: lineEnd(point.y) };
      return { ...point, x };
    }
    case "ArrowUp": return { ...point, y: atLine(point.y - 1) };
    case "ArrowDown": return { ...point, y: atLine(point.y + 1) };
    case "PageUp": return { ...point, y: atLine(point.y - size.rows) };
    case "PageDown": return { ...point, y: atLine(point.y + size.rows) };
    case "Home": return ctrl ? { x: 0, y: 0 } : { ...point, x: 0 };
    case "End": return ctrl ? { x: lineEnd(lastLine), y: lastLine } : { ...point, x: lineEnd(point.y) };
    default: return null;
  }
}

/** The cells from `a` to `b`, whichever comes first, as xterm.js selects them. */
export function selectionBetween(a: Point, b: Point, cols: number): { column: number; row: number; length: number } {
  const [start, end] = a.y < b.y || (a.y === b.y && a.x <= b.x) ? [a, b] : [b, a];
  return { column: start.x, row: start.y, length: (end.y - start.y) * cols + end.x - start.x + 1 };
}

export interface MarkMode {
  key: (event: KeyboardEvent) => void;
  end: () => void;
}

const hex = (color: unknown, fallback: string) => typeof color === "string" && /^#[0-9a-f]{6}$/i.test(color) ? color : fallback;

/**
 * Mark mode, as in Windows Terminal: a cursor of its own over the buffer, from the
 * shell's cursor. Shift extends a selection from where it started, Enter copies it,
 * Escape leaves.
 */
export function startMarkMode(terminal: Terminal, copy: () => void, ended: () => void): MarkMode {
  const buffer = () => terminal.buffer.active;
  let point: Point = { x: buffer().cursorX, y: buffer().baseY + buffer().cursorY };
  let anchor: Point | undefined;
  let decoration: IDecoration | undefined;

  const draw = () => {
    decoration?.dispose();
    const marker = terminal.registerMarker(point.y - (buffer().baseY + buffer().cursorY));
    const theme = terminal.options.theme ?? {};
    decoration = terminal.registerDecoration({ marker, x: point.x, width: 1, backgroundColor: hex(theme.cursor, "#ffffff"), foregroundColor: hex(theme.background, "#000000"), layer: "top" });
    if (decoration) decoration.onDispose(() => marker.dispose());
    else marker.dispose();
    const top = buffer().viewportY;
    if (point.y < top) terminal.scrollToLine(point.y);
    else if (point.y >= top + terminal.rows) terminal.scrollToLine(point.y - terminal.rows + 1);
    if (!anchor) return terminal.clearSelection();
    const { column, row, length } = selectionBetween(anchor, point, terminal.cols);
    terminal.select(column, row, length);
  };

  let over = false;
  const end = () => {
    if (over) return;
    over = true;
    decoration?.dispose();
    ended();
  };

  const key = (event: KeyboardEvent) => {
    event.preventDefault();
    const letter = event.key.toLowerCase();
    if (event.key === "Escape") return end();
    if (event.key === "Enter" || (event.ctrlKey && letter === "c")) {
      if (terminal.hasSelection()) copy();
      terminal.clearSelection();
      return end();
    }
    if (event.ctrlKey && letter === "a") {
      anchor = { x: 0, y: 0 };
      point = { x: terminal.cols - 1, y: buffer().length - 1 };
      return draw();
    }
    const size = { cols: terminal.cols, rows: terminal.rows, lines: buffer().length };
    const next = moveMark(point, event.key, event.ctrlKey, size, (y) => buffer().getLine(y)?.translateToString() ?? "");
    if (!next) return;
    if (event.shiftKey) anchor ??= point;
    else anchor = undefined;
    point = next;
    draw();
  };

  draw();
  return { key, end };
}

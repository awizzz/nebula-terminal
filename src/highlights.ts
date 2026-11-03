import type { IBufferLine, IDecoration, IDisposable, Terminal } from "@xterm/xterm";
import type { Highlight } from "./types";
import { tabColors } from "./themes";

export const MAX_HIGHLIGHTS = 30;
const MAX_TEXT = 200;
/** Decorations a pane keeps; the oldest go first. */
const MAX_DECORATIONS = 2_000;

/** Highlights read back from storage: odd entries dropped, long ones cut. */
export function sanitizeHighlights(value: unknown): Highlight[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((entry): Highlight[] => {
    if (typeof entry !== "object" || entry === null) return [];
    const { id, text, color, notify } = entry as Partial<Highlight>;
    if (typeof id !== "string" || id.length > 64 || typeof text !== "string") return [];
    return [{ id, text: text.slice(0, MAX_TEXT), color: tabColors.some((option) => option.id === color) ? color! : "red", notify: notify === true }];
  }).slice(0, MAX_HIGHLIGHTS);
}

/**
 * What a highlight looks for: `/…/` is a regular expression, anything else the text
 * itself, in any case. Null for an empty or broken one.
 */
export function highlightPattern(text: string): RegExp | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  const regex = /^\/(.+)\/([a-z]*)$/.exec(trimmed);
  try {
    if (regex) return new RegExp(regex[1]!, `${regex[2]!.replace(/[gy]/g, "")}g`);
    return new RegExp(trimmed.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "gi");
  } catch {
    return null;
  }
}

/** Where each highlight matches in `text`, as [start, end) offsets, the first rule winning overlaps. */
export function findMatches(text: string, rules: readonly { pattern: RegExp }[]): { rule: number; start: number; end: number }[] {
  const found: { rule: number; start: number; end: number }[] = [];
  rules.forEach(({ pattern }, rule) => {
    pattern.lastIndex = 0;
    for (const match of text.matchAll(pattern)) {
      const start = match.index;
      const end = start + match[0].length;
      if (end > start && !found.some((other) => start < other.end && end > other.start)) found.push({ rule, start, end });
    }
  });
  return found;
}

function mix(color: string, base: string, amount: number): string {
  const channel = (hex: string, index: number) => parseInt(hex.slice(1 + index * 2, 3 + index * 2), 16);
  if (!/^#[0-9a-f]{6}$/i.test(base)) return color;
  return `#${[0, 1, 2].map((index) => Math.round(channel(color, index) * amount + channel(base, index) * (1 - amount)).toString(16).padStart(2, "0")).join("")}`;
}

/** A row's text, and the cell each character sits in (wide characters take two). */
function rowText(line: IBufferLine, cols: number): { text: string; columns: number[] } {
  let text = "";
  const columns: number[] = [];
  for (let x = 0; x < cols; x++) {
    const cell = line.getCell(x);
    if (!cell || cell.getWidth() === 0) continue;
    const chars = cell.getChars() || " ";
    for (let index = 0; index < chars.length; index++) columns.push(x);
    text += chars;
  }
  return { text, columns };
}

/**
 * Colors the highlights in each line a pane finishes, and reports the ones that ask to
 * be told. Lines are looked at once, as their line feed is parsed, so a tab out of sight
 * counts too.
 */
export function startHighlights(
  terminal: Terminal,
  rules: () => readonly Highlight[],
  onNotify: (highlight: Highlight, line: string) => void,
): IDisposable {
  let compiledFrom: readonly Highlight[] | undefined;
  let compiled: { highlight: Highlight; pattern: RegExp }[] = [];
  const decorations: IDecoration[] = [];

  const lineFeeds = terminal.onLineFeed(() => {
    const current = rules();
    if (current !== compiledFrom) {
      compiledFrom = current;
      compiled = current.flatMap((highlight) => {
        const pattern = highlightPattern(highlight.text);
        return pattern ? [{ highlight, pattern }] : [];
      });
    }
    if (!compiled.length || terminal.buffer.active.type !== "normal") return;
    const buffer = terminal.buffer.normal;
    const cursor = buffer.baseY + buffer.cursorY;
    // The line just finished, rows it wrapped onto included.
    let first = cursor - 1;
    while (first > 0 && buffer.getLine(first)?.isWrapped) first--;
    const rows: { row: number; text: string; columns: number[]; offset: number }[] = [];
    let text = "";
    for (let row = first; row < cursor; row++) {
      const line = buffer.getLine(row);
      if (!line) continue;
      const part = rowText(line, terminal.cols);
      rows.push({ row, ...part, offset: text.length });
      text += part.text;
    }
    const matches = findMatches(text, compiled);
    if (!matches.length) return;
    const background = String(terminal.options.theme?.background ?? "#000000");
    for (const { rule, start, end } of matches) {
      const { highlight } = compiled[rule]!;
      const color = tabColors.find((option) => option.id === highlight.color)?.value ?? tabColors[0]!.value;
      for (const { row, columns, offset, text: part } of rows) {
        const from = Math.max(start, offset) - offset;
        const to = Math.min(end, offset + part.length) - offset;
        if (from >= to) continue;
        const marker = terminal.registerMarker(row - cursor);
        const decoration = terminal.registerDecoration({
          marker,
          x: columns[from]!,
          width: columns[to - 1]! - columns[from]! + 1,
          backgroundColor: mix(color, background, 0.4),
          overviewRulerOptions: { color },
        });
        if (!decoration) {
          marker.dispose();
          continue;
        }
        decoration.onDispose(() => marker.dispose());
        decorations.push(decoration);
      }
      if (highlight.notify) onNotify(highlight, text.trim());
    }
    while (decorations.length > MAX_DECORATIONS) decorations.shift()?.dispose();
  });

  return {
    dispose: () => {
      lineFeeds.dispose();
      for (const decoration of decorations) decoration.dispose();
    },
  };
}

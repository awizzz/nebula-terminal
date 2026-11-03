import type { SavedCommand } from "./types";

export const MAX_SAVED_COMMANDS = 100;
const MAX_NAME = 60;
const MAX_COMMAND = 2_000;

/** Saved commands read back from storage: odd entries are dropped, long ones cut. */
export function sanitizeSavedCommands(value: unknown): SavedCommand[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((entry): SavedCommand[] => {
    if (typeof entry !== "object" || entry === null) return [];
    const { id, name, command, shortcut } = entry as Partial<SavedCommand>;
    if (typeof id !== "string" || id.length > 64 || typeof command !== "string" || !command.trim()) return [];
    return [{
      id,
      name: typeof name === "string" ? name.slice(0, MAX_NAME) : "",
      command: command.slice(0, MAX_COMMAND),
      ...(typeof shortcut === "string" && shortcut && shortcut.length <= 48 ? { shortcut } : {}),
    }];
  }).slice(0, MAX_SAVED_COMMANDS);
}

/** What the palette shows for a command: its name, or the command itself. */
export function savedCommandLabel(saved: SavedCommand): string {
  return saved.name.trim() || saved.command.split(/\r?\n/)[0]!.trim();
}

/** The text sent to the shell: each line as if typed and run with Enter. */
export function commandInput(command: string): string {
  return command.replace(/\r?\n/g, "\r").replace(/\r*$/, "\r");
}

/** `{{host}}` in a saved command: asked for before it runs. `${HOME}` and `{a,b}` are left alone. */
const BLANK = /\{\{\s*([^{}\r\n]{1,40}?)\s*\}\}/g;
const MAX_BLANKS = 8;

/** The blanks of a command, each name once, in order. */
export function blanksIn(command: string): string[] {
  const names = new Set<string>();
  for (const match of command.matchAll(BLANK)) names.add(match[1]!);
  return [...names].slice(0, MAX_BLANKS);
}

/** The command with every blank replaced by what was typed for it. */
export function fillBlanks(command: string, values: Record<string, string>): string {
  return command.replace(BLANK, (whole, name: string) => values[name] ?? whole);
}

const LAST_VALUES_KEY = "nebula-terminal.blanks.v1";

/** What was typed for each blank last time, per saved command. Kept on this PC only. */
export function lastBlankValues(id: string): Record<string, string> {
  try {
    const all = JSON.parse(localStorage.getItem(LAST_VALUES_KEY) ?? "{}") as Record<string, Record<string, string>>;
    const values = all[id];
    return values && typeof values === "object" ? values : {};
  } catch {
    return {};
  }
}

export function rememberBlankValues(id: string, values: Record<string, string>): void {
  try {
    const all = JSON.parse(localStorage.getItem(LAST_VALUES_KEY) ?? "{}") as Record<string, Record<string, string>>;
    // Most recent last, so the oldest go first once there are too many.
    delete all[id];
    all[id] = values;
    const ids = Object.keys(all);
    for (const old of ids.slice(0, Math.max(0, ids.length - 100))) delete all[old];
    localStorage.setItem(LAST_VALUES_KEY, JSON.stringify(all));
  } catch {
    // Storage full or blocked: the values just aren't suggested next time.
  }
}

import type { SavedCommand } from "./types";

export const MAX_SAVED_COMMANDS = 100;
const MAX_NAME = 60;
const MAX_COMMAND = 2_000;

/** Saved commands read back from storage: odd entries are dropped, long ones cut. */
export function sanitizeSavedCommands(value: unknown): SavedCommand[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((entry): SavedCommand[] => {
    if (typeof entry !== "object" || entry === null) return [];
    const { id, name, command } = entry as Partial<SavedCommand>;
    if (typeof id !== "string" || id.length > 64 || typeof command !== "string" || !command.trim()) return [];
    return [{ id, name: typeof name === "string" ? name.slice(0, MAX_NAME) : "", command: command.slice(0, MAX_COMMAND) }];
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

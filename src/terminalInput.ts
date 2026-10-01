import type { TerminalProfile } from "./types";

type QuoteStyle = "posix" | "powershell" | "cmd";

const PASTE_START = "\x1b[200~";
const PASTE_END = "\x1b[201~";

function quoteStyle(profile: TerminalProfile): QuoteStyle {
  switch (profile.kind) {
    case "pwsh":
    case "powershell":
      return "powershell";
    case "cmd":
      return "cmd";
    case "custom": {
      // A custom profile can run anything; guess from the program it starts.
      const program = (profile.executable ?? "").toLowerCase().split(/[\\/]/).pop() ?? "";
      if (program.startsWith("pwsh") || program.startsWith("powershell")) return "powershell";
      if (program.startsWith("cmd")) return "cmd";
      return "posix";
    }
    default:
      return "posix";
  }
}

/** `C:\Users\me` → `/mnt/c/Users/me`, for a path dropped into a WSL shell. */
function wslPath(path: string): string {
  const drive = /^([A-Za-z]):[\\/](.*)$/.exec(path);
  if (!drive) return path;
  return `/mnt/${drive[1]!.toLowerCase()}/${drive[2]!.replaceAll("\\", "/")}`;
}

/**
 * Quotes a dropped file path so the profile's shell reads it as one literal word.
 * Double quotes are only safe in Command Prompt: PowerShell and bash expand `$(…)`
 * inside them, so a file named `a$(calc).txt` would run `calc`.
 */
export function quoteDroppedPath(path: string, profile: TerminalProfile): string {
  const style = quoteStyle(profile);
  if (style === "cmd") return `"${path}"`;
  if (style === "powershell") return `'${path.replaceAll("'", "''")}'`;
  const text = profile.kind === "wsl" ? wslPath(path) : path;
  return `'${text.replaceAll("'", "'\\''")}'`;
}

/** Input that came from a paste rather than from typing. */
export function isPaste(data: string): boolean {
  return data.includes(PASTE_START) || (data.length > 1 && /[\r\n]/.test(data));
}

function pastedText(data: string): string {
  return data.replaceAll(PASTE_START, "").replaceAll(PASTE_END, "").replace(/\r\n?/g, "\n");
}

/** Lines in a paste, not counting one trailing newline. */
export function pasteLineCount(data: string): number {
  return pastedText(data).replace(/\n$/, "").split("\n").length;
}

/** The pasted text as the confirmation dialog shows it. */
export function pastePreview(data: string): string {
  return pastedText(data);
}

/**
 * A single line pasted with a trailing newline would run at once. Like Windows
 * Terminal, drop that newline so the user still presses Enter.
 */
export function trimSingleLinePaste(data: string): string {
  if (pasteLineCount(data) !== 1) return data;
  const bracketed = data.startsWith(PASTE_START) && data.endsWith(PASTE_END);
  const inner = bracketed ? data.slice(PASTE_START.length, -PASTE_END.length) : data;
  const trimmed = inner.replace(/[\r\n]+$/, "");
  return bracketed ? `${PASTE_START}${trimmed}${PASTE_END}` : trimmed;
}

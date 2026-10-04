import type { TerminalTheme } from "./themes";

/**
 * Reads color schemes from Windows Terminal (one scheme, a list, or a settings.json
 * with a `schemes` array, comments allowed) and from iTerm2 (`.itermcolors` files).
 */

type Ansi = TerminalTheme["ansi"];

/** Imported schemes kept in the preferences; the oldest go first. */
export const MAX_CUSTOM_THEMES = 50;

const HEX = /^#?([0-9a-f]{6})([0-9a-f]{2})?$/i;
const SHORT_HEX = /^#?([0-9a-f]{3})$/i;

/** `#RGB`, `#RRGGBB` or `#RRGGBBAA` as `#rrggbb`, or null. */
export function normalizeColor(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const text = value.trim();
  const full = HEX.exec(text);
  if (full) return `#${full[1]!.toLowerCase()}`;
  const short = SHORT_HEX.exec(text);
  if (short) return `#${short[1]!.split("").map((c) => c + c).join("").toLowerCase()}`;
  return null;
}

function luminance(hex: string): number {
  const channel = (offset: number) => {
    const value = Number.parseInt(hex.slice(offset, offset + 2), 16) / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

function slug(name: string): string {
  const text = name.toLowerCase().normalize("NFKD").replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  return text.slice(0, 40) || "scheme";
}

function makeTheme(name: string, background: string, foreground: string, ansi: string[], cursor: string | null): TerminalTheme {
  // A cursor color that stands out from the text makes a good accent; otherwise blue.
  const accent = cursor && cursor !== foreground && cursor !== background ? cursor : ansi[12]!;
  return {
    id: `custom-${slug(name)}`,
    name: name.trim().slice(0, 60) || "Imported scheme",
    scheme: luminance(background) > 0.4 ? "light" : "dark",
    accent,
    background,
    foreground,
    ansi: ansi as Ansi,
  };
}

/** JSON with line and block comments and trailing commas, as settings.json allows. */
function parseJsonc(text: string): unknown {
  let out = "";
  let inString = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i]!;
    if (inString) {
      out += c;
      if (c === "\\") out += text[++i] ?? "";
      else if (c === '"') inString = false;
      continue;
    }
    if (c === '"') {
      inString = true;
      out += c;
    } else if (c === "/" && text[i + 1] === "/") {
      while (i < text.length && text[i] !== "\n") i++;
      out += "\n";
    } else if (c === "/" && text[i + 1] === "*") {
      i += 2;
      while (i < text.length && !(text[i] === "*" && text[i + 1] === "/")) i++;
      i++;
    } else {
      out += c;
    }
  }
  return JSON.parse(out.replace(/,(\s*[}\]])/g, "$1"));
}

const WINDOWS_TERMINAL_KEYS = [
  ["black"], ["red"], ["green"], ["yellow"], ["blue"], ["purple", "magenta"], ["cyan"], ["white"],
  ["brightBlack"], ["brightRed"], ["brightGreen"], ["brightYellow"], ["brightBlue"], ["brightPurple", "brightMagenta"], ["brightCyan"], ["brightWhite"],
];

function windowsTerminalScheme(value: unknown): TerminalTheme | null {
  if (!value || typeof value !== "object") return null;
  const scheme = value as Record<string, unknown>;
  const background = normalizeColor(scheme.background);
  const foreground = normalizeColor(scheme.foreground);
  if (!background || !foreground) return null;
  const ansi: string[] = [];
  for (const names of WINDOWS_TERMINAL_KEYS) {
    const color = names.map((name) => normalizeColor(scheme[name])).find(Boolean);
    if (!color) return null;
    ansi.push(color);
  }
  const name = typeof scheme.name === "string" ? scheme.name : "Imported scheme";
  return makeTheme(name, background, foreground, ansi, normalizeColor(scheme.cursorColor));
}

/** An iTerm2 color: `<key>Red Component</key><real>0.5</real>` and so on. */
function itermColor(dict: string): string | null {
  const values: Record<string, number> = {};
  for (const [, channel, number] of dict.matchAll(/<key>(Red|Green|Blue) Component<\/key>\s*<(?:real|integer)>([^<]+)<\/(?:real|integer)>/g)) {
    const value = Number.parseFloat(number!);
    if (Number.isFinite(value)) values[channel!] = value;
  }
  const hex = (name: string) => {
    const value = values[name];
    return value === undefined ? null : Math.round(Math.min(1, Math.max(0, value)) * 255).toString(16).padStart(2, "0");
  };
  const [red, green, blue] = [hex("Red"), hex("Green"), hex("Blue")];
  return red && green && blue ? `#${red}${green}${blue}` : null;
}

/** An .itermcolors file: a plist whose top-level keys name colors, each one a dict. */
function itermScheme(text: string, name: string): TerminalTheme {
  if (!text.includes("<plist")) throw new Error("This file has no iTerm2 colors in it.");
  const colors: Record<string, string> = {};
  for (const [, key, dict] of text.matchAll(/<key>([^<]+)<\/key>\s*<dict>([\s\S]*?)<\/dict>/g)) {
    const color = itermColor(dict!);
    if (color) colors[key!.trim()] = color;
  }
  const ansi = Array.from({ length: 16 }, (_, index) => colors[`Ansi ${index} Color`]);
  const background = colors["Background Color"];
  const foreground = colors["Foreground Color"];
  if (!background || !foreground || ansi.some((color) => !color)) {
    throw new Error("This .itermcolors file misses some of the 16 colors, the background or the text color.");
  }
  return makeTheme(name, background, foreground, ansi as string[], colors["Cursor Color"] ?? null);
}

/** The schemes in a file. `fileName` names an iTerm2 scheme, which has no name inside. */
export function parseColorSchemes(text: string, fileName = "Imported scheme"): TerminalTheme[] {
  const trimmed = text.trim();
  if (trimmed.startsWith("<")) {
    return [itermScheme(trimmed, fileName.replace(/\.itermcolors$/i, ""))];
  }
  let json: unknown;
  try {
    json = parseJsonc(trimmed);
  } catch {
    throw new Error("This file is neither JSON from Windows Terminal nor an iTerm2 scheme.");
  }
  const candidates = Array.isArray(json)
    ? json
    : json && typeof json === "object" && Array.isArray((json as { schemes?: unknown }).schemes)
      ? (json as { schemes: unknown[] }).schemes
      : [json];
  const schemes = candidates.map(windowsTerminalScheme).filter((theme): theme is TerminalTheme => theme !== null);
  if (schemes.length === 0) {
    throw new Error("No color scheme found: a scheme needs a background, a foreground and the 16 colors.");
  }
  return schemes;
}

function sameColors(a: TerminalTheme, b: TerminalTheme): boolean {
  return a.background === b.background && a.foreground === b.foreground && a.ansi.every((color, index) => color === b.ansi[index]);
}

/**
 * Separates the schemes worth adding from the ones a built-in theme already has, so
 * importing Dracula selects the Dracula that ships with the app. A new scheme that only
 * shares a built-in name gets "(imported)" after it.
 */
export function sortOutSchemes(imported: readonly TerminalTheme[], builtIn: readonly TerminalTheme[]): { added: TerminalTheme[]; repeated: TerminalTheme[] } {
  const added = new Map<string, TerminalTheme>();
  const repeated: TerminalTheme[] = [];
  for (const theme of imported) {
    const same = builtIn.find((candidate) => sameColors(candidate, theme));
    if (same) {
      if (!repeated.includes(same)) repeated.push(same);
      continue;
    }
    const taken = builtIn.some((candidate) => candidate.name.toLowerCase() === theme.name.toLowerCase());
    added.set(theme.id, taken ? { ...theme, name: `${theme.name} (imported)` } : theme);
  }
  return { added: [...added.values()], repeated };
}

/** Checks a custom theme read back from the preferences. */
export function isCustomTheme(value: unknown): value is TerminalTheme {
  if (!value || typeof value !== "object") return false;
  const theme = value as Partial<TerminalTheme>;
  return typeof theme.id === "string"
    && theme.id.startsWith("custom-")
    && typeof theme.name === "string"
    && (theme.scheme === "dark" || theme.scheme === "light")
    && [theme.accent, theme.background, theme.foreground].every((color) => normalizeColor(color) === color)
    && Array.isArray(theme.ansi)
    && theme.ansi.length === 16
    && theme.ansi.every((color) => normalizeColor(color) === color);
}

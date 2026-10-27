import type { TerminalTheme } from "./themes";
import { contrastRatio, luminance, normalizeColor, toWindowsTerminalScheme } from "./themeImport";

export type ThemeColor = "background" | "foreground" | "accent" | number;

/** Names of ANSI 0–15, in the order of `TerminalTheme.ansi`. */
export const ANSI_NAMES = ["Black", "Red", "Green", "Yellow", "Blue", "Magenta", "Cyan", "White"] as const;

/** Text needs 4.5:1 against its background (WCAG AA); colored output, 3:1. */
export const TEXT_CONTRAST = 4.5;
export const COLOR_CONTRAST = 3;

/** ANSI colors that print text on the background: not black and white, which shells use as such. */
export function isChromatic(index: number): boolean {
  return index % 8 !== 0 && index % 8 !== 7;
}

/**
 * A custom copy of a theme to edit: "Dracula custom", then "Dracula custom 2", with an id
 * no other theme has.
 */
export function editableCopy(theme: TerminalTheme, existing: readonly TerminalTheme[]): TerminalTheme {
  const base = theme.name.replace(/ custom( \d+)?$/, "");
  const names = new Set(existing.map((candidate) => candidate.name.toLowerCase()));
  const ids = new Set(existing.map((candidate) => candidate.id));
  let name = `${base} custom`;
  for (let index = 2; names.has(name.toLowerCase()); index++) name = `${base} custom ${index}`;
  const slug = name.toLowerCase().normalize("NFKD").replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 40) || "theme";
  let id = `custom-${slug}`;
  for (let index = 2; ids.has(id); index++) id = `custom-${slug}-${index}`;
  return { ...theme, id, name, ansi: [...theme.ansi] };
}

export function colorOf(theme: TerminalTheme, color: ThemeColor): string {
  return typeof color === "number" ? theme.ansi[color]! : theme[color];
}

/** The theme with one color changed; a background that turns light makes the theme light. */
export function withColor(theme: TerminalTheme, color: ThemeColor, value: string): TerminalTheme {
  const hex = normalizeColor(value);
  if (!hex) return theme;
  const next = { ...theme, ansi: [...theme.ansi] as TerminalTheme["ansi"] };
  if (typeof color === "number") next.ansi[color] = hex;
  else next[color] = hex;
  next.scheme = luminance(next.background) > 0.4 ? "light" : "dark";
  return next;
}

/** Colors that are hard to read on the theme's background. */
export function lowContrast(theme: TerminalTheme): ThemeColor[] {
  const weak: ThemeColor[] = [];
  if (contrastRatio(theme.foreground, theme.background) < TEXT_CONTRAST) weak.push("foreground");
  theme.ansi.forEach((color, index) => {
    if (isChromatic(index) && contrastRatio(color, theme.background) < COLOR_CONTRAST) weak.push(index);
  });
  return weak;
}

/** Well-known schemes the gallery shows first, before the rest in alphabetical order. */
export const FEATURED_THEMES = [
  "Nord", "Rose Pine", "Rose Pine Moon", "Rose Pine Dawn", "Catppuccin Macchiato", "Catppuccin Frappe", "Catppuccin Latte",
  "TokyoNight Storm", "TokyoNight Day", "Kanagawa Wave", "Kanagawa Dragon", "Everforest Dark Med", "Everforest Light Med",
  "Gruvbox Material Dark", "Gruvbox Light", "iTerm2 Solarized Dark", "iTerm2 Solarized Light", "GitHub Dark", "GitHub Light Default",
  "Atom One Dark", "Atom One Light", "Ayu", "Ayu Mirage", "Ayu Light", "Monokai Pro", "Night Owl", "Material Darker",
  "Nightfox", "Dayfox", "Flexoki Dark", "Flexoki Light", "Iceberg Dark", "Iceberg Light", "Poimandres", "Vesper",
  "Oxocarbon", "Melange Dark", "Horizon", "Snazzy", "Cobalt2", "Zenburn",
];

/** The featured themes first, in their order, then the others as they were. */
export function featuredFirst(list: readonly TerminalTheme[]): TerminalTheme[] {
  const rank = new Map(FEATURED_THEMES.map((name, index) => [name, index]));
  const featured = list.filter((theme) => rank.has(theme.name)).sort((a, b) => rank.get(a.name)! - rank.get(b.name)!);
  return [...featured, ...list.filter((theme) => !rank.has(theme.name))];
}

/** Gallery search: every word of the query in the name, in any order. */
export function matchesQuery(theme: TerminalTheme, query: string): boolean {
  const name = theme.name.toLowerCase();
  return query.toLowerCase().split(/\s+/).filter(Boolean).every((word) => name.includes(word));
}

/** Saves the theme as a Windows Terminal scheme, `<name>.json`. */
export function downloadScheme(theme: TerminalTheme): void {
  const blob = new Blob([`${JSON.stringify(toWindowsTerminalScheme(theme), null, 2)}\n`], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `${theme.name.replace(/[\\/:*?"<>|]+/g, "").trim() || "theme"}.json`;
  anchor.click();
  URL.revokeObjectURL(url);
}

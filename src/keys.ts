const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "AltGraph", "OS"]);

/** Canonical name for the non-modifier key of a shortcut, e.g. "T", "Tab", "Plus", "F5". */
export function keyName(key: string): string {
  if (key === "+") return "Plus";
  if (key === " ") return "Space";
  if (key === "Esc") return "Escape";
  return key.length === 1 ? key.toUpperCase() : key;
}

/** Builds a shortcut string such as "Ctrl+Shift+T" from a keyboard event, or null for a bare modifier. */
export function shortcutFromEvent(event: KeyboardEvent): string | null {
  if (MODIFIER_KEYS.has(event.key)) return null;
  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Win");
  parts.push(keyName(event.key));
  return parts.join("+");
}

/**
 * Physical keys whose meaning should survive keyboard layouts: digits on AZERTY need Shift,
 * "=" needs Shift on QWERTZ, and the numpad reports its own codes. Letters are deliberately
 * not mapped, so Ctrl+Shift+A on AZERTY never fires a Ctrl+Shift+Q binding.
 */
function codeAliases(code: string | undefined): string[] {
  if (!code) return [];
  const digit = /^(?:Digit|Numpad)([0-9])$/.exec(code);
  if (digit) return [digit[1]!];
  if (code === "NumpadAdd") return ["plus", "="];
  if (code === "NumpadSubtract") return ["-"];
  return [];
}

export function matchesShortcut(event: KeyboardEvent, shortcut: string): boolean {
  if (!shortcut) return false;
  const parts = shortcut.split("+").map((part) => part.trim().toLowerCase()).filter(Boolean);
  const key = parts.at(-1) ?? "";
  const names = new Set([keyName(event.key).toLowerCase(), ...codeAliases(event.code)]);
  // "Ctrl+=" and "Ctrl+Plus" both mean zoom in, whichever of the two keys the layout has.
  if (names.has("=")) names.add("plus");
  if (names.has("plus")) names.add("=");
  return names.has(key)
    && event.ctrlKey === parts.includes("ctrl")
    && event.shiftKey === parts.includes("shift")
    && event.altKey === parts.includes("alt")
    && event.metaKey === (parts.includes("meta") || parts.includes("win"));
}

/** Splits a shortcut into keycaps for display. */
export function shortcutKeys(shortcut: string): string[] {
  return shortcut.split("+").map((part) => part.trim()).filter(Boolean);
}

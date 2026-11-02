/**
 * Global shortcuts name physical keys, as `KeyboardEvent.code` does, so the key under
 * Escape stays that key on any layout: ` in QWERTY, ² in AZERTY. src-tauri/src/quick.rs
 * reads the same names.
 */

export const defaultQuickShortcut = "Win+Backquote";

const KEY = /^(?:Key[A-Z]|Digit[0-9]|F(?:[1-9]|1[0-9]|2[0-4])|Backquote|Minus|Equal|BracketLeft|BracketRight|Semicolon|Quote|Backslash|Comma|Period|Slash|IntlBackslash|Space|PageUp|PageDown|End|Home|Arrow(?:Left|Up|Right|Down)|Insert|Delete)$/;
const MODIFIERS = ["Ctrl", "Alt", "Shift", "Win"] as const;

/** What a US keyboard prints, for when the layout can't be read. */
const US_LABELS: Record<string, string> = {
  Backquote: "`", Minus: "-", Equal: "=", BracketLeft: "[", BracketRight: "]", Semicolon: ";", Quote: "'",
  Backslash: "\\", Comma: ",", Period: ".", Slash: "/", IntlBackslash: "\\", Space: "Space",
  ArrowLeft: "←", ArrowUp: "↑", ArrowRight: "→", ArrowDown: "↓", PageUp: "Page Up", PageDown: "Page Down",
};

export function isGlobalShortcut(value: string): boolean {
  const parts = value.split("+");
  const key = parts.pop() ?? "";
  const unique = new Set(parts);
  return KEY.test(key)
    && unique.size === parts.length
    && parts.every((part) => (MODIFIERS as readonly string[]).includes(part))
    && parts.some((part) => part !== "Shift");
}

/** The shortcut a key press makes, or null while only modifiers are down or without Ctrl, Alt or Win. */
export function globalShortcutFromEvent(event: Pick<KeyboardEvent, "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">): string | null {
  if (!KEY.test(event.code)) return null;
  const held = [event.ctrlKey, event.altKey, event.shiftKey, event.metaKey];
  const parts = MODIFIERS.filter((_, index) => held[index]);
  const shortcut = [...parts, event.code].join("+");
  return isGlobalShortcut(shortcut) ? shortcut : null;
}

/** "Win+²" on AZERTY when `layout` is the keyboard's layout map, "Win+`" without one. */
export function globalShortcutLabel(shortcut: string, layout?: ReadonlyMap<string, string>): string {
  const parts = shortcut.split("+");
  const code = parts.pop() ?? "";
  const printed = layout?.get(code);
  const key = printed && printed.trim() ? printed.toUpperCase() : US_LABELS[code] ?? code.replace(/^(?:Key|Digit)/, "");
  return [...parts, key].join("+");
}

interface KeyboardLayoutApi {
  getLayoutMap(): Promise<ReadonlyMap<string, string>>;
}

/** The keyboard's layout map, where the browser has one (WebView2 does). */
export async function keyboardLayout(): Promise<ReadonlyMap<string, string> | undefined> {
  const keyboard = (navigator as Navigator & { keyboard?: KeyboardLayoutApi }).keyboard;
  try {
    return await keyboard?.getLayoutMap();
  } catch {
    return undefined;
  }
}

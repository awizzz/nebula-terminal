import { describe, expect, it } from "vitest";
import { globalShortcutFromEvent, globalShortcutLabel, isGlobalShortcut } from "./quickShortcut";

const press = (code: string, mods: { ctrl?: boolean; alt?: boolean; shift?: boolean; win?: boolean } = {}) =>
  ({ code, ctrlKey: !!mods.ctrl, altKey: !!mods.alt, shiftKey: !!mods.shift, metaKey: !!mods.win });

describe("global shortcuts", () => {
  it("are recorded by physical key", () => {
    expect(globalShortcutFromEvent(press("Backquote", { win: true }))).toBe("Win+Backquote");
    expect(globalShortcutFromEvent(press("KeyT", { ctrl: true, alt: true }))).toBe("Ctrl+Alt+KeyT");
    expect(globalShortcutFromEvent(press("F12", { shift: true, ctrl: true }))).toBe("Ctrl+Shift+F12");
  });

  it("need Ctrl, Alt or Win and a key the host knows", () => {
    expect(globalShortcutFromEvent(press("KeyA", { shift: true }))).toBeNull();
    expect(globalShortcutFromEvent(press("KeyA"))).toBeNull();
    expect(globalShortcutFromEvent(press("ShiftLeft", { shift: true, ctrl: true }))).toBeNull();
    expect(globalShortcutFromEvent(press("NumpadEnter", { ctrl: true }))).toBeNull();
    expect(isGlobalShortcut("Win+Win+KeyA")).toBe(false);
    expect(isGlobalShortcut("Hyper+KeyA")).toBe(false);
  });

  it("show what the keyboard prints", () => {
    const azerty = new Map([["Backquote", "²"], ["KeyQ", "a"]]);
    expect(globalShortcutLabel("Win+Backquote", azerty)).toBe("Win+²");
    expect(globalShortcutLabel("Ctrl+Alt+KeyQ", azerty)).toBe("Ctrl+Alt+A");
    expect(globalShortcutLabel("Win+Backquote")).toBe("Win+`");
    expect(globalShortcutLabel("Alt+Digit1")).toBe("Alt+1");
    expect(globalShortcutLabel("Win+ArrowDown")).toBe("Win+↓");
  });
});

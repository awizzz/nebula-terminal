import { describe, expect, it } from "vitest";
import { keyName, matchesShortcut, shortcutFromEvent, shortcutKeys } from "./keys";

const key = (init: Partial<KeyboardEvent>) => ({ ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, key: "", ...init }) as KeyboardEvent;

describe("keyName", () => {
  it("uppercases printable keys and names awkward ones", () => {
    expect(keyName("t")).toBe("T");
    expect(keyName("+")).toBe("Plus");
    expect(keyName(" ")).toBe("Space");
    expect(keyName("Tab")).toBe("Tab");
  });
});

describe("shortcutFromEvent", () => {
  it("orders modifiers consistently", () => {
    expect(shortcutFromEvent(key({ key: "T", ctrlKey: true, shiftKey: true }))).toBe("Ctrl+Shift+T");
    expect(shortcutFromEvent(key({ key: "1", ctrlKey: true, altKey: true }))).toBe("Ctrl+Alt+1");
  });

  it("ignores a bare modifier", () => {
    expect(shortcutFromEvent(key({ key: "Control", ctrlKey: true }))).toBeNull();
  });
});

describe("matchesShortcut", () => {
  it("matches the default bindings", () => {
    expect(matchesShortcut(key({ key: "T", ctrlKey: true, shiftKey: true }), "Ctrl+Shift+T")).toBe(true);
    expect(matchesShortcut(key({ key: ",", ctrlKey: true }), "Ctrl+,")).toBe(true);
    expect(matchesShortcut(key({ key: "=", ctrlKey: true }), "Ctrl+=")).toBe(true);
    expect(matchesShortcut(key({ key: "Tab", ctrlKey: true, shiftKey: true }), "Ctrl+Shift+Tab")).toBe(true);
  });

  it("requires the exact modifiers", () => {
    expect(matchesShortcut(key({ key: "t", ctrlKey: true }), "Ctrl+Shift+T")).toBe(false);
    expect(matchesShortcut(key({ key: "Tab", ctrlKey: true, shiftKey: true }), "Ctrl+Tab")).toBe(false);
  });

  it("keeps zoom working across layouts", () => {
    expect(matchesShortcut(key({ key: "à", code: "Digit0", ctrlKey: true }), "Ctrl+0")).toBe(true);
    expect(matchesShortcut(key({ key: "+", code: "BracketRight", ctrlKey: true }), "Ctrl+=")).toBe(true);
    expect(matchesShortcut(key({ key: "+", code: "NumpadAdd", ctrlKey: true }), "Ctrl+=")).toBe(true);
    expect(matchesShortcut(key({ key: "-", code: "NumpadSubtract", ctrlKey: true }), "Ctrl+-")).toBe(true);
  });

  it("does not map letters by physical position", () => {
    expect(matchesShortcut(key({ key: "A", code: "KeyQ", ctrlKey: true, shiftKey: true }), "Ctrl+Shift+Q")).toBe(false);
  });

  it("never matches an empty binding", () => {
    expect(matchesShortcut(key({ key: "a" }), "")).toBe(false);
  });

  it("round-trips recorded shortcuts", () => {
    const event = key({ key: "+", ctrlKey: true, shiftKey: true });
    expect(matchesShortcut(event, shortcutFromEvent(event)!)).toBe(true);
  });
});

describe("shortcutKeys", () => {
  it("splits into keycaps", () => {
    expect(shortcutKeys("Ctrl+Shift+P")).toEqual(["Ctrl", "Shift", "P"]);
  });
});

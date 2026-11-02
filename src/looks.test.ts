import { describe, expect, it } from "vitest";
import { ADMIN_ACCENT, lookFor, sanitizeLooks, windowLook, withLook } from "./looks";
import { defaultPreferences } from "./preferences";
import { resolveTheme } from "./themes";

const preferences = { ...defaultPreferences, profileLooks: { "ssh:prod": { themeId: "dracula", fontSize: 16 }, pwsh: { fontFamily: "Consolas" } } };

describe("profile looks", () => {
  it("change only what the profile sets", () => {
    const prod = lookFor(preferences, "ssh:prod");
    expect(prod.themeId).toBe("dracula");
    expect(prod.accent).toBe(resolveTheme("dracula").accent);
    expect(prod.fontSize).toBe(16);
    expect(prod.fontFamily).toBe(defaultPreferences.fontFamily);
    const pwsh = lookFor(preferences, "pwsh");
    expect(pwsh.themeId).toBe(defaultPreferences.themeId);
    expect(pwsh.accent).toBe(defaultPreferences.accent);
    expect(pwsh.fontFamily).toBe("Consolas");
    expect(lookFor(preferences, "cmd")).toBe(preferences);
    expect(lookFor(preferences, "toString")).toBe(preferences);
  });

  it("are edited one field at a time and disappear when empty", () => {
    let looks = withLook({}, "wsl:Ubuntu", { themeId: "gruvbox-dark" });
    looks = withLook(looks, "wsl:Ubuntu", { fontSize: 13 });
    expect(looks).toEqual({ "wsl:Ubuntu": { themeId: "gruvbox-dark", fontSize: 13 } });
    looks = withLook(looks, "wsl:Ubuntu", { themeId: undefined, fontSize: undefined });
    expect(looks).toEqual({});
  });

  it("are checked when read back", () => {
    const looks = sanitizeLooks({ a: { themeId: "nope", fontSize: 99, fontFamily: "  Fira Code " }, b: "x", c: { themeId: "nope" }, ["__proto__"]: { fontSize: 12 } }, []);
    expect(looks.a).toEqual({ fontSize: 32, fontFamily: "Fira Code" });
    expect(Object.hasOwn(looks, "b")).toBe(false);
    expect(Object.hasOwn(looks, "c")).toBe(false);
    expect(Object.getPrototypeOf(looks)).toBe(Object.prototype);
  });

  it("mark admin windows when asked", () => {
    expect(windowLook(defaultPreferences, true).accent).toBe(ADMIN_ACCENT);
    expect(windowLook(defaultPreferences, false).accent).toBe(defaultPreferences.accent);
    expect(windowLook({ ...defaultPreferences, markAdministratorWindows: false }, true).accent).toBe(defaultPreferences.accent);
  });
});

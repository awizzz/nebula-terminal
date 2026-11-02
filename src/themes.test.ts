import { describe, expect, it } from "vitest";
import { normalizeThemeId, resolveTheme, themes, xtermTheme } from "./themes";

const hex = /^#[0-9a-f]{6}$/i;

describe("themes", () => {
  it("have unique ids and complete palettes", () => {
    expect(new Set(themes.map((theme) => theme.id)).size).toBe(themes.length);
    for (const theme of themes) {
      expect(theme.ansi).toHaveLength(16);
      for (const color of [theme.accent, theme.background, theme.foreground, ...theme.ansi]) expect(color).toMatch(hex);
    }
  });

  it("falls back to the default theme", () => {
    expect(resolveTheme("missing").id).toBe("nebula");
    expect(normalizeThemeId(42)).toBeUndefined();
  });

  it("maps the palette onto xterm's names", () => {
    const theme = resolveTheme("campbell");
    const colors = xtermTheme(theme, "#123456");
    expect(colors.red).toBe(theme.ansi[1]);
    expect(colors.brightWhite).toBe(theme.ansi[15]);
    expect(colors.cursor).toBe("#123456");
  });
});

describe("high contrast themes", () => {
  it("keep every color at 7:1 against the background", async () => {
    const { contrastRatio } = await import("./themeImport");
    for (const id of ["high-contrast-dark", "high-contrast-light"]) {
      const theme = themes.find((candidate) => candidate.id === id)!;
      // Black on a dark theme and white on a light one are the background itself, by convention.
      const skip = theme.scheme === "dark" ? [0] : [7, 15];
      expect(contrastRatio(theme.foreground, theme.background)).toBeGreaterThanOrEqual(7);
      expect(contrastRatio(theme.accent, theme.background)).toBeGreaterThanOrEqual(7);
      theme.ansi.forEach((color, index) => {
        if (!skip.includes(index)) expect(contrastRatio(color, theme.background), `${id} color ${index}`).toBeGreaterThanOrEqual(7);
      });
    }
  });
});

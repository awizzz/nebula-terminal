import { describe, expect, it } from "vitest";
import { themes } from "./themes";
import gallery from "./assets/color-schemes.json";
import { galleryThemes, type GalleryEntry } from "./themeImport";
import { FEATURED_THEMES, editableCopy, featuredFirst, isChromatic, lowContrast, matchesQuery, withColor } from "./themeEdit";

const nebula = themes[0]!;

describe("editableCopy", () => {
  it("names copies after the theme and never reuses a name or an id", () => {
    const first = editableCopy(nebula, themes);
    expect(first).toMatchObject({ id: "custom-nebula-custom", name: "Nebula custom", background: nebula.background });
    const second = editableCopy(nebula, [...themes, first]);
    expect(second).toMatchObject({ id: "custom-nebula-custom-2", name: "Nebula custom 2" });
    // A copy of a copy doesn't pile up "custom custom".
    expect(editableCopy(second, [...themes, first, second]).name).toBe("Nebula custom 3");
  });

  it("copies the colors instead of sharing them", () => {
    const copy = editableCopy(nebula, themes);
    copy.ansi[1] = "#000000";
    expect(nebula.ansi[1]).not.toBe("#000000");
  });
});

describe("withColor", () => {
  it("changes one color, and the scheme when the background changes side", () => {
    const light = withColor(nebula, "background", "#fafafa");
    expect(light).toMatchObject({ background: "#fafafa", scheme: "light" });
    expect(withColor(nebula, 4, "#ABC").ansi[4]).toBe("#aabbcc");
    expect(withColor(nebula, "accent", "not a color")).toBe(nebula);
  });
});

describe("lowContrast", () => {
  it("flags text and colored output that are hard to read", () => {
    expect(lowContrast(nebula)).toEqual([]);
    const muddy = withColor(withColor(nebula, "foreground", "#333333"), 1, nebula.background);
    expect(lowContrast(muddy)).toEqual(["foreground", 1]);
    // Black and white are meant to sit close to the background.
    expect(isChromatic(0)).toBe(false);
    expect(isChromatic(15)).toBe(false);
    expect(lowContrast(withColor(nebula, 0, nebula.background))).toEqual([]);
  });
});

describe("matchesQuery", () => {
  it("finds a theme from words of its name in any order", () => {
    const theme = { ...nebula, name: "Tokyo Night Storm" };
    expect(matchesQuery(theme, "storm tokyo")).toBe(true);
    expect(matchesQuery(theme, "  ")).toBe(true);
    expect(matchesQuery(theme, "day")).toBe(false);
  });
});

describe("featuredFirst", () => {
  it("puts every featured scheme first, in its order, and keeps the rest", () => {
    const all = galleryThemes(gallery.schemes as GalleryEntry[], themes);
    const sorted = featuredFirst(all);
    expect(sorted.slice(0, FEATURED_THEMES.length).map((theme) => theme.name)).toEqual(FEATURED_THEMES);
    expect(sorted).toHaveLength(all.length);
  });
});

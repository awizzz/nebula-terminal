import { describe, expect, it } from "vitest";
import { themes } from "./themes";
import { isCustomTheme, normalizeColor, parseColorSchemes, sortOutSchemes } from "./themeImport";

const dracula = {
  name: "Dracula",
  background: "#282A36",
  foreground: "#F8F8F2",
  cursorColor: "#FF79C6",
  black: "#21222C", red: "#FF5555", green: "#50FA7B", yellow: "#F1FA8C", blue: "#BD93F9", purple: "#FF79C6", cyan: "#8BE9FD", white: "#F8F8F2",
  brightBlack: "#6272A4", brightRed: "#FF6E6E", brightGreen: "#69FF94", brightYellow: "#FFFFA5", brightBlue: "#D6ACFF", brightPurple: "#FF92DF", brightCyan: "#A4FFFF", brightWhite: "#FFFFFF",
};

function itermFile(colors: Record<string, [number, number, number]>): string {
  const entries = Object.entries(colors).map(([key, [r, g, b]]) => `
    <key>${key}</key>
    <dict>
      <key>Blue Component</key><real>${b}</real>
      <key>Color Space</key><string>sRGB</string>
      <key>Green Component</key><real>${g}</real>
      <key>Red Component</key><real>${r}</real>
    </dict>`).join("");
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>${entries}</dict></plist>`;
}

describe("parseColorSchemes", () => {
  it("reads a Windows Terminal scheme", () => {
    const [theme] = parseColorSchemes(JSON.stringify(dracula));
    expect(theme).toMatchObject({ id: "custom-dracula", name: "Dracula", scheme: "dark", background: "#282a36", accent: "#ff79c6" });
    expect(theme!.ansi[5]).toBe("#ff79c6");
    expect(isCustomTheme(theme)).toBe(true);
  });

  it("reads the schemes of a settings.json with comments and trailing commas", () => {
    const settings = `{
      // Windows Terminal settings
      "profiles": { "list": [] },
      /* two schemes */
      "schemes": [
        ${JSON.stringify(dracula)},
        ${JSON.stringify({ ...dracula, name: "Paper", background: "#FAFAFA", foreground: "#222222", cursorColor: "#222222" })},
      ],
    }`;
    const themes = parseColorSchemes(settings);
    expect(themes.map((theme) => theme.name)).toEqual(["Dracula", "Paper"]);
    expect(themes[1]!.scheme).toBe("light");
    // A cursor the color of the text isn't an accent: bright blue is.
    expect(themes[1]!.accent).toBe("#d6acff");
  });

  it("reads an iTerm2 scheme", () => {
    const colors: Record<string, [number, number, number]> = {
      "Background Color": [0, 0, 0],
      "Foreground Color": [1, 1, 1],
      "Cursor Color": [1, 0.5, 0],
    };
    for (let index = 0; index < 16; index++) colors[`Ansi ${index} Color`] = [index / 15, 0, 1 - index / 15];
    const [theme] = parseColorSchemes(itermFile(colors), "Solarized Dark.itermcolors");
    expect(theme).toMatchObject({ name: "Solarized Dark", background: "#000000", foreground: "#ffffff", accent: "#ff8000" });
    expect(theme!.ansi[15]).toBe("#ff0000");
  });

  it("explains what is wrong", () => {
    expect(() => parseColorSchemes("not json")).toThrow(/neither JSON/);
    expect(() => parseColorSchemes(JSON.stringify({ name: "x", background: "#000" }))).toThrow(/16 colors/);
    expect(() => parseColorSchemes(itermFile({ "Background Color": [0, 0, 0] }))).toThrow(/misses/);
  });

  it("keeps built-in themes instead of copying them", () => {
    const schemes = parseColorSchemes(JSON.stringify([
      dracula,
      { ...dracula, name: "Dracula", background: "#1e1f29" },
      { ...dracula, name: "Midnight", background: "#000000" },
      { ...dracula, name: "Midnight", background: "#000000", foreground: "#eeeeee" },
    ]));
    const { added, repeated } = sortOutSchemes(schemes, themes);
    expect(repeated.map((theme) => theme.id)).toEqual(["dracula"]);
    // The same name as a built-in theme with other colors, and a name given twice: the last one wins.
    expect(added.map((theme) => [theme.id, theme.name, theme.foreground])).toEqual([
      ["custom-dracula", "Dracula (imported)", "#f8f8f2"],
      ["custom-midnight", "Midnight", "#eeeeee"],
    ]);
  });

  it("normalizes colors", () => {
    expect(normalizeColor("#ABC")).toBe("#aabbcc");
    expect(normalizeColor("12ab34ff")).toBe("#12ab34");
    expect(normalizeColor("red")).toBeNull();
  });
});

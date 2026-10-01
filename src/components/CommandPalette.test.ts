import { describe, expect, it } from "vitest";
import { matchCommand, type PaletteCommand } from "./CommandPalette";

const command = (label: string, extra: Partial<PaletteCommand> = {}): PaletteCommand => ({ id: label, group: "Test", label, run: () => undefined, ...extra });

describe("matchCommand", () => {
  it("matches everything for an empty query", () => {
    expect(matchCommand(command("Split right"), "")?.positions).toEqual([]);
  });

  it("highlights a subsequence", () => {
    expect(matchCommand(command("Split right"), "spr")?.positions).toEqual([0, 1, 6]);
  });

  it("prefers word starts and consecutive letters", () => {
    const tight = matchCommand(command("Split right"), "split")!;
    const loose = matchCommand(command("New Windows PowerShell tab"), "split");
    expect(loose === null || tight.score > loose.score).toBe(true);
  });

  it("falls back to keywords and group", () => {
    expect(matchCommand(command("Keyboard shortcuts", { keywords: "keybindings" }), "keybindings")).not.toBeNull();
    expect(matchCommand(command("Close tab"), "zzz")).toBeNull();
  });
});

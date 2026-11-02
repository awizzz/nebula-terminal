import { describe, expect, it } from "vitest";
import { defaultPreferences } from "./preferences";
import { applySynced, digestOf, readSyncDocument, syncDocument, syncedPart } from "./sync";

const here = { ...defaultPreferences, explorerMenu: true, startingFolder: "custom" as const, workingDirectory: "D:\\Work", fileOpener: "custom" as const, fileOpenerCommand: "notepad {file}", syncFile: "C:\\Users\\me\\OneDrive\\nebula.json" };

describe("settings sync", () => {
  it("leaves out what belongs to one PC", () => {
    const part = syncedPart(here);
    for (const key of ["explorerMenu", "pathCommand", "startingFolder", "workingDirectory", "fileOpener", "fileOpenerCommand", "backgroundImage", "syncFile"]) {
      expect(Object.hasOwn(part, key)).toBe(false);
    }
    expect(part.themeId).toBe(defaultPreferences.themeId);
  });

  it("round-trips through the file and keeps this PC's own settings", () => {
    const there = { ...defaultPreferences, themeId: "dracula", fontSize: 16, fileOpenerCommand: "calc.exe", startingFolder: "desktop" as const };
    const read = readSyncDocument(syncDocument(there, 42))!;
    expect(read.savedAt).toBe(42);
    const applied = applySynced(here, read.preferences);
    expect(applied.themeId).toBe("dracula");
    expect(applied.fontSize).toBe(16);
    expect(applied.fileOpenerCommand).toBe("notepad {file}");
    expect(applied.startingFolder).toBe("custom");
    expect(applied.syncFile).toBe(here.syncFile);
    // Applying what was written changes nothing, so two PCs don't write the file back and forth.
    expect(digestOf(applySynced(applied, syncedPart(applied)))).toBe(digestOf(applied));
  });

  it("refuses files that aren't Nebula Terminal settings", () => {
    expect(readSyncDocument("not json")).toBeNull();
    expect(readSyncDocument(JSON.stringify({ app: "Other", version: 1, savedAt: 1, preferences: {} }))).toBeNull();
    expect(readSyncDocument(JSON.stringify({ app: "Nebula Terminal", version: 2, savedAt: 1, preferences: {} }))).toBeNull();
    expect(readSyncDocument(JSON.stringify({ app: "Nebula Terminal", version: 1, savedAt: 1, preferences: [] }))).toBeNull();
  });
});

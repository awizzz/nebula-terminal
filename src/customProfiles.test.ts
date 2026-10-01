import { describe, expect, it } from "vitest";
import { deleteCustomProfile, detectProfiles, listCustomProfiles, previewSplitArguments, saveCustomProfile } from "./customProfiles";
import type { CustomProfileDraft } from "./types";

const draft = (extra: Partial<CustomProfileDraft> = {}): CustomProfileDraft => ({ name: "Node", executable: "node.exe", arguments: "", cwd: "", accent: "#3FB27F", ...extra });

describe("previewSplitArguments", () => {
  it("splits on whitespace outside quotes", () => {
    expect(previewSplitArguments("  -NoLogo  -File run.ps1 ")).toEqual(["-NoLogo", "-File", "run.ps1"]);
    expect(previewSplitArguments("")).toEqual([]);
  });

  it("keeps quoted text and empty arguments", () => {
    expect(previewSplitArguments('-File "C:\\My Scripts\\a.ps1" ""')).toEqual(["-File", "C:\\My Scripts\\a.ps1", ""]);
    expect(previewSplitArguments('say \\"hi\\"')).toEqual(["say", '"hi"']);
  });
});

describe("custom profiles in the browser preview", () => {
  it("adds, edits and removes a profile", async () => {
    const before = (await listCustomProfiles()).length;
    const added = await saveCustomProfile(draft({ arguments: '--inspect "two words"' }));
    expect(added.id).toMatch(/^custom:[0-9a-f-]{36}$/);
    expect(added.args).toEqual(["--inspect", "two words"]);
    expect(added.arguments).toBe('--inspect "two words"');
    expect(added.accent).toBe("#3fb27f");
    expect((await detectProfiles()).find((profile) => profile.id === added.id)?.kind).toBe("custom");

    const edited = await saveCustomProfile(draft({ id: added.id, name: "Node 22" }));
    expect(edited.id).toBe(added.id);
    expect((await listCustomProfiles()).find((profile) => profile.id === added.id)?.name).toBe("Node 22");

    await deleteCustomProfile(added.id);
    expect(await listCustomProfiles()).toHaveLength(before);
  });

  it("reports the field that needs fixing", async () => {
    await expect(saveCustomProfile(draft({ name: " " }))).rejects.toMatchObject({ field: "name" });
    await expect(saveCustomProfile(draft({ executable: "" }))).rejects.toMatchObject({ field: "executable" });
    await expect(saveCustomProfile(draft({ accent: "teal" }))).rejects.toMatchObject({ field: "accent" });
  });
});

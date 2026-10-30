import { describe, expect, it } from "vitest";
import { deleteCustomProfile, detectProfiles, draftOf, listCustomProfiles, newDraft, previewSplitArguments, saveCustomProfile, sshCommand } from "./customProfiles";
import type { CustomProfileDraft } from "./types";

const draft = (extra: Partial<CustomProfileDraft> = {}): CustomProfileDraft => ({ ...newDraft("#3FB27F"), name: "Node", executable: "node.exe", ...extra });
const connection = (ssh: Partial<CustomProfileDraft["ssh"]>): CustomProfileDraft => ({ ...newDraft("#5b8def", "ssh"), name: "Build server", ssh: { ...newDraft("").ssh, ...ssh } });

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

describe("SSH connections", () => {
  it("build the ssh command from the fields", () => {
    const { ssh } = connection({ host: " build.example.com ", user: "deploy", port: "2222", identityFile: "\"C:\\Keys\\my key\"", localForwards: "8080:localhost:80\n\n5433:db:5432", remoteForwards: "9000:localhost:3000" });
    expect(sshCommand(ssh)).toBe('ssh -p 2222 -i "C:\\Keys\\my key" -L 8080:localhost:80 -L 5433:db:5432 -R 9000:localhost:3000 deploy@build.example.com');
    expect(sshCommand(connection({}).ssh)).toBe("ssh");
  });

  it("are saved, listed with the SSH hosts and edited again", async () => {
    const saved = await saveCustomProfile(connection({ host: "10.0.0.5", port: "22", localForwards: "127.0.0.1:8080:localhost:80" }));
    expect(saved).toMatchObject({ executable: "ssh", cwd: null, args: ["-p", "22", "-L", "127.0.0.1:8080:localhost:80", "10.0.0.5"] });
    expect(saved.ssh).toEqual({ host: "10.0.0.5", user: null, port: 22, identityFile: null, localForwards: ["127.0.0.1:8080:localhost:80"], remoteForwards: [] });
    expect((await detectProfiles()).find((profile) => profile.id === saved.id)?.kind).toBe("ssh");

    const again = draftOf(saved);
    expect(again).toMatchObject({ kind: "ssh", name: "Build server", ssh: { host: "10.0.0.5", port: "22", localForwards: "127.0.0.1:8080:localhost:80" } });
    await deleteCustomProfile(saved.id);
  });

  it("refuse values ssh would read as options", async () => {
    await expect(saveCustomProfile(connection({ host: "" }))).rejects.toMatchObject({ field: "host" });
    await expect(saveCustomProfile(connection({ host: "-oProxyCommand=calc" }))).rejects.toMatchObject({ field: "host" });
    await expect(saveCustomProfile(connection({ host: "pi", user: "-l root" }))).rejects.toMatchObject({ field: "user" });
    await expect(saveCustomProfile(connection({ host: "pi", port: "65536" }))).rejects.toMatchObject({ field: "port" });
    await expect(saveCustomProfile(connection({ host: "pi", localForwards: "8080" }))).rejects.toMatchObject({ field: "localForwards" });
    await expect(saveCustomProfile(connection({ host: "pi", remoteForwards: "1:-oX:2" }))).rejects.toMatchObject({ field: "remoteForwards" });
  });
});

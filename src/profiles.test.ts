import { describe, expect, it } from "vitest";
import { groupProfiles, MENU_GROUPING_THRESHOLD, pickProfile, previewProfiles, profileCommandLabel, profileGroup, profileMenuSections, profileTag, refreshTabProfiles } from "./profiles";
import { paneLeaf } from "./layout";
import type { TerminalProfile, TerminalTab } from "./types";

const custom: TerminalProfile = { id: "custom:6f1c2a7e-3b7d-4c55-9d0e-8a41f2b6c901", name: "Python", kind: "custom", available: true, accent: "#3fb27f" };
const byId = (id: string) => previewProfiles.find((profile) => profile.id === id)!;

describe("pickProfile", () => {
  it("honors an installed choice", () => {
    expect(pickProfile(previewProfiles, "cmd")?.id).toBe("cmd");
    expect(pickProfile([...previewProfiles, custom], custom.id)?.id).toBe(custom.id);
  });

  it("falls back to the first installed shell", () => {
    expect(pickProfile(previewProfiles, "gitbash")?.id).toBe("nebula");
    expect(pickProfile(previewProfiles, "custom:removed")?.id).toBe("nebula");
    expect(pickProfile(previewProfiles, "")?.id).toBe("nebula");
    expect(pickProfile(previewProfiles.map((profile) => ({ ...profile, available: profile.id === "pwsh" })))?.id).toBe("pwsh");
  });

  it("returns nothing when no shell is installed", () => {
    expect(pickProfile(previewProfiles.map((profile) => ({ ...profile, available: false })))).toBeUndefined();
  });
});

describe("profile groups", () => {
  it("are read from the id prefix", () => {
    expect(profileGroup({ id: "wsl" })).toBe("shell");
    expect(profileGroup({ id: "wsl:Debian" })).toBe("wsl");
    expect(profileGroup({ id: "ssh:pi" })).toBe("ssh");
    expect(profileGroup(custom)).toBe("custom");
  });

  it("keep the order shells, WSL, SSH, custom and skip empty groups", () => {
    const mixed = [custom, byId("ssh:pi"), byId("wsl:Debian"), byId("pwsh")];
    expect(groupProfiles(mixed).map(({ group, profiles }) => [group, profiles.map((profile) => profile.id)])).toEqual([
      ["shell", ["pwsh"]],
      ["wsl", ["wsl:Debian"]],
      ["ssh", ["ssh:pi"]],
      ["custom", [custom.id]],
    ]);
    expect(groupProfiles([byId("cmd")]).map(({ group }) => group)).toEqual(["shell"]);
  });
});

describe("profileMenuSections", () => {
  it("keeps a short list flat", () => {
    const short = [byId("nebula"), byId("pwsh"), byId("wsl"), byId("wsl:Debian")];
    const sections = profileMenuSections(short);
    expect(sections).toHaveLength(1);
    expect(sections[0]!.heading).toBeUndefined();
    expect(sections[0]!.profiles.map((profile) => profile.id)).toEqual(["nebula", "pwsh", "wsl", "wsl:Debian"]);
    expect(profileMenuSections([])).toEqual([]);
  });

  it("adds headings once the list gets long", () => {
    const long = [...previewProfiles, custom];
    expect(long.length).toBeGreaterThan(MENU_GROUPING_THRESHOLD);
    expect(profileMenuSections(long).map((section) => section.heading)).toEqual([undefined, "WSL distributions", "SSH hosts", "Custom profiles"]);
  });
});

describe("profile labels", () => {
  it("say what opening the profile does", () => {
    expect(profileCommandLabel(byId("pwsh"))).toBe("New PowerShell tab");
    expect(profileCommandLabel(byId("ssh:pi"))).toBe("Connect to pi");
    expect(profileCommandLabel(custom)).toBe("New Python tab");
  });

  it("tag distributions and hosts", () => {
    expect(profileTag(byId("wsl:Debian"))).toBe("WSL");
    expect(profileTag(byId("ssh:pi"))).toBe("SSH");
    expect(profileTag(byId("wsl"))).toBeUndefined();
    expect(profileTag(custom)).toBeUndefined();
  });
});

describe("refreshTabProfiles", () => {
  const tab = (profile: TerminalProfile): TerminalTab => ({
    id: "t", title: profile.name, activePaneId: "p", layout: paneLeaf("p"), panes: [{ id: "p", profile }],
  });

  it("picks up a renamed profile", () => {
    const renamed = { ...custom, name: "Python 3.13", accent: "#8b8cf0" };
    expect(refreshTabProfiles([tab(custom)], [renamed])[0]!.panes[0]!.profile).toEqual(renamed);
  });

  it("leaves a pane alone when its profile was removed", () => {
    const [refreshed] = refreshTabProfiles([tab(custom)], previewProfiles);
    expect(refreshed!.panes[0]!.profile).toBe(custom);
  });
});

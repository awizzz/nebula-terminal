import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { paneIds, paneLeaf, splitPane, type LayoutNode } from "./layout";
import { previewProfiles } from "./profiles";
import { loadSession, saveSession } from "./session";
import type { TerminalProfile, TerminalTab } from "./types";

const [nebula, pwsh] = previewProfiles as [typeof previewProfiles[0], typeof previewProfiles[0]];
const custom: TerminalProfile = { id: "custom:0b9d4e63-58a2-4f1b-a7c4-2e5d9f8a6b10", name: "Python", kind: "custom", available: true, accent: "#3fb27f" };
const debian = previewProfiles.find((profile) => profile.id === "wsl:Debian")!;

function tab(...profiles: TerminalProfile[]): TerminalTab {
  const panes = profiles.map((profile, index) => ({ id: `p${index}`, profile }));
  const layout = panes.slice(1).reduce<LayoutNode>((node, pane, index) => splitPane(node, panes[index]!.id, pane.id, "vertical"), paneLeaf(panes[0]!.id));
  return { id: crypto.randomUUID(), title: profiles[0]!.name, panes, layout, activePaneId: panes[0]!.id };
}

beforeEach(() => {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
    removeItem: (key: string) => void store.delete(key),
  });
});

afterEach(() => vi.unstubAllGlobals());

describe("session", () => {
  it("round-trips a mixed layout with its names and colors", () => {
    const layout = splitPane(splitPane(paneLeaf("one"), "one", "two", "vertical"), "two", "three", "horizontal");
    const tab: TerminalTab = {
      id: "tab",
      title: "~/src",
      customTitle: "Build",
      color: "teal",
      panes: [{ id: "one", profile: nebula }, { id: "two", profile: pwsh }, { id: "three", profile: nebula }],
      layout,
      activePaneId: "three",
    };
    saveSession([tab], "tab");
    const restored = loadSession(previewProfiles)!.tabs[0]!;
    expect(restored).toMatchObject({ title: "~/src", customTitle: "Build", color: "teal" });
    expect(restored.layout).toMatchObject({ type: "split", direction: "vertical", children: [{ type: "pane" }, { type: "split", direction: "horizontal" }] });
    const order = paneIds(restored.layout);
    expect(order.map((id) => restored.panes.find((pane) => pane.id === id)?.profile.id)).toEqual(["nebula", "pwsh", "nebula"]);
    expect(restored.activePaneId).toBe(order[2]);
  });

  it("migrates a version 2 snapshot", () => {
    localStorage.setItem("nebula-terminal.session.v2", JSON.stringify({
      version: 2,
      activeTabIndex: 0,
      tabs: [{ title: "PowerShell", splitDirection: "horizontal", activePaneIndex: 1, panes: [{ profileId: "pwsh" }, { profileId: "gone" }], paneSizes: [3, 1] }],
    }));
    const tab = loadSession(previewProfiles)!.tabs[0]!;
    expect(tab.layout).toMatchObject({ type: "split", direction: "horizontal", sizes: [0.75, 0.25] });
    expect(tab.panes.map((pane) => pane.profile.id)).toEqual(["pwsh", "nebula"]);
    expect(tab.activePaneId).toBe(tab.panes[1]!.id);
    expect(tab.customTitle).toBeUndefined();
  });

  it("falls back to one pane for a broken layout and drops unknown values", () => {
    localStorage.setItem("nebula-terminal.session.v3", JSON.stringify({
      version: 3,
      activeTabIndex: 9,
      tabs: [{ title: "x".repeat(500), customTitle: "  ", color: "chartreuse", activePaneIndex: -2, layout: { type: "split", direction: "vertical", children: "nope" } }, null],
    }));
    const { tabs, activeTabId } = loadSession(previewProfiles)!;
    expect(tabs).toHaveLength(1);
    expect(tabs[0]).toMatchObject({ title: "Nebula", customTitle: undefined, color: undefined, layout: { type: "pane" } });
    expect(tabs[0]!.panes).toHaveLength(1);
    expect(activeTabId).toBe(tabs[0]!.id);
  });

  it("ignores snapshots it doesn't understand", () => {
    localStorage.setItem("nebula-terminal.session.v3", JSON.stringify({ version: 4, tabs: [], activeTabIndex: 0 }));
    expect(loadSession(previewProfiles)).toBeNull();
    localStorage.setItem("nebula-terminal.session.v3", "{");
    expect(loadSession(previewProfiles)).toBeNull();
  });
});

describe("profiles in a saved session", () => {
  it("restores WSL, SSH and custom profiles that still exist", () => {
    const first = tab(debian, custom);
    saveSession([first], first.id);
    const restored = loadSession([...previewProfiles, custom])!;
    expect(restored.tabs[0]!.panes.map((pane) => pane.profile.id)).toEqual(["wsl:Debian", custom.id]);
  });

  it("opens the first available shell in place of a removed custom profile", () => {
    const first = tab(custom);
    const second = tab(debian);
    saveSession([first, second], second.id);
    const restored = loadSession(previewProfiles)!;
    expect(restored.tabs.map((restoredTab) => restoredTab.panes[0]!.profile.id)).toEqual(["nebula", "wsl:Debian"]);
    expect(restored.activeTabId).toBe(restored.tabs[1]!.id);
  });

  it("does the same for a custom profile whose program is missing", () => {
    const first = tab(custom);
    saveSession([first], first.id);
    expect(loadSession([...previewProfiles, { ...custom, available: false }])!.tabs[0]!.panes[0]!.profile.id).toBe("nebula");
  });
});

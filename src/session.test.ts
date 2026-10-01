import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { paneIds, paneLeaf, splitPane } from "./layout";
import { previewProfiles } from "./profiles";
import { loadSession, saveSession } from "./session";
import type { TerminalTab } from "./types";

const [nebula, pwsh] = previewProfiles as [typeof previewProfiles[0], typeof previewProfiles[0]];

beforeEach(() => {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => store.set(key, value),
    removeItem: (key: string) => store.delete(key),
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

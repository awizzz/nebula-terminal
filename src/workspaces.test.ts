import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { paneLeaf, splitPane, type LayoutNode } from "./layout";
import { previewProfiles } from "./profiles";
import { describe as summary, loadWorkspaces, openWorkspace, renamed, saveWorkspaces, suggestName, withWorkspace, type Workspace } from "./workspaces";
import type { TerminalProfile, TerminalTab } from "./types";

const [nebula, pwsh] = previewProfiles as [TerminalProfile, TerminalProfile];

function tab(cwd: string | undefined, ...profiles: TerminalProfile[]): TerminalTab {
  const panes = profiles.map((profile, index) => ({ id: `p${index}-${crypto.randomUUID()}`, profile, cwd }));
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

describe("workspaces", () => {
  it("saves tabs under a name and opens them again with their splits and folders", () => {
    const tabs = [tab("C:\\Projects\\naeris", nebula, pwsh), tab(undefined, pwsh)];
    const saved = withWorkspace([], "Naeris", tabs, tabs[1]!.id);
    saveWorkspaces(saved);
    const [workspace] = loadWorkspaces();
    expect(workspace).toMatchObject({ name: "Naeris" });
    expect(summary(workspace!)).toBe("2 tabs, 3 panes");
    const opened = openWorkspace(workspace!, previewProfiles)!;
    expect(opened.tabs.map((candidate) => candidate.panes.map((pane) => [pane.profile.id, pane.cwd]))).toEqual([
      [[nebula.id, "C:\\Projects\\naeris"], [pwsh.id, "C:\\Projects\\naeris"]],
      [[pwsh.id, undefined]],
    ]);
    expect(opened.activeTabId).toBe(opened.tabs[1]!.id);
  });

  it("replaces a workspace of the same name and keeps the newest first", () => {
    const first = withWorkspace([], "api", [tab(undefined, nebula)], "");
    const second = withWorkspace(first, "web", [tab(undefined, nebula)], "");
    const again = withWorkspace(second, "API", [tab(undefined, pwsh), tab(undefined, pwsh)], "");
    expect(again.map((workspace) => [workspace.name, workspace.snapshot.tabs.length])).toEqual([["API", 2], ["web", 1]]);
    expect(again[0]!.id).toBe(first[0]!.id);
    expect(renamed(again, again[1]!.id, "  frontend ")[1]!.name).toBe("frontend");
    expect(renamed(again, again[1]!.id, "   ")[1]!.name).toBe("web");
  });

  it("suggests the first tab's folder, then numbered names", () => {
    expect(suggestName([tab("C:\\Projects\\naeris", nebula)], [])).toBe("naeris");
    const existing: Workspace[] = withWorkspace([], "naeris", [tab(undefined, nebula)], "");
    expect(suggestName([tab("C:\\Projects\\naeris", nebula)], existing)).toBe("Workspace 2");
    expect(suggestName([tab("C:\\", nebula)], [])).toBe("Workspace 1");
  });

  it("drops what isn't a workspace when reading them back", () => {
    localStorage.setItem("nebula-terminal.workspaces.v1", JSON.stringify([{ id: "x", name: "", savedAt: 1, snapshot: { tabs: [] } }, "junk", null]));
    expect(loadWorkspaces()).toEqual([]);
    localStorage.setItem("nebula-terminal.workspaces.v1", "{not json");
    expect(loadWorkspaces()).toEqual([]);
  });
});

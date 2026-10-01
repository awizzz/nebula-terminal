import { MAX_PANES, layoutFromFlat, paneIds, paneLeaf, sanitizeLayout, type LayoutNode } from "./layout";
import { pickProfile } from "./profiles";
import { isTabColor } from "./themes";
import type { PersistedLayout, PersistedTab, SessionSnapshot, TerminalPaneModel, TerminalProfile, TerminalTab } from "./types";

const STORAGE_KEY = "nebula-terminal.session.v3";
const LEGACY_STORAGE_KEYS = ["nebula-terminal.session.v2", "nebula-terminal.session.v1"];
const MAX_TABS = 20;
const MAX_TITLE = 120;

function persistLayout(node: LayoutNode, panes: ReadonlyMap<string, TerminalPaneModel>): PersistedLayout {
  if (node.type === "pane") return { type: "pane", profileId: panes.get(node.id)?.profile.id ?? "" };
  return { type: "split", direction: node.direction, sizes: node.sizes, children: node.children.map((child) => persistLayout(child, panes)) };
}

export function saveSession(tabs: TerminalTab[], activeTabId: string): void {
  const snapshot: SessionSnapshot = {
    version: 3,
    activeTabIndex: Math.max(0, tabs.findIndex((tab) => tab.id === activeTabId)),
    tabs: tabs.map((tab) => ({
      title: tab.title,
      customTitle: tab.customTitle,
      color: tab.color,
      activePaneIndex: Math.max(0, paneIds(tab.layout).indexOf(tab.activePaneId)),
      layout: persistLayout(tab.layout, new Map(tab.panes.map((pane) => [pane.id, pane]))),
    })),
  };
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(snapshot));
  } catch {
    // Restoring the workspace is optional; a storage failure must never block a PTY.
  }
}

function readTitle(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const title = value.trim();
  return title && title.length <= MAX_TITLE ? title : undefined;
}

function readIndex(value: unknown, length: number): number {
  return typeof value === "number" && Number.isInteger(value) && value >= 0 ? Math.min(value, length - 1) : 0;
}

/** Rebuilds one tab. Version 3 saves a layout tree; older versions one row or column of panes. */
function restoreTab(saved: PersistedTab, available: ReadonlyMap<string, TerminalProfile>, fallback: TerminalProfile): TerminalTab {
  let panes: TerminalPaneModel[] = [];
  const addPane = (profileId: unknown) => {
    const pane = { id: crypto.randomUUID(), profile: (typeof profileId === "string" && available.get(profileId)) || fallback };
    panes.push(pane);
    return pane.id;
  };

  let layout: LayoutNode | null = null;
  if (saved.layout !== undefined) {
    layout = sanitizeLayout(saved.layout, (leaf) => addPane(leaf.profileId));
  } else if (Array.isArray(saved.panes)) {
    const ids = saved.panes.slice(0, MAX_PANES).map((pane) => addPane(typeof pane === "object" && pane !== null ? pane.profileId : undefined));
    layout = layoutFromFlat(ids, saved.splitDirection === "horizontal" ? "horizontal" : "vertical", Array.isArray(saved.paneSizes) ? saved.paneSizes : undefined);
  }
  if (!layout) {
    // A broken layout costs the splits, never the tab.
    panes = [];
    layout = paneLeaf(addPane(undefined));
  }

  const order = paneIds(layout);
  return {
    id: crypto.randomUUID(),
    title: readTitle(saved.title) ?? panes[0]!.profile.name,
    customTitle: readTitle(saved.customTitle),
    color: isTabColor(saved.color) ? saved.color : undefined,
    panes,
    layout,
    activePaneId: order[readIndex(saved.activePaneIndex, order.length)]!,
  };
}

export function loadSession(profiles: TerminalProfile[]): { tabs: TerminalTab[]; activeTabId: string } | null {
  try {
    const raw = [STORAGE_KEY, ...LEGACY_STORAGE_KEYS].map((key) => localStorage.getItem(key)).find((value) => value !== null);
    if (!raw) return null;
    const snapshot = JSON.parse(raw) as SessionSnapshot;
    if (typeof snapshot !== "object" || snapshot === null || ![1, 2, 3].includes(snapshot.version) || !Array.isArray(snapshot.tabs)) return null;

    const available = new Map(profiles.filter((profile) => profile.available).map((profile) => [profile.id, profile]));
    const fallback = pickProfile(profiles);
    if (!fallback) return null;

    const tabs = snapshot.tabs
      .slice(0, MAX_TABS)
      .filter((saved): saved is PersistedTab => typeof saved === "object" && saved !== null)
      .map((saved) => restoreTab(saved, available, fallback));
    if (tabs.length === 0) return null;

    return {
      tabs,
      activeTabId: tabs[readIndex(snapshot.activeTabIndex, tabs.length)]!.id,
    };
  } catch {
    return null;
  }
}

export function clearSession(): void {
  localStorage.removeItem(STORAGE_KEY);
  for (const key of LEGACY_STORAGE_KEYS) localStorage.removeItem(key);
}

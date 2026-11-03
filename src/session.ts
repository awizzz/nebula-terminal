import { isStartableFolder } from "./folders";
import { MAX_PANES, layoutFromFlat, paneIds, paneLeaf, sanitizeLayout, type LayoutNode } from "./layout";
import { pickProfile } from "./profiles";
import { isTabColor } from "./themes";
import type { PersistedLayout, PersistedTab, SessionSnapshot, TerminalPaneModel, TerminalProfile, TerminalTab } from "./types";

const STORAGE_KEY = "nebula-terminal.session.v3";
const LEGACY_STORAGE_KEYS = ["nebula-terminal.session.v2", "nebula-terminal.session.v1"];
const MAX_TABS = 20;
const MAX_TITLE = 120;

/** Pinned tabs on the left, each group in its own order. */
export function pinnedFirst(tabs: TerminalTab[]): TerminalTab[] {
  return [...tabs.filter((tab) => tab.pinned), ...tabs.filter((tab) => !tab.pinned)];
}

/** Where a pane's screen is kept between sessions. Workspaces keep none. */
export type ScreenKeys = (paneId: string) => string;

function persistLayout(node: LayoutNode, panes: ReadonlyMap<string, TerminalPaneModel>, screens?: ScreenKeys): PersistedLayout {
  if (node.type === "pane") {
    const pane = panes.get(node.id);
    return { type: "pane", profileId: pane?.profile.id ?? "", cwd: pane?.cwd, ...(screens && pane ? { screen: screens(pane.id) } : {}) };
  }
  return { type: "split", direction: node.direction, sizes: node.sizes, children: node.children.map((child) => persistLayout(child, panes, screens)) };
}

/** Tabs as they are saved: names, colors, splits, and the profile and folder of each pane. */
export function snapshotOf(tabs: TerminalTab[], activeTabId: string, screens?: ScreenKeys): SessionSnapshot {
  return {
    version: 3,
    activeTabIndex: Math.max(0, tabs.findIndex((tab) => tab.id === activeTabId)),
    tabs: tabs.map((tab) => ({
      title: tab.title,
      customTitle: tab.customTitle,
      color: tab.color,
      ...(tab.pinned ? { pinned: true } : {}),
      activePaneIndex: Math.max(0, paneIds(tab.layout).indexOf(tab.activePaneId)),
      layout: persistLayout(tab.layout, new Map(tab.panes.map((pane) => [pane.id, pane])), screens),
    })),
  };
}

export function saveSession(tabs: TerminalTab[], activeTabId: string, screens?: ScreenKeys): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(snapshotOf(tabs, activeTabId, screens)));
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
  const addPane = (profileId: unknown, cwd?: unknown, screen?: unknown) => {
    const pane: TerminalPaneModel = {
      id: crypto.randomUUID(),
      profile: (typeof profileId === "string" && available.get(profileId)) || fallback,
      cwd: isStartableFolder(cwd) ? cwd : undefined,
      ...(typeof screen === "string" && screen.length <= 200 ? { restoreScreen: screen } : {}),
    };
    panes.push(pane);
    return pane.id;
  };

  let layout: LayoutNode | null = null;
  if (saved.layout !== undefined) {
    layout = sanitizeLayout(saved.layout, (leaf) => addPane(leaf.profileId, leaf.cwd, leaf.screen));
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
    ...(saved.pinned === true ? { pinned: true } : {}),
  };
}

/**
 * Tabs rebuilt from a saved snapshot, checked like anything read back from storage. A
 * profile that no longer exists opens the default one; a folder that isn't valid, none.
 */
export function restoreSnapshot(value: unknown, profiles: TerminalProfile[]): { tabs: TerminalTab[]; activeTabId: string } | null {
  try {
    const snapshot = value as SessionSnapshot;
    if (typeof snapshot !== "object" || snapshot === null || ![1, 2, 3].includes(snapshot.version) || !Array.isArray(snapshot.tabs)) return null;

    const available = new Map(profiles.filter((profile) => profile.available).map((profile) => [profile.id, profile]));
    const fallback = pickProfile(profiles);
    if (!fallback) return null;

    const tabs = snapshot.tabs
      .slice(0, MAX_TABS)
      .filter((saved): saved is PersistedTab => typeof saved === "object" && saved !== null)
      .map((saved) => restoreTab(saved, available, fallback));
    if (tabs.length === 0) return null;
    tabs.splice(0, tabs.length, ...pinnedFirst(tabs));

    return {
      tabs,
      activeTabId: tabs[readIndex(snapshot.activeTabIndex, tabs.length)]!.id,
    };
  } catch {
    return null;
  }
}

export function loadSession(profiles: TerminalProfile[]): { tabs: TerminalTab[]; activeTabId: string } | null {
  try {
    const raw = [STORAGE_KEY, ...LEGACY_STORAGE_KEYS].map((key) => localStorage.getItem(key)).find((value) => value !== null);
    return raw ? restoreSnapshot(JSON.parse(raw), profiles) : null;
  } catch {
    return null;
  }
}

export function clearSession(): void {
  localStorage.removeItem(STORAGE_KEY);
  for (const key of LEGACY_STORAGE_KEYS) localStorage.removeItem(key);
}

const WINDOWS_KEY = "nebula-terminal.windows.v1";
const MAX_WINDOWS = 8;

type SavedWindows = Record<string, { savedAt: number; snapshot: SessionSnapshot }>;

function readWindows(): SavedWindows {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(WINDOWS_KEY) ?? "{}");
    return typeof parsed === "object" && parsed !== null && !Array.isArray(parsed) ? parsed as SavedWindows : {};
  } catch {
    return {};
  }
}

function writeWindows(windows: SavedWindows): void {
  try {
    localStorage.setItem(WINDOWS_KEY, JSON.stringify(windows));
  } catch {
    // Like the main session: a convenience that must never block a shell.
  }
}

/** The tabs of a window other than the main one, by its label, for the next launch. */
export function saveWindowSession(label: string, tabs: TerminalTab[], activeTabId: string, screens?: ScreenKeys): void {
  const windows = readWindows();
  windows[label] = { savedAt: Date.now(), snapshot: snapshotOf(tabs, activeTabId, screens) };
  writeWindows(Object.fromEntries(Object.entries(windows).sort(([, a], [, b]) => b.savedAt - a.savedAt).slice(0, MAX_WINDOWS)));
}

/** A window closed while others stay open: like in Windows Terminal, its tabs go with it. */
export function forgetWindowSession(label: string): void {
  const windows = readWindows();
  if (!Object.hasOwn(windows, label)) return;
  delete windows[label];
  writeWindows(windows);
}

/** The other windows saved at exit, oldest first, taken out: each saves itself again once open. */
export function takeWindowSessions(): SessionSnapshot[] {
  const windows = readWindows();
  writeWindows({});
  return Object.values(windows)
    .filter((saved) => typeof saved === "object" && saved !== null && typeof saved.savedAt === "number")
    .sort((a, b) => a.savedAt - b.savedAt)
    .map((saved) => saved.snapshot);
}

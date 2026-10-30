import { restoreSnapshot, snapshotOf } from "./session";
import type { SessionSnapshot, TerminalProfile, TerminalTab } from "./types";

/** A named set of tabs to open again in one go: their names, colors, splits and folders. */
export interface Workspace {
  id: string;
  name: string;
  savedAt: number;
  snapshot: SessionSnapshot;
}

const STORAGE_KEY = "nebula-terminal.workspaces.v1";
export const MAX_WORKSPACES = 30;
const MAX_NAME = 60;

function isWorkspace(value: unknown): value is Workspace {
  if (!value || typeof value !== "object") return false;
  const workspace = value as Partial<Workspace>;
  return typeof workspace.id === "string" && workspace.id.length <= 64
    && typeof workspace.name === "string" && workspace.name.trim().length > 0 && workspace.name.length <= MAX_NAME
    && typeof workspace.savedAt === "number"
    && typeof workspace.snapshot === "object" && workspace.snapshot !== null && Array.isArray(workspace.snapshot.tabs);
}

export function loadWorkspaces(): Workspace[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter(isWorkspace).slice(0, MAX_WORKSPACES) : [];
  } catch {
    return [];
  }
}

export function saveWorkspaces(workspaces: Workspace[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(workspaces.slice(0, MAX_WORKSPACES)));
  } catch {
    // Like the session, workspaces are a convenience; a full storage can't break a tab.
  }
}

/** A name to suggest: the folder the first tab is in, or "Workspace 2" and so on. */
export function suggestName(tabs: TerminalTab[], existing: readonly Workspace[]): string {
  const folder = tabs[0]?.panes[0]?.cwd?.split(/[\\/]/).filter(Boolean).at(-1);
  const taken = new Set(existing.map((workspace) => workspace.name.toLowerCase()));
  if (folder && !/^[a-z]:$/i.test(folder) && !taken.has(folder.toLowerCase())) return folder.slice(0, MAX_NAME);
  for (let index = existing.length + 1; ; index++) {
    const name = `Workspace ${index}`;
    if (!taken.has(name.toLowerCase())) return name;
  }
}

/**
 * Saves the tabs under `name`. A workspace that already has the name is replaced, and
 * the list keeps the most recent first.
 */
export function withWorkspace(existing: readonly Workspace[], name: string, tabs: TerminalTab[], activeTabId: string): Workspace[] {
  const trimmed = name.trim().slice(0, MAX_NAME);
  if (!trimmed || tabs.length === 0) return [...existing];
  const previous = existing.find((workspace) => workspace.name.toLowerCase() === trimmed.toLowerCase());
  const workspace: Workspace = { id: previous?.id ?? crypto.randomUUID(), name: trimmed, savedAt: Date.now(), snapshot: snapshotOf(tabs, activeTabId) };
  return [workspace, ...existing.filter((candidate) => candidate.id !== workspace.id)].slice(0, MAX_WORKSPACES);
}

export function renamed(existing: readonly Workspace[], id: string, name: string): Workspace[] {
  const trimmed = name.trim().slice(0, MAX_NAME);
  if (!trimmed) return [...existing];
  return existing.map((workspace) => workspace.id === id ? { ...workspace, name: trimmed } : workspace);
}

/** The workspace's tabs, rebuilt with today's profiles. */
export function openWorkspace(workspace: Workspace, profiles: TerminalProfile[]): { tabs: TerminalTab[]; activeTabId: string } | null {
  return restoreSnapshot(workspace.snapshot, profiles);
}

/** "3 tabs, 5 panes" for menus. */
export function describe(workspace: Workspace): string {
  const tabs = workspace.snapshot.tabs.length;
  const panes = workspace.snapshot.tabs.reduce((sum, tab) => sum + countPanes(tab.layout), 0);
  const tabsText = `${tabs} tab${tabs === 1 ? "" : "s"}`;
  return panes > tabs ? `${tabsText}, ${panes} panes` : tabsText;
}

function countPanes(layout: unknown): number {
  if (!layout || typeof layout !== "object") return 1;
  const node = layout as { type?: string; children?: unknown[] };
  return node.type === "split" && Array.isArray(node.children) ? node.children.reduce<number>((sum, child) => sum + countPanes(child), 0) : 1;
}

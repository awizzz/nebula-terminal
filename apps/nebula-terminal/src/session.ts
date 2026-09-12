import type { SessionSnapshot, TerminalProfile, TerminalTab } from "./types";

const STORAGE_KEY = "nebula-terminal.session.v2";
const LEGACY_STORAGE_KEY = "nebula-terminal.session.v1";

export function saveSession(tabs: TerminalTab[], activeTabId: string): void {
  const snapshot: SessionSnapshot = {
    version: 2,
    activeTabIndex: Math.max(0, tabs.findIndex((tab) => tab.id === activeTabId)),
    tabs: tabs.map((tab) => ({
      title: tab.title,
      splitDirection: tab.splitDirection,
      activePaneIndex: Math.max(0, tab.panes.findIndex((pane) => pane.id === tab.activePaneId)),
      panes: tab.panes.map((pane) => ({ profileId: pane.profile.id })),
      paneSizes: tab.paneSizes,
    })),
  };
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(snapshot));
  } catch {
    // Restoring the workspace is optional; a storage failure must never block a PTY.
  }
}

export function loadSession(profiles: TerminalProfile[]): { tabs: TerminalTab[]; activeTabId: string } | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY) ?? localStorage.getItem(LEGACY_STORAGE_KEY);
    if (!raw) return null;
    const snapshot = JSON.parse(raw) as SessionSnapshot;
    if (![1, 2].includes(snapshot.version) || !Array.isArray(snapshot.tabs) || snapshot.tabs.length === 0) return null;

    const available = new Map(profiles.filter((profile) => profile.available).map((profile) => [profile.id, profile]));
    const fallback = available.get("nebula") ?? available.values().next().value;
    if (!fallback) return null;

    const tabs = snapshot.tabs.slice(0, 20).map((saved) => {
      const sourcePanes = Array.isArray(saved.panes) ? saved.panes.slice(0, 4) : [];
      const panes = (sourcePanes.length ? sourcePanes : [{ profileId: fallback.id }]).map((pane) => ({
        id: crypto.randomUUID(),
        profile: available.get(pane.profileId) ?? fallback,
      }));
      return {
        id: crypto.randomUUID(),
        title: typeof saved.title === "string" && saved.title.length <= 120 ? saved.title : panes[0]!.profile.name,
        panes,
        activePaneId: panes[Math.min(saved.activePaneIndex, panes.length - 1)]!.id,
        splitDirection: saved.splitDirection === "horizontal" ? "horizontal" : "vertical",
        paneSizes: Array.isArray(saved.paneSizes) && saved.paneSizes.length === panes.length
          ? saved.paneSizes.map((size) => typeof size === "number" && Number.isFinite(size) ? Math.max(0.1, size) : 1)
          : panes.map(() => 1),
      } satisfies TerminalTab;
    });

    return {
      tabs,
      activeTabId: tabs[Math.min(snapshot.activeTabIndex, tabs.length - 1)]!.id,
    };
  } catch {
    return null;
  }
}

export function clearSession(): void {
  localStorage.removeItem(STORAGE_KEY);
  localStorage.removeItem(LEGACY_STORAGE_KEY);
}

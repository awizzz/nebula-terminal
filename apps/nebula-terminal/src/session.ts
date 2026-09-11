import type { SessionSnapshot, TerminalProfile, TerminalTab } from "./types";

const STORAGE_KEY = "nebula-terminal.session.v1";

export function saveSession(tabs: TerminalTab[], activeTabId: string): void {
  const snapshot: SessionSnapshot = {
    version: 1,
    activeTabIndex: Math.max(0, tabs.findIndex((tab) => tab.id === activeTabId)),
    tabs: tabs.map((tab) => ({
      title: tab.title,
      splitDirection: tab.splitDirection,
      activePaneIndex: Math.max(0, tab.panes.findIndex((pane) => pane.id === tab.activePaneId)),
      panes: tab.panes.map((pane) => ({ profileId: pane.profile.id })),
    })),
  };
  localStorage.setItem(STORAGE_KEY, JSON.stringify(snapshot));
}

export function loadSession(profiles: TerminalProfile[]): { tabs: TerminalTab[]; activeTabId: string } | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const snapshot = JSON.parse(raw) as SessionSnapshot;
    if (snapshot.version !== 1 || !Array.isArray(snapshot.tabs) || snapshot.tabs.length === 0) return null;

    const available = new Map(profiles.filter((profile) => profile.available).map((profile) => [profile.id, profile]));
    const fallback = available.get("nebula") ?? available.values().next().value;
    if (!fallback) return null;

    const tabs = snapshot.tabs.map((saved) => {
      const panes = (saved.panes.length ? saved.panes : [{ profileId: fallback.id }]).map((pane) => ({
        id: crypto.randomUUID(),
        profile: available.get(pane.profileId) ?? fallback,
      }));
      return {
        id: crypto.randomUUID(),
        title: saved.title || panes[0]!.profile.name,
        panes,
        activePaneId: panes[Math.min(saved.activePaneIndex, panes.length - 1)]!.id,
        splitDirection: saved.splitDirection ?? "vertical",
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
}

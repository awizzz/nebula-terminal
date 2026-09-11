import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Titlebar from "./components/Titlebar";
import TerminalPane from "./components/TerminalPane";
import SettingsPanel from "./components/SettingsPanel";
import CommandPalette, { type PaletteCommand } from "./components/CommandPalette";
import NewTabMenu from "./components/NewTabMenu";
import SearchBar from "./components/SearchBar";
import { defaultPreferences, loadPreferences, savePreferences } from "./preferences";
import { clearSession, loadSession, saveSession } from "./session";
import type { AppearancePreferences, SplitDirection, TerminalPaneModel, TerminalProfile, TerminalTab } from "./types";

const previewProfiles: TerminalProfile[] = [
  { id: "nebula", name: "Nebula", kind: "nebula", available: true, accent: "#8b7cf6" },
  { id: "cmd", name: "Command Prompt", kind: "cmd", available: true, executable: "cmd.exe", accent: "#78dba9" },
  { id: "powershell", name: "Windows PowerShell", kind: "powershell", available: true, executable: "powershell.exe", accent: "#72a9f7" },
  { id: "pwsh", name: "PowerShell 7", kind: "pwsh", available: false, accent: "#a98df4" },
  { id: "wsl", name: "WSL", kind: "wsl", available: false, accent: "#f2c877" },
];

function makePane(profile: TerminalProfile): TerminalPaneModel {
  return { id: crypto.randomUUID(), profile };
}

function makeTab(profile: TerminalProfile): TerminalTab {
  const pane = makePane(profile);
  return {
    id: crypto.randomUUID(),
    title: profile.name,
    panes: [pane],
    activePaneId: pane.id,
    splitDirection: "vertical",
  };
}

function matchesShortcut(event: KeyboardEvent, shortcut: string): boolean {
  const parts = shortcut.split("+").map((part) => part.trim().toLowerCase()).filter(Boolean);
  const key = parts.at(-1) ?? "";
  const wantsCtrl = parts.includes("ctrl");
  const wantsShift = parts.includes("shift");
  const wantsAlt = parts.includes("alt");
  const wantsMeta = parts.includes("meta") || parts.includes("win");
  return event.key.toLowerCase() === key
    && event.ctrlKey === wantsCtrl
    && event.shiftKey === wantsShift
    && event.altKey === wantsAlt
    && event.metaKey === wantsMeta;
}

function remapTabs(tabs: TerminalTab[], profiles: TerminalProfile[]): TerminalTab[] {
  const available = new Map(profiles.filter((profile) => profile.available).map((profile) => [profile.id, profile]));
  const fallback = available.get("nebula") ?? available.values().next().value;
  if (!fallback) return tabs;
  return tabs.map((tab) => ({
    ...tab,
    panes: tab.panes.map((pane) => ({ ...pane, profile: available.get(pane.profile.id) ?? fallback })),
  }));
}

export default function App() {
  const [profiles, setProfiles] = useState<TerminalProfile[]>(previewProfiles);
  const [preferences, setPreferences] = useState<AppearancePreferences>(() => loadPreferences());
  const firstTabRef = useRef(makeTab(previewProfiles[0]!));
  const [tabs, setTabs] = useState<TerminalTab[]>([firstTabRef.current]);
  const [activeTabId, setActiveTabId] = useState(firstTabRef.current.id);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [profileMenuOpen, setProfileMenuOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchNonce, setSearchNonce] = useState(0);
  const hydratedRef = useRef(false);
  const closingRef = useRef(false);
  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;

  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0];
  const activePane = activeTab?.panes.find((pane) => pane.id === activeTab.activePaneId) ?? activeTab?.panes[0];

  const hydrateWorkspace = useCallback((detected: TerminalProfile[]) => {
    setProfiles(detected);
    if (!hydratedRef.current) {
      hydratedRef.current = true;
      if (preferences.restoreSession) {
        const restored = loadSession(detected);
        if (restored) {
          setTabs(restored.tabs);
          setActiveTabId(restored.activeTabId);
          return;
        }
      }
    }
    setTabs((current) => remapTabs(current, detected));
  }, [preferences.restoreSession]);

  useEffect(() => {
    if (!isTauri()) {
      hydrateWorkspace(previewProfiles);
      return;
    }
    void invoke<TerminalProfile[]>("detect_profiles")
      .then(hydrateWorkspace)
      .catch(() => hydrateWorkspace(previewProfiles));
  }, [hydrateWorkspace]);

  useEffect(() => {
    savePreferences(preferences);
    document.documentElement.style.setProperty("--accent", preferences.accent);
    document.documentElement.dataset.animation = preferences.animationLevel;
    document.documentElement.dataset.tabDensity = preferences.tabDensity;
    if (isTauri()) {
      void invoke("set_window_effect", { mode: preferences.backgroundMode, dark: true }).catch(() => undefined);
    }
  }, [preferences]);

  useEffect(() => {
    if (hydratedRef.current && preferences.restoreSession) saveSession(tabs, activeTabId);
  }, [tabs, activeTabId, preferences.restoreSession]);

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow().onCloseRequested((event) => {
      if (closingRef.current || !preferences.confirmCloseMultipleTabs || tabsRef.current.length <= 1) return;
      event.preventDefault();
      if (window.confirm(`Close Nebula Terminal and ${tabsRef.current.length} open tabs?`)) {
        closingRef.current = true;
        void getCurrentWindow().close();
      }
    }).then((dispose) => { unlisten = dispose; });
    return () => unlisten?.();
  }, [preferences.confirmCloseMultipleTabs]);

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow().onDragDropEvent((event) => {
      if (event.payload.type === "drop" && event.payload.paths.length) {
        window.dispatchEvent(new CustomEvent("nebula:insert-paths", { detail: event.payload.paths }));
      }
    }).then((dispose) => { unlisten = dispose; });
    return () => unlisten?.();
  }, []);

  const resolveProfile = useCallback((profileId?: string) => {
    const requested = profileId ?? preferences.defaultProfileId;
    return profiles.find((profile) => profile.id === requested && profile.available)
      ?? profiles.find((profile) => profile.id === "nebula" && profile.available)
      ?? profiles.find((profile) => profile.available);
  }, [preferences.defaultProfileId, profiles]);

  const openNewTab = useCallback((profileId?: string) => {
    const profile = resolveProfile(profileId);
    if (!profile) return;
    const next = makeTab(profile);
    setTabs((current) => [...current, next]);
    setActiveTabId(next.id);
  }, [resolveProfile]);

  const closeTab = useCallback((id: string) => {
    setTabs((current) => {
      if (current.length === 1) return current;
      const index = current.findIndex((tab) => tab.id === id);
      const next = current.filter((tab) => tab.id !== id);
      if (id === activeTabId) {
        const replacement = next[Math.min(Math.max(index, 0), next.length - 1)];
        if (replacement) setActiveTabId(replacement.id);
      }
      return next;
    });
  }, [activeTabId]);

  const moveTab = useCallback((fromId: string, toId: string) => {
    setTabs((current) => {
      const from = current.findIndex((tab) => tab.id === fromId);
      const to = current.findIndex((tab) => tab.id === toId);
      if (from < 0 || to < 0 || from === to) return current;
      const next = [...current];
      const [moved] = next.splice(from, 1);
      if (!moved) return current;
      next.splice(to, 0, moved);
      return next;
    });
  }, []);

  const setActivePane = useCallback((tabId: string, paneId: string) => {
    setTabs((current) => current.map((tab) => tab.id === tabId ? { ...tab, activePaneId: paneId } : tab));
    setActiveTabId(tabId);
  }, []);

  const splitActive = useCallback((direction: SplitDirection, profileId?: string) => {
    const profile = resolveProfile(profileId ?? activePane?.profile.id);
    if (!profile || !activeTab) return;
    const pane = makePane(profile);
    setTabs((current) => current.map((tab) => {
      if (tab.id !== activeTab.id || tab.panes.length >= 4) return tab;
      return { ...tab, panes: [...tab.panes, pane], activePaneId: pane.id, splitDirection: direction };
    }));
  }, [activePane?.profile.id, activeTab, resolveProfile]);

  const closePane = useCallback((tabId: string, paneId: string) => {
    const targetTab = tabsRef.current.find((tab) => tab.id === tabId);
    if (!targetTab) return;
    if (targetTab.panes.length <= 1) {
      closeTab(tabId);
      return;
    }
    setTabs((current) => current.map((tab) => {
      if (tab.id !== tabId) return tab;
      const index = tab.panes.findIndex((pane) => pane.id === paneId);
      const panes = tab.panes.filter((pane) => pane.id !== paneId);
      if (index < 0 || panes.length === tab.panes.length) return tab;
      const activePaneId = tab.activePaneId === paneId
        ? (panes[Math.min(index, panes.length - 1)] ?? panes[0]!).id
        : tab.activePaneId;
      return { ...tab, panes, activePaneId };
    }));
  }, [closeTab]);

  const closeActivePane = useCallback(() => {
    if (!activeTab) return;
    closePane(activeTab.id, activeTab.activePaneId);
  }, [activeTab, closePane]);

  const adjustFontSize = useCallback((delta: number) => {
    setPreferences((current) => ({ ...current, fontSize: Math.min(32, Math.max(8, current.fontSize + delta)) }));
  }, []);

  const commands = useMemo<PaletteCommand[]>(() => [
    { id: "new-default", label: "New terminal", detail: `Open ${resolveProfile()?.name ?? "default profile"}`, shortcut: preferences.keybindings.newTab, run: () => openNewTab() },
    ...profiles.filter((profile) => profile.available).map((profile) => ({ id: `new-${profile.id}`, label: `New ${profile.name} tab`, detail: profile.executable, run: () => openNewTab(profile.id) })),
    { id: "split-vertical", label: "Split pane vertically", detail: "Create another terminal beside the active pane", shortcut: preferences.keybindings.splitVertical, run: () => splitActive("vertical") },
    { id: "split-horizontal", label: "Split pane horizontally", detail: "Create another terminal below the active pane", shortcut: preferences.keybindings.splitHorizontal, run: () => splitActive("horizontal") },
    { id: "close-pane", label: "Close active pane", detail: "Close the focused split", shortcut: preferences.keybindings.closePane, run: closeActivePane },
    { id: "find", label: "Find in terminal", detail: "Search the active terminal buffer", shortcut: preferences.keybindings.find, run: () => setSearchOpen(true) },
    { id: "settings", label: "Open settings", detail: "Appearance, terminal, profiles and keybindings", shortcut: preferences.keybindings.settings, run: () => setSettingsOpen(true) },
    { id: "reset-appearance", label: "Reset settings", detail: "Restore Nebula defaults", run: () => setPreferences(defaultPreferences) },
  ], [closeActivePane, openNewTab, preferences.keybindings, profiles, resolveProfile, splitActive]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.matches("input, textarea, select, [contenteditable='true']")) return;
      const bindings = preferences.keybindings;
      const action = (shortcut: string, run: () => void) => {
        if (!matchesShortcut(event, shortcut)) return false;
        event.preventDefault();
        run();
        return true;
      };
      if (action(bindings.commandPalette, () => setPaletteOpen(true))) return;
      if (action(bindings.newTab, () => openNewTab())) return;
      if (action(bindings.settings, () => setSettingsOpen(true))) return;
      if (action(bindings.find, () => setSearchOpen(true))) return;
      if (action(bindings.splitVertical, () => splitActive("vertical"))) return;
      if (action(bindings.splitHorizontal, () => splitActive("horizontal"))) return;
      if (action(bindings.closePane, closeActivePane)) return;
      if (activeTab) action(bindings.closeTab, () => closeTab(activeTab.id));
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [activeTab, closeActivePane, closeTab, openNewTab, preferences.keybindings, splitActive]);

  const imageStyle = preferences.backgroundMode === "image" && preferences.backgroundImage
    ? { backgroundImage: `url(${JSON.stringify(preferences.backgroundImage)})`, opacity: preferences.backgroundImageOpacity }
    : undefined;

  return (
    <main className={`app-shell app-shell--${preferences.backgroundMode}`}>
      {imageStyle && <div className="app-background-image" style={imageStyle} />}
      <Titlebar
        tabs={tabs}
        activeTabId={activeTabId}
        onSelectTab={setActiveTabId}
        onCloseTab={closeTab}
        onMoveTab={moveTab}
        onNewTab={() => openNewTab()}
        onOpenProfileMenu={() => setProfileMenuOpen((open) => !open)}
        onOpenPalette={() => setPaletteOpen(true)}
        onOpenSettings={() => setSettingsOpen(true)}
      />
      <NewTabMenu open={profileMenuOpen} profiles={profiles} onPick={openNewTab} onClose={() => setProfileMenuOpen(false)} />

      <div className="workspace">
        <div className="terminal-stack">
          {tabs.map((tab) => (
            <div key={tab.id} className={`terminal-layer ${tab.id === activeTabId ? "terminal-layer--active" : ""}`}>
              <div className={`pane-grid pane-grid--${tab.splitDirection}`} data-count={tab.panes.length}>
                {tab.panes.map((pane) => {
                  const focused = tab.id === activeTabId && pane.id === tab.activePaneId && !settingsOpen && !paletteOpen && !searchOpen;
                  return (
                    <div key={pane.id} className={`pane-frame ${pane.id === tab.activePaneId ? "pane-frame--active" : ""}`}>
                      {tab.panes.length > 1 && <div className="pane-chip"><span style={{ background: pane.profile.accent }} />{pane.profile.name}<button type="button" onClick={() => closePane(tab.id, pane.id)}>×</button></div>}
                      <TerminalPane
                        paneId={pane.id}
                        profile={pane.profile}
                        preferences={preferences}
                        focused={focused}
                        searchRequest={pane.id === tab.activePaneId ? { query: searchQuery, nonce: searchNonce } : undefined}
                        onFocus={() => setActivePane(tab.id, pane.id)}
                        onFontSizeDelta={adjustFontSize}
                      />
                    </div>
                  );
                })}
              </div>
            </div>
          ))}
          <SearchBar open={searchOpen} query={searchQuery} onQueryChange={(query) => { setSearchQuery(query); setSearchNonce((nonce) => nonce + 1); }} onNext={() => setSearchNonce((nonce) => nonce + 1)} onClose={() => setSearchOpen(false)} />
        </div>

        <footer className="statusbar">
          <div className="status-left"><span className="status-dot" /><span>{activePane?.profile.name ?? "Nebula"}</span><span className="status-divider" /><span>{isTauri() ? "Native PTY" : "UI preview"}</span>{activeTab && activeTab.panes.length > 1 && <><span className="status-divider" /><span>{activeTab.panes.length} panes</span></>}</div>
          <div className="status-right"><button type="button" onClick={() => splitActive("vertical")}>Split ↔</button><button type="button" onClick={() => splitActive("horizontal")}>Split ↕</button><button type="button" onClick={() => setSearchOpen(true)}>Find</button><button type="button" onClick={() => setPaletteOpen(true)}>Commands</button><button type="button" onClick={() => setSettingsOpen(true)}>Settings</button></div>
        </footer>
      </div>

      <SettingsPanel
        open={settingsOpen}
        profiles={profiles}
        preferences={preferences}
        onChange={setPreferences}
        onClose={() => setSettingsOpen(false)}
        onReset={() => setPreferences(defaultPreferences)}
        onClearSession={() => { clearSession(); window.alert("Saved workspace cleared."); }}
      />
      {settingsOpen && <button className="settings-scrim" type="button" onClick={() => setSettingsOpen(false)} aria-label="Close settings" />}
      <CommandPalette open={paletteOpen} commands={commands} onClose={() => setPaletteOpen(false)} />
    </main>
  );
}

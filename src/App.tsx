import { Fragment, Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Columns2, Command, Rows2, Search, Settings } from "lucide-react";
import Titlebar from "./components/Titlebar";
import TerminalPane from "./components/TerminalPane";
import NewTabMenu from "./components/NewTabMenu";
import SearchBar from "./components/SearchBar";
import { defaultPreferences, loadPreferences, savePreferences } from "./preferences";
import { clearSession, loadSession, saveSession } from "./session";
import type { PaletteCommand } from "./components/CommandPalette";
import type { AppearancePreferences, SplitDirection, TerminalPaneModel, TerminalProfile, TerminalTab } from "./types";

const SettingsPanel = lazy(() => import("./components/SettingsPanel"));
const CommandPalette = lazy(() => import("./components/CommandPalette"));

const previewProfiles: TerminalProfile[] = [
  { id: "nebula", name: "Nebula", kind: "nebula", available: true, accent: "#f2a93b" },
  { id: "cmd", name: "Command Prompt", kind: "cmd", available: true, executable: "cmd.exe", accent: "#70b391" },
  { id: "powershell", name: "Windows PowerShell", kind: "powershell", available: true, executable: "powershell.exe", accent: "#6e9fbd" },
  { id: "pwsh", name: "PowerShell 7", kind: "pwsh", available: false, accent: "#8f9fbd" },
  { id: "wsl", name: "WSL", kind: "wsl", available: false, accent: "#59a99c" },
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
    paneSizes: [1],
  };
}

function matchesShortcut(event: KeyboardEvent, shortcut: string): boolean {
  const parts = shortcut.split("+").map((part) => part.trim().toLowerCase()).filter(Boolean);
  const key = parts.at(-1) ?? "";
  return event.key.toLowerCase() === key
    && event.ctrlKey === parts.includes("ctrl")
    && event.shiftKey === parts.includes("shift")
    && event.altKey === parts.includes("alt")
    && event.metaKey === (parts.includes("meta") || parts.includes("win"));
}

function remapTabs(tabs: TerminalTab[], profiles: TerminalProfile[]): TerminalTab[] {
  const available = new Map(profiles.filter((profile) => profile.available).map((profile) => [profile.id, profile]));
  const fallback = available.get("nebula") ?? available.values().next().value;
  if (!fallback) return tabs;
  return tabs.map((tab) => ({
    ...tab,
    paneSizes: tab.paneSizes.length === tab.panes.length ? tab.paneSizes : tab.panes.map(() => 1),
    panes: tab.panes.map((pane) => ({ ...pane, profile: available.get(pane.profile.id) ?? fallback })),
  }));
}

export default function App() {
  const [profiles, setProfiles] = useState<TerminalProfile[]>(previewProfiles);
  const [preferences, setPreferences] = useState<AppearancePreferences>(() => loadPreferences());
  const preferencesRef = useRef(preferences);
  preferencesRef.current = preferences;
  const firstTabRef = useRef(makeTab(previewProfiles[0]!));
  const [tabs, setTabs] = useState<TerminalTab[]>([firstTabRef.current]);
  const [activeTabId, setActiveTabId] = useState(firstTabRef.current.id);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [profileMenuOpen, setProfileMenuOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchSignal, setSearchSignal] = useState({ nonce: 0, backwards: false });
  const [notice, setNotice] = useState<string | null>(null);
  const hydratedRef = useRef(false);
  const closingRef = useRef(false);
  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;

  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0];
  const activePane = activeTab?.panes.find((pane) => pane.id === activeTab.activePaneId) ?? activeTab?.panes[0];
  const nativeHost = isTauri();

  const hydrateWorkspace = useCallback((detected: TerminalProfile[]) => {
    setProfiles(detected);
    if (!hydratedRef.current) {
      hydratedRef.current = true;
      if (preferencesRef.current.restoreSession) {
        const restored = loadSession(detected);
        if (restored) {
          setTabs(restored.tabs);
          setActiveTabId(restored.activeTabId);
          return;
        }
      }
      const first = detected.find((profile) => profile.id === preferencesRef.current.defaultProfileId && profile.available)
        ?? detected.find((profile) => profile.id === "nebula" && profile.available)
        ?? detected.find((profile) => profile.available);
      if (first) {
        const tab = makeTab(first);
        setTabs([tab]);
        setActiveTabId(tab.id);
      }
      return;
    }
    setTabs((current) => remapTabs(current, detected));
  }, []);

  useEffect(() => {
    if (!nativeHost) {
      hydrateWorkspace(previewProfiles);
      return;
    }
    void invoke<TerminalProfile[]>("detect_profiles")
      .then(hydrateWorkspace)
      .catch(() => hydrateWorkspace(previewProfiles));
  }, [hydrateWorkspace, nativeHost]);

  useEffect(() => {
    savePreferences(preferences);
    document.documentElement.style.setProperty("--accent", preferences.accent);
    document.documentElement.dataset.animation = preferences.animationLevel;
    document.documentElement.dataset.tabDensity = preferences.tabDensity;
    if (nativeHost) {
      void invoke("set_window_effect", { mode: preferences.backgroundMode, dark: true }).catch(() => undefined);
    }
  }, [nativeHost, preferences]);

  useEffect(() => {
    if (hydratedRef.current && preferences.restoreSession) saveSession(tabs, activeTabId);
  }, [tabs, activeTabId, preferences.restoreSession]);

  useEffect(() => {
    if (!notice) return;
    const timeout = window.setTimeout(() => setNotice(null), 2600);
    return () => window.clearTimeout(timeout);
  }, [notice]);

  useEffect(() => {
    if (!nativeHost) return;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow().onCloseRequested((event) => {
      if (closingRef.current || !preferences.confirmCloseMultipleTabs || tabsRef.current.length <= 1) return;
      event.preventDefault();
      if (window.confirm(`Close Nebula Terminal with ${tabsRef.current.length} open tabs?`)) {
        closingRef.current = true;
        void getCurrentWindow().close();
      }
    }).then((dispose) => { unlisten = dispose; });
    return () => unlisten?.();
  }, [nativeHost, preferences.confirmCloseMultipleTabs]);

  useEffect(() => {
    if (!nativeHost) return;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow().onDragDropEvent((event) => {
      if (event.payload.type === "drop" && event.payload.paths.length) {
        window.dispatchEvent(new CustomEvent("nebula:insert-paths", { detail: event.payload.paths }));
      }
    }).then((dispose) => { unlisten = dispose; });
    return () => unlisten?.();
  }, [nativeHost]);

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
    if (nativeHost && tabsRef.current.length === 1 && tabsRef.current[0]?.id === id) {
      void getCurrentWindow().close();
      return;
    }
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
  }, [activeTabId, nativeHost]);

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

  const setPaneTitle = useCallback((tabId: string, paneId: string, title: string) => {
    setTabs((current) => current.map((tab) => tab.id === tabId && tab.activePaneId === paneId && tab.title !== title ? { ...tab, title } : tab));
  }, []);

  const splitActive = useCallback((direction: SplitDirection, profileId?: string) => {
    const profile = resolveProfile(profileId ?? activePane?.profile.id);
    if (!profile || !activeTab) return;
    const pane = makePane(profile);
    setTabs((current) => current.map((tab) => {
      if (tab.id !== activeTab.id || tab.panes.length >= 4) return tab;
      const sizes = tab.paneSizes.length === tab.panes.length ? tab.paneSizes : tab.panes.map(() => 1);
      return { ...tab, panes: [...tab.panes, pane], paneSizes: [...sizes, 1], activePaneId: pane.id, splitDirection: direction };
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
      const sizes = (tab.paneSizes.length === tab.panes.length ? tab.paneSizes : tab.panes.map(() => 1)).filter((_, paneIndex) => paneIndex !== index);
      const activePaneId = tab.activePaneId === paneId
        ? (panes[Math.min(index, panes.length - 1)] ?? panes[0]!).id
        : tab.activePaneId;
      return { ...tab, panes, paneSizes: sizes, activePaneId };
    }));
  }, [closeTab]);

  const closeActivePane = useCallback(() => {
    if (activeTab) closePane(activeTab.id, activeTab.activePaneId);
  }, [activeTab, closePane]);

  const beginPaneResize = (event: ReactPointerEvent<HTMLButtonElement>, tabId: string, index: number, direction: SplitDirection) => {
    event.preventDefault();
    const tab = tabsRef.current.find((candidate) => candidate.id === tabId);
    const container = event.currentTarget.parentElement;
    if (!tab || !container) return;
    const sizes = tab.paneSizes.length === tab.panes.length ? [...tab.paneSizes] : tab.panes.map(() => 1);
    const total = sizes.reduce((sum, size) => sum + size, 0);
    const pair = sizes[index]! + sizes[index + 1]!;
    const start = direction === "vertical" ? event.clientX : event.clientY;
    const pixels = direction === "vertical" ? container.clientWidth : container.clientHeight;
    const minimum = Math.max(0.08 * total, 0.12);
    document.documentElement.classList.add("is-resizing-pane");

    const move = (moveEvent: PointerEvent) => {
      const current = direction === "vertical" ? moveEvent.clientX : moveEvent.clientY;
      const delta = ((current - start) / Math.max(1, pixels)) * total;
      const first = Math.min(pair - minimum, Math.max(minimum, sizes[index]! + delta));
      const nextSizes = [...sizes];
      nextSizes[index] = first;
      nextSizes[index + 1] = pair - first;
      setTabs((currentTabs) => currentTabs.map((candidate) => candidate.id === tabId ? { ...candidate, paneSizes: nextSizes } : candidate));
    };
    const finish = () => {
      document.documentElement.classList.remove("is-resizing-pane");
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", finish);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish, { once: true });
  };

  const adjustFontSize = useCallback((delta: number) => {
    setPreferences((current) => ({ ...current, fontSize: Math.min(32, Math.max(8, current.fontSize + delta)) }));
  }, []);

  const commands = useMemo<PaletteCommand[]>(() => [
    { id: "new-default", label: "New terminal", detail: `Open ${resolveProfile()?.name ?? "the default profile"}`, shortcut: preferences.keybindings.newTab, run: () => openNewTab() },
    ...profiles.filter((profile) => profile.available).map((profile) => ({ id: `new-${profile.id}`, label: `New ${profile.name} tab`, detail: profile.executable, run: () => openNewTab(profile.id) })),
    { id: "split-vertical", label: "Split right", detail: "Open another terminal beside this one", shortcut: preferences.keybindings.splitVertical, run: () => splitActive("vertical") },
    { id: "split-horizontal", label: "Split down", detail: "Open another terminal below this one", shortcut: preferences.keybindings.splitHorizontal, run: () => splitActive("horizontal") },
    { id: "close-pane", label: "Close active pane", detail: "Close the focused split", shortcut: preferences.keybindings.closePane, run: closeActivePane },
    { id: "find", label: "Find in terminal", detail: "Search the active terminal history", shortcut: preferences.keybindings.find, run: () => setSearchOpen(true) },
    { id: "settings", label: "Open settings", detail: "Appearance, terminal, profiles and shortcuts", shortcut: preferences.keybindings.settings, run: () => setSettingsOpen(true) },
    { id: "reset-appearance", label: "Reset settings", detail: "Restore the Solar Noir defaults", run: () => setPreferences(defaultPreferences) },
  ], [closeActivePane, openNewTab, preferences.keybindings, profiles, resolveProfile, splitActive]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const isTerminalInput = target?.classList.contains("xterm-helper-textarea");
      if (!isTerminalInput && target?.matches("input, textarea, select, [contenteditable='true']")) return;
      const bindings = preferences.keybindings;
      const action = (shortcut: string, run: () => void) => {
        if (!matchesShortcut(event, shortcut)) return false;
        event.preventDefault();
        event.stopPropagation();
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
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [activeTab, closeActivePane, closeTab, openNewTab, preferences.keybindings, splitActive]);

  const requestSearch = (backwards: boolean) => setSearchSignal((current) => ({ nonce: current.nonce + 1, backwards }));
  const imageStyle = preferences.backgroundMode === "image" && preferences.backgroundImage
    ? { backgroundImage: `url("${preferences.backgroundImage}")`, opacity: preferences.backgroundImageOpacity }
    : undefined;
  const shellStyle = { "--atmosphere": preferences.atmosphere } as CSSProperties;

  return (
    <main className={`app-shell app-shell--${preferences.backgroundMode}`} style={shellStyle}>
      <div className="atmosphere" aria-hidden="true"><span className="atmosphere__sun" /><span className="atmosphere__tide" /><span className="atmosphere__grain" /></div>
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
              <div className={`pane-grid pane-grid--${tab.splitDirection}`}>
                {tab.panes.map((pane, index) => {
                  const focused = tab.id === activeTabId && pane.id === tab.activePaneId && !settingsOpen && !paletteOpen && !searchOpen;
                  const size = tab.paneSizes[index] ?? 1;
                  return (
                    <Fragment key={pane.id}>
                      <div className={`pane-frame ${pane.id === tab.activePaneId ? "pane-frame--active" : ""}`} style={{ flexGrow: size, flexBasis: 0 }}>
                        {tab.panes.length > 1 && <div className="pane-chip"><span style={{ background: pane.profile.accent }} />{pane.profile.name}<button type="button" onClick={() => closePane(tab.id, pane.id)} aria-label={`Close ${pane.profile.name} pane`}>×</button></div>}
                        <TerminalPane
                          profile={pane.profile}
                          preferences={preferences}
                          focused={focused}
                          searchRequest={pane.id === tab.activePaneId ? { query: searchQuery, nonce: searchSignal.nonce, backwards: searchSignal.backwards } : undefined}
                          onFocus={() => setActivePane(tab.id, pane.id)}
                          onFontSizeDelta={adjustFontSize}
                          onTitleChange={(title) => setPaneTitle(tab.id, pane.id, title)}
                        />
                      </div>
                      {index < tab.panes.length - 1 && <button className="pane-divider" type="button" aria-label="Resize terminal panes" onPointerDown={(event) => beginPaneResize(event, tab.id, index, tab.splitDirection)} />}
                    </Fragment>
                  );
                })}
              </div>
            </div>
          ))}
          <SearchBar
            open={searchOpen}
            query={searchQuery}
            onQueryChange={(query) => { setSearchQuery(query); setSearchSignal((current) => ({ nonce: current.nonce + 1, backwards: false })); }}
            onNext={() => requestSearch(false)}
            onPrevious={() => requestSearch(true)}
            onClose={() => setSearchOpen(false)}
          />
        </div>

        <footer className="statusbar">
          <div className="status-left"><span className="status-dot" /><span>{activePane?.profile.name ?? "Nebula"}</span><span className="status-divider" /><span>{nativeHost ? "Native PTY" : "Interface preview"}</span>{activeTab && activeTab.panes.length > 1 && <><span className="status-divider" /><span>{activeTab.panes.length} panes</span></>}</div>
          <div className="status-right">
            <button type="button" onClick={() => splitActive("vertical")} title="Split right"><Columns2 size={13} />Split</button>
            <button type="button" onClick={() => splitActive("horizontal")} title="Split down"><Rows2 size={13} />Stack</button>
            <button type="button" onClick={() => setSearchOpen(true)}><Search size={13} />Find</button>
            <button type="button" onClick={() => setPaletteOpen(true)}><Command size={13} />Commands</button>
            <button type="button" onClick={() => setSettingsOpen(true)}><Settings size={13} />Settings</button>
          </div>
        </footer>
      </div>

      {settingsOpen && <button className="settings-scrim" type="button" onClick={() => setSettingsOpen(false)} aria-label="Close settings" />}
      <Suspense fallback={null}>
        <SettingsPanel
          open={settingsOpen}
          profiles={profiles}
          preferences={preferences}
          onChange={setPreferences}
          onClose={() => setSettingsOpen(false)}
          onReset={() => setPreferences({ ...defaultPreferences, keybindings: { ...defaultPreferences.keybindings } })}
          onClearSession={() => { clearSession(); setNotice("Saved workspace cleared"); }}
        />
        <CommandPalette open={paletteOpen} commands={commands} onClose={() => setPaletteOpen(false)} />
      </Suspense>
      {notice && <div className="toast" role="status">{notice}</div>}
    </main>
  );
}

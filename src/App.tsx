import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ArrowDownToLine, ArrowLeftRight, ArrowUpToLine, ClipboardCheck, ClipboardPaste, Columns2, Copy, CopyPlus, Eraser, Keyboard, Minus, Monitor, Palette, PanelTopClose, Pencil, Plus, Rows2, Search, Settings2, SquareX, TextSelect, X, ZoomIn } from "lucide-react";
import Titlebar, { tabTitle } from "./components/Titlebar";
import TerminalPane, { type FinishedCommand } from "./components/TerminalPane";
import { finishedMessage, notify } from "./notify";
import UpdateBanner from "./components/UpdateBanner";
import { automaticCheckDue, checkForUpdate, dismiss as dismissUpdate, isDismissed, markChecked, type UpdateInfo } from "./updates";
import SearchBar from "./components/SearchBar";
import Menu, { type MenuEntry } from "./components/Menu";
import ProfileIcon from "./components/ProfileIcon";
import { defaultKeybindings, defaultPreferences, loadPreferences, savePreferences } from "./preferences";
import { clearSession, loadSession, saveSession } from "./session";
import { pickProfile, previewProfiles, profileCommandLabel, profileGroup, profileGroupLabels, profileMenuSections, profileTag, refreshTabProfiles } from "./profiles";
import { detectProfiles } from "./customProfiles";
import { readableOn } from "./color";
import { resolveTheme, tabColors, themes } from "./themes";
import { MAX_PANES, computeLayout, findNeighbor, minimumExtent, nodeAt, paneIds, paneLeaf, removePane, resizePair, setSplitSizes, splitPane, type Divider, type FocusDirection, type Rect } from "./layout";
import { matchesShortcut } from "./keys";
import { getPane } from "./paneRegistry";
import type { PaletteCommand } from "./components/CommandPalette";
import type { SettingsPage } from "./components/SettingsPanel";
import type { AppearancePreferences, SplitDirection, TabColor, TerminalPaneModel, TerminalProfile, TerminalTab } from "./types";

const SettingsPanel = lazy(() => import("./components/SettingsPanel"));
const CommandPalette = lazy(() => import("./components/CommandPalette"));

const TAB_CLOSE_MS = 150;
/** Smallest pane a split or a divider drag may produce, in pixels. */
const MIN_PANE_WIDTH = 120;
const MIN_PANE_HEIGHT = 72;

const arrowDirections: Record<string, FocusDirection> = { ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down" };

type OpenMenu =
  | { kind: "profiles"; x: number; y: number }
  | { kind: "tab"; tabId: string; x: number; y: number }
  | { kind: "terminal"; x: number; y: number };

type Confirmation = { title: string; body: string; action: string; run: () => void };

function makePane(profile: TerminalProfile, cwd?: string): TerminalPaneModel {
  return { id: crypto.randomUUID(), profile, cwd };
}

function makeTab(profile: TerminalProfile, cwd?: string): TerminalTab {
  const pane = makePane(profile, cwd);
  return { id: crypto.randomUUID(), title: profile.name, panes: [pane], layout: paneLeaf(pane.id), activePaneId: pane.id };
}

function paneElement(paneId: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`.pane[data-pane-id="${paneId}"]`);
}

/** One pixel before a pane that doesn't start at the tab's edge belongs to the divider there. */
const edge = (start: number) => (start > 1e-6 ? 1 : 0);

function paneStyle(rect: Rect | undefined): CSSProperties {
  if (!rect) return { display: "none" };
  return {
    left: `calc(${rect.x * 100}% + ${edge(rect.x)}px)`,
    top: `calc(${rect.y * 100}% + ${edge(rect.y)}px)`,
    width: `calc(${rect.width * 100}% - ${edge(rect.x)}px)`,
    height: `calc(${rect.height * 100}% - ${edge(rect.y)}px)`,
  };
}

function dividerStyle({ rect, direction }: Divider): CSSProperties {
  return direction === "vertical"
    ? { left: `${rect.x * 100}%`, top: `calc(${rect.y * 100}% + ${edge(rect.y)}px)`, width: 1, height: `calc(${rect.height * 100}% - ${edge(rect.y)}px)` }
    : { top: `${rect.y * 100}%`, left: `calc(${rect.x * 100}% + ${edge(rect.x)}px)`, height: 1, width: `calc(${rect.width * 100}% - ${edge(rect.x)}px)` };
}

/** Shells often report their executable path as the title; show the profile name instead. */
function cleanTitle(title: string, profile: TerminalProfile): string {
  const trimmed = title.replace(/^Administrator:\s*/i, "").trim();
  if (!trimmed || /^[a-z]:\\.*\.exe$/i.test(trimmed) || /\\(pwsh|powershell|cmd|bash|wsl)\.exe$/i.test(trimmed)) return profile.name;
  return trimmed;
}

export default function App() {
  const nativeHost = isTauri();
  const [profiles, setProfiles] = useState<TerminalProfile[]>(nativeHost ? [] : previewProfiles);
  const [preferences, setPreferences] = useState<AppearancePreferences>(() => loadPreferences());
  const preferencesRef = useRef(preferences);
  preferencesRef.current = preferences;
  const [tabs, setTabs] = useState<TerminalTab[]>([]);
  const [activeTabId, setActiveTabId] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsPage, setSettingsPage] = useState<SettingsPage | undefined>();
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [menu, setMenu] = useState<OpenMenu | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchSignal, setSearchSignal] = useState({ nonce: 0, backwards: false });
  const [searchResult, setSearchResult] = useState<{ index: number; count: number } | null>(null);
  const [activity, setActivity] = useState<ReadonlySet<string>>(() => new Set());
  const [finished, setFinished] = useState<ReadonlyMap<string, boolean>>(() => new Map());
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [closing, setClosing] = useState<ReadonlySet<string>>(() => new Set());
  const [notice, setNotice] = useState<string | null>(null);
  const [renamingTabId, setRenamingTabId] = useState<string | null>(null);
  const [resizingDivider, setResizingDivider] = useState<string | null>(null);
  const [ready, setReady] = useState(false);
  const hydratedRef = useRef(false);
  const closingWindowRef = useRef(false);
  /** Tabs playing their close animation; they are gone as far as every action is concerned. */
  const closingRef = useRef(new Set<string>());
  const [micaFailed, setMicaFailed] = useState(false);
  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;
  const activeTabIdRef = useRef(activeTabId);
  activeTabIdRef.current = activeTabId;

  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0];
  const activePane = activeTab?.panes.find((pane) => pane.id === activeTab.activePaneId) ?? activeTab?.panes[0];
  const theme = resolveTheme(preferences.themeId);
  const overlayOpen = settingsOpen || paletteOpen || confirmation !== null;
  // A tab closed mid-rename takes its editor with it, so only a live tab counts.
  const renaming = renamingTabId !== null && tabs.some((tab) => tab.id === renamingTabId && !closing.has(tab.id));
  const renamingRef = useRef(renaming);
  renamingRef.current = renaming;

  const hydrateWorkspace = useCallback((detected: TerminalProfile[]) => {
    setProfiles(detected);
    if (!hydratedRef.current) {
      hydratedRef.current = true;
      setReady(true);
      const restored = preferencesRef.current.restoreSession ? loadSession(detected) : null;
      if (restored) {
        setTabs(restored.tabs);
        setActiveTabId(restored.activeTabId);
        return;
      }
      const first = pickProfile(detected, preferencesRef.current.defaultProfileId);
      if (first) {
        const tab = makeTab(first);
        setTabs([tab]);
        setActiveTabId(tab.id);
      }
      return;
    }
    setTabs((current) => refreshTabProfiles(current, detected));
  }, []);

  useEffect(() => {
    void detectProfiles()
      .then(hydrateWorkspace)
      .catch(() => hydrateWorkspace([]));
  }, [hydrateWorkspace]);

  /** Detects profiles again after custom profiles changed; open panes keep running. */
  const refreshProfiles = useCallback(async () => {
    try {
      hydrateWorkspace(await detectProfiles());
    } catch {
      // Keep the current list; the next launch detects again.
    }
  }, [hydrateWorkspace]);

  useEffect(() => {
    savePreferences(preferences);
    const root = document.documentElement;
    root.style.setProperty("--accent", preferences.accent);
    root.style.setProperty("--term-bg", theme.background);
    root.style.setProperty("--term-fg", theme.foreground);
    root.dataset.scheme = theme.scheme;
    root.dataset.animation = preferences.animationLevel;
    root.dataset.density = preferences.tabDensity;
    root.dataset.background = preferences.backgroundMode === "mica" && micaFailed ? "solid" : preferences.backgroundMode;
    root.dataset.host = nativeHost ? "native" : "browser";
    root.style.setProperty("--on-accent", readableOn(preferences.accent));
    root.style.setProperty("--term-surface", preferences.terminalOpacity >= 1 ? theme.background : `color-mix(in srgb, ${theme.background} ${Math.round(preferences.terminalOpacity * 100)}%, transparent)`);
    document.querySelector('meta[name="theme-color"]')?.setAttribute("content", theme.background);
  }, [micaFailed, nativeHost, preferences, theme]);

  useEffect(() => {
    if (!nativeHost) return;
    // Mica needs Windows 11; elsewhere fall back to an opaque window instead of a see-through one.
    void invoke("set_window_effect", { mode: preferences.backgroundMode, dark: theme.scheme === "dark" })
      .then(() => setMicaFailed(false))
      .catch(() => setMicaFailed(preferences.backgroundMode === "mica"));
  }, [nativeHost, preferences.backgroundMode, theme.scheme]);

  useEffect(() => {
    if (hydratedRef.current && preferences.restoreSession && tabs.length) saveSession(tabs, activeTabId);
  }, [tabs, activeTabId, preferences.restoreSession]);

  useEffect(() => {
    if (!notice) return;
    const timeout = window.setTimeout(() => setNotice(null), 2400);
    return () => window.clearTimeout(timeout);
  }, [notice]);

  useEffect(() => {
    setActivity((current) => {
      if (!current.has(activeTabId)) return current;
      const next = new Set(current);
      next.delete(activeTabId);
      return next;
    });
    setFinished((current) => {
      if (!current.has(activeTabId)) return current;
      const next = new Map(current);
      next.delete(activeTabId);
      return next;
    });
  }, [activeTabId]);

  // Look for a new version a few seconds after launch, at most once a day.
  useEffect(() => {
    if (!preferences.checkForUpdates || !automaticCheckDue()) return;
    const timer = window.setTimeout(() => {
      markChecked();
      void checkForUpdate()
        .then((info) => {
          if (info && !isDismissed(info.version)) setUpdate(info);
        })
        .catch(() => undefined);
    }, 4000);
    return () => window.clearTimeout(timer);
  }, [preferences.checkForUpdates]);

  /** The user can't see this tab: another tab is in front, or the window is in the background. */
  const tabUnseen = useCallback((tabId: string) => document.hidden || !document.hasFocus() || tabId !== activeTabIdRef.current, []);

  const commandFinished = useCallback((tabId: string, command: FinishedCommand) => {
    const { notifyLongCommands, longCommandSeconds } = preferencesRef.current;
    if (!notifyLongCommands || command.seconds < longCommandSeconds || !tabUnseen(tabId)) return;
    const tab = tabsRef.current.find((candidate) => candidate.id === tabId);
    if (tabId !== activeTabIdRef.current) {
      setFinished((current) => new Map(current).set(tabId, command.code === null || command.code === 0));
    }
    const { title, body } = finishedMessage(command, tab ? tabTitle(tab) : "Nebula Terminal");
    notify(title, body);
  }, [tabUnseen]);

  /** OSC 9 / OSC 777 from a program: shown when its tab is out of sight. */
  const programNotification = useCallback((tabId: string, title: string, body: string) => {
    if (!preferencesRef.current.notifyLongCommands || !tabUnseen(tabId)) return;
    notify(title, body);
  }, [tabUnseen]);

  const closeWindow = useCallback(() => {
    if (!nativeHost) return;
    closingWindowRef.current = true;
    void getCurrentWindow().close();
  }, [nativeHost]);

  useEffect(() => {
    if (!nativeHost) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void getCurrentWindow().onCloseRequested((event) => {
      const count = tabsRef.current.filter((tab) => !closingRef.current.has(tab.id)).length;
      if (closingWindowRef.current || !preferencesRef.current.confirmCloseMultipleTabs || count <= 1) return;
      event.preventDefault();
      setConfirmation({ title: `Close ${count} tabs?`, body: "Every shell running in this window will be stopped.", action: "Close all", run: closeWindow });
    }).then((dispose) => { if (disposed) dispose(); else unlisten = dispose; });
    return () => { disposed = true; unlisten?.(); };
  }, [closeWindow, nativeHost]);

  useEffect(() => {
    if (!nativeHost) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void getCurrentWindow().onDragDropEvent((event) => {
      if (event.payload.type === "drop" && event.payload.paths.length) {
        window.dispatchEvent(new CustomEvent("nebula:insert-paths", { detail: event.payload.paths }));
      }
    }).then((dispose) => { if (disposed) dispose(); else unlisten = dispose; });
    return () => { disposed = true; unlisten?.(); };
  }, [nativeHost]);

  const resolveProfile = useCallback((profileId?: string) => pickProfile(profiles, profileId ?? preferences.defaultProfileId), [preferences.defaultProfileId, profiles]);

  /** The focused pane's folder, when new tabs and splits should open there. */
  const currentFolder = useCallback(() => {
    if (!preferencesRef.current.openInCurrentFolder) return undefined;
    const tab = tabsRef.current.find((candidate) => candidate.id === activeTabIdRef.current);
    return tab?.panes.find((pane) => pane.id === tab.activePaneId)?.cwd;
  }, []);

  const openNewTab = useCallback((profileId?: string) => {
    const profile = resolveProfile(profileId);
    if (!profile) return;
    const next = makeTab(profile, currentFolder());
    setTabs((current) => [...current, next]);
    setActiveTabId(next.id);
  }, [currentFolder, resolveProfile]);

  const removeTabs = useCallback((ids: string[]) => {
    const closingIds = closingRef.current;
    const remove = new Set(ids.filter((id) => !closingIds.has(id)));
    if (remove.size === 0) return;
    const current = tabsRef.current;
    const live = current.filter((tab) => !closingIds.has(tab.id) && !remove.has(tab.id));
    if (live.length === 0) {
      if (nativeHost) {
        closeWindow();
        return;
      }
      // The browser preview has no window to close; keep one tab open.
      const profile = resolveProfile();
      if (profile) {
        const replacement = makeTab(profile);
        live.push(replacement);
        setTabs((latest) => [...latest, replacement]);
      }
    }
    if (remove.has(activeTabIdRef.current) || closingIds.has(activeTabIdRef.current)) {
      const index = current.findIndex((tab) => tab.id === activeTabIdRef.current);
      const isLive = (tab: TerminalTab) => live.includes(tab);
      const neighbor = current.slice(index + 1).find(isLive) ?? current.slice(0, Math.max(index, 0)).reverse().find(isLive) ?? live[0];
      if (neighbor) setActiveTabId(neighbor.id);
    }

    remove.forEach((id) => closingIds.add(id));
    setClosing(new Set(closingIds));
    const finish = () => {
      setTabs((latest) => latest.filter((tab) => !remove.has(tab.id)));
      remove.forEach((id) => closingIds.delete(id));
      setClosing(new Set(closingIds));
    };
    if (preferencesRef.current.animationLevel === "off") finish();
    else window.setTimeout(finish, TAB_CLOSE_MS);
  }, [closeWindow, nativeHost, resolveProfile]);

  const closeTab = useCallback((id: string) => removeTabs([id]), [removeTabs]);

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

  const cycleTab = useCallback((delta: number) => {
    const open = tabsRef.current.filter((tab) => !closingRef.current.has(tab.id));
    if (open.length < 2) return;
    const index = open.findIndex((tab) => tab.id === activeTabIdRef.current);
    const next = open[(index + delta + open.length) % open.length];
    if (next) setActiveTabId(next.id);
  }, []);

  const setActivePane = useCallback((tabId: string, paneId: string) => {
    setTabs((current) => current.map((tab) => tab.id === tabId && tab.activePaneId !== paneId ? { ...tab, activePaneId: paneId } : tab));
    setActiveTabId(tabId);
  }, []);

  const setPaneTitle = useCallback((tabId: string, pane: TerminalPaneModel, title: string) => {
    const clean = cleanTitle(title, pane.profile);
    setTabs((current) => current.map((tab) => tab.id === tabId && tab.activePaneId === pane.id && tab.title !== clean ? { ...tab, title: clean } : tab));
  }, []);

  const setPaneFolder = useCallback((tabId: string, paneId: string, cwd: string) => {
    setTabs((current) => current.map((tab) => tab.id !== tabId || !tab.panes.some((pane) => pane.id === paneId && pane.cwd !== cwd)
      ? tab
      : { ...tab, panes: tab.panes.map((pane) => pane.id === paneId ? { ...pane, cwd } : pane) }));
  }, []);

  const markActivity = useCallback((tabId: string) => {
    if (tabId === activeTabIdRef.current) return;
    setActivity((current) => current.has(tabId) ? current : new Set([...current, tabId]));
  }, []);

  const splitActive = useCallback((direction: SplitDirection, profileId?: string) => {
    if (!activeTab) return;
    if (activeTab.panes.length >= MAX_PANES) {
      setNotice(`A tab holds up to ${MAX_PANES} panes`);
      return;
    }
    const box = paneElement(activeTab.activePaneId)?.getBoundingClientRect();
    const room = direction === "vertical" ? box?.width : box?.height;
    const minimum = direction === "vertical" ? MIN_PANE_WIDTH : MIN_PANE_HEIGHT;
    if (room && room < minimum * 2 + 1) {
      setNotice("Not enough room to split this pane");
      return;
    }
    const profile = resolveProfile(profileId ?? activePane?.profile.id);
    if (!profile) return;
    const pane = makePane(profile, currentFolder());
    setTabs((current) => current.map((tab) => {
      if (tab.id !== activeTab.id) return tab;
      return { ...tab, panes: [...tab.panes, pane], layout: splitPane(tab.layout, tab.activePaneId, pane.id, direction), activePaneId: pane.id };
    }));
  }, [activePane?.profile.id, activeTab, currentFolder, resolveProfile]);

  const closePane = useCallback((tabId: string, paneId: string) => {
    const targetTab = tabsRef.current.find((tab) => tab.id === tabId);
    if (!targetTab) return;
    if (targetTab.panes.length <= 1) {
      closeTab(tabId);
      return;
    }
    setTabs((current) => current.map((tab) => {
      if (tab.id !== tabId || !tab.panes.some((pane) => pane.id === paneId)) return tab;
      const { layout, focusId } = removePane(tab.layout, paneId);
      if (!layout) return tab;
      const panes = tab.panes.filter((pane) => pane.id !== paneId);
      const activePaneId = tab.activePaneId === paneId ? focusId ?? paneIds(layout)[0]! : tab.activePaneId;
      return { ...tab, panes, layout, activePaneId };
    }));
  }, [closeTab]);

  const closeActivePane = useCallback(() => {
    if (activeTab) closePane(activeTab.id, activeTab.activePaneId);
  }, [activeTab, closePane]);

  const focusNeighborPane = useCallback((key: string) => {
    const direction = arrowDirections[key];
    if (!activeTab || activeTab.panes.length < 2 || !direction) return false;
    const rects = new Map<string, Rect>();
    for (const pane of activeTab.panes) {
      const box = paneElement(pane.id)?.getBoundingClientRect();
      if (box) rects.set(pane.id, { x: box.left, y: box.top, width: box.width, height: box.height });
    }
    const next = findNeighbor(rects, activeTab.activePaneId, direction);
    if (next) setActivePane(activeTab.id, next);
    return true;
  }, [activeTab, setActivePane]);

  const duplicateTab = useCallback((tabId: string) => {
    const tab = tabsRef.current.find((candidate) => candidate.id === tabId);
    const pane = tab?.panes.find((candidate) => candidate.id === tab.activePaneId) ?? tab?.panes[0];
    if (!tab || !pane) return;
    const profile = resolveProfile(pane.profile.id);
    if (!profile) return;
    const next = { ...makeTab(profile, pane.cwd), color: tab.color };
    setTabs((current) => [...current, next]);
    setActiveTabId(next.id);
  }, [resolveProfile]);

  const renameTab = useCallback((tabId: string, name: string) => {
    const customTitle = name.trim().slice(0, 120) || undefined;
    setTabs((current) => current.map((tab) => tab.id === tabId ? { ...tab, customTitle } : tab));
  }, []);

  const setTabColor = useCallback((tabId: string, color: TabColor | undefined) => {
    setTabs((current) => current.map((tab) => tab.id === tabId ? { ...tab, color } : tab));
  }, []);

  const beginPaneResize = (event: ReactPointerEvent<HTMLDivElement>, tabId: string, divider: Divider) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const tab = tabsRef.current.find((candidate) => candidate.id === tabId);
    const split = tab && nodeAt(tab.layout, divider.path);
    const container = event.currentTarget.parentElement;
    if (!container || split?.type !== "split") return;
    const { direction, index, path } = divider;
    const across = direction === "vertical";
    const pixels = Math.max(1, across ? divider.splitRect.width * container.clientWidth : divider.splitRect.height * container.clientHeight);
    const paneMinimum = across ? MIN_PANE_WIDTH : MIN_PANE_HEIGHT;
    const minFirst = minimumExtent(split.children[index]!, direction, paneMinimum) / pixels;
    const minSecond = minimumExtent(split.children[index + 1]!, direction, paneMinimum) / pixels;
    const start = across ? event.clientX : event.clientY;
    const key = `${path.join(".")}:${index}`;
    setResizingDivider(key);
    document.documentElement.classList.add("is-resizing", `is-resizing--${direction}`);

    const move = (moveEvent: PointerEvent) => {
      const delta = ((across ? moveEvent.clientX : moveEvent.clientY) - start) / pixels;
      const sizes = resizePair(split.sizes, index, delta, minFirst, minSecond);
      setTabs((currentTabs) => currentTabs.map((candidate) => candidate.id === tabId ? { ...candidate, layout: setSplitSizes(candidate.layout, path, sizes) } : candidate));
    };
    const finish = () => {
      document.documentElement.classList.remove("is-resizing", `is-resizing--${direction}`);
      setResizingDivider(null);
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", finish);
  };

  const adjustFontSize = useCallback((delta: number) => {
    setPreferences((current) => ({ ...current, fontSize: Math.min(32, Math.max(8, current.fontSize + delta)) }));
  }, []);

  const openSettings = useCallback((page?: SettingsPage) => {
    setSettingsPage(page);
    setSettingsOpen(true);
  }, []);

  const openSearch = useCallback(() => {
    setSearchOpen(true);
    setSearchResult(null);
  }, []);

  const jumpToCommand = useCallback((direction: -1 | 1) => {
    if (!getPane(activePane?.id)?.jumpToCommand(direction)) setNotice("This shell doesn't mark its commands");
  }, [activePane?.id]);

  const copyLastOutput = useCallback(() => {
    setNotice(getPane(activePane?.id)?.copyLastOutput() ? "Output copied" : "No command output to copy yet");
  }, [activePane?.id]);

  const keys = preferences.keybindings;
  const commands = useMemo<PaletteCommand[]>(() => {
    const defaultProfile = resolveProfile();
    return [
      { id: "new-tab", group: "Tabs", label: "New tab", detail: defaultProfile?.name, icon: <Plus size={15} />, shortcut: keys.newTab, run: () => openNewTab() },
      { id: "rename-tab", group: "Tabs", label: "Rename tab", icon: <Pencil size={15} />, keywords: "title name", run: () => activeTab && setRenamingTabId(activeTab.id) },
      { id: "duplicate-tab", group: "Tabs", label: "Duplicate tab", icon: <CopyPlus size={15} />, run: () => activeTab && duplicateTab(activeTab.id) },
      { id: "next-tab", group: "Tabs", label: "Next tab", icon: <ArrowLeftRight size={15} />, shortcut: keys.nextTab, run: () => cycleTab(1) },
      { id: "previous-tab", group: "Tabs", label: "Previous tab", icon: <ArrowLeftRight size={15} />, shortcut: keys.previousTab, run: () => cycleTab(-1) },
      { id: "close-tab", group: "Tabs", label: "Close tab", icon: <X size={15} />, shortcut: keys.closeTab, run: () => activeTab && closeTab(activeTab.id) },
      { id: "close-other-tabs", group: "Tabs", label: "Close other tabs", icon: <PanelTopClose size={15} />, run: () => activeTab && removeTabs(tabsRef.current.filter((tab) => tab.id !== activeTab.id).map((tab) => tab.id)) },
      ...profiles.filter((profile) => profile.available).map((profile): PaletteCommand => ({
        id: `new-${profile.id}`, group: "Profiles", label: profileCommandLabel(profile), detail: profileTag(profile) ?? (profileGroup(profile) === "custom" ? "Custom" : undefined),
        icon: <ProfileIcon kind={profile.kind} accent={profile.accent} size={15} />, keywords: `${profile.kind} ${profileGroupLabels[profileGroup(profile)]} shell`, run: () => openNewTab(profile.id),
      })),
      { id: "split-right", group: "Panes", label: "Split right", icon: <Columns2 size={15} />, shortcut: keys.splitVertical, run: () => splitActive("vertical") },
      { id: "split-down", group: "Panes", label: "Split down", icon: <Rows2 size={15} />, shortcut: keys.splitHorizontal, run: () => splitActive("horizontal") },
      { id: "close-pane", group: "Panes", label: "Close pane", icon: <SquareX size={15} />, shortcut: keys.closePane, run: closeActivePane },
      ...[undefined, ...tabColors].map((color): PaletteCommand => ({
        id: `tab-color-${color?.id ?? "none"}`, group: "Tab color", label: `Tab color: ${color?.name ?? "None"}`, keywords: "colour",
        icon: <span className={`palette__swatch ${color ? "" : "palette__swatch--none"}`} style={color ? { background: color.value } : undefined} />,
        run: () => activeTab && setTabColor(activeTab.id, color?.id),
      })),
      { id: "find", group: "Terminal", label: "Find", icon: <Search size={15} />, shortcut: keys.find, run: openSearch },
      { id: "copy", group: "Terminal", label: "Copy", icon: <Copy size={15} />, shortcut: "Ctrl+C", run: () => getPane(activePane?.id)?.copy() },
      { id: "paste", group: "Terminal", label: "Paste", icon: <ClipboardPaste size={15} />, shortcut: "Ctrl+V", run: () => getPane(activePane?.id)?.paste() },
      { id: "select-all", group: "Terminal", label: "Select all", icon: <TextSelect size={15} />, run: () => getPane(activePane?.id)?.selectAll() },
      { id: "clear", group: "Terminal", label: "Clear scrollback", icon: <Eraser size={15} />, run: () => getPane(activePane?.id)?.clear() },
      { id: "previous-command", group: "Terminal", label: "Previous command", icon: <ArrowUpToLine size={15} />, keywords: "jump scroll prompt", shortcut: keys.previousCommand, run: () => jumpToCommand(-1) },
      { id: "next-command", group: "Terminal", label: "Next command", icon: <ArrowDownToLine size={15} />, keywords: "jump scroll prompt", shortcut: keys.nextCommand, run: () => jumpToCommand(1) },
      { id: "copy-output", group: "Terminal", label: "Copy last command output", icon: <ClipboardCheck size={15} />, keywords: "result", run: copyLastOutput },
      { id: "zoom-in", group: "View", label: "Zoom in", icon: <ZoomIn size={15} />, shortcut: keys.zoomIn, run: () => adjustFontSize(1) },
      { id: "zoom-out", group: "View", label: "Zoom out", icon: <Minus size={15} />, shortcut: keys.zoomOut, run: () => adjustFontSize(-1) },
      { id: "zoom-reset", group: "View", label: "Reset zoom", shortcut: keys.zoomReset, run: () => setPreferences((current) => ({ ...current, fontSize: defaultPreferences.fontSize })) },
      { id: "compact-tabs", group: "View", label: preferences.tabDensity === "compact" ? "Use normal tabs" : "Use compact tabs", run: () => setPreferences((current) => ({ ...current, tabDensity: current.tabDensity === "compact" ? "comfortable" : "compact" })) },
      ...themes.map((candidate): PaletteCommand => ({
        id: `theme-${candidate.id}`, group: "Theme", label: `Theme: ${candidate.name}`, keywords: `color scheme ${candidate.scheme}`,
        icon: <span className="palette__swatch" style={{ background: candidate.background, boxShadow: `inset 0 0 0 4px ${candidate.background}, inset 0 0 0 9px ${candidate.accent}` }} />,
        run: () => setPreferences((current) => ({ ...current, themeId: candidate.id, accent: candidate.accent })),
      })),
      { id: "settings", group: "App", label: "Settings", icon: <Settings2 size={15} />, shortcut: keys.settings, run: () => openSettings() },
      { id: "appearance", group: "App", label: "Appearance settings", icon: <Palette size={15} />, keywords: "theme font", run: () => openSettings("appearance") },
      { id: "keyboard", group: "App", label: "Keyboard shortcuts", icon: <Keyboard size={15} />, keywords: "keybindings", run: () => openSettings("keyboard") },
      { id: "profiles", group: "App", label: "Profile settings", icon: <Monitor size={15} />, keywords: "shells wsl ssh custom default", run: () => openSettings("profiles") },
    ];
  }, [activePane?.id, activeTab, adjustFontSize, closeActivePane, closeTab, copyLastOutput, cycleTab, duplicateTab, jumpToCommand, keys, openNewTab, openSearch, openSettings, preferences.tabDensity, profiles, removeTabs, resolveProfile, setTabColor, splitActive]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest("[data-shortcut-recorder]")) return;
      const isTerminalInput = target?.classList.contains("xterm-helper-textarea");
      if (!isTerminalInput && target?.matches("input, textarea, select, [contenteditable='true']")) return;
      const action = (shortcut: string, run: () => void) => {
        if (!matchesShortcut(event, shortcut)) return false;
        event.preventDefault();
        event.stopPropagation();
        run();
        return true;
      };
      if (action(keys.commandPalette, () => setPaletteOpen((open) => !open))) return;
      if (action(keys.settings, () => (settingsOpen ? setSettingsOpen(false) : openSettings()))) return;
      if (overlayOpen || menu) return;
      if (action(keys.newTab, () => openNewTab())) return;
      if (action(keys.find, openSearch)) return;
      if (action(keys.splitVertical, () => splitActive("vertical"))) return;
      if (action(keys.splitHorizontal, () => splitActive("horizontal"))) return;
      if (action(keys.closePane, closeActivePane)) return;
      if (action(keys.nextTab, () => cycleTab(1))) return;
      if (action(keys.previousTab, () => cycleTab(-1))) return;
      if (action(keys.zoomIn, () => adjustFontSize(1))) return;
      if (action(keys.zoomOut, () => adjustFontSize(-1))) return;
      if (action(keys.zoomReset, () => setPreferences((current) => ({ ...current, fontSize: defaultPreferences.fontSize })))) return;
      if (activeTab && action(keys.closeTab, () => closeTab(activeTab.id))) return;
      // A shell that marks no commands keeps these keys: editors and TUIs use Ctrl+Up and Ctrl+Down.
      const jump = (shortcut: string, direction: -1 | 1) => {
        if (!matchesShortcut(event, shortcut) || !getPane(activeTab?.activePaneId)?.jumpToCommand(direction)) return false;
        event.preventDefault();
        event.stopPropagation();
        return true;
      };
      if (jump(keys.previousCommand, -1) || jump(keys.nextCommand, 1)) return;
      if (event.altKey && !event.ctrlKey && !event.shiftKey && focusNeighborPane(event.key)) {
        event.preventDefault();
        event.stopPropagation();
        return;
      }
      // On Windows AltGr arrives as Ctrl+Alt; never steal the character it types (~ # { [ | on AZERTY…).
      const typesCharacter = event.key.length === 1 && !/^[0-9]$/.test(event.key);
      if (event.ctrlKey && event.altKey && !event.shiftKey && !event.getModifierState("AltGraph") && !typesCharacter && /^Digit[1-9]$/.test(event.code)) {
        const visible = tabsRef.current.filter((tab) => !closingRef.current.has(tab.id));
        const digit = Number(event.code.slice(5));
        const target = digit === 9 ? visible.at(-1) : visible[digit - 1];
        if (target) {
          event.preventDefault();
          event.stopPropagation();
          setActiveTabId(target.id);
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [activeTab, adjustFontSize, closeActivePane, closeTab, cycleTab, focusNeighborPane, keys, menu, openNewTab, openSearch, openSettings, overlayOpen, settingsOpen, splitActive]);

  const menuEntries = (open: OpenMenu): MenuEntry[] => {
    if (open.kind === "profiles") {
      const defaultId = resolveProfile()?.id;
      const sections = profileMenuSections(profiles);
      const grouped = sections.some((section) => section.heading);
      return [
        ...sections.flatMap(({ heading, profiles: members }, index): MenuEntry[] => [
          ...(index > 0 ? ["separator" as const] : []),
          ...(heading ? [{ heading }] : []),
          ...members.map((profile): MenuEntry => ({
            id: profile.id,
            label: profile.name,
            detail: profile.available ? (grouped ? undefined : profileTag(profile)) : profileGroup(profile) === "custom" ? "Program not found" : "Not installed",
            disabled: !profile.available,
            icon: <ProfileIcon kind={profile.kind} accent={profile.accent} />,
            shortcut: profile.id === defaultId ? keys.newTab : undefined,
            run: () => openNewTab(profile.id),
          })),
        ]),
        "separator",
        { id: "palette", label: "Command palette", icon: <Search size={15} />, shortcut: keys.commandPalette, run: () => setPaletteOpen(true) },
        { id: "settings", label: "Settings", icon: <Settings2 size={15} />, shortcut: keys.settings, run: () => openSettings() },
      ];
    }
    if (open.kind === "tab") {
      const index = tabs.findIndex((tab) => tab.id === open.tabId);
      const color = tabs[index]?.color;
      return [
        { id: "rename", label: "Rename tab", icon: <Pencil size={15} />, run: () => setRenamingTabId(open.tabId) },
        { id: "duplicate", label: "Duplicate tab", icon: <CopyPlus size={15} />, run: () => duplicateTab(open.tabId) },
        { id: "split-right", label: "Split right", icon: <Columns2 size={15} />, disabled: open.tabId !== activeTabId, run: () => splitActive("vertical") },
        "separator",
        { heading: "Color" },
        {
          label: "Tab color",
          swatches: [undefined, ...tabColors].map((option) => ({
            id: `color-${option?.id ?? "none"}`,
            label: option?.name ?? "No color",
            color: option?.value,
            checked: color === option?.id,
            run: () => setTabColor(open.tabId, option?.id),
          })),
        },
        "separator",
        { id: "close-right", label: "Close tabs to the right", disabled: index === tabs.length - 1, run: () => removeTabs(tabs.slice(index + 1).map((tab) => tab.id)) },
        { id: "close-others", label: "Close other tabs", disabled: tabs.length < 2, run: () => removeTabs(tabs.filter((tab) => tab.id !== open.tabId).map((tab) => tab.id)) },
        { id: "close", label: "Close tab", icon: <X size={15} />, shortcut: open.tabId === activeTabId ? keys.closeTab : undefined, run: () => closeTab(open.tabId) },
      ];
    }
    const pane = getPane(activePane?.id);
    const full = (activeTab?.panes.length ?? 0) >= MAX_PANES;
    return [
      { id: "copy", label: "Copy", icon: <Copy size={15} />, shortcut: "Ctrl+C", disabled: !pane?.hasSelection(), run: () => pane?.copy() },
      { id: "paste", label: "Paste", icon: <ClipboardPaste size={15} />, shortcut: "Ctrl+V", run: () => pane?.paste() },
      { id: "select-all", label: "Select all", icon: <TextSelect size={15} />, run: () => pane?.selectAll() },
      { id: "copy-output", label: "Copy last output", icon: <ClipboardCheck size={15} />, disabled: !pane?.hasCommandOutput(), run: copyLastOutput },
      "separator",
      { id: "find", label: "Find…", icon: <Search size={15} />, shortcut: keys.find, run: openSearch },
      { id: "split-right", label: "Split right", icon: <Columns2 size={15} />, shortcut: keys.splitVertical, disabled: full, run: () => splitActive("vertical") },
      { id: "split-down", label: "Split down", icon: <Rows2 size={15} />, shortcut: keys.splitHorizontal, disabled: full, run: () => splitActive("horizontal") },
      "separator",
      { id: "clear", label: "Clear scrollback", icon: <Eraser size={15} />, run: () => pane?.clear() },
      { id: "close-pane", label: (activeTab?.panes.length ?? 0) > 1 ? "Close pane" : "Close tab", icon: <SquareX size={15} />, shortcut: (activeTab?.panes.length ?? 0) > 1 ? keys.closePane : keys.closeTab, run: closeActivePane },
    ];
  };

  const requestSearch = (backwards: boolean) => setSearchSignal((current) => ({ nonce: current.nonce + 1, backwards }));
  // A rename started from a menu or the palette keeps the focus in its field.
  const closeOverlayFocus = () => requestAnimationFrame(() => {
    if (renamingRef.current) return;
    // The closing overlay may have opened another one (Find from a menu) that took focus.
    const focused = document.activeElement;
    if (focused && focused !== document.body && !focused.closest(".xterm")) return;
    getPane(activePane?.id)?.focus();
  });
  const showImage = preferences.backgroundMode === "image" && preferences.backgroundImage;
  const noShells = ready && nativeHost && !profiles.some((profile) => profile.available);

  return (
    <main className={`app ${ready ? "is-ready" : ""}`}>
      {showImage && <div className="app__image" style={{ backgroundImage: `url("${preferences.backgroundImage}")`, opacity: preferences.backgroundImageOpacity }} />}
      <Titlebar
        tabs={tabs}
        activeTabId={activeTabId}
        activity={activity}
        finished={finished}
        closing={closing}
        renamingId={renaming ? renamingTabId : null}
        newTabShortcut={keys.newTab}
        paletteShortcut={keys.commandPalette}
        settingsShortcut={keys.settings}
        onSelectTab={setActiveTabId}
        onCloseTab={closeTab}
        onMoveTab={moveTab}
        onNewTab={() => openNewTab()}
        onOpenProfileMenu={(x, y) => setMenu({ kind: "profiles", x, y })}
        onTabContextMenu={(tabId, x, y) => setMenu({ kind: "tab", tabId, x, y })}
        onRenameStart={setRenamingTabId}
        onRename={renameTab}
        onRenameEnd={() => setRenamingTabId(null)}
        onOpenPalette={() => setPaletteOpen(true)}
        onOpenSettings={() => openSettings()}
      />

      <div className="workspace">
        {tabs.map((tab) => {
          const visible = tab.id === activeTabId;
          const { panes: rects, dividers } = computeLayout(tab.layout);
          return (
            <div key={tab.id} className={`workspace__tab ${visible ? "is-visible" : ""}`} aria-hidden={!visible}>
              {/* Panes stay in one flat, keyed list whatever the tree looks like, so React never
                  remounts a terminal (and restarts its shell) when the layout changes shape. */}
              <div className={`panes ${tab.panes.length > 1 ? "is-split" : ""}`}>
                {tab.panes.map((pane) => {
                  const focused = visible && pane.id === tab.activePaneId && !overlayOpen && !searchOpen && !menu && !renaming;
                  return (
                    <div key={pane.id} className={`pane ${pane.id === tab.activePaneId ? "is-active" : ""}`} data-pane-id={pane.id} style={paneStyle(rects.get(pane.id))}>
                      <TerminalPane
                        paneId={pane.id}
                        profile={pane.profile}
                        startIn={pane.cwd}
                        preferences={preferences}
                        focused={focused}
                        visible={visible}
                        searchRequest={visible && pane.id === tab.activePaneId && searchOpen ? { query: searchQuery, nonce: searchSignal.nonce, backwards: searchSignal.backwards } : undefined}
                        onFocus={() => setActivePane(tab.id, pane.id)}
                        onFontSizeDelta={adjustFontSize}
                        onTitleChange={(title) => setPaneTitle(tab.id, pane, title)}
                        onActivity={() => markActivity(tab.id)}
                        onCommandFinished={(command) => commandFinished(tab.id, command)}
                        onNotify={(title, body) => programNotification(tab.id, title, body)}
                        onFolderChange={(cwd) => setPaneFolder(tab.id, pane.id, cwd)}
                        onSearchResult={setSearchResult}
                        onContextMenu={(x, y) => { setActivePane(tab.id, pane.id); setMenu({ kind: "terminal", x, y }); }}
                        onClose={() => closePane(tab.id, pane.id)}
                      />
                    </div>
                  );
                })}
                {/* Outer dividers come last so they win where hit areas meet at a T junction. */}
                {dividers.sort((a, b) => b.path.length - a.path.length).map((divider) => {
                  const key = `${divider.path.join(".")}:${divider.index}`;
                  return (
                    <div
                      key={key}
                      className={`pane-divider pane-divider--${divider.direction} ${visible && resizingDivider === key ? "is-dragging" : ""}`}
                      style={dividerStyle(divider)}
                      role="separator"
                      aria-orientation={divider.direction === "vertical" ? "vertical" : "horizontal"}
                      aria-label="Resize panes"
                      onPointerDown={(event) => beginPaneResize(event, tab.id, divider)}
                    />
                  );
                })}
              </div>
            </div>
          );
        })}

        {noShells && (
          <div className="empty-state">
            <h2>No shell found</h2>
            <p>Nebula Terminal looks for PowerShell, Command Prompt, Git Bash and WSL. None of them could be found on this PC.</p>
          </div>
        )}

        <SearchBar
          open={searchOpen}
          query={searchQuery}
          result={searchResult}
          onQueryChange={(query) => { setSearchQuery(query); if (!query) setSearchResult(null); setSearchSignal((current) => ({ nonce: current.nonce + 1, backwards: false })); }}
          onNext={() => requestSearch(false)}
          onPrevious={() => requestSearch(true)}
          onClose={() => { setSearchOpen(false); setSearchResult(null); closeOverlayFocus(); }}
        />
      </div>

      {menu && <Menu {...menu} label={menu.kind === "profiles" ? "Open a profile" : menu.kind === "tab" ? "Tab" : "Terminal"} width={menu.kind === "terminal" ? 250 : menu.kind === "tab" ? 260 : 280} entries={menuEntries(menu)} onClose={() => { setMenu(null); closeOverlayFocus(); }} />}

      <Suspense fallback={null}>
        <SettingsPanel
          open={settingsOpen}
          initialPage={settingsPage}
          profiles={profiles}
          preferences={preferences}
          onChange={setPreferences}
          onClose={() => { setSettingsOpen(false); closeOverlayFocus(); }}
          onReset={() => { setPreferences({ ...defaultPreferences, keybindings: { ...defaultKeybindings } }); setNotice("Settings reset"); }}
          onClearSession={clearSession}
          onProfilesChanged={refreshProfiles}
        />
        <CommandPalette open={paletteOpen} commands={commands} onClose={() => { setPaletteOpen(false); closeOverlayFocus(); }} />
      </Suspense>

      {confirmation && (
        <div className="overlay overlay--dialog" role="presentation" onMouseDown={() => setConfirmation(null)}>
          <div className="dialog" role="alertdialog" aria-modal="true" aria-labelledby="confirm-title" onMouseDown={(event) => event.stopPropagation()} onKeyDown={(event) => event.key === "Escape" && setConfirmation(null)}>
            <h2 id="confirm-title">{confirmation.title}</h2>
            <p>{confirmation.body}</p>
            <div className="dialog__actions">
              <button className="button" type="button" onClick={() => setConfirmation(null)}>Cancel</button>
              <button className="button button--primary" type="button" autoFocus onClick={() => { const { run } = confirmation; setConfirmation(null); run(); }}>{confirmation.action}</button>
            </div>
          </div>
        </div>
      )}

      {notice && <div className="toast" role="status">{notice}</div>}
      {update && <UpdateBanner update={update} onDismiss={() => { dismissUpdate(update.version); setUpdate(null); }} />}
    </main>
  );
}

import { Fragment, Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ArrowLeftRight, ClipboardPaste, Columns2, Copy, CopyPlus, Eraser, Keyboard, Minus, Palette, PanelTopClose, Plus, Rows2, Search, Settings2, SquareX, TextSelect, X, ZoomIn } from "lucide-react";
import Titlebar from "./components/Titlebar";
import TerminalPane from "./components/TerminalPane";
import SearchBar from "./components/SearchBar";
import Menu, { type MenuEntry } from "./components/Menu";
import ProfileIcon from "./components/ProfileIcon";
import { defaultKeybindings, defaultPreferences, loadPreferences, savePreferences } from "./preferences";
import { clearSession, loadSession, saveSession } from "./session";
import { pickProfile, previewProfiles } from "./profiles";
import { resolveTheme, themes } from "./themes";
import { matchesShortcut } from "./keys";
import { getPane } from "./paneRegistry";
import type { PaletteCommand } from "./components/CommandPalette";
import type { SettingsPage } from "./components/SettingsPanel";
import type { AppearancePreferences, SplitDirection, TerminalPaneModel, TerminalProfile, TerminalTab } from "./types";

const SettingsPanel = lazy(() => import("./components/SettingsPanel"));
const CommandPalette = lazy(() => import("./components/CommandPalette"));

const MAX_PANES = 4;
const TAB_CLOSE_MS = 150;

type OpenMenu =
  | { kind: "profiles"; x: number; y: number }
  | { kind: "tab"; tabId: string; x: number; y: number }
  | { kind: "terminal"; x: number; y: number };

type Confirmation = { title: string; body: string; action: string; run: () => void };

function makePane(profile: TerminalProfile): TerminalPaneModel {
  return { id: crypto.randomUUID(), profile };
}

function makeTab(profile: TerminalProfile): TerminalTab {
  const pane = makePane(profile);
  return { id: crypto.randomUUID(), title: profile.name, panes: [pane], activePaneId: pane.id, splitDirection: "vertical", paneSizes: [1] };
}

function remapTabs(tabs: TerminalTab[], profiles: TerminalProfile[]): TerminalTab[] {
  const available = new Map(profiles.filter((profile) => profile.available).map((profile) => [profile.id, profile]));
  const fallback = pickProfile(profiles);
  if (!fallback) return tabs;
  return tabs.map((tab) => ({
    ...tab,
    paneSizes: tab.paneSizes.length === tab.panes.length ? tab.paneSizes : tab.panes.map(() => 1),
    panes: tab.panes.map((pane) => ({ ...pane, profile: available.get(pane.profile.id) ?? fallback })),
  }));
}

/** Shells often report their executable path as the title; show the profile name instead. */
function cleanTitle(title: string, profile: TerminalProfile): string {
  const trimmed = title.replace(/^Administrator:\s*/i, "").trim();
  if (!trimmed || /^[a-z]:\\.*\.exe$/i.test(trimmed) || /\\(pwsh|powershell|cmd|bash|wsl)\.exe$/i.test(trimmed)) return profile.name;
  return trimmed;
}

/** Black or white, whichever reads better on the given hex color. */
function readableOn(hex: string): string {
  const value = Number.parseInt(hex.slice(1), 16);
  const [r, g, b] = [(value >> 16) & 255, (value >> 8) & 255, value & 255].map((channel) => {
    const c = channel / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  }) as [number, number, number];
  const luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  return luminance > 0.36 ? "#141414" : "#ffffff";
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
  const [closing, setClosing] = useState<ReadonlySet<string>>(() => new Set());
  const [notice, setNotice] = useState<string | null>(null);
  const [ready, setReady] = useState(false);
  const hydratedRef = useRef(false);
  const closingWindowRef = useRef(false);
  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;
  const activeTabIdRef = useRef(activeTabId);
  activeTabIdRef.current = activeTabId;

  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0];
  const activePane = activeTab?.panes.find((pane) => pane.id === activeTab.activePaneId) ?? activeTab?.panes[0];
  const theme = resolveTheme(preferences.themeId);
  const overlayOpen = settingsOpen || paletteOpen || confirmation !== null;

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
    setTabs((current) => remapTabs(current, detected));
  }, []);

  useEffect(() => {
    if (!nativeHost) {
      hydrateWorkspace(previewProfiles);
      return;
    }
    void invoke<TerminalProfile[]>("detect_profiles")
      .then(hydrateWorkspace)
      .catch(() => hydrateWorkspace([]));
  }, [hydrateWorkspace, nativeHost]);

  useEffect(() => {
    savePreferences(preferences);
    const root = document.documentElement;
    root.style.setProperty("--accent", preferences.accent);
    root.style.setProperty("--term-bg", theme.background);
    root.style.setProperty("--term-fg", theme.foreground);
    root.dataset.scheme = theme.scheme;
    root.dataset.animation = preferences.animationLevel;
    root.dataset.density = preferences.tabDensity;
    root.dataset.background = preferences.backgroundMode;
    root.dataset.host = nativeHost ? "native" : "browser";
    root.style.setProperty("--on-accent", readableOn(preferences.accent));
    root.style.setProperty("--term-surface", preferences.terminalOpacity >= 1 ? theme.background : `color-mix(in srgb, ${theme.background} ${Math.round(preferences.terminalOpacity * 100)}%, transparent)`);
    document.querySelector('meta[name="theme-color"]')?.setAttribute("content", theme.background);
    if (nativeHost) void invoke("set_window_effect", { mode: preferences.backgroundMode, dark: theme.scheme === "dark" }).catch(() => undefined);
  }, [nativeHost, preferences, theme]);

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
  }, [activeTabId]);

  const closeWindow = useCallback(() => {
    if (!nativeHost) return;
    closingWindowRef.current = true;
    void getCurrentWindow().close();
  }, [nativeHost]);

  useEffect(() => {
    if (!nativeHost) return;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow().onCloseRequested((event) => {
      const count = tabsRef.current.length;
      if (closingWindowRef.current || !preferencesRef.current.confirmCloseMultipleTabs || count <= 1) return;
      event.preventDefault();
      setConfirmation({ title: `Close ${count} tabs?`, body: "Every shell running in this window will be stopped.", action: "Close all", run: closeWindow });
    }).then((dispose) => { unlisten = dispose; });
    return () => unlisten?.();
  }, [closeWindow, nativeHost]);

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

  const resolveProfile = useCallback((profileId?: string) => pickProfile(profiles, profileId ?? preferences.defaultProfileId), [preferences.defaultProfileId, profiles]);

  const openNewTab = useCallback((profileId?: string) => {
    const profile = resolveProfile(profileId);
    if (!profile) return;
    const next = makeTab(profile);
    setTabs((current) => [...current, next]);
    setActiveTabId(next.id);
  }, [resolveProfile]);

  const removeTabs = useCallback((ids: string[]) => {
    const remove = new Set(ids);
    const current = tabsRef.current;
    const remaining = current.filter((tab) => !remove.has(tab.id));
    if (remaining.length === 0) {
      if (nativeHost) {
        closeWindow();
        return;
      }
      const profile = resolveProfile();
      if (profile) remaining.push(makeTab(profile));
    }
    if (remove.has(activeTabIdRef.current)) {
      const index = current.findIndex((tab) => tab.id === activeTabIdRef.current);
      const neighbor = current.slice(index + 1).find((tab) => !remove.has(tab.id))
        ?? current.slice(0, index).reverse().find((tab) => !remove.has(tab.id))
        ?? remaining[0];
      if (neighbor) setActiveTabId(neighbor.id);
    }

    const finish = () => {
      setTabs((latest) => {
        const kept = latest.filter((tab) => !remove.has(tab.id));
        return kept.length ? kept : remaining;
      });
      setClosing((latest) => {
        const next = new Set(latest);
        ids.forEach((id) => next.delete(id));
        return next;
      });
    };
    if (preferencesRef.current.animationLevel === "off" || remaining.some((tab) => !current.includes(tab))) {
      finish();
      return;
    }
    setClosing((latest) => new Set([...latest, ...ids]));
    window.setTimeout(finish, TAB_CLOSE_MS);
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
    const open = tabsRef.current.filter((tab) => !closing.has(tab.id));
    if (open.length < 2) return;
    const index = open.findIndex((tab) => tab.id === activeTabIdRef.current);
    const next = open[(index + delta + open.length) % open.length];
    if (next) setActiveTabId(next.id);
  }, [closing]);

  const setActivePane = useCallback((tabId: string, paneId: string) => {
    setTabs((current) => current.map((tab) => tab.id === tabId && tab.activePaneId !== paneId ? { ...tab, activePaneId: paneId } : tab));
    setActiveTabId(tabId);
  }, []);

  const setPaneTitle = useCallback((tabId: string, pane: TerminalPaneModel, title: string) => {
    const clean = cleanTitle(title, pane.profile);
    setTabs((current) => current.map((tab) => tab.id === tabId && tab.activePaneId === pane.id && tab.title !== clean ? { ...tab, title: clean } : tab));
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
    const profile = resolveProfile(profileId ?? activePane?.profile.id);
    if (!profile) return;
    const pane = makePane(profile);
    setTabs((current) => current.map((tab) => {
      if (tab.id !== activeTab.id) return tab;
      const sizes = tab.paneSizes.length === tab.panes.length ? tab.paneSizes : tab.panes.map(() => 1);
      // Panes in a tab share one direction; the first split decides it.
      const splitDirection = tab.panes.length > 1 ? tab.splitDirection : direction;
      return { ...tab, panes: [...tab.panes, pane], paneSizes: [...sizes, 1], activePaneId: pane.id, splitDirection };
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
      if (index < 0) return tab;
      const panes = tab.panes.filter((pane) => pane.id !== paneId);
      const sizes = (tab.paneSizes.length === tab.panes.length ? tab.paneSizes : tab.panes.map(() => 1)).filter((_, paneIndex) => paneIndex !== index);
      const activePaneId = tab.activePaneId === paneId ? (panes[Math.min(index, panes.length - 1)] ?? panes[0]!).id : tab.activePaneId;
      return { ...tab, panes, paneSizes: sizes, activePaneId };
    }));
  }, [closeTab]);

  const closeActivePane = useCallback(() => {
    if (activeTab) closePane(activeTab.id, activeTab.activePaneId);
  }, [activeTab, closePane]);

  const focusNeighborPane = useCallback((key: string) => {
    if (!activeTab || activeTab.panes.length < 2) return false;
    const forward = activeTab.splitDirection === "vertical" ? "ArrowRight" : "ArrowDown";
    const backward = activeTab.splitDirection === "vertical" ? "ArrowLeft" : "ArrowUp";
    if (key !== forward && key !== backward) return false;
    const index = activeTab.panes.findIndex((pane) => pane.id === activeTab.activePaneId);
    const next = activeTab.panes[index + (key === forward ? 1 : -1)];
    if (next) setActivePane(activeTab.id, next.id);
    return true;
  }, [activeTab, setActivePane]);

  const duplicateTab = useCallback((tabId: string) => {
    const tab = tabsRef.current.find((candidate) => candidate.id === tabId);
    const pane = tab?.panes.find((candidate) => candidate.id === tab.activePaneId) ?? tab?.panes[0];
    if (pane) openNewTab(pane.profile.id);
  }, [openNewTab]);

  const beginPaneResize = (event: ReactPointerEvent<HTMLDivElement>, tabId: string, index: number, direction: SplitDirection) => {
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
    document.documentElement.classList.add("is-resizing", `is-resizing--${direction}`);

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
      document.documentElement.classList.remove("is-resizing", `is-resizing--${direction}`);
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", finish);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish, { once: true });
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

  const keys = preferences.keybindings;
  const commands = useMemo<PaletteCommand[]>(() => {
    const defaultProfile = resolveProfile();
    return [
      { id: "new-tab", group: "Tabs", label: "New tab", detail: defaultProfile?.name, icon: <Plus size={15} />, shortcut: keys.newTab, run: () => openNewTab() },
      ...profiles.filter((profile) => profile.available).map((profile): PaletteCommand => ({
        id: `new-${profile.id}`, group: "Tabs", label: `New ${profile.name} tab`, icon: <ProfileIcon kind={profile.kind} size={15} />, keywords: profile.kind, run: () => openNewTab(profile.id),
      })),
      { id: "duplicate-tab", group: "Tabs", label: "Duplicate tab", icon: <CopyPlus size={15} />, run: () => activeTab && duplicateTab(activeTab.id) },
      { id: "next-tab", group: "Tabs", label: "Next tab", icon: <ArrowLeftRight size={15} />, shortcut: keys.nextTab, run: () => cycleTab(1) },
      { id: "previous-tab", group: "Tabs", label: "Previous tab", icon: <ArrowLeftRight size={15} />, shortcut: keys.previousTab, run: () => cycleTab(-1) },
      { id: "close-tab", group: "Tabs", label: "Close tab", icon: <X size={15} />, shortcut: keys.closeTab, run: () => activeTab && closeTab(activeTab.id) },
      { id: "close-other-tabs", group: "Tabs", label: "Close other tabs", icon: <PanelTopClose size={15} />, run: () => activeTab && removeTabs(tabsRef.current.filter((tab) => tab.id !== activeTab.id).map((tab) => tab.id)) },
      { id: "split-right", group: "Panes", label: "Split right", icon: <Columns2 size={15} />, shortcut: keys.splitVertical, run: () => splitActive("vertical") },
      { id: "split-down", group: "Panes", label: "Split down", icon: <Rows2 size={15} />, shortcut: keys.splitHorizontal, run: () => splitActive("horizontal") },
      { id: "close-pane", group: "Panes", label: "Close pane", icon: <SquareX size={15} />, shortcut: keys.closePane, run: closeActivePane },
      { id: "find", group: "Terminal", label: "Find", icon: <Search size={15} />, shortcut: keys.find, run: openSearch },
      { id: "copy", group: "Terminal", label: "Copy", icon: <Copy size={15} />, shortcut: "Ctrl+C", run: () => getPane(activePane?.id)?.copy() },
      { id: "paste", group: "Terminal", label: "Paste", icon: <ClipboardPaste size={15} />, shortcut: "Ctrl+V", run: () => getPane(activePane?.id)?.paste() },
      { id: "select-all", group: "Terminal", label: "Select all", icon: <TextSelect size={15} />, run: () => getPane(activePane?.id)?.selectAll() },
      { id: "clear", group: "Terminal", label: "Clear scrollback", icon: <Eraser size={15} />, run: () => getPane(activePane?.id)?.clear() },
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
    ];
  }, [activePane?.id, activeTab, adjustFontSize, closeActivePane, closeTab, cycleTab, duplicateTab, keys, openNewTab, openSearch, openSettings, preferences.tabDensity, profiles, removeTabs, resolveProfile, splitActive]);

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
      if (event.altKey && !event.ctrlKey && !event.shiftKey && focusNeighborPane(event.key)) {
        event.preventDefault();
        event.stopPropagation();
        return;
      }
      if (event.ctrlKey && event.altKey && !event.shiftKey && /^Digit[1-9]$/.test(event.code)) {
        const visible = tabsRef.current.filter((tab) => !closing.has(tab.id));
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
  }, [activeTab, adjustFontSize, closeActivePane, closeTab, closing, cycleTab, focusNeighborPane, keys, menu, openNewTab, openSearch, openSettings, overlayOpen, settingsOpen, splitActive]);

  const menuEntries = (open: OpenMenu): MenuEntry[] => {
    if (open.kind === "profiles") {
      const defaultId = resolveProfile()?.id;
      return [
        ...profiles.map((profile): MenuEntry => ({
          id: profile.id,
          label: profile.name,
          detail: profile.available ? undefined : "Not installed",
          disabled: !profile.available,
          icon: <ProfileIcon kind={profile.kind} />,
          shortcut: profile.id === defaultId ? keys.newTab : undefined,
          run: () => openNewTab(profile.id),
        })),
        "separator",
        { id: "palette", label: "Command palette", icon: <Search size={15} />, shortcut: keys.commandPalette, run: () => setPaletteOpen(true) },
        { id: "settings", label: "Settings", icon: <Settings2 size={15} />, shortcut: keys.settings, run: () => openSettings() },
      ];
    }
    if (open.kind === "tab") {
      const index = tabs.findIndex((tab) => tab.id === open.tabId);
      return [
        { id: "duplicate", label: "Duplicate tab", icon: <CopyPlus size={15} />, run: () => duplicateTab(open.tabId) },
        { id: "split-right", label: "Split right", icon: <Columns2 size={15} />, disabled: open.tabId !== activeTabId, run: () => splitActive("vertical") },
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
  const closeOverlayFocus = () => requestAnimationFrame(() => getPane(activePane?.id)?.focus());
  const showImage = preferences.backgroundMode === "image" && preferences.backgroundImage;
  const noShells = ready && nativeHost && !profiles.some((profile) => profile.available);

  return (
    <main className={`app ${ready ? "is-ready" : ""}`}>
      {showImage && <div className="app__image" style={{ backgroundImage: `url("${preferences.backgroundImage}")`, opacity: preferences.backgroundImageOpacity }} />}
      <Titlebar
        tabs={tabs}
        activeTabId={activeTabId}
        activity={activity}
        closing={closing}
        newTabShortcut={keys.newTab}
        paletteShortcut={keys.commandPalette}
        settingsShortcut={keys.settings}
        onSelectTab={setActiveTabId}
        onCloseTab={closeTab}
        onMoveTab={moveTab}
        onNewTab={() => openNewTab()}
        onOpenProfileMenu={(x, y) => setMenu({ kind: "profiles", x, y })}
        onTabContextMenu={(tabId, x, y) => setMenu({ kind: "tab", tabId, x, y })}
        onOpenPalette={() => setPaletteOpen(true)}
        onOpenSettings={() => openSettings()}
      />

      <div className="workspace">
        {tabs.map((tab) => {
          const visible = tab.id === activeTabId;
          return (
            <div key={tab.id} className={`workspace__tab ${visible ? "is-visible" : ""}`} aria-hidden={!visible}>
              <div className={`panes panes--${tab.splitDirection} ${tab.panes.length > 1 ? "is-split" : ""}`}>
                {tab.panes.map((pane, index) => {
                  const focused = visible && pane.id === tab.activePaneId && !overlayOpen && !searchOpen && !menu;
                  return (
                    <Fragment key={pane.id}>
                      <div className={`pane ${pane.id === tab.activePaneId ? "is-active" : ""}`} style={{ flexGrow: tab.paneSizes[index] ?? 1, flexBasis: 0 }}>
                        <TerminalPane
                          paneId={pane.id}
                          profile={pane.profile}
                          preferences={preferences}
                          focused={focused}
                          visible={visible}
                          searchRequest={visible && pane.id === tab.activePaneId && searchOpen ? { query: searchQuery, nonce: searchSignal.nonce, backwards: searchSignal.backwards } : undefined}
                          onFocus={() => setActivePane(tab.id, pane.id)}
                          onFontSizeDelta={adjustFontSize}
                          onTitleChange={(title) => setPaneTitle(tab.id, pane, title)}
                          onActivity={() => markActivity(tab.id)}
                          onSearchResult={setSearchResult}
                          onContextMenu={(x, y) => { setActivePane(tab.id, pane.id); setMenu({ kind: "terminal", x, y }); }}
                          onClose={() => closePane(tab.id, pane.id)}
                        />
                      </div>
                      {index < tab.panes.length - 1 && (
                        <div
                          className="pane-divider"
                          role="separator"
                          aria-orientation={tab.splitDirection === "vertical" ? "vertical" : "horizontal"}
                          aria-label="Resize panes"
                          onPointerDown={(event) => beginPaneResize(event, tab.id, index, tab.splitDirection)}
                        />
                      )}
                    </Fragment>
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

      {menu && <Menu {...menu} label={menu.kind === "profiles" ? "Open a shell" : menu.kind === "tab" ? "Tab" : "Terminal"} width={menu.kind === "profiles" ? 280 : 250} entries={menuEntries(menu)} onClose={() => { setMenu(null); closeOverlayFocus(); }} />}

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
    </main>
  );
}

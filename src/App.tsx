import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, ProgressBarStatus } from "@tauri-apps/api/window";
import { AppWindow, ArrowDownToLine, FileDown, FolderOpen, Maximize2, SquareTerminal as TerminalSquare, ArrowLeftRight, ArrowUpToLine, ClipboardCheck, ClipboardPaste, Columns2, Copy, CopyPlus, Eraser, Keyboard, Minus, Monitor, Palette, PanelTopClose, Pencil, Plus, LayoutPanelTop, Radio, Rows2, Save, Search, Settings2, ShieldCheck, SquareX, TextSelect, X, ZoomIn } from "lucide-react";
import Titlebar, { tabTitle } from "./components/Titlebar";
import TerminalPane, { type FinishedCommand } from "./components/TerminalPane";
import { finishedMessage, notify } from "./notify";
import UpdateBanner from "./components/UpdateBanner";
import { automaticCheckDue, checkForUpdate, dismiss as dismissUpdate, isDismissed, markChecked, type UpdateInfo } from "./updates";
import SearchBar from "./components/SearchBar";
import Menu, { type MenuEntry } from "./components/Menu";
import ProfileIcon from "./components/ProfileIcon";
import { defaultKeybindings, defaultPreferences, isPreferencesKey, loadPreferences, sanitizePreferences, savePreferences } from "./preferences";
import { administratorLaunch, isAdministrator, openAsAdministrator } from "./administrator";
import { clearSession, forgetWindowSession, loadSession, restoreSnapshot, saveSession, saveWindowSession, takeWindowSessions } from "./session";
import { describe as describeWorkspace, loadWorkspaces, openWorkspace, saveWorkspaces, suggestName, withWorkspace, type Workspace } from "./workspaces";
import { pickProfile, previewProfiles, profileCommandLabel, profileGroup, profileGroupLabels, profileMenuSections, profileTag, refreshTabProfiles } from "./profiles";
import { detectProfiles } from "./customProfiles";
import * as splash from "./splash";
import { isQuickTerminal, isTabsWindow, transferId } from "./windowKind";
import { lookFor, windowLook, withLook } from "./looks";
import { windowProgress, type Progress } from "./progress";
import { blanksIn, commandInput, fillBlanks, lastBlankValues, rememberBlankValues, savedCommandLabel } from "./savedCommands";
import { folderParts, loadRecentFolders, rememberFolder, saveRecentFolders } from "./recentFolders";
import { applySynced, digestOf, loadSyncState, readSyncDocument, saveSyncState, syncDocument } from "./sync";
import { readableOn } from "./color";
import { resolveTheme, tabColors, themes } from "./themes";
import { MAX_PANES, computeLayout, findNeighbor, minimumExtent, nodeAt, paneIds, paneLeaf, removePane, resizePair, setSplitSizes, splitPane, type Divider, type FocusDirection, type Rect } from "./layout";
import { matchesShortcut } from "./keys";
import { getPane } from "./paneRegistry";
import type { PaletteCommand } from "./components/CommandPalette";
import type { SettingsPage } from "./components/SettingsPanel";
import type { AppearancePreferences, DefaultTerminal, HandoffInfo, Launchers, SavedCommand, SplitDirection, TabColor, TerminalPaneModel, TerminalProfile, TerminalTab } from "./types";

const SettingsPanel = lazy(() => import("./components/SettingsPanel"));
const CommandPalette = lazy(() => import("./components/CommandPalette"));
const FindEverywhere = lazy(() => import("./components/FindEverywhere"));

/** How this copy of the app was started. */
interface Launch {
  folder?: string;
  /** The profile of the first tab, when it isn't the default one. */
  profileId?: string;
  forHandoff: boolean;
  handoffs: HandoffInfo[];
  /** Tabs to open instead of the saved session: a window's own, or those moved into it. */
  restored?: { tabs: TerminalTab[]; activeTabId: string } | null;
}

/** A tab on its way to another window (windows.rs keeps it until that window asks). */
interface TabTransfer {
  kind: "tab";
  tab: Pick<TerminalTab, "id" | "title" | "customTitle" | "color" | "layout" | "activePaneId" | "panes">;
}

/** The tabs another window had when the app closed. */
interface WindowTransfer {
  kind: "window";
  snapshot: unknown;
}

/** A launch that found only the notification area icon, with the folder it asked for. */
interface LaunchTransfer {
  kind: "launch";
  folder?: string | null;
}

/** Windows that keep tabs in the session: the main one and the windows opened for tabs. */
const savesSession = !isAdministrator && !isQuickTerminal;
const windowLabel = isTauri() ? getCurrentWindow().label : "main";

function tabFromTransfer(transfer: TabTransfer["tab"]): TerminalTab {
  return { ...transfer, broadcast: false };
}

const TAB_CLOSE_MS = 150;
/** Smallest pane a split or a divider drag may produce, in pixels. */
const MIN_PANE_WIDTH = 120;
const MIN_PANE_HEIGHT = 72;

const arrowDirections: Record<string, FocusDirection> = { ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down" };

type OpenMenu =
  | { kind: "profiles"; x: number; y: number }
  | { kind: "tab"; tabId: string; x: number; y: number }
  | { kind: "terminal"; x: number; y: number };

/** A question before an action; `input` asks for a value (a name) first. */
type Confirmation = { title: string; body: string; action: string; inputs?: { label: string; value: string; maxLength?: number }[]; run: (values: string[]) => void };

function makePane(profile: TerminalProfile, cwd?: string): TerminalPaneModel {
  return { id: crypto.randomUUID(), profile, cwd };
}

function makeTab(profile: TerminalProfile, cwd?: string): TerminalTab {
  const pane = makePane(profile, cwd);
  return { id: crypto.randomUUID(), title: profile.name, panes: [pane], layout: paneLeaf(pane.id), activePaneId: pane.id };
}

/** A tab for a console Windows handed over; its title is the console's own. */
function makeHandoffTab(profile: TerminalProfile, handoff: HandoffInfo): TerminalTab {
  const tab = makeTab(profile);
  return { ...tab, title: handoff.title || "Console", panes: tab.panes.map((pane) => ({ ...pane, handoffId: handoff.id })) };
}

function paneElement(paneId: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`.pane[data-pane-id="${paneId}"]`);
}

/** One pixel before a pane that doesn't start at the tab's edge belongs to the divider there. */
const edge = (start: number) => (start > 1e-6 ? 1 : 0);

const WHOLE_TAB: Rect = { x: 0, y: 0, width: 1, height: 1 };

/** Whether a media query matches, following its changes (dark mode, contrast themes). */
function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() => typeof matchMedia === "function" && matchMedia(query).matches);
  useEffect(() => {
    const media = matchMedia(query);
    const update = () => setMatches(media.matches);
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, [query]);
  return matches;
}

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
  // An admin window starts with the settings of the window that opened it.
  const [preferences, setPreferences] = useState<AppearancePreferences>(() => {
    const launch = administratorLaunch();
    return launch?.preferences ? sanitizePreferences(launch.preferences) : loadPreferences();
  });
  const preferencesRef = useRef(preferences);
  preferencesRef.current = preferences;
  const [tabs, setTabs] = useState<TerminalTab[]>([]);
  const [activeTabId, setActiveTabId] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsPage, setSettingsPage] = useState<SettingsPage | undefined>();
  const [paletteOpen, setPaletteOpen] = useState(false);
  // The palette also switches tabs: the same list, with the open tabs only.
  const [switchingTabs, setSwitchingTabs] = useState(false);
  const [findEverywhereOpen, setFindEverywhereOpen] = useState(false);
  const [menu, setMenu] = useState<OpenMenu | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchSignal, setSearchSignal] = useState({ nonce: 0, backwards: false });
  const [searchResult, setSearchResult] = useState<{ index: number; count: number } | null>(null);
  const searchOptions = useMemo(() => ({
    caseSensitive: preferences.searchCaseSensitive,
    wholeWord: preferences.searchWholeWord,
    regex: preferences.searchRegex,
  }), [preferences.searchCaseSensitive, preferences.searchRegex, preferences.searchWholeWord]);
  const [activity, setActivity] = useState<ReadonlySet<string>>(() => new Set());
  const [finished, setFinished] = useState<ReadonlyMap<string, boolean>>(() => new Map());
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [closing, setClosing] = useState<ReadonlySet<string>>(() => new Set());
  const [notice, setNotice] = useState<string | null>(null);
  // OSC 9;4 progress by tab, the latest report last.
  const [progress, setProgress] = useState<ReadonlyMap<string, Progress>>(() => new Map());
  const [renamingTabId, setRenamingTabId] = useState<string | null>(null);
  const [resizingDivider, setResizingDivider] = useState<string | null>(null);
  const [ready, setReady] = useState(false);
  // Restored tabs that aren't in front start their shells once the one in front is up.
  const [deferredTabs, setDeferredTabs] = useState<ReadonlySet<string>>(() => new Set());
  const deferredTabsRef = useRef(deferredTabs);
  deferredTabsRef.current = deferredTabs;
  const hydratedRef = useRef(false);
  const removeTabsRef = useRef<(ids: string[]) => void>(() => undefined);
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
  // What the window draws with: an admin window's red accent, each profile's own look,
  // and the chrome in the colors of the pane in front.
  const forcedColors = useMediaQuery("(forced-colors: active)");
  const prefersDark = useMediaQuery("(prefers-color-scheme: dark)");
  const drawn = useMemo(() => windowLook(preferences, isAdministrator, forcedColors ? (prefersDark ? "dark" : "light") : undefined), [forcedColors, prefersDark, preferences]);
  const lookOf = useMemo(() => {
    const cache = new Map<string, AppearancePreferences>();
    return (profileId: string | undefined) => {
      if (!profileId) return drawn;
      const look = cache.get(profileId) ?? lookFor(drawn, profileId);
      cache.set(profileId, look);
      return look;
    };
  }, [drawn]);
  const activeLook = lookOf(activePane?.profile.id);
  const activeProfileIdRef = useRef(activePane?.profile.id);
  activeProfileIdRef.current = activePane?.profile.id;
  const theme = resolveTheme(activeLook.themeId, activeLook.customThemes);
  const overlayOpen = settingsOpen || paletteOpen || findEverywhereOpen || confirmation !== null;
  // A tab closed mid-rename takes its editor with it, so only a live tab counts.
  const renaming = renamingTabId !== null && tabs.some((tab) => tab.id === renamingTabId && !closing.has(tab.id));
  const renamingRef = useRef(renaming);
  renamingRef.current = renaming;

  /**
   * `launch.folder` is the folder the app was started with (`nebula-terminal C:\Projects`).
   * A copy started for a console Windows handed over opens only that console's tab.
   */
  const hydrateWorkspace = useCallback((detected: TerminalProfile[], launch: Launch = { forHandoff: false, handoffs: [] }) => {
    setProfiles(detected);
    if (!hydratedRef.current) {
      hydratedRef.current = true;
      setReady(true);
      // An admin window opens what it was asked for, never the tabs of a past session.
      const restored = launch.restored !== undefined
        ? launch.restored
        : preferencesRef.current.restoreSession && savesSession && !isTabsWindow ? loadSession(detected) : null;
      const first = pickProfile(detected, launch.profileId ?? preferencesRef.current.defaultProfileId);
      const wantsTab = launch.folder || (!restored && !launch.forHandoff);
      const opened = first && wantsTab ? [makeTab(first, launch.folder)] : [];
      if (first) opened.push(...launch.handoffs.map((handoff) => makeHandoffTab(first, handoff)));
      const all = [...restored?.tabs ?? [], ...opened];
      const activeId = opened.at(-1)?.id ?? restored?.activeTabId ?? "";
      setTabs(all);
      setActiveTabId(activeId);
      setDeferredTabs(new Set(restored?.tabs.map((tab) => tab.id).filter((id) => id !== activeId)));
      const front = all.find((tab) => tab.id === activeId) ?? all[0];
      const shell = front?.panes.find((pane) => pane.id === front.activePaneId) ?? front?.panes[0];
      if (shell) {
        splash.advance("session", `Starting ${shell.profile.name}`);
        // A shell that prints nothing yet (an ssh login, say) still gets its window.
        window.setTimeout(splash.finish, 3000);
      } else {
        splash.finish();
      }
      return;
    }
    setTabs((current) => refreshTabProfiles(current, detected));
  }, []);


  const splashReady = useCallback(() => splash.finish(), []);

  const reportProgress = useCallback((tabId: string, report: Progress | null) => {
    setProgress((current) => {
      if (!report && !current.has(tabId)) return current;
      const next = new Map(current);
      next.delete(tabId);
      if (report) next.set(tabId, report);
      return next;
    });
  }, []);

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
    root.style.setProperty("--accent", activeLook.accent);
    root.style.setProperty("--term-bg", theme.background);
    root.style.setProperty("--term-fg", theme.foreground);
    root.dataset.scheme = theme.scheme;
    root.dataset.animation = preferences.animationLevel;
    root.dataset.density = preferences.tabDensity;
    root.dataset.background = preferences.backgroundMode === "mica" && micaFailed ? "solid" : preferences.backgroundMode;
    root.dataset.host = nativeHost ? "native" : "browser";
    root.style.setProperty("--on-accent", readableOn(activeLook.accent));
    root.style.setProperty("--term-surface", preferences.terminalOpacity >= 1 ? theme.background : `color-mix(in srgb, ${theme.background} ${Math.round(preferences.terminalOpacity * 100)}%, transparent)`);
    document.querySelector('meta[name="theme-color"]')?.setAttribute("content", theme.background);
    splash.rememberLook({ background: theme.background, foreground: theme.foreground, accent: activeLook.accent, scheme: theme.scheme, animation: preferences.animationLevel });
  }, [activeLook.accent, micaFailed, nativeHost, preferences, theme]);

  // The taskbar button shows the progress of the tab in front, or of one in the background.
  const shownProgress = windowProgress(progress, activeTabId);
  useEffect(() => {
    if (!nativeHost) return;
    const state = shownProgress
      ? { status: shownProgress.state as ProgressBarStatus, progress: shownProgress.value }
      : { status: ProgressBarStatus.None };
    void getCurrentWindow().setProgressBar(state).catch(() => undefined);
  }, [nativeHost, shownProgress?.state, shownProgress?.value]);

  // The theme picked last for each scheme is the one "Follow Windows" comes back to.
  useEffect(() => {
    const slot = resolveTheme(preferences.themeId, preferences.customThemes).scheme === "dark" ? "darkThemeId" : "lightThemeId";
    if (preferences[slot] !== preferences.themeId) setPreferences((current) => ({ ...current, [slot]: current.themeId }));
  }, [preferences]);

  useEffect(() => {
    if (!preferences.followSystemTheme) return;
    const media = matchMedia("(prefers-color-scheme: dark)");
    const follow = () => setPreferences((current) => {
      const id = media.matches ? current.darkThemeId : current.lightThemeId;
      if (id === current.themeId) return current;
      const next = resolveTheme(id, current.customThemes);
      return { ...current, themeId: next.id, accent: next.accent };
    });
    follow();
    media.addEventListener("change", follow);
    return () => media.removeEventListener("change", follow);
  }, [preferences.followSystemTheme]);

  // Once the shell in front is up and the loading screen gone, the other restored tabs
  // start, one at a time so typing in the first one stays smooth.
  useEffect(() => {
    if (!ready) return;
    let cancelled = false;
    let timer = 0;
    const startNext = () => {
      const [next] = deferredTabsRef.current;
      if (cancelled || next === undefined) return;
      setDeferredTabs((current) => new Set([...current].filter((id) => id !== next)));
      timer = window.setTimeout(startNext, 120);
    };
    void splash.finished.then(startNext);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [ready]);

  useEffect(() => {
    if (deferredTabs.has(activeTabId)) setDeferredTabs((current) => new Set([...current].filter((id) => id !== activeTabId)));
  }, [activeTabId, deferredTabs]);

  useEffect(() => {
    if (!nativeHost) return;
    // Mica needs Windows 11; elsewhere fall back to an opaque window instead of a see-through one.
    void invoke("set_window_effect", { mode: preferences.backgroundMode, dark: theme.scheme === "dark" })
      .then(() => setMicaFailed(false))
      .catch(() => setMicaFailed(preferences.backgroundMode === "mica"));
  }, [nativeHost, preferences.backgroundMode, theme.scheme]);

  useEffect(() => {
    if (!hydratedRef.current || !preferences.restoreSession || !tabs.length || !savesSession) return;
    if (isTabsWindow) saveWindowSession(windowLabel, tabs, activeTabId);
    else saveSession(tabs, activeTabId);
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
    // Updates install from an ordinary window, so the app never restarts elevated.
    if (isAdministrator || isQuickTerminal || !preferences.checkForUpdates || !automaticCheckDue()) return;
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
    void getCurrentWindow().onCloseRequested(async (event) => {
      const count = tabsRef.current.filter((tab) => !closingRef.current.has(tab.id)).length;
      if (!closingWindowRef.current && preferencesRef.current.confirmCloseMultipleTabs && count > 1) {
        event.preventDefault();
        setConfirmation({ title: `Close ${count} tabs?`, body: "Every shell running in this window will be stopped.", action: "Close all", run: closeWindow });
        return;
      }
      // A window closed while others stay open takes its tabs with it, as in Windows
      // Terminal. The last one keeps them for the next launch.
      if (savesSession && await invoke<number>("terminal_window_count").catch(() => 1) > 1) {
        if (isTabsWindow) forgetWindowSession(windowLabel);
        else clearSession();
      }
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

  const openNewTab = useCallback((profileId?: string, folder?: string) => {
    const profile = resolveProfile(profileId);
    if (!profile) return;
    const next = makeTab(profile, folder ?? currentFolder());
    setTabs((current) => [...current, next]);
    setActiveTabId(next.id);
  }, [currentFolder, resolveProfile]);

  const [workspaces, setWorkspaces] = useState<Workspace[]>(() => loadWorkspaces());
  const changeWorkspaces = useCallback((next: Workspace[]) => {
    setWorkspaces(next);
    saveWorkspaces(next);
  }, []);

  /** Asks for a name, then saves the open tabs (splits, folders, names, colors) under it. */
  const saveTabsAsWorkspace = useCallback(() => {
    const live = tabsRef.current.filter((tab) => !closingRef.current.has(tab.id));
    if (live.length === 0) return;
    setConfirmation({
      title: "Save these tabs as a workspace",
      body: `${live.length} tab${live.length === 1 ? "" : "s"}, with their splits and folders. Open them again from the new-tab menu or the command palette; a workspace with the same name is replaced.`,
      action: "Save",
      inputs: [{ label: "Name", value: suggestName(live, workspaces) }],
      run: ([name = ""]) => {
        changeWorkspaces(withWorkspace(workspaces, name, tabsRef.current.filter((tab) => !closingRef.current.has(tab.id)), activeTabIdRef.current));
        setNotice(`Saved “${name.trim()}”`);
      },
    });
  }, [changeWorkspaces, workspaces]);

  /** Opens a workspace's tabs next to the ones already open. */
  const openSavedWorkspace = useCallback((workspace: Workspace) => {
    const opened = openWorkspace(workspace, profiles);
    if (!opened) {
      setNotice(`“${workspace.name}” couldn't be opened`);
      return;
    }
    setTabs((current) => [...current, ...opened.tabs]);
    setActiveTabId(opened.activeTabId);
  }, [profiles]);

  /** Opens an admin window through UAC. An admin window opens a tab instead: it already is one. */
  const openAsAdmin = useCallback((profileId?: string, folder?: string) => {
    if (isAdministrator) {
      openNewTab(profileId, folder);
      return;
    }
    const profile = resolveProfile(profileId);
    if (!profile) return;
    if (!nativeHost) {
      setNotice("Administrator windows open in the desktop app");
      return;
    }
    openAsAdministrator(profile.id, folder ?? currentFolder(), preferencesRef.current)
      .catch((error) => setNotice(String(error)));
  }, [currentFolder, nativeHost, openNewTab, resolveProfile]);

  // A second launch (Explorer's "Open in Nebula Terminal") lands here as a new tab.
  const openNewTabRef = useRef(openNewTab);
  openNewTabRef.current = openNewTab;
  useEffect(() => {
    if (!nativeHost) return;
    const stop = listen<string | null>("open-tab", (event) => openNewTabRef.current(undefined, event.payload ?? undefined));
    return () => void stop.then((unlisten) => unlisten());
  }, [nativeHost]);
  // "Run as administrator" when UAC is off: this window is elevated already.
  useEffect(() => {
    if (!nativeHost) return;
    const stop = listen<{ profileId?: string | null; folder?: string | null }>("open-request", (event) => openNewTabRef.current(event.payload.profileId ?? undefined, event.payload.folder ?? undefined));
    return () => void stop.then((unlisten) => unlisten());
  }, [nativeHost]);

  /** Broadcast input: what is typed in one pane of the tab also goes to the others. */
  const broadcastInput = useCallback((tabId: string, paneId: string, data: string) => {
    const tab = tabsRef.current.find((candidate) => candidate.id === tabId);
    if (!tab?.broadcast) return;
    for (const pane of tab.panes) {
      if (pane.id !== paneId) getPane(pane.id)?.write(data);
    }
  }, []);

  const toggleBroadcast = useCallback((tabId?: string) => {
    const id = tabId ?? activeTabIdRef.current;
    const tab = tabsRef.current.find((candidate) => candidate.id === id);
    if (!tab) return;
    const broadcast = !tab.broadcast;
    setTabs((current) => current.map((candidate) => candidate.id === id ? { ...candidate, broadcast } : candidate));
    setNotice(broadcast
      ? tab.panes.length > 1 ? `Typing goes to all ${tab.panes.length} panes` : "Typing will go to every pane you open in this tab"
      : "Typing goes to one pane again");
  }, []);

  /** Opens a tab for each console Windows handed over since the last look. */
  const openHandoffs = useCallback(async () => {
    // Until the workspace exists, hydration takes them.
    if (!hydratedRef.current) return;
    const handoffs = await invoke<HandoffInfo[]>("take_handoffs").catch(() => []);
    const profile = resolveProfile();
    if (!profile || handoffs.length === 0) return;
    const opened = handoffs.map((handoff) => makeHandoffTab(profile, handoff));
    setTabs((current) => [...current, ...opened]);
    setActiveTabId(opened.at(-1)!.id);
  }, [resolveProfile]);

  const openHandoffsRef = useRef(openHandoffs);
  openHandoffsRef.current = openHandoffs;
  useEffect(() => {
    if (!nativeHost) return;
    const stop = listen("handoff", () => void openHandoffsRef.current());
    return () => void stop.then((unlisten) => unlisten());
  }, [nativeHost]);

  useEffect(() => {
    const request = administratorLaunch();
    const launch: Promise<Launch> = request
      ? Promise.resolve({ folder: request.folder ?? undefined, profileId: request.profileId ?? undefined, forHandoff: false, handoffs: [] })
      : nativeHost && isTabsWindow
      // A window opened later can still take a console Windows handed over.
      ? invoke<HandoffInfo[]>("take_handoffs").catch(() => []).then((handoffs): Launch => ({ forHandoff: handoffs.length > 0, handoffs }))
      : nativeHost && !isQuickTerminal
      ? Promise.all([
        invoke<string | null>("launch_folder").catch(() => null),
        invoke<boolean>("launched_for_handoff").catch(() => false),
        invoke<HandoffInfo[]>("take_handoffs").catch(() => []),
      ]).then(([folder, forHandoff, handoffs]) => ({ folder: folder ?? undefined, forHandoff, handoffs }))
      : Promise.resolve({ forHandoff: false, handoffs: [] });
    splash.advance("shells", "Finding your shells");
    void Promise.all([detectProfiles(), launch])
      .then(async ([detected, started]) => {
        const own = await windowTabs(detected);
        hydrateWorkspace(detected, { ...started, ...own, folder: own.folder ?? started.folder });
      })
      .catch(() => hydrateWorkspace([]))
      // A console that arrived while the workspace was loading.
      .then(() => openHandoffsRef.current());
  }, [hydrateWorkspace, nativeHost]);

  /**
   * The tabs this window opens with, when they aren't the main session: a tab moved into
   * a new window, or the other windows saved at exit, which the main window opens again.
   */
  const windowTabs = useCallback(async (detected: TerminalProfile[]): Promise<Pick<Launch, "restored" | "folder">> => {
    if (isTabsWindow) {
      const transfer = transferId ? await invoke<TabTransfer | WindowTransfer | LaunchTransfer | null>("take_transfer", { id: transferId }).catch(() => null) : null;
      if (transfer?.kind === "tab") return { restored: { tabs: [tabFromTransfer(transfer.tab)], activeTabId: transfer.tab.id } };
      if (transfer?.kind === "window") return { restored: restoreSnapshot(transfer.snapshot, detected) };
      if (transfer?.kind === "launch") return { restored: null, folder: transfer.folder ?? undefined };
      return { restored: null };
    }
    // A workspace chosen for startup wins over the last session.
    const startup = savesSession && preferencesRef.current.startupWorkspace
      ? loadWorkspaces().find((workspace) => workspace.id === preferencesRef.current.startupWorkspace)
      : undefined;
    if (startup) return { restored: openWorkspace(startup, detected) };
    if (!nativeHost || !savesSession || !preferencesRef.current.restoreSession) return {};
    const others = takeWindowSessions();
    const own = loadSession(detected);
    // With no tabs of its own, the main window takes the first saved window's.
    const first = own ?? (others.length ? restoreSnapshot(others.shift(), detected) : null);
    if (others.length) void invoke("restore_windows", { snapshots: others.map((snapshot): WindowTransfer => ({ kind: "window", snapshot })) }).catch(() => undefined);
    return { restored: first };
  }, [nativeHost]);

  /** Moves a tab, its shells still running, to the window under the pointer or a new one. */
  const moveTabOut = useCallback(async (tabId: string, newWindow: boolean) => {
    const tab = tabsRef.current.find((candidate) => candidate.id === tabId);
    const live = tabsRef.current.filter((candidate) => !closingRef.current.has(candidate.id));
    // A window's only tab is the window: dragging it out would only move the window.
    if (!nativeHost || !tab || (newWindow && live.length <= 1)) return;
    const panes = await Promise.all(tab.panes.map(async (pane) => ({
      ...pane,
      handoffId: undefined,
      moved: await getPane(pane.id)?.detach() ?? { screen: "" },
    })));
    const transfer: TabTransfer = {
      kind: "tab",
      tab: { id: tab.id, title: tab.title, customTitle: tab.customTitle, color: tab.color, layout: tab.layout, activePaneId: tab.activePaneId, panes },
    };
    try {
      await invoke("move_tab", { transfer, newWindow });
    } catch (error) {
      tab.panes.forEach((pane) => getPane(pane.id)?.undoDetach());
      setNotice(`The tab couldn't be moved: ${String(error)}`);
      return;
    }
    removeTabsRef.current([tabId]);
  }, [nativeHost]);

  const tearOffTab = useCallback((tabId: string) => {
    void invoke<boolean>("window_under_cursor")
      .then((overAnother) => moveTabOut(tabId, !overAnother))
      .catch(() => undefined);
  }, [moveTabOut]);

  // A tab dragged here from another window.
  useEffect(() => {
    if (!nativeHost || isQuickTerminal) return;
    const stop = listen<string>("tab-arrived", (event) => {
      void invoke<TabTransfer | null>("take_transfer", { id: event.payload }).then((transfer) => {
        if (transfer?.kind !== "tab") return;
        const tab = tabFromTransfer(transfer.tab);
        setTabs((current) => [...current, tab]);
        setActiveTabId(tab.id);
      }).catch(() => undefined);
    });
    return () => void stop.then((unlisten) => unlisten());
  }, [nativeHost]);

  const [defaultTerminal, setDefaultTerminal] = useState<DefaultTerminal | null>(null);
  // Both belong to the main window: admin windows and the quick terminal leave them alone.
  const changeDefaultTerminal = useCallback((enabled: boolean | null) => {
    if (!nativeHost || isAdministrator || isQuickTerminal) return;
    invoke<DefaultTerminal>("default_terminal", { enabled })
      .then(setDefaultTerminal)
      .catch((error) => setNotice(String(error)));
  }, [nativeHost]);
  useEffect(() => changeDefaultTerminal(null), [changeDefaultTerminal]);

  const [launchers, setLaunchers] = useState<Launchers | null>(null);
  useEffect(() => {
    if (!nativeHost || isAdministrator || isQuickTerminal) return;
    invoke<Launchers>("sync_launchers", { explorerMenu: preferences.explorerMenu, command: preferences.pathCommand, windowsTerminal: preferences.windowsTerminalProfile })
      .then(setLaunchers)
      .catch((error) => setNotice(String(error)));
  }, [nativeHost, preferences.explorerMenu, preferences.pathCommand, preferences.windowsTerminalProfile]);

  // The main window owns the quick terminal's shortcut, so there's one, whichever window changed it.
  const [quickTerminalProblem, setQuickTerminalProblem] = useState<string | null>(null);
  useEffect(() => {
    if (!nativeHost || isAdministrator || isQuickTerminal) return;
    invoke("set_quick_terminal", { enabled: preferences.quickTerminal, shortcut: preferences.quickTerminalShortcut, height: preferences.quickTerminalHeight, keepRunning: preferences.quickTerminalKeepRunning })
      .then(() => setQuickTerminalProblem(null))
      .catch((error: unknown) => setQuickTerminalProblem(String(error)));
  }, [nativeHost, preferences.quickTerminal, preferences.quickTerminalShortcut, preferences.quickTerminalHeight, preferences.quickTerminalKeepRunning]);

  // Settings sync through a file (sync.ts). The main window reads and writes it; the
  // others get the changes through local storage, as with any setting.
  const syncsSettings = nativeHost && savesSession && !isTabsWindow;
  const syncRef = useRef(loadSyncState());
  // Nothing is written to a file before it has been read once: its settings may be newer.
  const readSyncFileRef = useRef<string | null>(null);
  const [syncProblem, setSyncProblem] = useState<string | null>(null);
  const writeSettings = useCallback(async (path: string, settings: AppearancePreferences) => {
    const savedAt = Date.now();
    try {
      const modified = await invoke<number>("write_sync_file", { path, contents: syncDocument(settings, savedAt) });
      syncRef.current = { savedAt, digest: digestOf(settings), modified };
      saveSyncState(syncRef.current);
      setSyncProblem(null);
    } catch (error) {
      setSyncProblem(String(error));
    }
  }, []);
  const pullSettings = useCallback(async () => {
    const path = preferencesRef.current.syncFile;
    if (!path) return;
    try {
      const file = await invoke<{ contents: string; modified: number } | null>("read_sync_file", { path });
      readSyncFileRef.current = path;
      if (!file) {
        // A new file: this PC's settings go in it.
        await writeSettings(path, preferencesRef.current);
        return;
      }
      const document = readSyncDocument(file.contents);
      if (!document) {
        setSyncProblem("This file isn't a Nebula Terminal settings file.");
        return;
      }
      setSyncProblem(null);
      syncRef.current = { ...syncRef.current, modified: file.modified };
      if (document.savedAt <= syncRef.current.savedAt) return;
      const applied = applySynced(preferencesRef.current, document.preferences);
      syncRef.current = { savedAt: document.savedAt, digest: digestOf(applied), modified: file.modified };
      saveSyncState(syncRef.current);
      setPreferences(applied);
    } catch (error) {
      setSyncProblem(String(error));
    }
  }, [writeSettings]);
  useEffect(() => {
    if (!syncsSettings || !preferences.syncFile) return;
    const path = preferences.syncFile;
    void pullSettings();
    const timer = window.setInterval(() => {
      void invoke<number | null>("sync_file_modified", { path }).then((modified) => {
        if (modified && modified !== syncRef.current.modified) void pullSettings();
      }).catch(() => undefined);
    }, 10_000);
    return () => window.clearInterval(timer);
  }, [preferences.syncFile, pullSettings, syncsSettings]);
  useEffect(() => {
    if (!syncsSettings || !preferences.syncFile || readSyncFileRef.current !== preferences.syncFile) return;
    if (digestOf(preferences) === syncRef.current.digest) return;
    const path = preferences.syncFile;
    const timer = window.setTimeout(() => void writeSettings(path, preferences), 1500);
    return () => window.clearTimeout(timer);
  }, [preferences, syncsSettings, writeSettings]);
  const chooseSyncFile = useCallback(async () => {
    const path = await invoke<string | null>("choose_sync_file").catch(() => null);
    if (!path) return;
    // The file's settings, if it has some, win over this PC's: that's how a second PC joins.
    syncRef.current = { savedAt: 0, digest: "", modified: 0 };
    readSyncFileRef.current = null;
    if (path === preferencesRef.current.syncFile) void pullSettings();
    else setPreferences((current) => ({ ...current, syncFile: path }));
  }, [pullSettings]);

  // Settings changed in the other window (main or quick) apply here too.
  useEffect(() => {
    const reload = (event: StorageEvent) => {
      if (event.key && isPreferencesKey(event.key)) setPreferences(loadPreferences());
    };
    window.addEventListener("storage", reload);
    return () => window.removeEventListener("storage", reload);
  }, []);

  // The quick terminal slides down when shown and back up before it hides.
  const [quickMotion, setQuickMotion] = useState<"in" | "out">("in");
  const hidingQuickRef = useRef(false);
  const hideQuickTerminal = useCallback(() => {
    if (hidingQuickRef.current) return;
    hidingQuickRef.current = true;
    setQuickMotion("out");
    const instant = preferencesRef.current.animationLevel === "off";
    window.setTimeout(() => {
      void invoke("hide_quick_terminal").finally(() => { hidingQuickRef.current = false; });
    }, instant ? 0 : 170);
  }, []);
  useEffect(() => {
    if (!nativeHost || !isQuickTerminal) return;
    const stops = [
      listen("quick-terminal-show", () => {
        hidingQuickRef.current = false;
        setQuickMotion("in");
        requestAnimationFrame(() => getPane(tabsRef.current.find((tab) => tab.id === activeTabIdRef.current)?.activePaneId)?.focus());
      }),
      listen("quick-terminal-hide", hideQuickTerminal),
      getCurrentWindow().onFocusChanged(({ payload: focused }) => {
        if (!focused && preferencesRef.current.quickTerminalHideOnBlur) hideQuickTerminal();
      }),
    ];
    return () => stops.forEach((stop) => void stop.then((unlisten) => unlisten()));
  }, [hideQuickTerminal, nativeHost]);

  const removeTabs = useCallback((ids: string[]) => {
    const closingIds = closingRef.current;
    const remove = new Set(ids.filter((id) => !closingIds.has(id)));
    if (remove.size === 0) return;
    const current = tabsRef.current;
    const live = current.filter((tab) => !closingIds.has(tab.id) && !remove.has(tab.id));
    if (live.length === 0) {
      if (nativeHost && !isQuickTerminal) {
        closeWindow();
        return;
      }
      // The quick terminal hides with a fresh tab for next time, and the browser
      // preview has no window to close: both keep one tab open.
      if (isQuickTerminal) hideQuickTerminal();
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
      setProgress((current) => [...remove].some((id) => current.has(id)) ? new Map([...current].filter(([id]) => !remove.has(id))) : current);
      remove.forEach((id) => closingIds.delete(id));
      setClosing(new Set(closingIds));
    };
    if (preferencesRef.current.animationLevel === "off") finish();
    else window.setTimeout(finish, TAB_CLOSE_MS);
  }, [closeWindow, hideQuickTerminal, nativeHost, resolveProfile]);
  removeTabsRef.current = removeTabs;

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
    // Another pane taking the focus ends a zoom, as in Windows Terminal.
    setTabs((current) => current.map((tab) => tab.id === tabId && tab.activePaneId !== paneId ? { ...tab, activePaneId: paneId, zoomedPaneId: undefined } : tab));
    setActiveTabId(tabId);
  }, []);

  /** Types a saved command into the pane in front, each line followed by Enter, once its blanks are filled in. */
  const runSavedCommand = useCallback((saved: SavedCommand) => {
    const type = (command: string) => {
      const pane = getPane(tabsRef.current.find((tab) => tab.id === activeTabIdRef.current)?.activePaneId);
      pane?.write(commandInput(command));
      pane?.focus();
    };
    const blanks = blanksIn(saved.command);
    if (!blanks.length) return type(saved.command);
    const last = lastBlankValues(saved.id);
    setConfirmation({
      title: savedCommandLabel(saved),
      body: "Fill in the blanks, and it runs in the pane in front.",
      action: "Run",
      inputs: blanks.map((name) => ({ label: name, value: last[name] ?? "", maxLength: 500 })),
      run: (values) => {
        const filled = Object.fromEntries(blanks.map((name, index) => [name, values[index]!]));
        rememberBlankValues(saved.id, filled);
        type(fillBlanks(saved.command, filled));
      },
    });
  }, []);

  /** The pane in front fills its tab, or gives the room back. */
  const togglePaneZoom = useCallback(() => {
    const tab = tabsRef.current.find((candidate) => candidate.id === activeTabIdRef.current);
    if (!tab || tab.panes.length < 2) return;
    setTabs((current) => current.map((candidate) => candidate.id === tab.id ? { ...candidate, zoomedPaneId: candidate.zoomedPaneId ? undefined : candidate.activePaneId } : candidate));
  }, []);

  const setPaneTitle = useCallback((tabId: string, pane: TerminalPaneModel, title: string) => {
    const clean = cleanTitle(title, pane.profile);
    setTabs((current) => current.map((tab) => tab.id === tabId && tab.activePaneId === pane.id && tab.title !== clean ? { ...tab, title: clean } : tab));
  }, []);

  const [recentFolders, setRecentFolders] = useState(loadRecentFolders);
  const setPaneFolder = useCallback((tabId: string, paneId: string, cwd: string) => {
    setTabs((current) => current.map((tab) => tab.id !== tabId || !tab.panes.some((pane) => pane.id === paneId && pane.cwd !== cwd)
      ? tab
      : { ...tab, panes: tab.panes.map((pane) => pane.id === paneId ? { ...pane, cwd } : pane) }));
    setRecentFolders((current) => {
      const next = rememberFolder(current, cwd);
      if (next[0] !== current[0] || next.length !== current.length) saveRecentFolders(next);
      return next;
    });
  }, []);

  /** Saves what the pane in front shows, or its last command's output, to a file. */
  const saveOutput = useCallback((lastOnly: boolean) => {
    const tab = tabsRef.current.find((candidate) => candidate.id === activeTabIdRef.current);
    const pane = getPane(tab?.activePaneId);
    const text = lastOnly ? pane?.lastOutput() : pane?.text();
    if (text === undefined) {
      setNotice(lastOnly ? "No command output to save yet" : "Nothing to save");
      return;
    }
    if (!nativeHost) {
      setNotice("Output saves from the desktop app");
      return;
    }
    const stamp = new Date().toISOString().slice(0, 16).replace("T", " ").replace(":", "h");
    const name = `${tab ? tabTitle(tab) : "Nebula"} ${stamp}`;
    void invoke<string | null>("save_text_file", { name, contents: text })
      .then((path) => { if (path) setNotice(`Saved to ${folderParts(path).name}`); })
      .catch((error: unknown) => setNotice(String(error)));
  }, [nativeHost]);

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
      return { ...tab, panes: [...tab.panes, pane], layout: splitPane(tab.layout, tab.activePaneId, pane.id, direction), activePaneId: pane.id, zoomedPaneId: undefined };
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
      return { ...tab, panes, layout, activePaneId, zoomedPaneId: undefined };
    }));
  }, [closeTab]);

  const closeActivePane = useCallback(() => {
    if (activeTab) closePane(activeTab.id, activeTab.activePaneId);
  }, [activeTab, closePane]);

  const focusNeighborPane = useCallback((key: string) => {
    const direction = arrowDirections[key];
    if (!activeTab || activeTab.panes.length < 2 || !direction) return false;
    // Hidden panes have no place to move to: moving ends the zoom first.
    if (activeTab.zoomedPaneId) {
      togglePaneZoom();
      return true;
    }
    const rects = new Map<string, Rect>();
    for (const pane of activeTab.panes) {
      const box = paneElement(pane.id)?.getBoundingClientRect();
      if (box) rects.set(pane.id, { x: box.left, y: box.top, width: box.width, height: box.height });
    }
    const next = findNeighbor(rects, activeTab.activePaneId, direction);
    if (next) setActivePane(activeTab.id, next);
    return true;
  }, [activeTab, setActivePane, togglePaneZoom]);

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

  /** The tab's shell again, in its folder, in an admin window. */
  const openTabAsAdmin = useCallback((tabId: string) => {
    const tab = tabsRef.current.find((candidate) => candidate.id === tabId);
    const pane = tab?.panes.find((candidate) => candidate.id === tab.activePaneId) ?? tab?.panes[0];
    if (pane) openAsAdmin(pane.profile.id, pane.cwd);
  }, [openAsAdmin]);

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

  /** Zooms the pane in front: its profile's own size if it has one, the app's otherwise. */
  const adjustFontSize = useCallback((delta: number) => {
    const profileId = activeProfileIdRef.current;
    const size = (value: number) => Math.min(32, Math.max(8, value + delta));
    setPreferences((current) => {
      const own = profileId && Object.hasOwn(current.profileLooks, profileId) ? current.profileLooks[profileId]?.fontSize : undefined;
      return own !== undefined && profileId
        ? { ...current, profileLooks: withLook(current.profileLooks, profileId, { fontSize: size(own) }) }
        : { ...current, fontSize: size(current.fontSize) };
    });
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
  const tabCommands = useMemo(() => tabs.filter((tab) => !closing.has(tab.id)).map((tab, index): PaletteCommand => {
    const pane = tab.panes.find((candidate) => candidate.id === tab.activePaneId) ?? tab.panes[0];
    return {
      id: `tab-${tab.id}`,
      group: "Tabs",
      label: tabTitle(tab),
      detail: pane?.cwd ? folderParts(pane.cwd).name : pane?.profile.name,
      icon: pane ? <ProfileIcon kind={pane.profile.kind} accent={pane.profile.accent} size={15} /> : undefined,
      shortcut: index < 9 ? `Ctrl+Alt+${index + 1}` : undefined,
      keywords: tab.panes.map((candidate) => `${candidate.profile.name} ${candidate.cwd ?? ""}`).join(" "),
      run: () => setActiveTabId(tab.id),
    };
  }), [closing, tabs]);
  const openTabSwitcher = useCallback(() => {
    setSwitchingTabs(true);
    setPaletteOpen(true);
  }, []);

  const commands = useMemo<PaletteCommand[]>(() => {
    const defaultProfile = resolveProfile();
    return [
      { id: "new-tab", group: "Tabs", label: "New tab", detail: defaultProfile?.name, icon: <Plus size={15} />, shortcut: keys.newTab, run: () => openNewTab() },
      { id: "save-workspace", group: "Workspaces", label: "Save these tabs as a workspace…", icon: <Save size={15} />, keywords: "layout session", run: saveTabsAsWorkspace },
      ...workspaces.map((workspace): PaletteCommand => ({ id: `workspace-${workspace.id}`, group: "Workspaces", label: `Open workspace: ${workspace.name}`, detail: describeWorkspace(workspace), icon: <LayoutPanelTop size={15} />, keywords: "layout session tabs", run: () => openSavedWorkspace(workspace) })),
      ...preferences.savedCommands.filter((saved) => saved.command.trim()).map((saved): PaletteCommand => ({
        id: `saved-${saved.id}`, group: "Saved commands", label: savedCommandLabel(saved), detail: saved.name.trim() ? saved.command.split(/\r?\n/)[0] : undefined,
        icon: <TerminalSquare size={15} />, keywords: `run snippet ${saved.command}`, run: () => runSavedCommand(saved),
      })),
      { id: "save-command", group: "Saved commands", label: "Save a command…", icon: <Plus size={15} />, keywords: "snippet saved commands", run: () => openSettings("behavior") },
      ...(isAdministrator ? [] : [{ id: "new-administrator-tab", group: "Tabs", label: "New tab as administrator", detail: defaultProfile?.name, icon: <ShieldCheck size={15} />, keywords: "admin elevated uac sudo", shortcut: keys.newAdministratorTab, run: () => openAsAdmin() }]),
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
      ...(isAdministrator ? [] : profiles.filter((profile) => profile.available && profileGroup(profile) !== "ssh").map((profile): PaletteCommand => ({
        id: `administrator-${profile.id}`, group: "Run as administrator", label: `${profile.name} as administrator`,
        icon: <ShieldCheck size={15} />, keywords: `admin elevated uac sudo ${profile.kind}`, run: () => openAsAdmin(profile.id),
      }))),
      { id: "split-right", group: "Panes", label: "Split right", icon: <Columns2 size={15} />, shortcut: keys.splitVertical, run: () => splitActive("vertical") },
      { id: "split-down", group: "Panes", label: "Split down", icon: <Rows2 size={15} />, shortcut: keys.splitHorizontal, run: () => splitActive("horizontal") },
      ...((activeTab?.panes.length ?? 0) > 1 ? [{ id: "zoom-pane", group: "Panes", label: activeTab?.zoomedPaneId ? "Show every pane" : "Zoom pane", icon: <Maximize2 size={15} />, keywords: "maximize fill focus", shortcut: keys.togglePaneZoom, run: togglePaneZoom }] : []),
      { id: "close-pane", group: "Panes", label: "Close pane", icon: <SquareX size={15} />, shortcut: keys.closePane, run: closeActivePane },
      { id: "broadcast", group: "Panes", label: activeTab?.broadcast ? "Stop typing in every pane" : "Type in every pane", icon: <Radio size={15} />, keywords: "broadcast input sync panes", shortcut: keys.toggleBroadcast, run: () => toggleBroadcast() },
      ...[undefined, ...tabColors].map((color): PaletteCommand => ({
        id: `tab-color-${color?.id ?? "none"}`, group: "Tab color", label: `Tab color: ${color?.name ?? "None"}`, keywords: "colour",
        icon: <span className={`palette__swatch ${color ? "" : "palette__swatch--none"}`} style={color ? { background: color.value } : undefined} />,
        run: () => activeTab && setTabColor(activeTab.id, color?.id),
      })),
      { id: "find", group: "Terminal", label: "Find", icon: <Search size={15} />, shortcut: keys.find, run: openSearch },
      { id: "copy", group: "Terminal", label: "Copy", icon: <Copy size={15} />, shortcut: "Ctrl+C", run: () => getPane(activePane?.id)?.copy() },
      { id: "copy-colors", group: "Terminal", label: "Copy with colors", icon: <Palette size={15} />, keywords: "html rich formatted", run: () => { if (!getPane(activePane?.id)?.copyWithColors()) setNotice("Select some text first"); } },
      { id: "paste", group: "Terminal", label: "Paste", icon: <ClipboardPaste size={15} />, shortcut: "Ctrl+V", run: () => getPane(activePane?.id)?.paste() },
      { id: "select-all", group: "Terminal", label: "Select all", icon: <TextSelect size={15} />, run: () => getPane(activePane?.id)?.selectAll() },
      { id: "clear", group: "Terminal", label: "Clear scrollback", icon: <Eraser size={15} />, run: () => getPane(activePane?.id)?.clear() },
      { id: "previous-command", group: "Terminal", label: "Previous command", icon: <ArrowUpToLine size={15} />, keywords: "jump scroll prompt", shortcut: keys.previousCommand, run: () => jumpToCommand(-1) },
      { id: "next-command", group: "Terminal", label: "Next command", icon: <ArrowDownToLine size={15} />, keywords: "jump scroll prompt", shortcut: keys.nextCommand, run: () => jumpToCommand(1) },
      { id: "copy-output", group: "Terminal", label: "Copy last command output", icon: <ClipboardCheck size={15} />, keywords: "result", run: copyLastOutput },
      { id: "switch-tab", group: "Tabs", label: "Switch to a tab…", icon: <ArrowLeftRight size={15} />, keywords: "go jump find tab", shortcut: keys.switchTab, run: () => requestAnimationFrame(openTabSwitcher) },
      { id: "find-everywhere", group: "Terminal", label: "Find in every tab…", icon: <Search size={15} />, keywords: "search grep all tabs panes", shortcut: keys.findEverywhere, run: () => setFindEverywhereOpen(true) },
      { id: "save-output", group: "Terminal", label: "Save output to a file…", icon: <FileDown size={15} />, keywords: "export log text scrollback", run: () => saveOutput(false) },
      { id: "save-last-output", group: "Terminal", label: "Save last command output to a file…", icon: <FileDown size={15} />, keywords: "export log result", run: () => saveOutput(true) },
      ...recentFolders.map((folder): PaletteCommand => ({ id: `recent-${folder}`, group: "Recent folders", label: `Open a tab in ${folderParts(folder).name}`, detail: folderParts(folder).parent, icon: <FolderOpen size={15} />, keywords: `recent folder cd ${folder}`, run: () => openNewTab(undefined, folder) })),
      { id: "zoom-in", group: "View", label: "Zoom in", icon: <ZoomIn size={15} />, shortcut: keys.zoomIn, run: () => adjustFontSize(1) },
      { id: "zoom-out", group: "View", label: "Zoom out", icon: <Minus size={15} />, shortcut: keys.zoomOut, run: () => adjustFontSize(-1) },
      { id: "zoom-reset", group: "View", label: "Reset zoom", shortcut: keys.zoomReset, run: () => setPreferences((current) => ({ ...current, fontSize: defaultPreferences.fontSize })) },
      { id: "compact-tabs", group: "View", label: preferences.tabDensity === "compact" ? "Use normal tabs" : "Use compact tabs", run: () => setPreferences((current) => ({ ...current, tabDensity: current.tabDensity === "compact" ? "comfortable" : "compact" })) },
      ...[...themes, ...preferences.customThemes].map((candidate): PaletteCommand => ({
        id: `theme-${candidate.id}`, group: "Theme", label: `Theme: ${candidate.name}`, keywords: `color scheme ${candidate.scheme}`,
        icon: <span className="palette__swatch" style={{ background: candidate.background, boxShadow: `inset 0 0 0 4px ${candidate.background}, inset 0 0 0 9px ${candidate.accent}` }} />,
        run: () => setPreferences((current) => ({ ...current, themeId: candidate.id, accent: candidate.accent })),
      })),
      { id: "settings", group: "App", label: "Settings", icon: <Settings2 size={15} />, shortcut: keys.settings, run: () => openSettings() },
      { id: "appearance", group: "App", label: "Appearance settings", icon: <Palette size={15} />, keywords: "theme font", run: () => openSettings("appearance") },
      { id: "keyboard", group: "App", label: "Keyboard shortcuts", icon: <Keyboard size={15} />, keywords: "keybindings", run: () => openSettings("keyboard") },
      { id: "profiles", group: "App", label: "Profile settings", icon: <Monitor size={15} />, keywords: "shells wsl ssh custom default", run: () => openSettings("profiles") },
    ];
  }, [activePane?.id, activeTab, adjustFontSize, closeActivePane, closeTab, copyLastOutput, cycleTab, duplicateTab, jumpToCommand, keys, openAsAdmin, openNewTab, openSavedWorkspace, openSearch, saveTabsAsWorkspace, workspaces, openSettings, preferences.customThemes, preferences.tabDensity, profiles, removeTabs, resolveProfile, setTabColor, splitActive, toggleBroadcast, togglePaneZoom, runSavedCommand, preferences.savedCommands, saveOutput, recentFolders]);

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
      if (action(keys.newAdministratorTab, () => openAsAdmin())) return;
      if (action(keys.find, openSearch)) return;
      if (action(keys.splitVertical, () => splitActive("vertical"))) return;
      if (action(keys.splitHorizontal, () => splitActive("horizontal"))) return;
      if (action(keys.closePane, closeActivePane)) return;
      if (action(keys.toggleBroadcast, () => toggleBroadcast())) return;
      if (action(keys.togglePaneZoom, togglePaneZoom)) return;
      if (action(keys.findEverywhere, () => setFindEverywhereOpen(true))) return;
      if (action(keys.switchTab, openTabSwitcher)) return;
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
  }, [activeTab, adjustFontSize, closeActivePane, closeTab, cycleTab, focusNeighborPane, keys, menu, openAsAdmin, openNewTab, openSearch, openSettings, overlayOpen, settingsOpen, splitActive, toggleBroadcast, togglePaneZoom, openTabSwitcher]);

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
            alternate: isAdministrator || profileGroup(profile) === "ssh" ? undefined : { label: "Run as administrator", icon: <ShieldCheck size={14} />, run: () => openAsAdmin(profile.id) },
          })),
        ]),
        ...(recentFolders.length ? [
          "separator" as const,
          { heading: "Recent folders" },
          ...recentFolders.slice(0, 5).map((folder): MenuEntry => ({ id: `recent-${folder}`, label: folderParts(folder).name, detail: folderParts(folder).parent, icon: <FolderOpen size={15} />, run: () => openNewTab(undefined, folder) })),
        ] : []),
        "separator",
        { heading: "Workspaces" },
        ...workspaces.map((workspace): MenuEntry => ({ id: `workspace-${workspace.id}`, label: workspace.name, detail: describeWorkspace(workspace), icon: <LayoutPanelTop size={15} />, run: () => openSavedWorkspace(workspace) })),
        { id: "save-workspace", label: "Save these tabs as a workspace…", icon: <Save size={15} />, run: saveTabsAsWorkspace },
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
        ...(nativeHost && tabs.length > 1 ? [{ id: "new-window", label: "Move to new window", icon: <AppWindow size={15} />, run: () => void moveTabOut(open.tabId, true) }] : []),
        ...(isAdministrator ? [] : [{ id: "administrator", label: "Open as administrator", icon: <ShieldCheck size={15} />, run: () => openTabAsAdmin(open.tabId) }]),
        { id: "split-right", label: "Split right", icon: <Columns2 size={15} />, disabled: open.tabId !== activeTabId, run: () => splitActive("vertical") },
        { id: "broadcast", label: tabs[index]?.broadcast ? "Stop typing in every pane" : "Type in every pane", icon: <Radio size={15} />, shortcut: open.tabId === activeTabId ? keys.toggleBroadcast : undefined, run: () => toggleBroadcast(open.tabId) },
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
      { id: "copy-colors", label: "Copy with colors", icon: <Palette size={15} />, disabled: !pane?.hasSelection(), run: () => pane?.copyWithColors() },
      { id: "paste", label: "Paste", icon: <ClipboardPaste size={15} />, shortcut: "Ctrl+V", run: () => pane?.paste() },
      { id: "select-all", label: "Select all", icon: <TextSelect size={15} />, run: () => pane?.selectAll() },
      { id: "copy-output", label: "Copy last output", icon: <ClipboardCheck size={15} />, disabled: !pane?.hasCommandOutput(), run: copyLastOutput },
      { id: "save-output", label: "Save output…", icon: <FileDown size={15} />, run: () => saveOutput(false) },
      "separator",
      { id: "find", label: "Find…", icon: <Search size={15} />, shortcut: keys.find, run: openSearch },
      { id: "split-right", label: "Split right", icon: <Columns2 size={15} />, shortcut: keys.splitVertical, disabled: full, run: () => splitActive("vertical") },
      { id: "split-down", label: "Split down", icon: <Rows2 size={15} />, shortcut: keys.splitHorizontal, disabled: full, run: () => splitActive("horizontal") },
      ...((activeTab?.panes.length ?? 0) > 1 ? [{ id: "zoom-pane", label: activeTab?.zoomedPaneId ? "Show every pane" : "Zoom pane", icon: <Maximize2 size={15} />, shortcut: keys.togglePaneZoom, run: togglePaneZoom }] : []),
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
    <main className={`app ${ready ? "is-ready" : ""}`} data-quick={isQuickTerminal ? quickMotion : undefined}>
      {showImage && <div className="app__image" style={{ backgroundImage: `url("${preferences.backgroundImage}")`, opacity: preferences.backgroundImageOpacity }} />}
      <Titlebar
        tabs={tabs}
        activeTabId={activeTabId}
        activity={activity}
        finished={finished}
        closing={closing}
        renamingId={renaming ? renamingTabId : null}
        progress={progress}
        newTabShortcut={keys.newTab}
        administrator={isAdministrator}
        quick={isQuickTerminal}
        onHide={hideQuickTerminal}
        paletteShortcut={keys.commandPalette}
        settingsShortcut={keys.settings}
        onSelectTab={setActiveTabId}
        onCloseTab={closeTab}
        onMoveTab={moveTab}
        onTearOff={nativeHost ? tearOffTab : undefined}
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
          if (!visible && deferredTabs.has(tab.id)) return null;
          const { panes: rects, dividers } = computeLayout(tab.layout);
          const zoomed = tab.zoomedPaneId && tab.panes.length > 1 ? tab.zoomedPaneId : undefined;
          return (
            <div key={tab.id} className={`workspace__tab ${visible ? "is-visible" : ""}`} aria-hidden={!visible}>
              {/* Panes stay in one flat, keyed list whatever the tree looks like, so React never
                  remounts a terminal (and restarts its shell) when the layout changes shape. */}
              <div className={`panes ${tab.panes.length > 1 && !zoomed ? "is-split" : ""} ${tab.broadcast ? "is-broadcast" : ""}`}>
                {tab.panes.map((pane) => {
                  const focused = visible && pane.id === tab.activePaneId && !overlayOpen && !searchOpen && !menu && !renaming;
                  return (
                    <div key={pane.id} className={`pane ${pane.id === tab.activePaneId ? "is-active" : ""}`} data-pane-id={pane.id} style={paneStyle(zoomed ? (pane.id === zoomed ? WHOLE_TAB : undefined) : rects.get(pane.id))}>
                      <TerminalPane
                        paneId={pane.id}
                        profile={pane.profile}
                        startIn={pane.cwd}
                        handoffId={pane.handoffId}
                        moved={pane.moved}
                        preferences={lookOf(pane.profile.id)}
                        focused={focused}
                        visible={visible && (!zoomed || pane.id === zoomed)}
                        searchRequest={visible && pane.id === tab.activePaneId && searchOpen ? { query: searchQuery, nonce: searchSignal.nonce, backwards: searchSignal.backwards, ...searchOptions } : undefined}
                        onFocus={() => setActivePane(tab.id, pane.id)}
                        onFontSizeDelta={adjustFontSize}
                        onTitleChange={(title) => setPaneTitle(tab.id, pane, title)}
                        onActivity={() => markActivity(tab.id)}
                        onCommandFinished={(command) => commandFinished(tab.id, command)}
                        onNotify={(title, body) => programNotification(tab.id, title, body)}
                        onFolderChange={(cwd) => setPaneFolder(tab.id, pane.id, cwd)}
                        onNotice={setNotice}
                        onReady={splashReady}
                        onProgress={(report) => reportProgress(tab.id, report)}
                        onSearchResult={setSearchResult}
                        onInput={(data) => broadcastInput(tab.id, pane.id, data)}
                        onContextMenu={(x, y) => { setActivePane(tab.id, pane.id); setMenu({ kind: "terminal", x, y }); }}
                        onClose={() => closePane(tab.id, pane.id)}
                      />
                    </div>
                  );
                })}
                {/* Outer dividers come last so they win where hit areas meet at a T junction. */}
                {!zoomed && dividers.sort((a, b) => b.path.length - a.path.length).map((divider) => {
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
          options={searchOptions}
          onOptionsChange={(options) => {
            setPreferences((current) => ({ ...current, searchCaseSensitive: options.caseSensitive, searchWholeWord: options.wholeWord, searchRegex: options.regex }));
            setSearchSignal((current) => ({ nonce: current.nonce + 1, backwards: false }));
          }}
          result={searchResult}
          onQueryChange={(query) => { setSearchQuery(query); if (!query) setSearchResult(null); setSearchSignal((current) => ({ nonce: current.nonce + 1, backwards: false })); }}
          onNext={() => requestSearch(false)}
          onPrevious={() => requestSearch(true)}
          onClose={() => { setSearchOpen(false); setSearchResult(null); closeOverlayFocus(); }}
        />
      </div>

      {menu && <Menu {...menu} label={menu.kind === "profiles" ? "Open a profile" : menu.kind === "tab" ? "Tab" : "Terminal"} width={menu.kind === "terminal" ? 250 : menu.kind === "tab" ? 260 : 280} entries={menuEntries(menu)} onClose={() => { setMenu(null); closeOverlayFocus(); }} />}

      <Suspense fallback={null}>
        <FindEverywhere
          open={findEverywhereOpen}
          tabs={tabs}
          onClose={() => { setFindEverywhereOpen(false); closeOverlayFocus(); }}
          onReveal={(tabId, paneId, found) => {
            setActivePane(tabId, paneId);
            // Once the tab is in front and the pane has its size.
            requestAnimationFrame(() => requestAnimationFrame(() => getPane(paneId)?.reveal(found)));
          }}
        />
      </Suspense>

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
          launchers={launchers}
          defaultTerminal={defaultTerminal}
          onDefaultTerminalChange={changeDefaultTerminal}
          workspaces={workspaces}
          quickTerminalProblem={quickTerminalProblem}
          sync={syncsSettings ? { problem: syncProblem, choose: () => void chooseSyncFile() } : undefined}
          onWorkspacesChange={changeWorkspaces}
          administrator={isAdministrator}
        />
        <CommandPalette
          open={paletteOpen}
          commands={switchingTabs ? tabCommands : commands}
          placeholder={switchingTabs ? "Type part of a tab's name, folder or shell" : undefined}
          empty={switchingTabs ? "No tab matches" : undefined}
          onClose={() => { setPaletteOpen(false); setSwitchingTabs(false); closeOverlayFocus(); }}
        />
      </Suspense>

      {confirmation && (
        <div className="overlay overlay--dialog" role="presentation" onMouseDown={() => setConfirmation(null)}>
          <div className="dialog" role="alertdialog" aria-modal="true" aria-labelledby="confirm-title" onMouseDown={(event) => event.stopPropagation()} onKeyDown={(event) => event.key === "Escape" && setConfirmation(null)}>
            <h2 id="confirm-title">{confirmation.title}</h2>
            <p>{confirmation.body}</p>
            <form
              className="dialog__form"
              onSubmit={(event) => {
                event.preventDefault();
                const form = new FormData(event.currentTarget);
                const values = (confirmation.inputs ?? []).map((_, index) => String(form.get(`value-${index}`) ?? "").trim());
                if (values.some((value) => !value)) return;
                const { run } = confirmation;
                setConfirmation(null);
                run(values);
              }}
            >
              {confirmation.inputs?.map((input, index) => (
                <label className="dialog__field" key={input.label}>
                  <span>{input.label}</span>
                  <input className="field" name={`value-${index}`} defaultValue={input.value} maxLength={input.maxLength ?? 60} required spellCheck={false} autoFocus={index === 0} onFocus={(event) => event.currentTarget.select()} />
                </label>
              ))}
              <div className="dialog__actions">
                <button className="button" type="button" onClick={() => setConfirmation(null)}>Cancel</button>
                <button className="button button--primary" type="submit" autoFocus={!confirmation.inputs?.length}>{confirmation.action}</button>
              </div>
            </form>
          </div>
        </div>
      )}

      {notice && <div className="toast" role="status">{notice}</div>}
      {update && <UpdateBanner update={update} onDismiss={() => { dismissUpdate(update.version); setUpdate(null); }} />}
    </main>
  );
}

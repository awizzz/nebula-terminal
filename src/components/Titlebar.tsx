import { useLayoutEffect, useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ChevronDown, ChevronsUp, Eye, Hourglass, Plus, Search, Settings2, ShieldAlert, X } from "lucide-react";
import ProfileIcon from "./ProfileIcon";
import { tabColorValue } from "../themes";
import type { TerminalTab } from "../types";
import type { Progress } from "../progress";

interface TitlebarProps {
  tabs: TerminalTab[];
  activeTabId: string;
  activity: ReadonlySet<string>;
  /** Tabs where a long command finished in the background, and whether it succeeded. */
  finished: ReadonlyMap<string, boolean>;
  closing: ReadonlySet<string>;
  /** The tab whose name is being edited in place. */
  renamingId: string | null;
  /** What programs report with OSC 9;4, by tab. */
  progress?: ReadonlyMap<string, Progress>;
  newTabShortcut: string;
  paletteShortcut: string;
  settingsShortcut: string;
  /** Every shell in this window runs as administrator. */
  administrator?: boolean;
  /** The quick terminal: one button sends it back up instead of the window controls. */
  quick?: boolean;
  onHide?: () => void;
  onSelectTab: (id: string) => void;
  onCloseTab: (id: string) => void;
  onMoveTab: (fromId: string, toId: string) => void;
  /** A tab dropped outside the window: to another window, or a new one. */
  onTearOff?: (tabId: string) => void;
  onNewTab: () => void;
  onOpenProfileMenu: (x: number, y: number) => void;
  onTabContextMenu: (tabId: string, x: number, y: number) => void;
  onRenameStart: (tabId: string) => void;
  /** An empty name gives the tab back its automatic title. */
  onRename: (tabId: string, name: string) => void;
  onRenameEnd: () => void;
  onOpenPalette: () => void;
  onOpenSettings: () => void;
}

const progressLabels: Record<Progress["state"], string> = { normal: "In progress", error: "Failed", indeterminate: "Working", paused: "Paused" };

function TabProgress({ progress }: { progress: Progress }) {
  const percent = progress.state === "indeterminate" ? undefined : progress.value;
  return (
    <span
      className={`tab__progress is-${progress.state}`}
      role="progressbar"
      aria-label={progressLabels[progress.state]}
      aria-valuenow={percent}
      aria-valuemin={0}
      aria-valuemax={100}
      style={{ "--progress": `${progress.value}%` } as CSSProperties}
    />
  );
}

export function tabTitle(tab: TerminalTab): string {
  return tab.customTitle ?? tab.title;
}

/** Inline editor for a tab name: Enter or clicking away keeps it, Escape cancels. */
function TabRename({ tab, onCommit, onCancel }: { tab: TerminalTab; onCommit: (name: string) => void; onCancel: () => void }) {
  const initial = tabTitle(tab);
  const [value, setValue] = useState(initial);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const doneRef = useRef(false);

  useLayoutEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  const finish = (commit: boolean) => {
    if (doneRef.current) return;
    doneRef.current = true;
    // Confirming the unchanged name must not freeze the shell's title.
    if (commit && value !== initial) onCommit(value);
    else onCancel();
  };

  return (
    <input
      ref={inputRef}
      className="tab__rename"
      value={value}
      maxLength={120}
      spellCheck={false}
      placeholder={tab.title}
      aria-label="Tab name"
      onChange={(event) => setValue(event.target.value)}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === "Enter") { event.preventDefault(); finish(true); }
        else if (event.key === "Escape") { event.preventDefault(); finish(false); }
      }}
      onBlur={() => finish(true)}
      onPointerDown={(event) => event.stopPropagation()}
      onDoubleClick={(event) => event.stopPropagation()}
      onContextMenu={(event) => event.stopPropagation()}
    />
  );
}

function WindowIcon({ type }: { type: "min" | "max" | "close" }) {
  if (type === "min") return <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5h10" /></svg>;
  if (type === "max") return <svg viewBox="0 0 10 10" aria-hidden="true"><rect x=".5" y=".5" width="9" height="9" rx="1.2" /></svg>;
  return <svg viewBox="0 0 10 10" aria-hidden="true"><path d="m.5.5 9 9m0-9-9 9" /></svg>;
}

export default function Titlebar({
  tabs,
  activeTabId,
  activity,
  finished,
  closing,
  renamingId,
  progress,
  newTabShortcut,
  administrator = false,
  quick = false,
  onHide,
  paletteShortcut,
  settingsShortcut,
  onSelectTab,
  onCloseTab,
  onMoveTab,
  onTearOff,
  onNewTab,
  onOpenProfileMenu,
  onTabContextMenu,
  onRenameStart,
  onRename,
  onRenameEnd,
  onOpenPalette,
  onOpenSettings,
}: TitlebarProps) {
  const menuButtonRef = useRef<HTMLButtonElement | null>(null);
  const stripRef = useRef<HTMLDivElement | null>(null);
  const suppressClickRef = useRef(false);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [tearingId, setTearingId] = useState<string | null>(null);

  // Pointer-driven reordering: WebView2 keeps HTML5 drag-and-drop for native file drops.
  const startDrag = (event: ReactPointerEvent, tabId: string) => {
    if (event.button !== 0 || (event.target as HTMLElement).closest(".tab__close, .tab__rename")) return;
    const startX = event.clientX;
    let dragging = false;
    let outside = false;
    const move = (moveEvent: PointerEvent) => {
      if (!dragging) {
        if (Math.abs(moveEvent.clientX - startX) < 6 && Math.abs(moveEvent.clientY - event.clientY) < 6) return;
        dragging = true;
        setDraggingId(tabId);
        onSelectTab(tabId);
      }
      // Well below the tabs, or out of the window: the tab is leaving it.
      const strip = stripRef.current?.getBoundingClientRect();
      const leaving = onTearOff !== undefined && (moveEvent.clientY > (strip?.bottom ?? 40) + 48 || moveEvent.clientY < -8 || moveEvent.clientX < -8 || moveEvent.clientX > window.innerWidth + 8);
      if (leaving !== outside) {
        outside = leaving;
        setTearingId(leaving ? tabId : null);
      }
      if (outside) return;
      const target = [...(stripRef.current?.querySelectorAll<HTMLElement>(".tab[data-tab-id]") ?? [])].find((element) => {
        const rect = element.getBoundingClientRect();
        return moveEvent.clientX >= rect.left && moveEvent.clientX <= rect.right;
      });
      const targetId = target?.dataset.tabId;
      if (targetId && targetId !== tabId) onMoveTab(tabId, targetId);
    };
    const finish = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
      if (dragging) {
        suppressClickRef.current = true;
        setDraggingId(null);
        setTearingId(null);
        requestAnimationFrame(() => { suppressClickRef.current = false; });
        if (outside) onTearOff?.(tabId);
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", finish);
  };
  const native = isTauri();

  const windowAction = (action: "minimize" | "maximize" | "close") => {
    if (!native) return;
    const win = getCurrentWindow();
    void (action === "minimize" ? win.minimize() : action === "maximize" ? win.toggleMaximize() : win.close());
  };

  return (
    <header className="titlebar">
      {administrator && (
        <span className="admin-badge" data-tauri-drag-region title="Every shell in this window runs as administrator">
          <ShieldAlert size={14} strokeWidth={2} aria-hidden="true" />
          Administrator
        </span>
      )}
      <div ref={stripRef} className="tab-strip" role="tablist" aria-label="Terminal tabs" data-tauri-drag-region>
        {tabs.map((tab, index) => {
          const pane = tab.panes.find((candidate) => candidate.id === tab.activePaneId) ?? tab.panes[0];
          const active = tab.id === activeTabId;
          const nextActive = tabs[index + 1]?.id === activeTabId;
          const title = tabTitle(tab);
          const color = tabColorValue(tab.color);
          const renaming = tab.id === renamingId && !closing.has(tab.id);
          const details = (
            <>
              {pane && <ProfileIcon kind={pane.profile.kind} accent={pane.profile.accent} size={15} />}
              {renaming
                ? <TabRename tab={tab} onCommit={(name) => { onRename(tab.id, name); onRenameEnd(); }} onCancel={onRenameEnd} />
                : <span className="tab__title">{title}</span>}
              {tab.panes.length > 1 && (
                <span className={`tab__panes ${tab.zoomedPaneId ? "is-zoomed" : ""}`} aria-label={tab.zoomedPaneId ? `1 of ${tab.panes.length} panes shown` : `${tab.panes.length} panes`} title={tab.zoomedPaneId ? "One pane fills the tab" : undefined}>
                  {tab.zoomedPaneId ? `1/${tab.panes.length}` : tab.panes.length}
                </span>
              )}
              {tab.watch && (
                <span className="tab__watch" role="img" aria-label={tab.watch === "output" ? "Watching for output" : "Watching for silence"} title={tab.watch === "output" ? "Watching for output" : "Watching for silence"}>
                  {tab.watch === "output" ? <Eye size={12} strokeWidth={2} /> : <Hourglass size={12} strokeWidth={2} />}
                </span>
              )}
              {!active && finished.has(tab.id)
                ? <span className={`tab__done ${finished.get(tab.id) ? "" : "is-failed"}`} aria-label={finished.get(tab.id) ? "Command finished" : "Command failed"} />
                : activity.has(tab.id) && !active && <span className="tab__activity" aria-label="New output" />}
            </>
          );
          return (
            <div
              key={tab.id}
              data-tab-id={closing.has(tab.id) ? undefined : tab.id}
              className={[
                "tab",
                active && "is-active",
                nextActive && "is-before-active",
                closing.has(tab.id) && "is-closing",
                draggingId === tab.id && "is-dragging",
                tearingId === tab.id && "is-tearing",
                color && "has-color",
              ].filter(Boolean).join(" ")}
              style={color ? { "--tab-color": color } as CSSProperties : undefined}
              onPointerDown={(event) => startDrag(event, tab.id)}
              onAuxClick={(event) => {
                if (event.button === 1) {
                  event.preventDefault();
                  onCloseTab(tab.id);
                }
              }}
              onContextMenu={(event) => {
                event.preventDefault();
                onTabContextMenu(tab.id, event.clientX, event.clientY);
              }}
            >
              {color && <span className="tab__color" aria-hidden="true" />}
              {progress?.has(tab.id) && <TabProgress progress={progress.get(tab.id)!} />}
              {renaming ? (
                <div className="tab__button is-renaming">{details}</div>
              ) : (
                <button
                  className="tab__button"
                  type="button"
                  role="tab"
                  aria-selected={active}
                  title={title}
                  onClick={() => !suppressClickRef.current && onSelectTab(tab.id)}
                  onDoubleClick={() => onRenameStart(tab.id)}
                >
                  {details}
                </button>
              )}
              <button className="tab__close" type="button" tabIndex={-1} onClick={() => onCloseTab(tab.id)} aria-label={`Close ${title}`}>
                <X size={13} strokeWidth={1.8} />
              </button>
            </div>
          );
        })}

        <div className="new-tab">
          <button type="button" onClick={onNewTab} aria-label="New tab" title={`New tab (${newTabShortcut})`}><Plus size={16} strokeWidth={1.7} /></button>
          <button
            ref={menuButtonRef}
            type="button"
            aria-label="Open a profile"
            aria-haspopup="menu"
            title="Open a profile"
            onClick={() => {
              const rect = menuButtonRef.current?.getBoundingClientRect();
              if (rect) onOpenProfileMenu(rect.left, rect.bottom + 4);
            }}
          >
            <ChevronDown size={14} strokeWidth={1.8} />
          </button>
        </div>
      </div>

      <div className="titlebar__actions">
        <button className="icon-button" type="button" onClick={onOpenPalette} aria-label="Command palette" title={`Command palette (${paletteShortcut})`}><Search size={15} strokeWidth={1.8} /></button>
        <button className="icon-button" type="button" onClick={onOpenSettings} aria-label="Settings" title={`Settings (${settingsShortcut})`}><Settings2 size={15} strokeWidth={1.8} /></button>
      </div>

      {quick ? (
        <div className="window-controls">
          <button type="button" onClick={onHide} aria-label="Hide the quick terminal" title="Hide" tabIndex={-1}><ChevronsUp size={16} strokeWidth={1.7} /></button>
        </div>
      ) : (
        <div className="window-controls">
          <button type="button" onClick={() => windowAction("minimize")} aria-label="Minimize" tabIndex={-1}><WindowIcon type="min" /></button>
          <button type="button" onClick={() => windowAction("maximize")} aria-label="Maximize" tabIndex={-1}><WindowIcon type="max" /></button>
          <button className="window-controls__close" type="button" onClick={() => windowAction("close")} aria-label="Close" tabIndex={-1}><WindowIcon type="close" /></button>
        </div>
      )}
    </header>
  );
}

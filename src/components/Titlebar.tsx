import { useLayoutEffect, useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ChevronDown, Plus, Search, Settings2, X } from "lucide-react";
import ProfileIcon from "./ProfileIcon";
import { tabColorValue } from "../themes";
import type { TerminalTab } from "../types";

interface TitlebarProps {
  tabs: TerminalTab[];
  activeTabId: string;
  activity: ReadonlySet<string>;
  /** Tabs where a long command finished in the background, and whether it succeeded. */
  finished: ReadonlyMap<string, boolean>;
  closing: ReadonlySet<string>;
  /** The tab whose name is being edited in place. */
  renamingId: string | null;
  newTabShortcut: string;
  paletteShortcut: string;
  settingsShortcut: string;
  onSelectTab: (id: string) => void;
  onCloseTab: (id: string) => void;
  onMoveTab: (fromId: string, toId: string) => void;
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
  newTabShortcut,
  paletteShortcut,
  settingsShortcut,
  onSelectTab,
  onCloseTab,
  onMoveTab,
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

  // Pointer-driven reordering: WebView2 keeps HTML5 drag-and-drop for native file drops.
  const startDrag = (event: ReactPointerEvent, tabId: string) => {
    if (event.button !== 0 || (event.target as HTMLElement).closest(".tab__close, .tab__rename")) return;
    const startX = event.clientX;
    let dragging = false;
    const move = (moveEvent: PointerEvent) => {
      if (!dragging) {
        if (Math.abs(moveEvent.clientX - startX) < 6) return;
        dragging = true;
        setDraggingId(tabId);
        onSelectTab(tabId);
      }
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
        requestAnimationFrame(() => { suppressClickRef.current = false; });
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
              {tab.panes.length > 1 && <span className="tab__panes" aria-label={`${tab.panes.length} panes`}>{tab.panes.length}</span>}
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

      <div className="window-controls">
        <button type="button" onClick={() => windowAction("minimize")} aria-label="Minimize" tabIndex={-1}><WindowIcon type="min" /></button>
        <button type="button" onClick={() => windowAction("maximize")} aria-label="Maximize" tabIndex={-1}><WindowIcon type="max" /></button>
        <button className="window-controls__close" type="button" onClick={() => windowAction("close")} aria-label="Close" tabIndex={-1}><WindowIcon type="close" /></button>
      </div>
    </header>
  );
}

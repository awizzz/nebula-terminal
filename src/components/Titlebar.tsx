import { useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ChevronDown, Plus, Search, Settings2, X } from "lucide-react";
import ProfileIcon from "./ProfileIcon";
import type { TerminalTab } from "../types";

interface TitlebarProps {
  tabs: TerminalTab[];
  activeTabId: string;
  activity: ReadonlySet<string>;
  closing: ReadonlySet<string>;
  newTabShortcut: string;
  paletteShortcut: string;
  settingsShortcut: string;
  onSelectTab: (id: string) => void;
  onCloseTab: (id: string) => void;
  onMoveTab: (fromId: string, toId: string) => void;
  onNewTab: () => void;
  onOpenProfileMenu: (x: number, y: number) => void;
  onTabContextMenu: (tabId: string, x: number, y: number) => void;
  onOpenPalette: () => void;
  onOpenSettings: () => void;
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
  closing,
  newTabShortcut,
  paletteShortcut,
  settingsShortcut,
  onSelectTab,
  onCloseTab,
  onMoveTab,
  onNewTab,
  onOpenProfileMenu,
  onTabContextMenu,
  onOpenPalette,
  onOpenSettings,
}: TitlebarProps) {
  const menuButtonRef = useRef<HTMLButtonElement | null>(null);
  const stripRef = useRef<HTMLDivElement | null>(null);
  const suppressClickRef = useRef(false);
  const [draggingId, setDraggingId] = useState<string | null>(null);

  // Pointer-driven reordering: WebView2 keeps HTML5 drag-and-drop for native file drops.
  const startDrag = (event: ReactPointerEvent, tabId: string) => {
    if (event.button !== 0 || (event.target as HTMLElement).closest(".tab__close")) return;
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
              ].filter(Boolean).join(" ")}
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
              <button
                className="tab__button"
                type="button"
                role="tab"
                aria-selected={active}
                title={tab.title}
                onClick={() => !suppressClickRef.current && onSelectTab(tab.id)}
              >
                {pane && <ProfileIcon kind={pane.profile.kind} size={15} />}
                <span className="tab__title">{tab.title}</span>
                {tab.panes.length > 1 && <span className="tab__panes" aria-label={`${tab.panes.length} panes`}>{tab.panes.length}</span>}
                {activity.has(tab.id) && !active && <span className="tab__activity" aria-label="New output" />}
              </button>
              <button className="tab__close" type="button" tabIndex={-1} onClick={() => onCloseTab(tab.id)} aria-label={`Close ${tab.title}`}>
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
            aria-label="Open a specific shell"
            aria-haspopup="menu"
            title="Open a specific shell"
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

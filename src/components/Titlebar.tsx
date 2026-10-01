import { useRef } from "react";
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
  const native = isTauri();

  const windowAction = (action: "minimize" | "maximize" | "close") => {
    if (!native) return;
    const win = getCurrentWindow();
    void (action === "minimize" ? win.minimize() : action === "maximize" ? win.toggleMaximize() : win.close());
  };

  return (
    <header className="titlebar">
      <div className="tab-strip" role="tablist" aria-label="Terminal tabs" data-tauri-drag-region>
        {tabs.map((tab, index) => {
          const pane = tab.panes.find((candidate) => candidate.id === tab.activePaneId) ?? tab.panes[0];
          const active = tab.id === activeTabId;
          const nextActive = tabs[index + 1]?.id === activeTabId;
          return (
            <div
              key={tab.id}
              draggable={!closing.has(tab.id)}
              className={[
                "tab",
                active && "is-active",
                nextActive && "is-before-active",
                closing.has(tab.id) && "is-closing",
              ].filter(Boolean).join(" ")}
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
              onDragStart={(event) => {
                event.dataTransfer.effectAllowed = "move";
                event.dataTransfer.setData("text/nebula-tab", tab.id);
              }}
              onDragOver={(event) => {
                if (event.dataTransfer.types.includes("text/nebula-tab")) event.preventDefault();
              }}
              onDrop={(event) => {
                const fromId = event.dataTransfer.getData("text/nebula-tab");
                if (fromId && fromId !== tab.id) onMoveTab(fromId, tab.id);
              }}
            >
              <button
                className="tab__button"
                type="button"
                role="tab"
                aria-selected={active}
                title={tab.title}
                onClick={() => onSelectTab(tab.id)}
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

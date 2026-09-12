import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ChevronDown, Command, Plus, Settings, X } from "lucide-react";
import nebulaMark from "../assets/nebula-mark.png";
import type { TerminalTab } from "../types";

interface TitlebarProps {
  tabs: TerminalTab[];
  activeTabId: string;
  onSelectTab: (id: string) => void;
  onCloseTab: (id: string) => void;
  onMoveTab: (fromId: string, toId: string) => void;
  onNewTab: () => void;
  onOpenProfileMenu: () => void;
  onOpenPalette: () => void;
  onOpenSettings: () => void;
}

function WindowIcon({ type }: { type: "min" | "max" | "close" }) {
  if (type === "min") return <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M2 6.5h8" /></svg>;
  if (type === "max") return <svg viewBox="0 0 12 12" aria-hidden="true"><rect x="2.5" y="2.5" width="7" height="7" rx=".5" /></svg>;
  return <svg viewBox="0 0 12 12" aria-hidden="true"><path d="m2.5 2.5 7 7m0-7-7 7" /></svg>;
}

export default function Titlebar({
  tabs,
  activeTabId,
  onSelectTab,
  onCloseTab,
  onMoveTab,
  onNewTab,
  onOpenProfileMenu,
  onOpenPalette,
  onOpenSettings,
}: TitlebarProps) {
  const invokeWindowAction = async (action: "minimize" | "maximize" | "close") => {
    if (!isTauri()) return;
    const win = getCurrentWindow();
    if (action === "minimize") await win.minimize();
    if (action === "maximize") await win.toggleMaximize();
    if (action === "close") await win.close();
  };

  return (
    <header className="titlebar">
      <div className="brand" data-tauri-drag-region>
        <img className="brand-mark" src={nebulaMark} alt="" />
        <span className="brand-name">Nebula</span>
      </div>

      <div className="tab-strip" role="tablist" aria-label="Terminal tabs">
        {tabs.map((tab) => {
          const activePane = tab.panes.find((pane) => pane.id === tab.activePaneId) ?? tab.panes[0];
          const active = tab.id === activeTabId;
          return (
            <div
              key={tab.id}
              draggable
              className={`tab-item ${active ? "tab-item--active" : ""}`}
              onAuxClick={(event) => {
                if (event.button === 1) {
                  event.preventDefault();
                  onCloseTab(tab.id);
                }
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
              <button className="tab" type="button" role="tab" aria-selected={active} onClick={() => onSelectTab(tab.id)}>
                <span className="tab-profile-dot" style={{ background: activePane?.profile.accent }} />
                <span className="tab-title">{tab.title}</span>
                {tab.panes.length > 1 && <span className="tab-pane-count">{tab.panes.length}</span>}
              </button>
              <button className="tab-close" type="button" onClick={() => onCloseTab(tab.id)} aria-label={`Close ${tab.title}`}><X size={12} /></button>
              {active && <span className="tab-orbit" aria-hidden="true" />}
            </div>
          );
        })}
        <div className="tab-add-group">
          <button className="icon-button tab-add" type="button" onClick={onNewTab} aria-label="New tab" title="New tab (Ctrl+Shift+T)"><Plus size={15} /></button>
          <button className="icon-button tab-menu" type="button" onClick={onOpenProfileMenu} aria-label="Choose profile" title="Choose terminal profile"><ChevronDown size={12} /></button>
        </div>
      </div>

      <div className="titlebar-drag" data-tauri-drag-region />

      <div className="titlebar-actions">
        <button className="icon-button" type="button" onClick={onOpenPalette} aria-label="Open command palette" title="Command palette (Ctrl+Shift+P)"><Command size={15} /></button>
        <button className="icon-button" type="button" onClick={onOpenSettings} aria-label="Open settings" title="Settings (Ctrl+,)"><Settings size={15} /></button>
      </div>

      <div className="window-controls">
        <button type="button" onClick={() => void invokeWindowAction("minimize")} aria-label="Minimize"><WindowIcon type="min" /></button>
        <button type="button" onClick={() => void invokeWindowAction("maximize")} aria-label="Maximize"><WindowIcon type="max" /></button>
        <button className="window-close" type="button" onClick={() => void invokeWindowAction("close")} aria-label="Close"><WindowIcon type="close" /></button>
      </div>
    </header>
  );
}

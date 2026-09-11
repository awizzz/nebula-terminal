import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import Titlebar from "./components/Titlebar";
import TerminalPane from "./components/TerminalPane";
import SettingsPanel from "./components/SettingsPanel";
import CommandPalette, { type PaletteCommand } from "./components/CommandPalette";
import { defaultPreferences, loadPreferences, savePreferences } from "./preferences";
import type { AppearancePreferences, TerminalProfile, TerminalTab } from "./types";

const previewProfiles: TerminalProfile[] = [
  { id: "nebula", name: "Nebula", kind: "nebula", available: true, accent: "#8b7cf6" },
  { id: "cmd", name: "Command Prompt", kind: "cmd", available: true, executable: "cmd.exe", accent: "#78dba9" },
  { id: "powershell", name: "Windows PowerShell", kind: "powershell", available: true, executable: "powershell.exe", accent: "#72a9f7" },
  { id: "pwsh", name: "PowerShell 7", kind: "pwsh", available: false, accent: "#a98df4" },
  { id: "wsl", name: "WSL", kind: "wsl", available: false, accent: "#f2c877" },
];

function makeTab(profile: TerminalProfile): TerminalTab {
  return {
    id: crypto.randomUUID(),
    title: profile.name,
    profile,
  };
}

export default function App() {
  const [profiles, setProfiles] = useState<TerminalProfile[]>(previewProfiles);
  const [tabs, setTabs] = useState<TerminalTab[]>([makeTab(previewProfiles[0]!)]);
  const [activeTabId, setActiveTabId] = useState(() => tabs[0]!.id);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [preferences, setPreferences] = useState<AppearancePreferences>(() => loadPreferences());

  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0];

  useEffect(() => {
    if (!isTauri()) return;
    void invoke<TerminalProfile[]>("detect_profiles")
      .then((detected) => setProfiles(detected))
      .catch(() => setProfiles(previewProfiles));
  }, []);

  useEffect(() => {
    savePreferences(preferences);
    document.documentElement.style.setProperty("--accent", preferences.accent);
    document.documentElement.dataset.animation = preferences.animationLevel;
    document.documentElement.dataset.tabDensity = preferences.tabDensity;
    if (isTauri()) {
      void invoke("set_window_effect", {
        mode: preferences.backgroundMode,
        dark: true,
      }).catch(() => undefined);
    }
  }, [preferences]);

  const openNewTab = useCallback((profileId = "nebula") => {
    const profile = profiles.find((item) => item.id === profileId && item.available)
      ?? profiles.find((item) => item.id === "nebula")
      ?? profiles.find((item) => item.available);
    if (!profile) return;
    const next = makeTab(profile);
    setTabs((current) => [...current, next]);
    setActiveTabId(next.id);
  }, [profiles]);

  const closeTab = useCallback((id: string) => {
    setTabs((current) => {
      if (current.length === 1) return current;
      const index = current.findIndex((tab) => tab.id === id);
      const next = current.filter((tab) => tab.id !== id);
      if (id === activeTabId) {
        const replacement = next[Math.min(index, next.length - 1)];
        if (replacement) setActiveTabId(replacement.id);
      }
      return next;
    });
  }, [activeTabId]);

  const commands = useMemo<PaletteCommand[]>(() => [
    { id: "new-nebula", label: "New Nebula tab", detail: "Open a native Nebula Shell session", shortcut: "Ctrl+Shift+T", run: () => openNewTab("nebula") },
    ...profiles.filter((profile) => profile.available && profile.id !== "nebula").map((profile) => ({
      id: `new-${profile.id}`,
      label: `New ${profile.name} tab`,
      detail: profile.executable,
      run: () => openNewTab(profile.id),
    })),
    { id: "settings", label: "Open appearance settings", detail: "Theme, type, opacity and window effect", shortcut: "Ctrl+,", run: () => setSettingsOpen(true) },
    { id: "reset-appearance", label: "Reset appearance", detail: "Restore the Nebula default theme", run: () => setPreferences(defaultPreferences) },
  ], [openNewTab, profiles]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "p") {
        event.preventDefault();
        setPaletteOpen(true);
      }
      if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "t") {
        event.preventDefault();
        openNewTab();
      }
      if (event.ctrlKey && event.key === ",") {
        event.preventDefault();
        setSettingsOpen(true);
      }
      if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "w" && activeTab) {
        event.preventDefault();
        closeTab(activeTab.id);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [activeTab, closeTab, openNewTab]);

  return (
    <main className={`app-shell app-shell--${preferences.backgroundMode}`}>
      <Titlebar
        tabs={tabs}
        activeTabId={activeTabId}
        onSelectTab={setActiveTabId}
        onCloseTab={closeTab}
        onNewTab={() => openNewTab()}
        onOpenPalette={() => setPaletteOpen(true)}
        onOpenSettings={() => setSettingsOpen(true)}
      />

      <div className="workspace">
        <div className="terminal-stack">
          {tabs.map((tab) => (
            <div key={tab.id} className={`terminal-layer ${tab.id === activeTabId ? "terminal-layer--active" : ""}`}>
              <TerminalPane
                tabId={tab.id}
                profile={tab.profile}
                preferences={preferences}
                focused={tab.id === activeTabId && !settingsOpen && !paletteOpen}
              />
            </div>
          ))}
        </div>

        <footer className="statusbar">
          <div className="status-left">
            <span className="status-dot" />
            <span>{activeTab?.profile.name ?? "Nebula"}</span>
            <span className="status-divider" />
            <span>{isTauri() ? "Native PTY" : "UI preview"}</span>
          </div>
          <div className="status-right">
            <button type="button" onClick={() => setPaletteOpen(true)}>Ctrl Shift P</button>
            <button type="button" onClick={() => setSettingsOpen(true)}>Appearance</button>
          </div>
        </footer>
      </div>

      <SettingsPanel
        open={settingsOpen}
        profiles={profiles}
        preferences={preferences}
        onChange={setPreferences}
        onClose={() => setSettingsOpen(false)}
        onReset={() => setPreferences(defaultPreferences)}
      />
      {settingsOpen && <button className="settings-scrim" type="button" onClick={() => setSettingsOpen(false)} aria-label="Close settings" />}

      <CommandPalette open={paletteOpen} commands={commands} onClose={() => setPaletteOpen(false)} />
    </main>
  );
}

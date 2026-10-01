import { useEffect, useRef, useState } from "react";
import { Brush, Command, MonitorCog, RotateCcw, SlidersHorizontal, TerminalSquare, X } from "lucide-react";
import { exportAppearance, importAppearance, themePresets } from "../preferences";
import nebulaMark from "../assets/nebula-mark.png";
import type { AppearancePreferences, KeybindingPreferences, TerminalProfile } from "../types";

interface SettingsPanelProps {
  open: boolean;
  profiles: TerminalProfile[];
  preferences: AppearancePreferences;
  onChange: (next: AppearancePreferences) => void;
  onClose: () => void;
  onReset: () => void;
  onClearSession: () => void;
}

type SettingsPage = "appearance" | "terminal" | "profiles" | "keybindings" | "advanced";

const pages: Array<{ id: SettingsPage; label: string; description: string }> = [
  { id: "appearance", label: "Appearance", description: "Color, light and motion" },
  { id: "terminal", label: "Terminal", description: "Type and cursor" },
  { id: "profiles", label: "Profiles", description: "Shells on this computer" },
  { id: "keybindings", label: "Shortcuts", description: "Keyboard commands" },
  { id: "advanced", label: "Workspace", description: "Memory and safeguards" },
];

const pageIcons = { appearance: Brush, terminal: TerminalSquare, profiles: MonitorCog, keybindings: Command, advanced: SlidersHorizontal };

function SectionTitle({ title, description }: { title: string; description: string }) {
  return <div className="settings-heading"><h3>{title}</h3><p>{description}</p></div>;
}

function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (value: boolean) => void; label: string }) {
  return (
    <button className={`toggle ${checked ? "toggle--on" : ""}`} type="button" role="switch" aria-checked={checked} aria-label={label} onClick={() => onChange(!checked)}>
      <span />
    </button>
  );
}

export default function SettingsPanel({ open, profiles, preferences, onChange, onClose, onReset, onClearSession }: SettingsPanelProps) {
  const [page, setPage] = useState<SettingsPage>("appearance");
  const [importError, setImportError] = useState<string | null>(null);
  const themeInputRef = useRef<HTMLInputElement | null>(null);
  const imageInputRef = useRef<HTMLInputElement | null>(null);
  const panelRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", closeOnEscape);
    requestAnimationFrame(() => panelRef.current?.querySelector<HTMLButtonElement>(".settings-close")?.focus());
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [onClose, open]);

  if (!open) return null;

  const patch = <K extends keyof AppearancePreferences>(key: K, value: AppearancePreferences[K]) => onChange({ ...preferences, [key]: value });
  const patchBinding = (key: keyof KeybindingPreferences, value: string) => onChange({
    ...preferences,
    keybindings: { ...preferences.keybindings, [key]: value },
  });

  const importTheme = async (file?: File) => {
    if (!file) return;
    try {
      setImportError(null);
      onChange(await importAppearance(file, preferences));
    } catch (error) {
      setImportError(String(error));
    }
  };

  const readBackground = (file?: File) => {
    if (!file) return;
    if (!file.type.startsWith("image/")) {
      setImportError("Choose an image file.");
      return;
    }
    if (file.size > 5_000_000) {
      setImportError("Choose an image smaller than 5 MB.");
      return;
    }
    setImportError(null);
    const reader = new FileReader();
    reader.onload = () => onChange({ ...preferences, backgroundMode: "image", backgroundImage: String(reader.result ?? "") });
    reader.onerror = () => setImportError("This image could not be read.");
    reader.readAsDataURL(file);
  };

  return (
    <aside ref={panelRef} className="settings-panel settings-panel--wide" aria-label="Settings" role="dialog" aria-modal="true">
      <div className="settings-sidebar">
        <div className="settings-brand"><img src={nebulaMark} alt="" /><div><strong>Nebula Terminal</strong><small>Version 0.2.0</small></div></div>
        <nav>
          {pages.map((item) => {
            const Icon = pageIcons[item.id];
            return (
            <button key={item.id} className={page === item.id ? "selected" : ""} type="button" onClick={() => setPage(item.id)}>
              <Icon size={15} aria-hidden="true" /><span><strong>{item.label}</strong><small>{item.description}</small></span>
            </button>
          );})}
        </nav>
        <div className="settings-sidebar-footer"><button type="button" onClick={onReset}><RotateCcw size={13} />Reset settings</button></div>
      </div>

      <div className="settings-content">
        <div className="settings-header">
          <div><span className="eyebrow">Settings</span><h2>{pages.find((item) => item.id === page)?.label}</h2></div>
          <button className="icon-button settings-close" type="button" onClick={onClose} aria-label="Close settings"><X size={17} /></button>
        </div>

        <div className="settings-scroll">
          {page === "appearance" && <>
            <section className="settings-section">
              <SectionTitle title="Light signatures" description="Each palette keeps the same Nebula identity with a different temperature." />
              <div className="theme-grid">
                {themePresets.map((theme) => (
                  <button key={theme.id} type="button" className={`theme-card ${preferences.themeId === theme.id ? "selected" : ""}`} onClick={() => onChange({ ...preferences, themeId: theme.id, accent: theme.accent })}>
                    <span className="theme-preview" style={{ background: theme.background }}>
                      <i style={{ background: theme.red }} /><i style={{ background: theme.green }} /><i style={{ background: theme.yellow }} /><i style={{ background: theme.blue }} /><i style={{ background: theme.magenta }} />
                    </span>
                    <strong>{theme.name}</strong>
                  </button>
                ))}
              </div>
            </section>

            <section className="settings-section">
              <SectionTitle title="Window" description="Shape the frame around the terminal without weakening text contrast." />
              <div className="setting-row">
                <div><strong>Background</strong><span>Mica falls back gracefully on unsupported systems.</span></div>
                <div className="segmented" role="group" aria-label="Background mode">
                  {(["mica", "solid", "image"] as const).map((mode) => <button key={mode} className={preferences.backgroundMode === mode ? "selected" : ""} type="button" onClick={() => patch("backgroundMode", mode)}>{mode}</button>)}
                </div>
              </div>
              <label className="setting-row setting-row--stacked">
                <div><strong>Accent</strong><span>Focus, active tabs, cursor and selection.</span></div>
                <div className="accent-control"><input type="color" value={preferences.accent} onChange={(event) => patch("accent", event.target.value)} /><code>{preferences.accent.toUpperCase()}</code></div>
              </label>
              <div className="setting-row">
                <div><strong>Background image</strong><span>Stored locally on this device.</span></div>
                <div className="button-row"><button className="secondary-button" type="button" onClick={() => imageInputRef.current?.click()}>Choose image</button>{preferences.backgroundImage && <button className="ghost-button" type="button" onClick={() => onChange({ ...preferences, backgroundImage: undefined, backgroundMode: "solid" })}>Remove</button>}</div>
                <input ref={imageInputRef} hidden type="file" accept="image/*" onChange={(event) => readBackground(event.target.files?.[0])} />
              </div>
              {preferences.backgroundMode === "image" && <label className="setting-row"><div><strong>Image opacity</strong><span>{Math.round(preferences.backgroundImageOpacity * 100)}%</span></div><input type="range" min="0.08" max="0.8" step="0.01" value={preferences.backgroundImageOpacity} onChange={(event) => patch("backgroundImageOpacity", Number(event.target.value))} /></label>}
              <label className="setting-row"><div><strong>Atmosphere</strong><span>{Math.round(preferences.atmosphere * 100)}% · glow around the workspace</span></div><input type="range" min="0" max="1" step="0.01" value={preferences.atmosphere} onChange={(event) => patch("atmosphere", Number(event.target.value))} /></label>
              <div className="setting-row"><div><strong>Animations</strong><span>Reduce motion without changing the layout.</span></div><select value={preferences.animationLevel} onChange={(event) => patch("animationLevel", event.target.value as AppearancePreferences["animationLevel"])}><option value="full">Full</option><option value="reduced">Reduced</option><option value="off">Off</option></select></div>
              <div className="setting-row"><div><strong>Tab density</strong><span>Compact gives more room to terminal sessions.</span></div><select value={preferences.tabDensity} onChange={(event) => patch("tabDensity", event.target.value as AppearancePreferences["tabDensity"])}><option value="comfortable">Comfortable</option><option value="compact">Compact</option></select></div>
            </section>

            <section className="settings-section">
              <SectionTitle title="Theme files" description="Move your look between computers without touching config files." />
              <div className="button-row"><button className="secondary-button" type="button" onClick={() => exportAppearance(preferences)}>Export theme</button><button className="secondary-button" type="button" onClick={() => themeInputRef.current?.click()}>Import theme</button></div>
              <input ref={themeInputRef} hidden type="file" accept="application/json,.json" onChange={(event) => void importTheme(event.target.files?.[0])} />
              {importError && <p className="settings-error">{importError}</p>}
            </section>
          </>}

          {page === "terminal" && <section className="settings-section">
            <SectionTitle title="Typography" description="Changes are applied live to every open pane." />
            <label className="setting-row setting-row--stacked"><div><strong>Font family</strong><span>Use any installed monospace family.</span></div><input className="text-field" value={preferences.fontFamily} onChange={(event) => patch("fontFamily", event.target.value)} /></label>
            <label className="setting-row"><div><strong>Font size</strong><span>{preferences.fontSize}px</span></div><input type="range" min="9" max="28" value={preferences.fontSize} onChange={(event) => patch("fontSize", Number(event.target.value))} /></label>
            <label className="setting-row"><div><strong>Line height</strong><span>{preferences.lineHeight.toFixed(2)}</span></div><input type="range" min="1" max="1.8" step="0.05" value={preferences.lineHeight} onChange={(event) => patch("lineHeight", Number(event.target.value))} /></label>
            <label className="setting-row"><div><strong>Padding</strong><span>{preferences.terminalPadding}px</span></div><input type="range" min="4" max="32" value={preferences.terminalPadding} onChange={(event) => patch("terminalPadding", Number(event.target.value))} /></label>
            <label className="setting-row"><div><strong>Opacity</strong><span>{Math.round(preferences.terminalOpacity * 100)}%</span></div><input type="range" min="0.65" max="1" step="0.01" value={preferences.terminalOpacity} onChange={(event) => patch("terminalOpacity", Number(event.target.value))} /></label>
            <div className="setting-row"><div><strong>Cursor style</strong><span>Choose the terminal caret.</span></div><select value={preferences.cursorStyle} onChange={(event) => patch("cursorStyle", event.target.value as AppearancePreferences["cursorStyle"])}><option value="bar">Bar</option><option value="block">Block</option><option value="underline">Underline</option></select></div>
            <div className="setting-row"><div><strong>Blinking cursor</strong><span>Disable for a calmer terminal.</span></div><Toggle label="Blinking cursor" checked={preferences.cursorBlink} onChange={(value) => patch("cursorBlink", value)} /></div>
            <div className="setting-row"><div><strong>Copy on select</strong><span>Automatically place selected terminal text on the clipboard.</span></div><Toggle label="Copy on select" checked={preferences.copyOnSelect} onChange={(value) => patch("copyOnSelect", value)} /></div>
            <label className="setting-row"><div><strong>Scrollback</strong><span>{preferences.scrollback.toLocaleString()} lines kept in memory</span></div><input type="range" min="1000" max="100000" step="1000" value={preferences.scrollback} onChange={(event) => patch("scrollback", Number(event.target.value))} /></label>
          </section>}

          {page === "profiles" && <section className="settings-section">
            <SectionTitle title="Shell profiles" description="Nebula is recommended, while CMD, PowerShell and WSL stay available for compatibility." />
            <div className="setting-row"><div><strong>Default profile</strong><span>Used by Ctrl+Shift+T and startup when no session is restored.</span></div><select value={preferences.defaultProfileId} onChange={(event) => patch("defaultProfileId", event.target.value)}>{profiles.filter((profile) => profile.available).map((profile) => <option key={profile.id} value={profile.id}>{profile.name}</option>)}</select></div>
            <label className="setting-row setting-row--stacked"><div><strong>Starting directory</strong><span>Leave empty to use the directory where Nebula Terminal was opened.</span></div><input className="text-field" value={preferences.workingDirectory} placeholder="C:\\Users\\you\\Projects" onChange={(event) => patch("workingDirectory", event.target.value)} /></label>
            <div className="profile-list">{profiles.map((profile) => <div key={profile.id} className={`profile-row ${profile.available ? "" : "profile-row--disabled"}`}><span className="profile-color" style={{ background: profile.accent }} /><div><strong>{profile.name}</strong><span>{profile.available ? profile.executable ?? "Built in" : "Not detected"}</span></div><span className="profile-state">{profile.available ? "Ready" : "Unavailable"}</span></div>)}</div>
          </section>}

          {page === "keybindings" && <section className="settings-section">
            <SectionTitle title="Keyboard shortcuts" description="Edit shortcut strings using Ctrl, Shift, Alt and a final key, for example Ctrl+Shift+T." />
            {Object.entries(preferences.keybindings).map(([key, value]) => <label className="setting-row" key={key}><div><strong>{key.replace(/([A-Z])/g, " $1").replace(/^./, (letter) => letter.toUpperCase())}</strong><span>Applied immediately.</span></div><input className="shortcut-field" value={value} onChange={(event) => patchBinding(key as keyof KeybindingPreferences, event.target.value)} /></label>)}
            <button className="ghost-button" type="button" onClick={() => patch("keybindings", { ...preferences.keybindings, ...{ newTab: "Ctrl+Shift+T", closeTab: "Ctrl+Shift+W", commandPalette: "Ctrl+Shift+P", settings: "Ctrl+,", find: "Ctrl+F", splitVertical: "Ctrl+Shift+D", splitHorizontal: "Ctrl+Shift+E", closePane: "Ctrl+Shift+Q" } })}>Reset shortcuts</button>
          </section>}

          {page === "advanced" && <section className="settings-section">
            <SectionTitle title="Workspace behavior" description="Choose what Nebula remembers and where it asks before acting." />
            <div className="setting-row"><div><strong>Restore workspace layout</strong><span>Reopen tabs, profiles and splits. Commands start in fresh processes.</span></div><Toggle label="Restore workspace layout" checked={preferences.restoreSession} onChange={(value) => patch("restoreSession", value)} /></div>
            <div className="setting-row"><div><strong>Check multiline paste</strong><span>Ask before several command lines are sent to a session.</span></div><Toggle label="Check multiline paste" checked={preferences.confirmMultilinePaste} onChange={(value) => patch("confirmMultilinePaste", value)} /></div>
            <div className="setting-row"><div><strong>Confirm multi-tab close</strong><span>Ask before closing a workspace with several tabs.</span></div><Toggle label="Confirm multi-tab close" checked={preferences.confirmCloseMultipleTabs} onChange={(value) => patch("confirmCloseMultipleTabs", value)} /></div>
            <div className="setting-row"><div><strong>Saved workspace</strong><span>Clear tabs and split layout stored on this device.</span></div><button className="secondary-button" type="button" onClick={onClearSession}>Clear saved session</button></div>
          </section>}
        </div>
      </div>
    </aside>
  );
}

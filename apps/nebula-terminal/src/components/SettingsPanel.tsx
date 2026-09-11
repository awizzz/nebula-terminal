import type { AppearancePreferences, TerminalProfile } from "../types";

interface SettingsPanelProps {
  open: boolean;
  profiles: TerminalProfile[];
  preferences: AppearancePreferences;
  onChange: (next: AppearancePreferences) => void;
  onClose: () => void;
  onReset: () => void;
}

function SectionTitle({ title, description }: { title: string; description: string }) {
  return (
    <div className="settings-heading">
      <h3>{title}</h3>
      <p>{description}</p>
    </div>
  );
}

export default function SettingsPanel({ open, profiles, preferences, onChange, onClose, onReset }: SettingsPanelProps) {
  if (!open) return null;

  const patch = <K extends keyof AppearancePreferences>(key: K, value: AppearancePreferences[K]) => {
    onChange({ ...preferences, [key]: value });
  };

  return (
    <aside className="settings-panel" aria-label="Settings">
      <div className="settings-header">
        <div>
          <span className="eyebrow">Nebula Terminal</span>
          <h2>Appearance</h2>
        </div>
        <button className="icon-button" type="button" onClick={onClose} aria-label="Close settings">×</button>
      </div>

      <div className="settings-scroll">
        <section className="settings-section">
          <SectionTitle title="Window" description="Tune the chrome without compromising terminal performance." />
          <div className="setting-row">
            <div><strong>Background</strong><span>Windows 11 uses Mica when available.</span></div>
            <div className="segmented" role="group" aria-label="Background mode">
              {(["mica", "solid"] as const).map((mode) => (
                <button key={mode} className={preferences.backgroundMode === mode ? "selected" : ""} type="button" onClick={() => patch("backgroundMode", mode)}>{mode}</button>
              ))}
            </div>
          </div>
          <label className="setting-row setting-row--stacked">
            <div><strong>Accent</strong><span>Used for focus, selection and active session state.</span></div>
            <div className="accent-control">
              <input type="color" value={preferences.accent} onChange={(e) => patch("accent", e.target.value)} aria-label="Accent color" />
              <code>{preferences.accent.toUpperCase()}</code>
            </div>
          </label>
        </section>

        <section className="settings-section">
          <SectionTitle title="Terminal" description="Live preview — changes apply to the active terminal immediately." />
          <label className="setting-row setting-row--stacked">
            <div><strong>Font</strong><span>Use any installed monospace family.</span></div>
            <input className="text-field" value={preferences.fontFamily} onChange={(e) => patch("fontFamily", e.target.value)} />
          </label>
          <label className="setting-row">
            <div><strong>Font size</strong><span>{preferences.fontSize}px</span></div>
            <input type="range" min="11" max="22" step="1" value={preferences.fontSize} onChange={(e) => patch("fontSize", Number(e.target.value))} />
          </label>
          <label className="setting-row">
            <div><strong>Padding</strong><span>{preferences.terminalPadding}px</span></div>
            <input type="range" min="6" max="28" step="1" value={preferences.terminalPadding} onChange={(e) => patch("terminalPadding", Number(e.target.value))} />
          </label>
          <label className="setting-row">
            <div><strong>Opacity</strong><span>{Math.round(preferences.terminalOpacity * 100)}%</span></div>
            <input type="range" min="0.72" max="1" step="0.01" value={preferences.terminalOpacity} onChange={(e) => patch("terminalOpacity", Number(e.target.value))} />
          </label>
          <div className="setting-row">
            <div><strong>Cursor</strong><span>Shape of the terminal caret.</span></div>
            <select value={preferences.cursorStyle} onChange={(e) => patch("cursorStyle", e.target.value as AppearancePreferences["cursorStyle"])}>
              <option value="bar">Bar</option><option value="block">Block</option><option value="underline">Underline</option>
            </select>
          </div>
        </section>

        <section className="settings-section">
          <SectionTitle title="Profiles" description="Detected locally by the native host." />
          <div className="profile-list">
            {profiles.map((profile) => (
              <div key={profile.id} className={`profile-row ${profile.available ? "" : "profile-row--disabled"}`}>
                <span className="profile-color" style={{ background: profile.accent }} />
                <div><strong>{profile.name}</strong><span>{profile.available ? profile.executable ?? "Built in" : "Not detected"}</span></div>
                <span className="profile-state">{profile.available ? "Ready" : "Unavailable"}</span>
              </div>
            ))}
          </div>
        </section>
      </div>

      <div className="settings-footer">
        <button className="ghost-button" type="button" onClick={onReset}>Reset appearance</button>
        <span>Preview 0.1.0</span>
      </div>
    </aside>
  );
}

import { useEffect, useRef, useState, type ReactNode } from "react";
import { Check, Info, Keyboard, Monitor, Palette, RotateCcw, SquareTerminal, ToggleRight, X } from "lucide-react";
import { version } from "../../package.json";
import appIcon from "../assets/icon.svg";
import { openExternal } from "../external";
import { shortcutFromEvent } from "../keys";
import { defaultFontFamily, defaultKeybindings, exportAppearance, importAppearance, keybindingLabels } from "../preferences";
import { accentSwatches, themes, type TerminalTheme } from "../themes";
import Keys from "./Keys";
import ProfileIcon from "./ProfileIcon";
import type { AppearancePreferences, KeybindingPreferences, TerminalProfile } from "../types";

interface SettingsPanelProps {
  open: boolean;
  initialPage?: SettingsPage;
  profiles: TerminalProfile[];
  preferences: AppearancePreferences;
  onChange: (next: AppearancePreferences) => void;
  onClose: () => void;
  onReset: () => void;
  onClearSession: () => void;
}

export type SettingsPage = "appearance" | "terminal" | "profiles" | "keyboard" | "behavior" | "about";

const pages: Array<{ id: SettingsPage; label: string; icon: typeof Palette }> = [
  { id: "appearance", label: "Appearance", icon: Palette },
  { id: "terminal", label: "Terminal", icon: SquareTerminal },
  { id: "profiles", label: "Shells", icon: Monitor },
  { id: "keyboard", label: "Keyboard", icon: Keyboard },
  { id: "behavior", label: "Behavior", icon: ToggleRight },
  { id: "about", label: "About", icon: Info },
];

const monospaceFonts = ["Cascadia Mono", "Cascadia Code", "Consolas", "JetBrains Mono", "Fira Code", "Source Code Pro", "Iosevka", "Hack", "Lucida Console"];
const REPOSITORY = "https://github.com/awizzz/nebula-shell";

/**
 * Re-encodes a background image so it fits comfortably in local storage next to the
 * other settings: at most 2560 px on the long side, WebP, under ~3 MB as a data URL.
 */
async function shrinkImage(file: File): Promise<string> {
  const bitmap = await createImageBitmap(file);
  let scale = Math.min(1, 2560 / Math.max(bitmap.width, bitmap.height));
  for (let quality = 0.86; ; quality -= 0.12) {
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(bitmap.width * scale));
    canvas.height = Math.max(1, Math.round(bitmap.height * scale));
    canvas.getContext("2d")?.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    const url = canvas.toDataURL("image/webp", quality);
    if (url.length <= 3_000_000 || quality < 0.4) {
      bitmap.close();
      if (url.length > 3_000_000) throw new Error("Image too large");
      return url;
    }
    scale *= 0.8;
  }
}

function Row({ label, description, children, stacked = false }: { label: string; description?: string; children: ReactNode; stacked?: boolean }) {
  return (
    <div className={`row ${stacked ? "row--stacked" : ""}`}>
      <div className="row__text">
        <span className="row__label">{label}</span>
        {description && <span className="row__description">{description}</span>}
      </div>
      <div className="row__control">{children}</div>
    </div>
  );
}

function Group({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="group">
      {title && <h3 className="group__title">{title}</h3>}
      <div className="group__rows">{children}</div>
    </section>
  );
}

function Switch({ checked, onChange, label }: { checked: boolean; onChange: (value: boolean) => void; label: string }) {
  return (
    <button className="switch" type="button" role="switch" aria-checked={checked} aria-label={label} onClick={() => onChange(!checked)}>
      <span className="switch__thumb" />
    </button>
  );
}

function Segmented<T extends string>({ value, options, onChange, label }: { value: T; options: Array<{ value: T; label: ReactNode }>; onChange: (value: T) => void; label: string }) {
  const index = Math.max(0, options.findIndex((option) => option.value === value));
  return (
    <div className="segmented" role="radiogroup" aria-label={label} style={{ "--count": options.length, "--index": index } as React.CSSProperties}>
      <span className="segmented__thumb" aria-hidden="true" />
      {options.map((option) => (
        <button key={option.value} type="button" role="radio" aria-checked={option.value === value} onClick={() => onChange(option.value)}>{option.label}</button>
      ))}
    </div>
  );
}

function Slider({ value, min, max, step, onChange, format, label }: { value: number; min: number; max: number; step: number; onChange: (value: number) => void; format: (value: number) => string; label: string }) {
  const progress = ((value - min) / (max - min)) * 100;
  return (
    <div className="slider">
      <input type="range" min={min} max={max} step={step} value={value} aria-label={label} style={{ "--progress": `${progress}%` } as React.CSSProperties} onChange={(event) => onChange(Number(event.target.value))} />
      <output>{format(value)}</output>
    </div>
  );
}

function ThemePreview({ theme }: { theme: TerminalTheme }) {
  const [, red, green, yellow, blue, , cyan] = theme.ansi;
  return (
    <span className="theme-card__preview" style={{ background: theme.background, color: theme.foreground }} aria-hidden="true">
      <span><b style={{ color: blue }}>PS</b> ~\app&gt; <span style={{ color: yellow }}>git</span> status</span>
      <span>On branch <b style={{ color: cyan }}>main</b></span>
      <span style={{ color: green }}>+ new  src/theme.ts</span>
      <span style={{ color: red }}>- old  src/legacy.ts</span>
    </span>
  );
}

function ShortcutRecorder({ action, value, conflict, onChange }: { action: string; value: string; conflict?: string; onChange: (value: string) => void }) {
  const [recording, setRecording] = useState(false);
  const ref = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!recording) return;
    const capture = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") { setRecording(false); return; }
      if ((event.key === "Backspace" || event.key === "Delete") && !event.ctrlKey && !event.altKey) {
        onChange("");
        setRecording(false);
        return;
      }
      const shortcut = shortcutFromEvent(event);
      if (!shortcut) return;
      onChange(shortcut);
      setRecording(false);
    };
    const stop = () => setRecording(false);
    window.addEventListener("keydown", capture, true);
    window.addEventListener("blur", stop);
    return () => {
      window.removeEventListener("keydown", capture, true);
      window.removeEventListener("blur", stop);
    };
  }, [onChange, recording]);

  return (
    <div className="shortcut">
      {conflict && !recording && <span className="shortcut__conflict">Also used by {conflict}</span>}
      <button
        ref={ref}
        type="button"
        data-shortcut-recorder
        className={`shortcut__field ${recording ? "is-recording" : ""} ${conflict ? "has-conflict" : ""}`}
        aria-label={`${action}: ${value || "not set"}. Press to change.`}
        onClick={() => setRecording((current) => !current)}
      >
        {recording ? <span className="shortcut__hint">Press a shortcut…</span> : value ? <Keys shortcut={value} /> : <span className="shortcut__hint">Not set</span>}
      </button>
    </div>
  );
}

export default function SettingsPanel({ open, initialPage, profiles, preferences, onChange, onClose, onReset, onClearSession }: SettingsPanelProps) {
  const [page, setPage] = useState<SettingsPage>("appearance");
  const [message, setMessage] = useState<{ tone: "error" | "info"; text: string } | null>(null);
  const [confirmReset, setConfirmReset] = useState(false);
  const themeInputRef = useRef<HTMLInputElement | null>(null);
  const imageInputRef = useRef<HTMLInputElement | null>(null);
  const panelRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented) onClose();
    };
    window.addEventListener("keydown", closeOnEscape);
    requestAnimationFrame(() => panelRef.current?.querySelector<HTMLButtonElement>(".settings__nav [aria-current='page']")?.focus());
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [onClose, open]);

  useEffect(() => {
    if (open && initialPage) setPage(initialPage);
  }, [initialPage, open]);

  useEffect(() => {
    setMessage(null);
    setConfirmReset(false);
  }, [page]);

  if (!open) return null;

  const patch = <K extends keyof AppearancePreferences>(key: K, value: AppearancePreferences[K]) => onChange({ ...preferences, [key]: value });
  const patchBinding = (key: keyof KeybindingPreferences, value: string) => onChange({ ...preferences, keybindings: { ...preferences.keybindings, [key]: value } });
  const pageIndex = pages.findIndex((item) => item.id === page);
  const available = profiles.filter((profile) => profile.available);

  const importTheme = async (file?: File) => {
    if (!file) return;
    try {
      onChange(await importAppearance(file, preferences));
      setMessage({ tone: "info", text: `Imported ${file.name}` });
    } catch (error) {
      setMessage({ tone: "error", text: error instanceof Error ? error.message : String(error) });
    }
  };

  const readBackground = async (file?: File) => {
    if (!file) return;
    if (!file.type.startsWith("image/")) return setMessage({ tone: "error", text: "Choose an image file." });
    if (file.size > 40_000_000) return setMessage({ tone: "error", text: "Choose an image smaller than 40 MB." });
    try {
      onChange({ ...preferences, backgroundMode: "image", backgroundImage: await shrinkImage(file) });
      setMessage(null);
    } catch {
      setMessage({ tone: "error", text: "This image could not be read." });
    }
  };

  const conflictFor = (key: keyof KeybindingPreferences) => {
    const value = preferences.keybindings[key].toLowerCase();
    if (!value) return undefined;
    const other = (Object.keys(preferences.keybindings) as Array<keyof KeybindingPreferences>)
      .find((candidate) => candidate !== key && preferences.keybindings[candidate].toLowerCase() === value);
    return other ? keybindingLabels[other] : undefined;
  };

  return (
    <div className="overlay overlay--settings" role="presentation" onMouseDown={onClose}>
      <section ref={panelRef} className="settings" role="dialog" aria-modal="true" aria-label="Settings" onMouseDown={(event) => event.stopPropagation()}>
        <nav className="settings__nav" aria-label="Settings sections" style={{ "--index": pageIndex } as React.CSSProperties}>
          <h2>Settings</h2>
          <div className="settings__nav-items">
            <span className="settings__indicator" aria-hidden="true" />
            {pages.map(({ id, label, icon: Icon }) => (
              <button key={id} type="button" aria-current={page === id ? "page" : undefined} onClick={() => setPage(id)}>
                <Icon size={16} strokeWidth={1.7} aria-hidden="true" />
                {label}
              </button>
            ))}
          </div>
        </nav>

        <div className="settings__main">
          <header className="settings__header">
            <h1>{pages[pageIndex]?.label}</h1>
            <button className="icon-button" type="button" onClick={onClose} aria-label="Close settings" title="Close (Esc)"><X size={16} /></button>
          </header>

          <div className="settings__scroll">
            <div className="settings__page" key={page}>
              {page === "appearance" && <>
                <Group title="Theme">
                  <div className="theme-grid" role="radiogroup" aria-label="Theme">
                    {themes.map((theme) => {
                      const selected = preferences.themeId === theme.id;
                      return (
                        <button key={theme.id} type="button" role="radio" aria-checked={selected} className="theme-card" onClick={() => onChange({ ...preferences, themeId: theme.id, accent: theme.accent })}>
                          <ThemePreview theme={theme} />
                          <span className="theme-card__name">{theme.name}{selected && <Check size={14} strokeWidth={2.2} />}</span>
                        </button>
                      );
                    })}
                  </div>
                </Group>

                <Group title="Window">
                  <Row label="Accent color" description="Cursor, selection, focus and active controls.">
                    <div className="swatches" role="radiogroup" aria-label="Accent color">
                      {accentSwatches.map((color) => (
                        <button key={color} type="button" role="radio" aria-checked={preferences.accent.toLowerCase() === color} aria-label={color} className="swatch" style={{ background: color }} onClick={() => patch("accent", color)} />
                      ))}
                      <label className="swatch swatch--custom" title="Custom color">
                        <input type="color" value={preferences.accent} onChange={(event) => patch("accent", event.target.value)} aria-label="Custom accent color" />
                      </label>
                    </div>
                  </Row>
                  <Row label="Background" description="Mica lets your desktop color show through on Windows 11.">
                    <Segmented label="Background" value={preferences.backgroundMode} onChange={(value) => value === "image" && !preferences.backgroundImage ? imageInputRef.current?.click() : patch("backgroundMode", value)} options={[{ value: "mica", label: "Mica" }, { value: "solid", label: "Solid" }, { value: "image", label: "Image" }]} />
                  </Row>
                  {preferences.backgroundMode === "image" && preferences.backgroundImage && <>
                    <Row label="Background image">
                      <div className="button-row">
                        <button className="button" type="button" onClick={() => imageInputRef.current?.click()}>Change…</button>
                        <button className="button" type="button" onClick={() => onChange({ ...preferences, backgroundImage: undefined, backgroundMode: "mica" })}>Remove</button>
                      </div>
                    </Row>
                    <Row label="Image visibility">
                      <Slider label="Image visibility" value={preferences.backgroundImageOpacity} min={0.05} max={0.8} step={0.01} format={(value) => `${Math.round(value * 100)}%`} onChange={(value) => patch("backgroundImageOpacity", value)} />
                    </Row>
                  </>}
                  <Row label="Tabs">
                    <Segmented label="Tab size" value={preferences.tabDensity} onChange={(value) => patch("tabDensity", value)} options={[{ value: "comfortable", label: "Normal" }, { value: "compact", label: "Compact" }]} />
                  </Row>
                  <Row label="Animations" description="Reduced keeps fades and removes movement.">
                    <Segmented label="Animations" value={preferences.animationLevel} onChange={(value) => patch("animationLevel", value)} options={[{ value: "full", label: "Full" }, { value: "reduced", label: "Reduced" }, { value: "off", label: "Off" }]} />
                  </Row>
                  <input ref={imageInputRef} hidden type="file" accept="image/*" onChange={(event) => { void readBackground(event.target.files?.[0]); event.target.value = ""; }} />
                </Group>

                <Group title="Share your look">
                  <Row label="Theme file" description="Colors, fonts and window style. Never includes paths or shortcuts.">
                    <div className="button-row">
                      <button className="button" type="button" onClick={() => themeInputRef.current?.click()}>Import…</button>
                      <button className="button" type="button" onClick={() => exportAppearance(preferences)}>Export</button>
                    </div>
                  </Row>
                  <input ref={themeInputRef} hidden type="file" accept="application/json,.json" onChange={(event) => { void importTheme(event.target.files?.[0]); event.target.value = ""; }} />
                </Group>
              </>}

              {page === "terminal" && <>
                <Group title="Text">
                  <Row label="Font" description="Any installed monospace font. Separate fallbacks with commas." stacked>
                    <input className="field" list="font-families" value={preferences.fontFamily} spellCheck={false} onChange={(event) => patch("fontFamily", event.target.value)} onBlur={(event) => !event.target.value.trim() && patch("fontFamily", defaultFontFamily)} />
                    <datalist id="font-families">{monospaceFonts.map((font) => <option key={font} value={font} />)}</datalist>
                  </Row>
                  <Row label="Size">
                    <div className="stepper">
                      <button type="button" aria-label="Smaller" onClick={() => patch("fontSize", Math.max(8, preferences.fontSize - 1))}>−</button>
                      <output>{preferences.fontSize} pt</output>
                      <button type="button" aria-label="Larger" onClick={() => patch("fontSize", Math.min(32, preferences.fontSize + 1))}>+</button>
                    </div>
                  </Row>
                  <Row label="Line height">
                    <Slider label="Line height" value={preferences.lineHeight} min={1} max={1.8} step={0.05} format={(value) => value.toFixed(2)} onChange={(value) => patch("lineHeight", value)} />
                  </Row>
                </Group>

                <Group title="Cursor">
                  <Row label="Shape">
                    <Segmented label="Cursor shape" value={preferences.cursorStyle} onChange={(value) => patch("cursorStyle", value)} options={[
                      { value: "bar", label: <><i className="caret caret--bar" />Bar</> },
                      { value: "block", label: <><i className="caret caret--block" />Block</> },
                      { value: "underline", label: <><i className="caret caret--underline" />Underline</> },
                    ]} />
                  </Row>
                  <Row label="Blink">
                    <Switch label="Blinking cursor" checked={preferences.cursorBlink} onChange={(value) => patch("cursorBlink", value)} />
                  </Row>
                </Group>

                <Group title="Layout">
                  <Row label="Padding">
                    <Slider label="Padding" value={preferences.terminalPadding} min={0} max={32} step={1} format={(value) => `${value} px`} onChange={(value) => patch("terminalPadding", value)} />
                  </Row>
                  <Row label="Background opacity" description="Below 100% the window background shows through.">
                    <Slider label="Background opacity" value={preferences.terminalOpacity} min={0.6} max={1} step={0.01} format={(value) => `${Math.round(value * 100)}%`} onChange={(value) => patch("terminalOpacity", value)} />
                  </Row>
                  <Row label="GPU acceleration" description="Turn off if text looks blurry, misplaced or flickers.">
                    <Switch label="GPU acceleration" checked={preferences.gpuAcceleration} onChange={(value) => patch("gpuAcceleration", value)} />
                  </Row>
                  <Row label="Scrollback" description="Lines kept per pane.">
                    <select className="field field--select" value={preferences.scrollback} onChange={(event) => patch("scrollback", Number(event.target.value))}>
                      {[1_000, 5_000, 10_000, 20_000, 50_000, 100_000].map((lines) => <option key={lines} value={lines}>{lines.toLocaleString("en-US")}</option>)}
                    </select>
                  </Row>
                </Group>
              </>}

              {page === "profiles" && <>
                <Group>
                  <Row label="Default shell" description="Opened by the + button and at startup.">
                    <select className="field field--select" value={available.some((profile) => profile.id === preferences.defaultProfileId) ? preferences.defaultProfileId : ""} onChange={(event) => patch("defaultProfileId", event.target.value)}>
                      <option value="">Automatic{available[0] ? ` (${available[0].name})` : ""}</option>
                      {available.map((profile) => <option key={profile.id} value={profile.id}>{profile.name}</option>)}
                    </select>
                  </Row>
                  <Row label="Starting folder" description="Leave empty for your user folder. Accepts ~ and %VARIABLES%." stacked>
                    <input className="field" value={preferences.workingDirectory} spellCheck={false} placeholder="%USERPROFILE%\Projects" onChange={(event) => patch("workingDirectory", event.target.value)} />
                  </Row>
                </Group>
                <Group title="Detected on this PC">
                  {profiles.map((profile) => (
                    <div key={profile.id} className={`shell-row ${profile.available ? "" : "is-missing"}`}>
                      <ProfileIcon kind={profile.kind} size={20} />
                      <div className="row__text">
                        <span className="row__label">{profile.name}</span>
                        <span className="row__description" title={profile.executable}>{profile.available ? profile.executable : "Not installed"}</span>
                      </div>
                      {profile.available && <span className="badge"><Check size={12} strokeWidth={2.4} />Ready</span>}
                    </div>
                  ))}
                </Group>
              </>}

              {page === "keyboard" && <>
                <Group title="Shortcuts">
                  {(Object.keys(defaultKeybindings) as Array<keyof KeybindingPreferences>).map((key) => (
                    <Row key={key} label={keybindingLabels[key]}>
                      <ShortcutRecorder action={keybindingLabels[key]} value={preferences.keybindings[key]} conflict={conflictFor(key)} onChange={(value) => patchBinding(key, value)} />
                    </Row>
                  ))}
                </Group>
                <Group title="Always available">
                  <Row label="Copy selection"><Keys shortcut="Ctrl+C" /></Row>
                  <Row label="Paste"><Keys shortcut="Ctrl+V" /></Row>
                  <Row label="Move between panes"><Keys shortcut="Alt+Arrow" /></Row>
                  <Row label="Go to tab 1–9"><Keys shortcut="Ctrl+Alt+1…9" /></Row>
                  <Row label="Zoom"><Keys shortcut="Ctrl+Wheel" /></Row>
                </Group>
                <div className="settings__footer-actions">
                  <button className="button" type="button" onClick={() => patch("keybindings", { ...defaultKeybindings })}><RotateCcw size={14} />Restore default shortcuts</button>
                </div>
              </>}

              {page === "behavior" && <>
                <Group title="Clipboard">
                  <Row label="Copy on select" description="Selecting text copies it right away.">
                    <Switch label="Copy on select" checked={preferences.copyOnSelect} onChange={(value) => patch("copyOnSelect", value)} />
                  </Row>
                  <Row label="Confirm multi-line paste" description="Review pasted text that would run several commands.">
                    <Switch label="Confirm multi-line paste" checked={preferences.confirmMultilinePaste} onChange={(value) => patch("confirmMultilinePaste", value)} />
                  </Row>
                </Group>
                <Group title="Tabs">
                  <Row label="Reopen tabs on launch" description="Restores tabs and splits. Shells start fresh.">
                    <Switch label="Reopen tabs on launch" checked={preferences.restoreSession} onChange={(value) => patch("restoreSession", value)} />
                  </Row>
                  <Row label="Confirm closing several tabs">
                    <Switch label="Confirm closing several tabs" checked={preferences.confirmCloseMultipleTabs} onChange={(value) => patch("confirmCloseMultipleTabs", value)} />
                  </Row>
                  <Row label="Saved tab layout">
                    <button className="button" type="button" onClick={() => { onClearSession(); setMessage({ tone: "info", text: "Saved layout cleared." }); }}>Clear</button>
                  </Row>
                </Group>
              </>}

              {page === "about" && <>
                <div className="about">
                  <img src={appIcon} alt="" width={64} height={64} />
                  <div>
                    <h2>Nebula Terminal</h2>
                    <p>Version {version}</p>
                  </div>
                </div>
                <Group>
                  <Row label="Source code"><button className="button" type="button" onClick={() => openExternal(REPOSITORY)}>Open on GitHub</button></Row>
                  <Row label="Found a bug?"><button className="button" type="button" onClick={() => openExternal(`${REPOSITORY}/issues/new/choose`)}>Report a problem</button></Row>
                  <Row label="License"><span className="row__value">MIT</span></Row>
                </Group>
                <Group title="Reset">
                  <Row label="Reset all settings" description="Theme, fonts, shells and shortcuts go back to their defaults.">
                    {confirmReset
                      ? <div className="button-row"><button className="button" type="button" onClick={() => setConfirmReset(false)}>Cancel</button><button className="button button--danger" type="button" onClick={() => { onReset(); setConfirmReset(false); }}>Reset</button></div>
                      : <button className="button" type="button" onClick={() => setConfirmReset(true)}>Reset…</button>}
                  </Row>
                </Group>
              </>}

              {message && <p className={`settings__message settings__message--${message.tone}`} role="status">{message.text}</p>}
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}

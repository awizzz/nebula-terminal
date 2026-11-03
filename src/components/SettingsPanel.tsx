import { Fragment, createContext, useContext, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type Dispatch, type FormEvent, type ReactNode, type SetStateAction } from "react";
import { Check, ChevronRight, Globe, Info, Keyboard, Monitor, Paintbrush, Palette, Pencil, Plus, RotateCcw, Search, SquareTerminal, ToggleRight, Trash, X } from "lucide-react";
import { version } from "../../package.json";
import appIcon from "../assets/icon.svg";
import { deleteCustomProfile, draftOf, listCustomProfiles, newDraft, saveCustomProfile, splitArguments, sshCommand } from "../customProfiles";
import { openExternal } from "../external";
import { checkForUpdate, installUpdate, markChecked, type UpdateInfo } from "../updates";
import { globalShortcutFromEvent, globalShortcutLabel, keyboardLayout } from "../quickShortcut";
import { shortcutFromEvent } from "../keys";
import { defaultFontFamily, defaultKeybindings, exportAppearance, importAppearance, keybindingLabels } from "../preferences";
import { groupProfiles, profileGroup, profileGroupLabels } from "../profiles";
import { accentSwatches, resolveTheme, tabColors, themes, type TerminalTheme } from "../themes";
import { highlightPattern, MAX_HIGHLIGHTS } from "../highlights";
import { withLook } from "../looks";
import { MAX_CUSTOM_THEMES, parseColorSchemes, sortOutSchemes } from "../themeImport";
import Keys from "./Keys";
import Segmented from "./Segmented";
import ThemeSection from "./ThemeSection";
import { describe as describeWorkspace, renamed, withPaneCommand, workspacePanes, type Workspace } from "../workspaces";
import ProfileIcon from "./ProfileIcon";
import type { AppearancePreferences, CustomProfile, CustomProfileDraft, CustomProfileField, DefaultTerminal, KeybindingPreferences, Launchers, ProfileFieldError, ProfileLook, SshDraft, TerminalProfile } from "../types";

const quickHeights = [0.3, 0.5, 0.7, 0.9];

interface SettingsPanelProps {
  open: boolean;
  initialPage?: SettingsPage;
  profiles: TerminalProfile[];
  preferences: AppearancePreferences;
  onChange: Dispatch<SetStateAction<AppearancePreferences>>;
  onClose: () => void;
  onReset: () => void;
  onClearSession: () => void;
  /** Called after a custom profile was added, changed or removed. */
  onProfilesChanged: () => Promise<void>;
  /** What Windows offers to open the app in a folder; null outside the desktop app. */
  launchers: Launchers | null;
  /** Whether Nebula is the default terminal; null outside the desktop app. */
  defaultTerminal: DefaultTerminal | null;
  onDefaultTerminalChange: (enabled: boolean) => void;
  /** An admin window: its settings come from the window that opened it. */
  administrator?: boolean;
  workspaces: Workspace[];
  onWorkspacesChange: (workspaces: Workspace[]) => void;
  /** Why the quick terminal's shortcut couldn't be registered, if it couldn't. */
  quickTerminalProblem?: string | null;
  /** Settings sync, in the window that runs it (the main one). */
  sync?: { problem: string | null; choose: () => void };
}

export type SettingsPage = "appearance" | "terminal" | "profiles" | "keyboard" | "behavior" | "about";

const pages: Array<{ id: SettingsPage; label: string; icon: typeof Palette }> = [
  { id: "appearance", label: "Appearance", icon: Palette },
  { id: "terminal", label: "Terminal", icon: SquareTerminal },
  { id: "profiles", label: "Profiles", icon: Monitor },
  { id: "keyboard", label: "Keyboard", icon: Keyboard },
  { id: "behavior", label: "Behavior", icon: ToggleRight },
  { id: "about", label: "About", icon: Info },
];

const monospaceFonts = ["Cascadia Mono", "Cascadia Code", "Consolas", "JetBrains Mono", "Fira Code", "Source Code Pro", "Iosevka", "Hack", "Lucida Console"];
const REPOSITORY = "https://github.com/awizzz/nebula-terminal";

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

/** What the settings search looks for, in lower case, and whether the group it's in matched. */
const SearchContext = createContext<{ words: string[]; group: string; groupMatches: boolean }>({ words: [], group: "", groupMatches: false });

const matchesAll = (words: string[], ...texts: (string | undefined)[]) => {
  const haystack = texts.filter(Boolean).join(" ").toLowerCase();
  return words.every((word) => haystack.includes(word));
};

function Row({ label, description, children, stacked = false, error = false }: { label: string; description?: string; children?: ReactNode; stacked?: boolean; error?: boolean }) {
  const search = useContext(SearchContext);
  const match = search.words.length > 0 && (search.groupMatches || matchesAll(search.words, search.group, label, description));
  return (
    <div className={`row ${stacked ? "row--stacked" : ""} ${match ? "row--match" : ""}`}>
      <div className="row__text">
        <span className="row__label">{label}</span>
        {description && <span className={`row__description ${error ? "row__description--error" : ""}`} role={error ? "alert" : undefined}>{description}</span>}
      </div>
      <div className="row__control">{children}</div>
    </div>
  );
}

function Group({ title, description, children }: { title?: string; description?: string; children: ReactNode }) {
  const { words } = useContext(SearchContext);
  const groupMatches = words.length > 0 && matchesAll(words, title, description);
  return (
    <SearchContext.Provider value={{ words, group: title ?? "", groupMatches }}>
      <section className={`group ${words.length && !groupMatches ? "is-filtered" : ""}`}>
        {title && <h3 className="group__title">{title}</h3>}
        {description && <p className="group__description">{description}</p>}
        <div className="group__rows">{children}</div>
      </section>
    </SearchContext.Provider>
  );
}

type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current" }
  | { kind: "available"; update: UpdateInfo }
  | { kind: "installing"; update: UpdateInfo }
  | { kind: "error"; message: string; update?: UpdateInfo };

/** "Check now", and what it found. */
function UpdateCheck() {
  const [state, setState] = useState<UpdateState>({ kind: "idle" });
  const errorText = (reason: unknown) => reason instanceof Error ? reason.message : String(reason);

  const check = () => {
    setState({ kind: "checking" });
    markChecked();
    checkForUpdate()
      .then((update) => setState(update ? { kind: "available", update } : { kind: "current" }))
      .catch((reason) => setState({ kind: "error", message: errorText(reason) }));
  };
  const install = (update: UpdateInfo) => {
    setState({ kind: "installing", update });
    installUpdate().catch((reason) => setState({ kind: "error", message: errorText(reason), update }));
  };

  const update = "update" in state ? state.update : undefined;
  const description = {
    idle: undefined,
    checking: "Checking GitHub…",
    current: "You have the latest version.",
    available: update && `Version ${update.version} is available.`,
    installing: "Downloading and checking the update. The app restarts when it's installed.",
    error: state.kind === "error" ? state.message : undefined,
  }[state.kind];

  return (
    <Row label="Check for updates" description={description}>
      <div className="button-row">
        {update && <button className="button" type="button" onClick={() => openExternal(update.url)}>What's new</button>}
        {update && state.kind !== "installing" && update.install !== "manual"
          ? <button className="button button--primary" type="button" onClick={() => install(update)}>Install and restart</button>
          : update && update.install === "manual"
            ? <button className="button button--primary" type="button" onClick={() => openExternal(update.url)}>Download</button>
            : <button className={`button ${state.kind === "checking" || state.kind === "installing" ? "is-busy" : ""}`} type="button" disabled={state.kind === "checking" || state.kind === "installing"} onClick={check}>
                {state.kind === "installing" ? "Installing…" : "Check now"}
              </button>}
      </div>
    </Row>
  );
}

function Switch({ checked, onChange, label, disabled = false }: { checked: boolean; onChange: (value: boolean) => void; label: string; disabled?: boolean }) {
  return (
    <button className="switch" type="button" role="switch" aria-checked={checked} aria-label={label} disabled={disabled} onClick={() => onChange(!checked)}>
      <span className="switch__thumb" />
    </button>
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

function schemeImportMessage(added: TerminalTheme[], repeated: TerminalTheme[]): string {
  const known = repeated.length === 1 ? `${repeated[0]!.name} is already built in` : `${repeated.length} schemes are already built in`;
  if (added.length === 0) return known;
  const imported = added.length === 1 ? `Imported ${added[0]!.name}` : `Imported ${added.length} color schemes`;
  return repeated.length === 0 ? imported : `${imported}; ${known}`;
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
      // A bare key would be swallowed in every terminal (Tab, Enter, letters).
      if (!event.ctrlKey && !event.altKey && !event.metaKey && !/^F([1-9]|1[0-9]|2[0-4])$/.test(event.key)) return;
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

/** Records a global shortcut by physical key, and shows it as the keyboard prints it. */
function GlobalShortcutRecorder({ value, disabled, onChange }: { value: string; disabled: boolean; onChange: (value: string) => void }) {
  const [recording, setRecording] = useState(false);
  const [layout, setLayout] = useState<ReadonlyMap<string, string>>();

  useEffect(() => {
    void keyboardLayout().then(setLayout);
  }, []);

  useEffect(() => {
    if (!recording) return;
    const capture = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") { setRecording(false); return; }
      const shortcut = globalShortcutFromEvent(event);
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

  const label = globalShortcutLabel(value, layout);
  return (
    <div className="shortcut">
      <button
        type="button"
        data-shortcut-recorder
        className={`shortcut__field ${recording ? "is-recording" : ""}`}
        disabled={disabled}
        aria-label={`Quick terminal shortcut: ${label}. Press to change.`}
        onClick={() => setRecording((current) => !current)}
      >
        {recording ? <span className="shortcut__hint">Press Ctrl, Alt or Win and a key…</span> : <Keys shortcut={label} />}
      </button>
    </div>
  );
}

const HEX_COLOR = /^#[0-9a-f]{6}$/i;
const NEW_PROFILE = "new";
const NEW_CONNECTION = "new-ssh";

type LookProps = { preferences: AppearancePreferences; onChange: Dispatch<SetStateAction<AppearancePreferences>> };

function ownLook(preferences: AppearancePreferences, profileId: string): ProfileLook | undefined {
  return Object.hasOwn(preferences.profileLooks, profileId) ? preferences.profileLooks[profileId] : undefined;
}

/** A chip with the profile's own theme, and the button that opens its look. */
function LookControls({ profileId, name, preferences, open, onToggle }: { profileId: string; name: string; preferences: AppearancePreferences; open: boolean; onToggle: () => void }) {
  const look = ownLook(preferences, profileId);
  const theme = look?.themeId ? resolveTheme(look.themeId, preferences.customThemes) : undefined;
  return (
    <>
      {look && (
        <span className="look-chip" title={`${name} has a look of its own`}>
          {theme && <span className="look-chip__swatch" style={{ background: theme.background, borderColor: theme.accent }} aria-hidden="true" />}
          {theme?.name ?? "Own font"}
        </span>
      )}
      <button className={`icon-button ${open ? "is-active" : ""}`} type="button" aria-expanded={open} aria-label={`Look of ${name}`} title="Theme and font for this profile" onClick={onToggle}>
        <Paintbrush size={15} strokeWidth={1.8} />
      </button>
    </>
  );
}

/** A profile's own theme, font and size. Whatever is left empty is the app's. */
function ProfileLookEditor({ profileId, name, preferences, onChange }: LookProps & { profileId: string; name: string }) {
  const look = ownLook(preferences, profileId) ?? {};
  const set = (change: ProfileLook) => onChange((current) => ({ ...current, profileLooks: withLook(current.profileLooks, profileId, change) }));
  const all = [...themes, ...preferences.customThemes];
  const id = useId();
  const byScheme = (scheme: TerminalTheme["scheme"]) => all.filter((theme) => theme.scheme === scheme).map((theme) => <option key={theme.id} value={theme.id}>{theme.name}</option>);
  return (
    <div className="look-editor" role="group" aria-label={`Look of ${name}`}>
      <div className="editor-field">
        <label className="editor-field__label" htmlFor={`${id}-theme`}>Theme</label>
        <select id={`${id}-theme`} className="field" value={look.themeId ?? ""} onChange={(event) => set({ themeId: event.target.value || undefined })}>
          <option value="">Same as the app</option>
          <optgroup label="Dark">{byScheme("dark")}</optgroup>
          <optgroup label="Light">{byScheme("light")}</optgroup>
        </select>
      </div>
      <div className="editor-field">
        <label className="editor-field__label" htmlFor={`${id}-font`}>Font</label>
        <input id={`${id}-font`} className="field" value={look.fontFamily ?? ""} maxLength={240} spellCheck={false} placeholder="Same as the app" onChange={(event) => set({ fontFamily: event.target.value || undefined })} />
      </div>
      <div className="editor-field look-editor__size">
        <label className="editor-field__label" htmlFor={`${id}-size`}>Size</label>
        <input id={`${id}-size`} className="field" type="number" min={8} max={32} value={look.fontSize ?? ""} placeholder={String(preferences.fontSize)} onChange={(event) => set({ fontSize: event.target.value ? Math.min(32, Math.max(8, Math.round(Number(event.target.value)))) : undefined })} />
      </div>
      <p className="field-note look-editor__note">Tabs of this profile use it, and the window takes its colors when one is in front.</p>
    </div>
  );
}

/** One detected profile: icon, name, what it runs, and its look. */
function ProfileRow({ profile, preferences, onChange }: LookProps & { profile: TerminalProfile }) {
  const [open, setOpen] = useState(false);
  const missing = profileGroup(profile) === "custom" ? "Program not found" : "Not installed";
  return (
    <>
      <div className={`shell-row shell-row--actions ${profile.available ? "" : "is-missing"}`}>
        <ProfileIcon kind={profile.kind} accent={profile.accent} size={20} />
        <div className="row__text">
          <span className="row__label">{profile.name}</span>
          <span className="row__description" title={profile.commandLine ?? profile.executable ?? undefined}>{profile.available ? profile.commandLine ?? profile.executable : missing}</span>
        </div>
        <div className="shell-row__actions">
          {profile.available && <span className="badge"><Check size={12} strokeWidth={2.4} />Ready</span>}
          <LookControls profileId={profile.id} name={profile.name} preferences={preferences} open={open} onToggle={() => setOpen((current) => !current)} />
        </div>
      </div>
      {open && <ProfileLookEditor profileId={profile.id} name={profile.name} preferences={preferences} onChange={onChange} />}
    </>
  );
}

/** Shows how the arguments line will be split, as the desktop host splits it. */
function ArgumentPreview({ line }: { line: string }) {
  const [args, setArgs] = useState<string[] | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    const timer = window.setTimeout(() => {
      splitArguments(line)
        .then((value) => { if (current) { setArgs(value); setProblem(null); } })
        .catch((error: unknown) => { if (current) setProblem(String(error)); });
    }, 120);
    return () => { current = false; window.clearTimeout(timer); };
  }, [line]);

  if (problem) return <p className="field-note field-note--error">{problem}</p>;
  if (!args) return null;
  return (
    <div className="arg-preview" aria-live="polite">
      <span className="arg-preview__label">{args.length === 0 ? "No arguments" : args.length === 1 ? "1 argument" : `${args.length} arguments`}</span>
      {args.map((arg, index) => <code key={index} className="arg-chip">{arg || <em>empty</em>}</code>)}
    </div>
  );
}

function EditorField({ id, label, note, error, children }: { id: string; label: string; note?: ReactNode; error?: string; children: ReactNode }) {
  return (
    <div className={`editor-field ${error ? "has-error" : ""}`}>
      <label className="editor-field__label" htmlFor={id} id={`${id}-label`}>{label}</label>
      {children}
      {error ? <p className="field-note field-note--error" id={`${id}-error`} role="alert">{error}</p> : note && <p className="field-note">{note}</p>}
    </div>
  );
}

/** Add or edit one custom profile. The host validates it again before saving. */
function ProfileEditor({ initial, onSave, onCancel }: { initial: CustomProfileDraft; onSave: (draft: CustomProfileDraft) => Promise<void>; onCancel: () => void }) {
  const [draft, setDraft] = useState(initial);
  const [error, setError] = useState<ProfileFieldError | null>(null);
  const [saving, setSaving] = useState(false);
  const [showForwards, setShowForwards] = useState(Boolean(initial.ssh.localForwards || initial.ssh.remoteForwards));
  const id = useId();
  const set = (key: "name" | "executable" | "arguments" | "cwd") => (event: { target: { value: string } }) => {
    setDraft((current) => ({ ...current, [key]: event.target.value }));
    if (error?.field === key) setError(null);
  };
  const setSsh = (key: keyof SshDraft) => (event: { target: { value: string } }) => {
    setDraft((current) => ({ ...current, ssh: { ...current.ssh, [key]: event.target.value } }));
    if (error?.field === key) setError(null);
  };
  const errorFor = (field: CustomProfileField) => error?.field === field ? error.message : undefined;
  const invalid = (field: CustomProfileField) => error?.field === field ? { "aria-invalid": true, "aria-describedby": `${id}-${field}-error` } : {};
  const accent = draft.accent.toLowerCase();
  const ssh = draft.kind === "ssh";
  const forwardsError = error?.field === "localForwards" || error?.field === "remoteForwards";

  useEffect(() => {
    if (forwardsError && !showForwards) setShowForwards(true);
    else if (error?.field) document.getElementById(`${id}-${error.field}`)?.focus();
  }, [error, forwardsError, showForwards, id]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (saving) return;
    setSaving(true);
    try {
      await onSave(draft);
    } catch (reason) {
      setError(reason && typeof reason === "object" && "message" in reason ? reason as ProfileFieldError : { field: null, message: String(reason) });
      setSaving(false);
    }
  };

  return (
    <form
      className="profile-editor"
      aria-label={initial.id ? `Edit ${initial.name}` : "New profile"}
      noValidate
      onSubmit={(event) => void submit(event)}
      onKeyDown={(event) => {
        if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); onCancel(); }
      }}
    >
      <div className="profile-editor__head">
        <ProfileIcon kind={ssh ? "ssh" : "custom"} accent={HEX_COLOR.test(accent) ? accent : undefined} size={20} />
        <span className="row__label">{initial.id ? (ssh ? "Edit connection" : "Edit profile") : (ssh ? "New SSH connection" : "New profile")}</span>
        <Segmented label="Profile type" value={draft.kind} onChange={(kind) => { setDraft((current) => ({ ...current, kind })); setError(null); }} options={[{ value: "program", label: "Program" }, { value: "ssh", label: "SSH connection" }]} />
      </div>

      <EditorField id={`${id}-name`} label="Name" error={errorFor("name")}>
        <input id={`${id}-name`} className="field" value={draft.name} maxLength={60} autoFocus spellCheck={false} placeholder={ssh ? "Build server" : "Python"} onChange={set("name")} {...invalid("name")} />
      </EditorField>

      {ssh ? <>
        <EditorField id={`${id}-host`} label="Server" error={errorFor("host")} note="A host name, an IP address, or a Host from ~/.ssh/config.">
          <input id={`${id}-host`} className="field field--mono" value={draft.ssh.host} maxLength={253} spellCheck={false} autoCapitalize="off" placeholder="server.example.com" onChange={setSsh("host")} {...invalid("host")} />
        </EditorField>

        <div className="editor-field-row">
          <EditorField id={`${id}-user`} label="User" error={errorFor("user")}>
            <input id={`${id}-user`} className="field field--mono" value={draft.ssh.user} maxLength={64} spellCheck={false} autoCapitalize="off" placeholder="Same as on Windows" onChange={setSsh("user")} {...invalid("user")} />
          </EditorField>
          <EditorField id={`${id}-port`} label="Port" error={errorFor("port")}>
            <input id={`${id}-port`} className="field field--mono" value={draft.ssh.port} maxLength={5} inputMode="numeric" spellCheck={false} placeholder="22" onChange={setSsh("port")} {...invalid("port")} />
          </EditorField>
        </div>

        <EditorField id={`${id}-identityFile`} label="Key file" error={errorFor("identityFile")} note="Optional. Without one, ssh tries your keys in ~/.ssh and the ssh-agent. Accepts ~ and %VARIABLES%.">
          <input id={`${id}-identityFile`} className="field field--mono" value={draft.ssh.identityFile} maxLength={1024} spellCheck={false} placeholder="~\.ssh\id_ed25519" onChange={setSsh("identityFile")} {...invalid("identityFile")} />
        </EditorField>

        <button className="disclosure" type="button" aria-expanded={showForwards} aria-controls={`${id}-forwards`} onClick={() => setShowForwards((open) => !open)}>
          <ChevronRight size={14} aria-hidden="true" />Port forwarding
        </button>
        {showForwards && <div className="editor-field-row" id={`${id}-forwards`}>
          <EditorField id={`${id}-localForwards`} label="Local ports" error={errorFor("localForwards")} note="A port on this PC, then where the server sends it. One per line.">
            <textarea id={`${id}-localForwards`} className="field field--mono field--area" rows={3} value={draft.ssh.localForwards} maxLength={2048} spellCheck={false} placeholder="8080:localhost:80" onChange={setSsh("localForwards")} {...invalid("localForwards")} />
          </EditorField>
          <EditorField id={`${id}-remoteForwards`} label="Remote ports" error={errorFor("remoteForwards")} note="A port on the server, then where this PC sends it. One per line.">
            <textarea id={`${id}-remoteForwards`} className="field field--mono field--area" rows={3} value={draft.ssh.remoteForwards} maxLength={2048} spellCheck={false} placeholder="9000:localhost:3000" onChange={setSsh("remoteForwards")} {...invalid("remoteForwards")} />
          </EditorField>
        </div>}

        <div className="arg-preview" aria-live="polite">
          <span className="arg-preview__label">Runs</span>
          <code className="arg-chip">{sshCommand(draft.ssh)}</code>
        </div>
      </> : <>
      <EditorField id={`${id}-executable`} label="Program" error={errorFor("executable")} note="A full path, or the name of a program on PATH. Accepts %VARIABLES%.">
        <input id={`${id}-executable`} className="field field--mono" value={draft.executable} maxLength={1024} spellCheck={false} placeholder="C:\Python313\python.exe" onChange={set("executable")} {...invalid("executable")} />
      </EditorField>

      <EditorField id={`${id}-arguments`} label="Arguments" error={errorFor("arguments")}>
        <input id={`${id}-arguments`} className="field field--mono" value={draft.arguments} maxLength={4096} spellCheck={false} placeholder="-NoExit -File &quot;C:\Scripts\start.ps1&quot;" onChange={set("arguments")} {...invalid("arguments")} />
        {!errorFor("arguments") && <ArgumentPreview line={draft.arguments} />}
      </EditorField>

      <EditorField id={`${id}-cwd`} label="Starting folder" error={errorFor("cwd")} note="Leave empty to use the starting folder above. Accepts ~ and %VARIABLES%.">
        <input id={`${id}-cwd`} className="field field--mono" value={draft.cwd} maxLength={1024} spellCheck={false} placeholder="Default starting folder" onChange={set("cwd")} {...invalid("cwd")} />
      </EditorField>
      </>}

      <EditorField id={`${id}-accent`} label="Color" error={errorFor("accent")}>
        <div className="swatches" role="radiogroup" aria-labelledby={`${id}-accent-label`} id={`${id}-accent`}>
          {accentSwatches.map((color) => (
            <button key={color} type="button" role="radio" aria-checked={accent === color} aria-label={color} className="swatch" style={{ background: color }} onClick={() => setDraft((current) => ({ ...current, accent: color }))} />
          ))}
          <label className="swatch swatch--custom" title="Custom color">
            <input type="color" value={HEX_COLOR.test(accent) ? accent : "#9aa3ab"} onChange={(event) => setDraft((current) => ({ ...current, accent: event.target.value }))} aria-label="Custom profile color" />
          </label>
        </div>
      </EditorField>

      <div className="profile-editor__footer">
        {error && error.field === null && <p className="field-note field-note--error" role="alert">{error.message}</p>}
        <div className="button-row">
          <button className="button" type="button" onClick={onCancel}>Cancel</button>
          <button className="button button--primary" type="submit" disabled={saving}>{initial.id ? "Save" : ssh ? "Add connection" : "Add profile"}</button>
        </div>
      </div>
    </form>
  );
}

/** The user's own profiles, with an inline editor to add, change and remove them. */
function CustomProfilesGroup({ profiles, defaultProfileId, onDefaultRemoved, onProfilesChanged, preferences, onChange }: LookProps & { profiles: TerminalProfile[]; defaultProfileId: string; onDefaultRemoved: () => void; onProfilesChanged: () => Promise<void> }) {
  const [entries, setEntries] = useState<CustomProfile[] | null>(null);
  const [lookOpen, setLookOpen] = useState<string | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    listCustomProfiles()
      .then((list) => { if (current) setEntries(list); })
      .catch((error: unknown) => { if (current) { setEntries([]); setProblem(String(error)); } });
    return () => { current = false; };
  }, []);

  if (!entries) return null;

  const detected = new Map(profiles.map((profile) => [profile.id, profile]));
  const unusedAccent = accentSwatches.find((color) => !entries.some((entry) => entry.accent === color)) ?? accentSwatches[0]!;

  const save = async (draft: CustomProfileDraft) => {
    const saved = await saveCustomProfile(draft);
    setEntries((current) => {
      const list = current ?? [];
      return list.some((entry) => entry.id === saved.id) ? list.map((entry) => entry.id === saved.id ? saved : entry) : [...list, saved];
    });
    setEditing(null);
    await onProfilesChanged();
  };

  const remove = async (id: string) => {
    try {
      await deleteCustomProfile(id);
      setEntries((current) => (current ?? []).filter((entry) => entry.id !== id));
      setRemoving(null);
      setProblem(null);
      if (defaultProfileId === id) onDefaultRemoved();
      await onProfilesChanged();
    } catch (error) {
      setProblem(String(error));
    }
  };

  const editorFor = (entry?: CustomProfile) => (
    <ProfileEditor
      key={entry?.id ?? editing}
      initial={entry ? draftOf(entry) : newDraft(unusedAccent, editing === NEW_CONNECTION ? "ssh" : "program")}
      onSave={save}
      onCancel={() => setEditing(null)}
    />
  );
  const startNew = (kind: string) => { setEditing(kind); setRemoving(null); };

  return (
    <Group title="Custom profiles" description="Open any program in a tab, or connect to a server over SSH, with its own name and color.">
      {entries.map((entry) => {
        if (editing === entry.id) return editorFor(entry);
        const available = detected.get(entry.id)?.available ?? true;
        const command = [entry.executable, entry.arguments].filter(Boolean).join(" ");
        const missing = entry.ssh ? "The OpenSSH client isn't installed" : `Program not found: ${entry.executable}`;
        return (
          <Fragment key={entry.id}>
          <div className={`shell-row shell-row--actions ${available ? "" : "is-missing"}`}>
            <ProfileIcon kind={entry.ssh ? "ssh" : "custom"} accent={entry.accent} size={20} />
            <div className="row__text">
              <span className="row__label">{entry.name}</span>
              <span className={`row__description ${available ? "" : "row__description--warning"}`} title={command}>{available ? command : missing}</span>
            </div>
            {removing === entry.id
              ? <div className="button-row">
                  <button className="button" type="button" onClick={() => setRemoving(null)}>Cancel</button>
                  <button className="button button--danger" type="button" autoFocus onClick={() => void remove(entry.id)}>Remove</button>
                </div>
              : <div className="shell-row__actions">
                  <LookControls profileId={entry.id} name={entry.name} preferences={preferences} open={lookOpen === entry.id} onToggle={() => setLookOpen((current) => current === entry.id ? null : entry.id)} />
                  <button className="icon-button" type="button" aria-label={`Edit ${entry.name}`} title="Edit" disabled={editing !== null} onClick={() => { setEditing(entry.id); setRemoving(null); }}><Pencil size={15} strokeWidth={1.8} /></button>
                  <button className="icon-button" type="button" aria-label={`Remove ${entry.name}`} title="Remove" disabled={editing !== null} onClick={() => setRemoving(entry.id)}><Trash size={15} strokeWidth={1.8} /></button>
                </div>}
          </div>
          {lookOpen === entry.id && <ProfileLookEditor profileId={entry.id} name={entry.name} preferences={preferences} onChange={onChange} />}
          </Fragment>
        );
      })}
      {editing === NEW_PROFILE || editing === NEW_CONNECTION
        ? editorFor()
        : <div className="row">
            <div className="row__text">
              <span className="row__label">{entries.length ? "Add another profile" : "Add a profile"}</span>
              <span className={`row__description ${problem ? "row__description--error" : ""}`}>{problem ?? (entries.length >= 50 ? "You can have up to 50 custom profiles." : "For example a REPL, a project shell or a server you reach over SSH.")}</span>
            </div>
            <div className="row__control button-row">
              <button className="button" type="button" disabled={editing !== null || entries.length >= 50} onClick={() => startNew(NEW_CONNECTION)}><Globe size={14} />Add SSH connection</button>
              <button className="button" type="button" disabled={editing !== null || entries.length >= 50} onClick={() => startNew(NEW_PROFILE)}><Plus size={14} />Add profile</button>
            </div>
          </div>}
    </Group>
  );
}

export default function SettingsPanel({ open, initialPage, profiles, preferences, onChange, onClose, onReset, onClearSession, onProfilesChanged, launchers, defaultTerminal, onDefaultTerminalChange, administrator = false, workspaces, onWorkspacesChange, quickTerminalProblem = null, sync }: SettingsPanelProps) {
  const [page, setPage] = useState<SettingsPage>("appearance");
  const [query, setQuery] = useState("");
  const words = useMemo(() => query.toLowerCase().split(/\s+/).filter(Boolean), [query]);
  const searching = words.length > 0;
  const shows = (id: SettingsPage) => searching || page === id;
  const pageLabel = (id: SettingsPage) => pages.find((item) => item.id === id)?.label ?? id;
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const [found, setFound] = useState(true);
  useLayoutEffect(() => {
    setFound(!searching || Boolean(scrollRef.current?.querySelector(".group:not(.is-filtered), .row--match")));
  });
  const [message, setMessage] = useState<{ tone: "error" | "info"; text: string } | null>(null);
  /** The workspace whose start commands are shown. */
  const [commandsOf, setCommandsOf] = useState<string | null>(null);
  const [confirmReset, setConfirmReset] = useState(false);
  const themeInputRef = useRef<HTMLInputElement | null>(null);
  const schemeInputRef = useRef<HTMLInputElement | null>(null);
  const imageInputRef = useRef<HTMLInputElement | null>(null);
  const panelRef = useRef<HTMLElement | null>(null);

  // The panel takes the focus when it opens, and only then: every change of a setting
  // renders the app again, with a new onClose, and must not pull the focus out of a field.
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  useEffect(() => {
    if (!open) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented) onCloseRef.current();
    };
    window.addEventListener("keydown", closeOnEscape);
    requestAnimationFrame(() => panelRef.current?.querySelector<HTMLButtonElement>(".settings__nav [aria-current='page']")?.focus());
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [open]);

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

  const importSchemes = async (file?: File) => {
    if (!file) return;
    if (file.size > 2_000_000) return setMessage({ tone: "error", text: "That file is too large to be a color scheme." });
    try {
      const { added, repeated } = sortOutSchemes(parseColorSchemes(await file.text(), file.name), themes);
      const kept = added.slice(0, MAX_CUSTOM_THEMES);
      const replaced = new Set(kept.map((theme) => theme.id));
      const first = kept[0] ?? repeated[0]!;
      onChange((current) => ({
        ...current,
        customThemes: [...current.customThemes.filter((theme) => !replaced.has(theme.id)), ...kept].slice(-MAX_CUSTOM_THEMES),
        themeId: first.id,
        accent: first.accent,
      }));
      setMessage({ tone: "info", text: schemeImportMessage(kept, repeated) });
    } catch (error) {
      setMessage({ tone: "error", text: error instanceof Error ? error.message : String(error) });
    }
  };


  const importTheme = async (file?: File) => {
    if (!file) return;
    try {
      onChange(await importAppearance(file));
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
      const backgroundImage = await shrinkImage(file);
      onChange((current) => ({ ...current, backgroundMode: "image", backgroundImage }));
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
        <nav className={`settings__nav ${searching ? "is-searching" : ""}`} aria-label="Settings sections" style={{ "--index": pageIndex } as React.CSSProperties}>
          <h2>Settings</h2>
          <label className="settings__search">
            <Search size={14} strokeWidth={1.8} aria-hidden="true" />
            <input type="search" value={query} placeholder="Search settings" aria-label="Search settings" spellCheck={false} onChange={(event) => setQuery(event.target.value)} />
          </label>
          <div className="settings__nav-items">
            <span className="settings__indicator" aria-hidden="true" />
            {pages.map(({ id, label, icon: Icon }) => (
              <button key={id} type="button" aria-current={!searching && page === id ? "page" : undefined} onClick={() => { setQuery(""); setPage(id); }}>
                <Icon size={16} strokeWidth={1.7} aria-hidden="true" />
                {label}
              </button>
            ))}
          </div>
        </nav>

        <div className="settings__main">
          <header className="settings__header">
            <h1>{searching ? "Search" : pages[pageIndex]?.label}</h1>
            <button className="icon-button" type="button" onClick={onClose} aria-label="Close settings" title="Close (Esc)"><X size={16} /></button>
          </header>
          {administrator && <p className="settings__note">Admin windows take their settings from the window that opens them, so changes made here don't last.</p>}

          <div className="settings__scroll" ref={scrollRef}>
            <SearchContext.Provider value={{ words, group: "", groupMatches: false }}>
            <div className={`settings__page ${searching ? "is-searching" : ""}`} key={searching ? "search" : page}>
              {searching && !found && <p className="settings__empty">Nothing matches “{query.trim()}”.</p>}
              {shows("appearance") && <section className="settings__section" aria-label={searching ? pageLabel("appearance") : undefined}>
                {searching && <h2 className="settings__section-title">{pageLabel("appearance")}</h2>}
                <Group title="Theme">
                  <Row label="Follow Windows" description={preferences.followSystemTheme ? "Pick one dark and one light theme below. The window wears the one that matches Windows' app mode." : "Switch between a dark and a light theme with Windows' app mode."}>
                    <Switch label="Follow Windows" checked={preferences.followSystemTheme} onChange={(value) => patch("followSystemTheme", value)} />
                  </Row>
                  <ThemeSection preferences={preferences} onChange={onChange} />
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
                  <Row label="Mark administrator windows" description="Administrator windows get a red accent, so you always know which shells are elevated.">
                    <Switch label="Mark administrator windows" checked={preferences.markAdministratorWindows} onChange={(value) => patch("markAdministratorWindows", value)} />
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
                  <Row label="Color scheme" description="From Windows Terminal (a scheme, or your whole settings.json) or iTerm2 (.itermcolors).">
                    <button className="button" type="button" onClick={() => schemeInputRef.current?.click()}>Import…</button>
                  </Row>
                  <input ref={schemeInputRef} hidden type="file" accept=".json,.itermcolors,application/json" onChange={(event) => { void importSchemes(event.target.files?.[0]); event.target.value = ""; }} />
                  <Row label="Theme file" description="Colors, fonts and window style. Never includes paths or shortcuts.">
                    <div className="button-row">
                      <button className="button" type="button" onClick={() => themeInputRef.current?.click()}>Import…</button>
                      <button className="button" type="button" onClick={() => exportAppearance(preferences)}>Export</button>
                    </div>
                  </Row>
                  <input ref={themeInputRef} hidden type="file" accept="application/json,.json" onChange={(event) => { void importTheme(event.target.files?.[0]); event.target.value = ""; }} />
                </Group>
              </section>}

              {shows("terminal") && <section className="settings__section" aria-label={searching ? pageLabel("terminal") : undefined}>
                {searching && <h2 className="settings__section-title">{pageLabel("terminal")}</h2>}
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
                  <Row label="Ligatures" description="=>, != and the like drawn as one symbol, in fonts that have ligatures: Cascadia Code, Fira Code, JetBrains Mono.">
                    <Switch label="Ligatures" checked={preferences.fontLigatures} onChange={(value) => patch("fontLigatures", value)} />
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
                <Group title="Commands">
                  <Row label="Command blocks" description="Point at a finished command to see it as one block and copy it, copy its output or run it again. Works with shell integration: Nebula, PowerShell, Git Bash and WSL.">
                    <Switch label="Command blocks" checked={preferences.commandBlocks} onChange={(value) => patch("commandBlocks", value)} />
                  </Row>
                </Group>
                <Group title="Highlights" description="Words colored in every pane as they're printed, like error or FAILED. Write /E\d{4}/ for a regular expression.">
                  {preferences.highlights.map((highlight, index) => {
                    const change = (update: Partial<typeof highlight>) => patch("highlights", preferences.highlights.map((candidate) => candidate.id === highlight.id ? { ...candidate, ...update } : candidate));
                    const broken = highlight.text.trim() !== "" && !highlightPattern(highlight.text);
                    return (
                      <div key={highlight.id} className="row highlight-row">
                        <input
                          className={`field field--mono ${broken ? "is-invalid" : ""}`}
                          value={highlight.text}
                          maxLength={200}
                          spellCheck={false}
                          placeholder="error"
                          aria-label={`Highlight ${index + 1}`}
                          aria-invalid={broken}
                          title={broken ? "This regular expression doesn't work" : undefined}
                          autoFocus={!highlight.text}
                          onChange={(event) => change({ text: event.target.value })}
                        />
                        <div className="highlight-row__colors" role="radiogroup" aria-label={`Color of highlight ${index + 1}`}>
                          {tabColors.map((color) => (
                            <button key={color.id} type="button" role="radio" className="menu__swatch" style={{ "--swatch": color.value } as React.CSSProperties} aria-checked={highlight.color === color.id} aria-label={color.name} title={color.name} onClick={() => change({ color: color.id })} />
                          ))}
                        </div>
                        <label className="highlight-row__notify">
                          <Switch label={`Notify for highlight ${index + 1}`} checked={highlight.notify} onChange={(value) => change({ notify: value })} />
                          <span>Notify</span>
                        </label>
                        <button className="icon-button" type="button" aria-label={`Remove highlight ${index + 1}`} title="Remove" onClick={() => patch("highlights", preferences.highlights.filter((candidate) => candidate.id !== highlight.id))}><Trash size={15} strokeWidth={1.8} /></button>
                      </div>
                    );
                  })}
                  <Row label={preferences.highlights.length ? "Add another" : "Highlight a word"} description="Notify tells you when it shows up in a tab you're not looking at.">
                    <button className="button" type="button" disabled={preferences.highlights.length >= MAX_HIGHLIGHTS} onClick={() => patch("highlights", [...preferences.highlights, { id: crypto.randomUUID(), text: "", color: "red", notify: false }])}><Plus size={14} />Add highlight</button>
                  </Row>
                </Group>
                <Group title="Accessibility">
                  <Row label="Screen reader support" description="Narrator, NVDA and JAWS can read the output and follow the cursor. Uses a little more memory and time per line.">
                    <Switch label="Screen reader support" checked={preferences.screenReaderMode} onChange={(value) => patch("screenReaderMode", value)} />
                  </Row>
                  <Row label="Follow Windows contrast themes" description="When a contrast theme is on in Windows, terminals use High Contrast Dark or Light, whatever their own theme.">
                    <Switch label="Follow Windows contrast themes" checked={preferences.followContrastThemes} onChange={(value) => patch("followContrastThemes", value)} />
                  </Row>
                </Group>
                <Group title="What programs can do">
                  <Row label="Show images" description="Sixel and iTerm2 images, from tools like chafa, timg or viu.">
                    <Switch label="Show images" checked={preferences.terminalImages} onChange={(value) => patch("terminalImages", value)} />
                  </Row>
                  <Row label="Copy to the clipboard" description="Programs like vim or tmux, here or over SSH, can put text on your clipboard (OSC 52). They can never read it.">
                    <Switch label="Copy to the clipboard" checked={preferences.clipboardWrite} onChange={(value) => patch("clipboardWrite", value)} />
                  </Row>
                </Group>
              </section>}

              {shows("profiles") && <section className="settings__section" aria-label={searching ? pageLabel("profiles") : undefined}>
                {searching && <h2 className="settings__section-title">{pageLabel("profiles")}</h2>}
                <Group>
                  <Row label="Default profile" description="Opened by the + button and at startup.">
                    <select className="field field--select" value={available.some((profile) => profile.id === preferences.defaultProfileId) ? preferences.defaultProfileId : ""} onChange={(event) => patch("defaultProfileId", event.target.value)}>
                      <option value="">Automatic{available[0] ? ` (${available[0].name})` : ""}</option>
                      {groupProfiles(available).map(({ group, profiles: members }) => {
                        const options = members.map((profile) => <option key={profile.id} value={profile.id}>{profile.name}</option>);
                        return group === "shell" ? options : <optgroup key={group} label={profileGroupLabels[group]}>{options}</optgroup>;
                      })}
                    </select>
                  </Row>
                  <Row label="Starting folder" description="Where new shells start. A profile with its own folder keeps it." stacked>
                    <Segmented
                      label="Starting folder"
                      value={preferences.startingFolder}
                      options={[
                        { value: "home", label: "User folder" },
                        { value: "desktop", label: "Desktop" },
                        { value: "documents", label: "Documents" },
                        { value: "custom", label: "Other" },
                      ]}
                      onChange={(value) => patch("startingFolder", value)}
                    />
                    {preferences.startingFolder === "custom" && (
                      <input className="field" value={preferences.workingDirectory} spellCheck={false} aria-label="Folder" placeholder="%USERPROFILE%\Projects" onChange={(event) => patch("workingDirectory", event.target.value)} />
                    )}
                  </Row>
                  <Row label="Open new tabs and splits in the current folder" description="Uses the folder the focused shell is in.">
                    <Switch label="Open new tabs and splits in the current folder" checked={preferences.openInCurrentFolder} onChange={(value) => patch("openInCurrentFolder", value)} />
                  </Row>
                  <Row label="Shell integration" description="PowerShell, Git Bash and WSL mark their commands and report their folder, as Nebula does. Your profile still runs first. Applies to new tabs.">
                    <Switch label="Shell integration" checked={preferences.shellIntegration} onChange={(value) => patch("shellIntegration", value)} />
                  </Row>
                </Group>
                <CustomProfilesGroup profiles={profiles} defaultProfileId={preferences.defaultProfileId} onDefaultRemoved={() => patch("defaultProfileId", "")} onProfilesChanged={onProfilesChanged} preferences={preferences} onChange={onChange} />
                {/* Custom profiles, SSH connections included, are listed in their own group above. */}
                {groupProfiles(profiles.filter((profile) => !profile.id.startsWith("custom:"))).map(({ group, profiles: members }) => (
                  <Group
                    key={group}
                    title={group === "shell" ? "Detected on this PC" : profileGroupLabels[group]}
                    description={group === "wsl" ? "One profile for each installed distribution." : group === "ssh" ? "Hosts from your ~/.ssh/config. Edit that file to add more." : undefined}
                  >
                    {members.map((profile) => <ProfileRow key={profile.id} profile={profile} preferences={preferences} onChange={onChange} />)}
                  </Group>
                ))}
              </section>}

              {shows("keyboard") && <section className="settings__section" aria-label={searching ? pageLabel("keyboard") : undefined}>
                {searching && <h2 className="settings__section-title">{pageLabel("keyboard")}</h2>}
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
                  <Row label="Resize the pane in front"><Keys shortcut="Alt+Shift+Arrow" /></Row>
                  <Row label="Go to tab 1–9"><Keys shortcut="Ctrl+Alt+1…9" /></Row>
                  <Row label="Zoom"><Keys shortcut="Ctrl+Wheel" /></Row>
                </Group>
                <div className="settings__footer-actions">
                  <button className="button" type="button" onClick={() => patch("keybindings", { ...defaultKeybindings })}><RotateCcw size={14} />Restore default shortcuts</button>
                </div>
              </section>}

              {shows("behavior") && <section className="settings__section" aria-label={searching ? pageLabel("behavior") : undefined}>
                {searching && <h2 className="settings__section-title">{pageLabel("behavior")}</h2>}
                <Group title="Clipboard">
                  <Row label="Copy on select" description="Selecting text copies it right away.">
                    <Switch label="Copy on select" checked={preferences.copyOnSelect} onChange={(value) => patch("copyOnSelect", value)} />
                  </Row>
                  <Row label="Confirm multi-line paste" description="Review pasted text that would run several commands.">
                    <Switch label="Confirm multi-line paste" checked={preferences.confirmMultilinePaste} onChange={(value) => patch("confirmMultilinePaste", value)} />
                  </Row>
                </Group>
                <Group title="Files in the output">
                  <Row label="Open files with" description="Ctrl+Click a path like src/main.rs:42 to open it at that line. Folders open in File Explorer.">
                    <select className="field field--select" value={preferences.fileOpener} onChange={(event) => patch("fileOpener", event.target.value as AppearancePreferences["fileOpener"])}>
                      <option value="auto">VS Code or Cursor, if installed</option>
                      <option value="vscode">VS Code</option>
                      <option value="cursor">Cursor</option>
                      <option value="default">The app Windows uses for the file</option>
                      <option value="custom">A command of my own</option>
                    </select>
                  </Row>
                  {preferences.fileOpener === "custom" && (
                    <Row label="Command" description="{file}, {line} and {column} are filled in; without {file}, the file goes last." stacked>
                      <input className="field field--mono" value={preferences.fileOpenerCommand} spellCheck={false} aria-label="Command to open files with" placeholder="notepad++ -n{line} {file}" onChange={(event) => patch("fileOpenerCommand", event.target.value)} />
                    </Row>
                  )}
                </Group>
                <Group title="Notifications">
                  <Row label="Notify when a long command finishes" description="When its tab is in the background or the window is not in front. Works in Nebula, in PowerShell, Git Bash and WSL with shell integration, and with programs that send notifications.">
                    <Switch label="Notify when a long command finishes" checked={preferences.notifyLongCommands} onChange={(value) => patch("notifyLongCommands", value)} />
                  </Row>
                  <Row label="Long means at least">
                    <select className="field field--select" value={preferences.longCommandSeconds} disabled={!preferences.notifyLongCommands} onChange={(event) => patch("longCommandSeconds", Number(event.target.value))}>
                      {[5, 10, 30, 60, 300].map((seconds) => <option key={seconds} value={seconds}>{seconds < 60 ? `${seconds} seconds` : `${seconds / 60} minute${seconds > 60 ? "s" : ""}`}</option>)}
                    </select>
                  </Row>
                </Group>
                <Group title="Quick terminal" description={administrator ? "Change this from an ordinary window." : undefined}>
                  <Row label="Quick terminal" description="A shortcut drops a terminal down from the top of the screen, over whatever you're doing, and the same shortcut puts it away. It has tabs of its own and works while Nebula Terminal is open.">
                    <Switch label="Quick terminal" checked={preferences.quickTerminal} disabled={administrator} onChange={(value) => patch("quickTerminal", value)} />
                  </Row>
                  <Row label="Shortcut" description={quickTerminalProblem ?? "Works whatever app is in front."} error={quickTerminalProblem !== null && preferences.quickTerminal}>
                    <GlobalShortcutRecorder value={preferences.quickTerminalShortcut} disabled={administrator || !preferences.quickTerminal} onChange={(value) => patch("quickTerminalShortcut", value)} />
                  </Row>
                  <Row label="Height">
                    <Segmented
                      label="Height of the quick terminal"
                      value={String(quickHeights.reduce((best, height) => Math.abs(height - preferences.quickTerminalHeight) < Math.abs(best - preferences.quickTerminalHeight) ? height : best))}
                      onChange={(value) => patch("quickTerminalHeight", Number(value))}
                      options={quickHeights.map((height) => ({ value: String(height), label: `${Math.round(height * 100)}%` }))}
                    />
                  </Row>
                  <Row label="Keep running in the notification area" description="When the last window closes, the shortcut still works. The icon there opens a window or quits.">
                    <Switch label="Keep running in the notification area" checked={preferences.quickTerminalKeepRunning} disabled={administrator || !preferences.quickTerminal} onChange={(value) => patch("quickTerminalKeepRunning", value)} />
                  </Row>
                  <Row label="Hide when you click elsewhere">
                    <Switch label="Hide when you click elsewhere" checked={preferences.quickTerminalHideOnBlur} disabled={administrator || !preferences.quickTerminal} onChange={(value) => patch("quickTerminalHideOnBlur", value)} />
                  </Row>
                </Group>
                <Group title="Windows" description={administrator ? "Change these from an ordinary window." : undefined}>
                  <Row
                    label="Default terminal"
                    description={defaultTerminal?.supported === false
                      ? "Needs Windows 11 version 22H2 or later."
                      : "Console programs you start from the Start menu, Explorer or Run open in a Nebula tab instead of a window of their own. Windows Terminal's console does the work behind it."}
                  >
                    <Switch label="Default terminal" checked={defaultTerminal?.enabled ?? false} disabled={!defaultTerminal?.supported} onChange={onDefaultTerminalChange} />
                  </Row>
                  <Row label="Open in Nebula Terminal" description="In the right-click menu of folders in File Explorer, with a second entry that opens an administrator window. On Windows 11, they're under Show more options.">
                    <Switch label="Open in Nebula Terminal" checked={launchers?.explorerMenu ?? false} disabled={!launchers} onChange={(value) => patch("explorerMenu", value)} />
                  </Row>
                  <Row label="Nebula in Windows Terminal" description="Adds Nebula to Windows Terminal's profiles, for when you work there. Windows Terminal picks it up the next time it starts.">
                    <Switch label="Nebula in Windows Terminal" checked={launchers?.windowsTerminal ?? false} disabled={!launchers} onChange={(value) => patch("windowsTerminalProfile", value)} />
                  </Row>
                  <Row label="nebula-terminal command" description="Type “nebula-terminal .” in a shell to open a tab in its folder. Shells that are already open don't see the change.">
                    <Switch label="nebula-terminal command" checked={launchers?.command ?? false} disabled={!launchers} onChange={(value) => patch("pathCommand", value)} />
                  </Row>
                </Group>
                <Group title="Workspaces" description={workspaces.length ? "Opened from the new-tab menu or the command palette, next to the tabs already open." : "Save the tabs you have open (splits and folders included) from the new-tab menu or the command palette, and open them again in one click."}>
                  {workspaces.length > 0 && (
                    <Row label="At startup, open" description="What the main window opens when Nebula Terminal starts.">
                      <select className="field field--select" value={workspaces.some((workspace) => workspace.id === preferences.startupWorkspace) ? preferences.startupWorkspace : ""} onChange={(event) => patch("startupWorkspace", event.target.value)}>
                        <option value="">The tabs from last time</option>
                        {workspaces.map((workspace) => <option key={workspace.id} value={workspace.id}>{workspace.name}</option>)}
                      </select>
                    </Row>
                  )}
                  {workspaces.map((workspace) => (
                    <Fragment key={workspace.id}>
                    <div className="row workspace-row">
                      <div className="row__text">
                        <input
                          key={workspace.name}
                          className="field workspace-row__name"
                          defaultValue={workspace.name}
                          maxLength={60}
                          spellCheck={false}
                          aria-label={`Name of ${workspace.name}`}
                          onBlur={(event) => event.target.value.trim() !== workspace.name && onWorkspacesChange(renamed(workspaces, workspace.id, event.target.value))}
                          onKeyDown={(event) => { if (event.key === "Enter") event.currentTarget.blur(); }}
                        />
                        <span className="row__description">{describeWorkspace(workspace)}, saved {new Date(workspace.savedAt).toLocaleDateString()}</span>
                      </div>
                      <div className="row__control button-row">
                        <button className="button" type="button" aria-expanded={commandsOf === workspace.id} onClick={() => setCommandsOf((open) => open === workspace.id ? null : workspace.id)}>
                          <ChevronRight size={14} className="workspace-row__chevron" />Commands
                        </button>
                        <button className="button" type="button" onClick={() => onWorkspacesChange(workspaces.filter((candidate) => candidate.id !== workspace.id))}><Trash size={14} />Delete</button>
                      </div>
                    </div>
                    {commandsOf === workspace.id && (
                      <div className="workspace-commands">
                        <p className="row__description">Typed into each pane when the workspace opens, as if you ran it there. Leave a pane empty for a plain shell.</p>
                        {workspacePanes(workspace).map((pane, _, all) => (
                          <label key={`${pane.tab}-${pane.pane}`} className="workspace-commands__pane">
                            <span className="workspace-commands__where">
                              <span>
                                {pane.tabTitle}{all.some((other) => other.tab === pane.tab && other.pane > 0) ? `, pane ${pane.pane + 1}` : ""}
                                {" · "}{profiles.find((profile) => profile.id === pane.profileId)?.name ?? pane.profileId}
                              </span>
                              {pane.cwd && <span className="row__description">{pane.cwd}</span>}
                            </span>
                            <input
                              key={pane.command}
                              className="field field--mono"
                              defaultValue={pane.command}
                              maxLength={2000}
                              spellCheck={false}
                              placeholder="npm run dev"
                              aria-label={`Command for ${pane.tabTitle}, pane ${pane.pane + 1}`}
                              onBlur={(event) => event.target.value.trim() !== pane.command && onWorkspacesChange(withPaneCommand(workspaces, workspace.id, pane.tab, pane.pane, event.target.value))}
                              onKeyDown={(event) => { if (event.key === "Enter") event.currentTarget.blur(); }}
                            />
                          </label>
                        ))}
                      </div>
                    )}
                    </Fragment>
                  ))}
                </Group>
                <Group title="Sync settings" description="Keep one settings file in OneDrive, Dropbox or a Git repository, and every PC that uses it gets the same themes, fonts, shortcuts and saved commands. Explorer and PATH entries, the starting folder, what opens files and the background image stay on each PC.">
                  <Row
                    label="Settings file"
                    description={sync?.problem ?? (preferences.syncFile ? preferences.syncFile : sync ? "Not synced. Choosing a file that already has settings uses them here." : "Change this from the main window.")}
                    error={Boolean(sync?.problem)}
                  >
                    <div className="button-row">
                      {preferences.syncFile && <button className="button" type="button" disabled={!sync} onClick={() => patch("syncFile", "")}>Stop syncing</button>}
                      <button className="button" type="button" disabled={!sync} onClick={sync?.choose}>{preferences.syncFile ? "Change file…" : "Choose file…"}</button>
                    </div>
                  </Row>
                </Group>
                <Group title="Saved commands" description="Run them from the command palette in the pane in front: each line runs as if you typed it and pressed Enter. Write {{host}} or {{branch}} for a part to fill in each time.">
                  {preferences.savedCommands.map((saved, index) => (
                    <div key={saved.id} className="row saved-command">
                      <input
                        className="field saved-command__name"
                        value={saved.name}
                        maxLength={60}
                        spellCheck={false}
                        placeholder="Name"
                        aria-label={`Name of saved command ${index + 1}`}
                        autoFocus={!saved.command && !saved.name}
                        onChange={(event) => patch("savedCommands", preferences.savedCommands.map((candidate) => candidate.id === saved.id ? { ...candidate, name: event.target.value } : candidate))}
                      />
                      <textarea
                        className="field field--mono field--area saved-command__text"
                        value={saved.command}
                        rows={Math.min(4, saved.command.split("\n").length)}
                        maxLength={2000}
                        spellCheck={false}
                        placeholder="git pull --rebase && npm install"
                        aria-label={`Saved command ${index + 1}`}
                        onChange={(event) => patch("savedCommands", preferences.savedCommands.map((candidate) => candidate.id === saved.id ? { ...candidate, command: event.target.value } : candidate))}
                      />
                      <button className="icon-button" type="button" aria-label={`Remove ${saved.name || `saved command ${index + 1}`}`} title="Remove" onClick={() => patch("savedCommands", preferences.savedCommands.filter((candidate) => candidate.id !== saved.id))}><Trash size={15} strokeWidth={1.8} /></button>
                    </div>
                  ))}
                  <div className="row">
                    <div className="row__text">
                      <span className="row__label">{preferences.savedCommands.length ? "Add another command" : "Save a command"}</span>
                      <span className="row__description">A build, a deploy, a login to a server: whatever you type often.</span>
                    </div>
                    <div className="row__control">
                      <button className="button" type="button" disabled={preferences.savedCommands.length >= 100} onClick={() => patch("savedCommands", [...preferences.savedCommands, { id: crypto.randomUUID(), name: "", command: "" }])}><Plus size={14} />Add command</button>
                    </div>
                  </div>
                </Group>
                <Group title="Tabs">
                  <Row label="Reopen tabs on launch" description="Restores tabs, splits and the folder each one was in. Shells start fresh.">
                    <Switch label="Reopen tabs on launch" checked={preferences.restoreSession} onChange={(value) => patch("restoreSession", value)} />
                  </Row>
                  <Row label="Show what panes printed" description="Reopened panes show their last output, up to 1,000 lines, above the new prompt. It's kept on this PC only.">
                    <Switch label="Show what panes printed" checked={preferences.restoreScreens && preferences.restoreSession} disabled={!preferences.restoreSession} onChange={(value) => patch("restoreScreens", value)} />
                  </Row>
                  <Row label="Reconnect SSH when it drops" description="A connection that drops tries again after 2, 5, 10, 20 and 30 seconds, with a countdown you can cancel.">
                    <Switch label="Reconnect SSH when it drops" checked={preferences.reconnectSsh} onChange={(value) => patch("reconnectSsh", value)} />
                  </Row>
                  <Row label="Confirm closing several tabs">
                    <Switch label="Confirm closing several tabs" checked={preferences.confirmCloseMultipleTabs} onChange={(value) => patch("confirmCloseMultipleTabs", value)} />
                  </Row>
                  <Row label="Saved tab layout">
                    <button className="button" type="button" onClick={() => { onClearSession(); setMessage({ tone: "info", text: "Saved layout cleared." }); }}>Clear</button>
                  </Row>
                </Group>
              </section>}

              {shows("about") && <section className="settings__section" aria-label={searching ? pageLabel("about") : undefined}>
                {searching && <h2 className="settings__section-title">{pageLabel("about")}</h2>}
                <div className="about">
                  <img src={appIcon} alt="" width={64} height={64} />
                  <div>
                    <h2>Nebula Terminal</h2>
                    <p>Version {version}</p>
                  </div>
                </div>
                <Group title="Updates">
                  <Row label="Check automatically" description="Looks for a new version on GitHub once a day. Nothing installs without you.">
                    <Switch label="Check for updates automatically" checked={preferences.checkForUpdates} onChange={(value) => patch("checkForUpdates", value)} />
                  </Row>
                  {administrator
                    ? <Row label="Install updates" description="From an ordinary window, so the app never restarts as administrator." />
                    : <UpdateCheck />}
                </Group>
                <Group>
                  <Row label="Source code"><button className="button" type="button" onClick={() => openExternal(REPOSITORY)}>Open on GitHub</button></Row>
                  <Row label="Found a bug?"><button className="button" type="button" onClick={() => openExternal(`${REPOSITORY}/issues/new/choose`)}>Report a problem</button></Row>
                  <Row label="License"><button className="button" type="button" onClick={() => openExternal(`${REPOSITORY}/blob/main/LICENSE`)}>PolyForm Shield 1.0.0</button></Row>
                </Group>
                <Group title="Reset">
                  <Row label="Reset all settings" description="Theme, fonts, shells and shortcuts go back to their defaults.">
                    {confirmReset
                      ? <div className="button-row"><button className="button" type="button" onClick={() => setConfirmReset(false)}>Cancel</button><button className="button button--danger" type="button" onClick={() => { onReset(); setConfirmReset(false); }}>Reset</button></div>
                      : <button className="button" type="button" onClick={() => setConfirmReset(true)}>Reset…</button>}
                  </Row>
                </Group>
              </section>}

              {message && <p className={`settings__message settings__message--${message.tone}`} role="status">{message.text}</p>}
            </div>
            </SearchContext.Provider>
          </div>
        </div>
      </section>
    </div>
  );
}

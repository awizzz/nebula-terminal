import { useRef, type Dispatch, type SetStateAction } from "react";
import { ArrowLeft, Download, RotateCcw, Trash, TriangleAlert } from "lucide-react";
import type { TerminalTheme } from "../themes";
import { contrastRatio, normalizeColor } from "../themeImport";
import { ANSI_NAMES, COLOR_CONTRAST, TEXT_CONTRAST, colorOf, downloadScheme, isChromatic, lowContrast, withColor, type ThemeColor } from "../themeEdit";
import type { AppearancePreferences } from "../types";

interface ThemeEditorProps {
  themeId: string;
  preferences: AppearancePreferences;
  onChange: Dispatch<SetStateAction<AppearancePreferences>>;
  /** `unchanged` is true when nothing was edited since the editor opened. */
  onClose: (unchanged: boolean) => void;
  onDelete: (id: string) => void;
}

/** `label` is what shows; `fullLabel`, what screen readers and the tooltip say. */
function ColorField({ label, fullLabel = label, value, warning, compact = false, onChange }: { label: string; fullLabel?: string; value: string; warning?: string; compact?: boolean; onChange: (value: string) => void }) {
  return (
    <div className={`color-field ${compact ? "color-field--compact" : ""} ${warning ? "has-warning" : ""}`} title={warning ?? `${fullLabel} ${value}`}>
      <label className="color-field__swatch" style={{ background: value }}>
        <input type="color" value={value} aria-label={fullLabel} onChange={(event) => onChange(event.target.value)} />
        {warning && <TriangleAlert className="color-field__warning" size={12} strokeWidth={2.4} aria-hidden="true" />}
      </label>
      <span className="color-field__text">
        <span className="color-field__label">{label}</span>
        {!compact && (
          <input
            key={value}
            className="color-field__hex"
            defaultValue={value}
            spellCheck={false}
            aria-label={`${fullLabel}, hex`}
            onBlur={(event) => {
              const hex = normalizeColor(event.target.value);
              if (hex) onChange(hex);
              else event.target.value = value;
            }}
            onKeyDown={(event) => { if (event.key === "Enter") event.currentTarget.blur(); }}
          />
        )}
      </span>
    </div>
  );
}

/** A terminal session that uses every color, to judge the theme as a whole. */
function ThemeSample({ theme }: { theme: TerminalTheme }) {
  const [, red, green, yellow, blue, magenta, cyan, , gray] = theme.ansi;
  return (
    <div className="theme-sample" style={{ background: theme.background, color: theme.foreground }} aria-hidden="true">
      <span><b style={{ color: blue }}>~/projects/app</b> on <b style={{ color: magenta }}>main</b> <span style={{ color: yellow }}>!1</span> <span style={{ color: gray }}>?1</span></span>
      <span><b style={{ color: green }}>❯ npm</b> test</span>
      <span><span style={{ color: green }}>✓</span> parses arguments <span style={{ color: gray }}>3 ms</span></span>
      <span><span style={{ color: red }}>✗</span> remembers the window size</span>
      <span><span style={{ color: yellow }}>warning</span> 1 snapshot outdated · <span style={{ color: cyan }}>src/ui/window.tsx</span></span>
      <span className="theme-sample__chips">
        {theme.ansi.map((color, index) => <i key={index} style={{ background: color }} />)}
        <span className="theme-sample__cursor" style={{ background: theme.accent }} />
      </span>
    </div>
  );
}

/** Edits a custom theme in place; the terminal shows every change as it's made. */
export default function ThemeEditor({ themeId, preferences, onChange, onClose, onDelete }: ThemeEditorProps) {
  const theme = preferences.customThemes.find((candidate) => candidate.id === themeId);
  const original = useRef(theme);
  if (!theme || !original.current) return null;
  const initial = original.current;

  const replace = (next: TerminalTheme) => onChange((current) => ({
    ...current,
    customThemes: current.customThemes.map((candidate) => candidate.id === themeId ? next : candidate),
    accent: current.themeId === themeId ? next.accent : current.accent,
  }));
  const setColor = (color: ThemeColor, value: string) => replace(withColor(theme, color, value));
  const unchanged = JSON.stringify(theme) === JSON.stringify(initial);

  const weak = new Set(lowContrast(theme));
  const textContrast = contrastRatio(theme.foreground, theme.background);
  const weakNames = theme.ansi.flatMap((_, index) => weak.has(index) ? [`${index > 7 ? "bright " : ""}${ANSI_NAMES[index % 8]!.toLowerCase()}`] : []);
  const warning = (color: ThemeColor) => {
    if (!weak.has(color)) return undefined;
    const ratio = contrastRatio(colorOf(theme, color), theme.background).toFixed(1);
    return color === "foreground"
      ? `Hard to read: ${ratio}:1 on the background, text needs ${TEXT_CONTRAST}:1`
      : `Hard to read: ${ratio}:1 on the background, colored text needs ${COLOR_CONTRAST}:1`;
  };

  return (
    <div className="theme-editor">
      <div className="theme-editor__bar">
        <button className="button" type="button" onClick={() => onClose(unchanged)}><ArrowLeft size={14} />Themes</button>
        <input
          className="field theme-editor__name"
          value={theme.name}
          maxLength={60}
          spellCheck={false}
          aria-label="Theme name"
          onChange={(event) => replace({ ...theme, name: event.target.value })}
          onBlur={(event) => { if (!event.target.value.trim()) replace({ ...theme, name: initial.name }); }}
        />
        <div className="button-row">
          <button className="button" type="button" disabled={unchanged} onClick={() => replace(initial)} title="Go back to the colors you started with"><RotateCcw size={14} />Undo changes</button>
          <button className="button" type="button" onClick={() => downloadScheme(theme)} title="A scheme for the schemes list of Windows Terminal's settings.json"><Download size={14} />Export for Windows Terminal</button>
          <button className="button" type="button" onClick={() => onDelete(theme.id)}><Trash size={14} />Delete</button>
        </div>
      </div>

      <ThemeSample theme={theme} />
      <p className={`theme-editor__contrast ${weak.size ? "is-weak" : ""}`}>
        Text on background: {textContrast.toFixed(1)}:1
        {textContrast < TEXT_CONTRAST ? `, below the ${TEXT_CONTRAST}:1 text needs.` : "."}
        {weakNames.length > 0 && ` Hard to read on this background: ${weakNames.join(", ")}.`}
      </p>

      <div className="theme-editor__group">
        <ColorField label="Background" value={theme.background} onChange={(value) => setColor("background", value)} />
        <ColorField label="Text" value={theme.foreground} warning={warning("foreground")} onChange={(value) => setColor("foreground", value)} />
        <ColorField label="Accent and cursor" value={theme.accent} onChange={(value) => setColor("accent", value)} />
      </div>
      {(["Normal", "Bright"] as const).map((row, half) => (
        <div key={row} className="theme-editor__ansi">
          <span className="theme-editor__row-label">{row}</span>
          {ANSI_NAMES.map((name, offset) => {
            const index = half * 8 + offset;
            return (
              <ColorField
                key={index}
                compact
                label={name}
                fullLabel={row === "Bright" ? `Bright ${name.toLowerCase()}` : name}
                value={theme.ansi[index]!}
                warning={isChromatic(index) ? warning(index) : undefined}
                onChange={(value) => setColor(index, value)}
              />
            );
          })}
        </div>
      ))}
    </div>
  );
}

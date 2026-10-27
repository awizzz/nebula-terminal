import { useState, type Dispatch, type SetStateAction } from "react";
import { Check, LayoutGrid, Pencil, Plus, X } from "lucide-react";
import { resolveTheme, themes, type TerminalTheme } from "../themes";
import { MAX_CUSTOM_THEMES } from "../themeImport";
import { editableCopy } from "../themeEdit";
import ThemeEditor from "./ThemeEditor";
import ThemeGallery from "./ThemeGallery";
import ThemePreview from "./ThemePreview";
import type { AppearancePreferences } from "../types";

type View = { kind: "grid" } | { kind: "gallery" } | { kind: "editor"; id: string; copiedFrom?: string };

interface ThemeSectionProps {
  preferences: AppearancePreferences;
  onChange: Dispatch<SetStateAction<AppearancePreferences>>;
}

/** Settings → Appearance → Theme: the themes you have, the gallery, and the editor. */
export default function ThemeSection({ preferences, onChange }: ThemeSectionProps) {
  const [view, setView] = useState<View>({ kind: "grid" });

  const select = (theme: TerminalTheme) => onChange((current) => ({ ...current, themeId: theme.id, accent: theme.accent }));

  const remove = (id: string) => onChange((current) => {
    const customThemes = current.customThemes.filter((theme) => theme.id !== id);
    return current.themeId === id
      ? { ...current, customThemes, themeId: themes[0]!.id, accent: themes[0]!.accent }
      : { ...current, customThemes };
  });

  /** A custom theme is edited as it is, unless a copy is asked for; any other one through a copy. */
  const edit = (theme: TerminalTheme, asCopy = false) => {
    if (theme.id.startsWith("custom-") && !asCopy) {
      select(theme);
      setView({ kind: "editor", id: theme.id });
      return;
    }
    const copy = editableCopy(theme, [...themes, ...preferences.customThemes]);
    onChange((current) => ({
      ...current,
      customThemes: [...current.customThemes, copy].slice(-MAX_CUSTOM_THEMES),
      themeId: copy.id,
      accent: copy.accent,
    }));
    setView({ kind: "editor", id: copy.id, copiedFrom: theme.id });
  };

  const closeEditor = (unchanged: boolean) => {
    // A copy left as it was is no theme of its own: give back the one it came from.
    if (view.kind === "editor" && view.copiedFrom && unchanged) {
      const from = resolveTheme(view.copiedFrom, preferences.customThemes);
      onChange((current) => ({
        ...current,
        customThemes: current.customThemes.filter((theme) => theme.id !== view.id),
        themeId: from.id,
        accent: from.accent,
      }));
    }
    setView({ kind: "grid" });
  };

  if (view.kind === "gallery") return <ThemeGallery preferences={preferences} onChange={onChange} onClose={() => setView({ kind: "grid" })} />;
  if (view.kind === "editor") {
    return (
      <ThemeEditor
        themeId={view.id}
        preferences={preferences}
        onChange={onChange}
        onClose={closeEditor}
        onDelete={(id) => { remove(id); setView({ kind: "grid" }); }}
      />
    );
  }

  const current = resolveTheme(preferences.themeId, preferences.customThemes);
  return (
    <>
      <div className="theme-grid" role="radiogroup" aria-label="Theme">
        {[...themes, ...preferences.customThemes].map((theme) => {
          const selected = preferences.themeId === theme.id;
          const custom = theme.id.startsWith("custom-");
          return (
            <div key={theme.id} className="theme-card-wrap">
              <button type="button" role="radio" aria-checked={selected} className="theme-card" onClick={() => select(theme)}>
                <ThemePreview theme={theme} />
                <span className="theme-card__name">{theme.name}{selected && <Check size={14} strokeWidth={2.2} />}</span>
              </button>
              <div className="theme-card__actions">
                <button type="button" aria-label={`Edit ${theme.name}`} title={custom ? "Edit" : "Edit a copy"} onClick={() => edit(theme)}><Pencil size={13} /></button>
                {custom && <button type="button" aria-label={`Remove ${theme.name}`} title="Remove" onClick={() => remove(theme.id)}><X size={13} /></button>}
              </div>
            </div>
          );
        })}
      </div>
      <div className="button-row theme-section__actions">
        <button className="button" type="button" onClick={() => setView({ kind: "gallery" })}><LayoutGrid size={14} />Browse 800+ themes</button>
        <button className="button" type="button" onClick={() => edit(current, true)}><Plus size={14} />New theme from {current.name}</button>
      </div>
    </>
  );
}

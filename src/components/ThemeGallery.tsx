import { useEffect, useMemo, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { ArrowLeft, Check, Search } from "lucide-react";
import { openExternal } from "../external";
import { themes, type TerminalTheme } from "../themes";
import { galleryThemes, MAX_CUSTOM_THEMES, type GalleryEntry } from "../themeImport";
import { featuredFirst, matchesQuery } from "../themeEdit";
import Segmented from "./Segmented";
import ThemePreview from "./ThemePreview";
import type { AppearancePreferences } from "../types";

const PAGE = 48;
const SOURCE = "https://github.com/mbadolato/iTerm2-Color-Schemes";

interface ThemeGalleryProps {
  preferences: AppearancePreferences;
  onChange: Dispatch<SetStateAction<AppearancePreferences>>;
  onClose: () => void;
}

/**
 * Hundreds of schemes to try. A click puts one on the terminal at once; when the gallery
 * closes, the themes tried and left behind go again, and the one in use stays.
 */
export default function ThemeGallery({ preferences, onChange, onClose }: ThemeGalleryProps) {
  const [all, setAll] = useState<TerminalTheme[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [query, setQuery] = useState("");
  const [scheme, setScheme] = useState<"all" | "dark" | "light">("all");
  const [shown, setShown] = useState(PAGE);
  const tried = useRef(new Set<string>());
  const sentinel = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let live = true;
    import("../assets/color-schemes.json")
      .then((data) => live && setAll(galleryThemes(data.default.schemes as GalleryEntry[], themes)))
      .catch(() => live && setFailed(true));
    return () => { live = false; };
  }, []);

  useEffect(() => () => {
    const added = tried.current;
    if (added.size === 0) return;
    onChange((current) => ({ ...current, customThemes: current.customThemes.filter((theme) => !added.has(theme.id) || theme.id === current.themeId) }));
  }, [onChange]);

  const visible = useMemo(() => {
    const found = (all ?? []).filter((theme) => (scheme === "all" || theme.scheme === scheme) && matchesQuery(theme, query));
    return query.trim() ? found : featuredFirst(found);
  }, [all, query, scheme]);
  useEffect(() => setShown(PAGE), [query, scheme]);

  useEffect(() => {
    const target = sentinel.current;
    if (!target) return;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) setShown((count) => count + PAGE);
    }, { rootMargin: "400px" });
    observer.observe(target);
    return () => observer.disconnect();
  }, [visible]);

  const use = (theme: TerminalTheme) => onChange((current) => {
    const present = current.customThemes.some((candidate) => candidate.id === theme.id);
    if (!present) tried.current.add(theme.id);
    return {
      ...current,
      customThemes: present ? current.customThemes : [...current.customThemes, theme].slice(-MAX_CUSTOM_THEMES),
      themeId: theme.id,
      accent: theme.accent,
    };
  });

  return (
    <div className="gallery">
      <div className="gallery__bar">
        <button className="button" type="button" onClick={onClose}><ArrowLeft size={14} />Themes</button>
        <label className="gallery__search">
          <Search size={14} aria-hidden="true" />
          <input type="search" value={query} spellCheck={false} placeholder={all ? `Search ${all.length} themes` : "Search themes"} aria-label="Search themes" onChange={(event) => setQuery(event.target.value)} />
        </label>
        <Segmented label="Light or dark" value={scheme} onChange={setScheme} options={[{ value: "all", label: "All" }, { value: "dark", label: "Dark" }, { value: "light", label: "Light" }]} />
      </div>

      {failed && <p className="gallery__note">The gallery couldn't be loaded.</p>}
      {!all && !failed && <p className="gallery__note">Loading themes…</p>}
      {all && visible.length === 0 && <p className="gallery__note">No theme matches “{query.trim()}”.</p>}

      <div className="theme-grid" role="radiogroup" aria-label="Gallery themes">
        {visible.slice(0, shown).map((theme) => {
          const selected = preferences.themeId === theme.id;
          return (
            <button key={theme.id} type="button" role="radio" aria-checked={selected} className="theme-card" onClick={() => use(theme)}>
              <ThemePreview theme={theme} />
              <span className="theme-card__name">{theme.name}{selected && <Check size={14} strokeWidth={2.2} />}</span>
            </button>
          );
        })}
      </div>
      {shown < visible.length && <div ref={sentinel} className="gallery__more" aria-hidden="true" />}

      <p className="gallery__credit">
        From <button type="button" className="link-button" onClick={() => openExternal(SOURCE)}>iTerm2-Color-Schemes</button> by Mark Badolato and the authors of each scheme (MIT). The theme you pick stays with your themes; the others you tried go when you leave.
      </p>
    </div>
  );
}

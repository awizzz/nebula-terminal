import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import Keys from "./Keys";

export interface MenuItem {
  id: string;
  label: string;
  icon?: ReactNode;
  shortcut?: string;
  detail?: string;
  disabled?: boolean;
  danger?: boolean;
  /** Swatches only: the color shown, none for a "no color" swatch. */
  color?: string;
  checked?: boolean;
  run: () => void;
}

/** A row of color swatches; each one is an item for keyboard navigation. */
export interface MenuSwatchRow {
  label: string;
  swatches: MenuItem[];
}

export type MenuEntry = MenuItem | "separator" | { heading: string } | MenuSwatchRow;

interface MenuProps {
  /** Viewport point the menu opens from. */
  x: number;
  y: number;
  entries: MenuEntry[];
  label: string;
  width?: number;
  onClose: () => void;
}

const isItem = (entry: MenuEntry): entry is MenuItem => typeof entry === "object" && "run" in entry;
const isSwatchRow = (entry: MenuEntry): entry is MenuSwatchRow => typeof entry === "object" && "swatches" in entry;

/** A floating menu with keyboard navigation that stays inside the window. */
export default function Menu({ x, y, entries, label, width = 260, onClose }: MenuProps) {
  const ref = useRef<HTMLDivElement | null>(null);
  const [position, setPosition] = useState({ left: x, top: y, originX: "left", originY: "top" });
  const items = entries.flatMap((entry) => isItem(entry) ? [entry] : isSwatchRow(entry) ? entry.swatches : []);
  const swatches = entries.flatMap((entry) => isSwatchRow(entry) ? entry.swatches : []);
  const [active, setActive] = useState(() => items.findIndex((item) => !item.disabled));

  useLayoutEffect(() => {
    const menu = ref.current;
    if (!menu) return;
    const { innerWidth, innerHeight } = window;
    const rect = menu.getBoundingClientRect();
    const flipX = x + rect.width > innerWidth - 8;
    const flipY = y + rect.height > innerHeight - 8;
    setPosition({
      left: flipX ? Math.max(8, x - rect.width) : x,
      top: flipY ? Math.max(8, y - rect.height) : y,
      originX: flipX ? "right" : "left",
      originY: flipY ? "bottom" : "top",
    });
    menu.focus();
  }, [x, y]);

  useEffect(() => {
    const closeOutside = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) onClose();
    };
    const closeOnBlur = () => onClose();
    window.addEventListener("mousedown", closeOutside, true);
    window.addEventListener("blur", closeOnBlur);
    window.addEventListener("resize", closeOnBlur);
    return () => {
      window.removeEventListener("mousedown", closeOutside, true);
      window.removeEventListener("blur", closeOnBlur);
      window.removeEventListener("resize", closeOnBlur);
    };
  }, [onClose]);

  // Long menus scroll; keep the keyboard selection in view.
  useEffect(() => {
    ref.current?.querySelector(".menu__item.is-active")?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const move = (delta: number) => {
    if (!items.some((item) => !item.disabled)) return;
    let next = active;
    do next = (next + delta + items.length) % items.length;
    while (items[next]?.disabled);
    setActive(next);
  };

  const run = (item: MenuItem | undefined) => {
    if (!item || item.disabled) return;
    onClose();
    item.run();
  };

  return (
    <div
      ref={ref}
      className="menu"
      role="menu"
      aria-label={label}
      tabIndex={-1}
      style={{ left: position.left, top: position.top, width, transformOrigin: `${position.originY} ${position.originX}` }}
      onContextMenu={(event) => event.preventDefault()}
      onKeyDown={(event) => {
        if (event.key === "ArrowDown") { event.preventDefault(); move(1); }
        else if (event.key === "ArrowUp") { event.preventDefault(); move(-1); }
        else if ((event.key === "ArrowLeft" || event.key === "ArrowRight") && swatches.includes(items[active]!)) { event.preventDefault(); move(event.key === "ArrowLeft" ? -1 : 1); }
        else if (event.key === "Home") { event.preventDefault(); setActive(items.findIndex((item) => !item.disabled)); }
        else if (event.key === "Enter" || event.key === " ") { event.preventDefault(); run(items[active]); }
        else if (event.key === "Escape" || event.key === "Tab") { event.preventDefault(); onClose(); }
        event.stopPropagation();
      }}
    >
      {entries.map((entry, index) => {
        if (entry === "separator") return <div key={`separator-${index}`} className="menu__separator" role="separator" />;
        if (isSwatchRow(entry)) {
          return (
            <div key={`swatches-${entry.label}`} className="menu__swatches" role="group" aria-label={entry.label}>
              {entry.swatches.map((swatch) => {
                const itemIndex = items.indexOf(swatch);
                return (
                  <button
                    key={swatch.id}
                    type="button"
                    role="menuitemradio"
                    aria-checked={swatch.checked ?? false}
                    aria-label={swatch.label}
                    title={swatch.label}
                    className={`menu__swatch ${swatch.color ? "" : "menu__swatch--none"} ${itemIndex === active ? "is-active" : ""}`}
                    style={swatch.color ? { "--swatch": swatch.color } as CSSProperties : undefined}
                    onMouseEnter={() => setActive(itemIndex)}
                    onClick={() => run(swatch)}
                  />
                );
              })}
            </div>
          );
        }
        if (!isItem(entry)) return <div key={`heading-${entry.heading}`} className="menu__heading">{entry.heading}</div>;
        const itemIndex = items.indexOf(entry);
        return (
          <button
            key={entry.id}
            type="button"
            role="menuitem"
            disabled={entry.disabled}
            className={`menu__item ${itemIndex === active ? "is-active" : ""} ${entry.danger ? "menu__item--danger" : ""}`}
            onMouseEnter={() => !entry.disabled && setActive(itemIndex)}
            onClick={() => run(entry)}
          >
            <span className="menu__icon">{entry.icon}</span>
            <span className="menu__label">
              {entry.label}
              {entry.detail && <small>{entry.detail}</small>}
            </span>
            {entry.shortcut && <Keys shortcut={entry.shortcut} subtle />}
          </button>
        );
      })}
    </div>
  );
}

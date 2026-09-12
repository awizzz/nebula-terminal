import { useEffect, useMemo, useRef, useState } from "react";
import { Search } from "lucide-react";

export interface PaletteCommand {
  id: string;
  label: string;
  detail?: string;
  shortcut?: string;
  run: () => void;
}

interface CommandPaletteProps {
  open: boolean;
  commands: PaletteCommand[];
  onClose: () => void;
}

export default function CommandPalette({ open, commands, onClose }: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const resultRefs = useRef<Array<HTMLButtonElement | null>>([]);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActiveIndex(0);
    requestAnimationFrame(() => inputRef.current?.focus());
  }, [open]);

  const filtered = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return commands;
    return commands.filter((command) => `${command.label} ${command.detail ?? ""}`.toLowerCase().includes(normalized));
  }, [commands, query]);

  useEffect(() => {
    setActiveIndex(0);
  }, [query]);

  useEffect(() => {
    resultRefs.current[activeIndex]?.scrollIntoView({ block: "nearest" });
  }, [activeIndex]);

  if (!open) return null;

  return (
    <div className="palette-backdrop" role="presentation" onMouseDown={onClose}>
      <section className="command-palette" role="dialog" aria-modal="true" aria-label="Command palette" onMouseDown={(e) => e.stopPropagation()}>
        <div className="palette-search">
          <Search size={18} aria-hidden="true" />
          <input
            ref={inputRef}
            value={query}
            placeholder="Search commands…"
            role="combobox"
            aria-expanded="true"
            aria-controls="palette-results"
            aria-activedescendant={filtered[activeIndex] ? `palette-${filtered[activeIndex].id}` : undefined}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.preventDefault();
                onClose();
              }
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setActiveIndex((index) => Math.min(filtered.length - 1, index + 1));
              }
              if (e.key === "ArrowUp") {
                e.preventDefault();
                setActiveIndex((index) => Math.max(0, index - 1));
              }
              if (e.key === "Enter" && filtered[activeIndex]) {
                filtered[activeIndex].run();
                onClose();
              }
            }}
          />
          <kbd>Esc</kbd>
        </div>
        <div className="palette-results" id="palette-results" role="listbox">
          {filtered.map((command, index) => (
            <button
              ref={(node) => { resultRefs.current[index] = node; }}
              id={`palette-${command.id}`}
              key={command.id}
              type="button"
              role="option"
              aria-selected={index === activeIndex}
              className={index === activeIndex ? "selected" : ""}
              onMouseEnter={() => setActiveIndex(index)}
              onClick={() => { command.run(); onClose(); }}
            >
              <div><strong>{command.label}</strong>{command.detail && <span>{command.detail}</span>}</div>
              {command.shortcut && <kbd>{command.shortcut}</kbd>}
            </button>
          ))}
          {filtered.length === 0 && <div className="palette-empty">No command matches “{query}”.</div>}
        </div>
      </section>
    </div>
  );
}

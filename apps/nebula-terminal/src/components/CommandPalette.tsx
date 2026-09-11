import { useEffect, useMemo, useRef, useState } from "react";

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
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    requestAnimationFrame(() => inputRef.current?.focus());
  }, [open]);

  const filtered = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return commands;
    return commands.filter((command) => `${command.label} ${command.detail ?? ""}`.toLowerCase().includes(normalized));
  }, [commands, query]);

  if (!open) return null;

  return (
    <div className="palette-backdrop" role="presentation" onMouseDown={onClose}>
      <section className="command-palette" role="dialog" aria-modal="true" aria-label="Command palette" onMouseDown={(e) => e.stopPropagation()}>
        <div className="palette-search">
          <svg viewBox="0 0 20 20" aria-hidden="true"><circle cx="9" cy="9" r="5.5" /><path d="m13.2 13.2 3.3 3.3" /></svg>
          <input
            ref={inputRef}
            value={query}
            placeholder="Search commands…"
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") onClose();
              if (e.key === "Enter" && filtered[0]) {
                filtered[0].run();
                onClose();
              }
            }}
          />
          <kbd>Esc</kbd>
        </div>
        <div className="palette-results">
          {filtered.map((command) => (
            <button key={command.id} type="button" onClick={() => { command.run(); onClose(); }}>
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

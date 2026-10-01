import { Fragment, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { CornerDownLeft, Search } from "lucide-react";
import Keys from "./Keys";

export interface PaletteCommand {
  id: string;
  group: string;
  label: string;
  icon?: ReactNode;
  detail?: string;
  shortcut?: string;
  keywords?: string;
  run: () => void;
}

interface CommandPaletteProps {
  open: boolean;
  commands: PaletteCommand[];
  onClose: () => void;
}

interface Match {
  command: PaletteCommand;
  score: number;
  positions: number[];
}

/** Subsequence match on the label (then keywords) that favors word starts and consecutive letters. */
function match(command: PaletteCommand, query: string): Match | null {
  if (!query) return { command, score: 0, positions: [] };
  const label = command.label.toLowerCase();
  const positions: number[] = [];
  let score = 0;
  let from = 0;
  for (const char of query) {
    const index = label.indexOf(char, from);
    if (index < 0) {
      const haystack = `${command.group} ${command.keywords ?? ""} ${command.detail ?? ""}`.toLowerCase();
      return query.split(/\s+/).every((word) => haystack.includes(word) || label.includes(word))
        ? { command, score: -100, positions: [] }
        : null;
    }
    const wordStart = index === 0 || label[index - 1] === " ";
    score += (wordStart ? 8 : 1) + (positions.at(-1) === index - 1 ? 5 : 0) - Math.min(index - from, 6) * 0.5;
    positions.push(index);
    from = index + 1;
  }
  return { command, score, positions };
}

function Highlight({ text, positions }: { text: string; positions: number[] }) {
  if (!positions.length) return <>{text}</>;
  const marked = new Set(positions);
  return <>{[...text].map((char, index) => marked.has(index) ? <mark key={index}>{char}</mark> : <Fragment key={index}>{char}</Fragment>)}</>;
}

export default function CommandPalette({ open, commands, onClose }: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);

  // Focus before the next keystroke arrives so fast typing never lands in the terminal.
  useLayoutEffect(() => {
    if (!open) return;
    setQuery("");
    setActiveIndex(0);
    inputRef.current?.focus();
  }, [open]);

  const results = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    const matches = commands.map((command) => match(command, normalized)).filter((item): item is Match => item !== null);
    return normalized ? matches.sort((a, b) => b.score - a.score) : matches;
  }, [commands, query]);

  useEffect(() => setActiveIndex(0), [query]);

  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>(`[data-index="${activeIndex}"]`)?.scrollIntoView({ block: "nearest" });
  }, [activeIndex]);

  if (!open) return null;

  const grouped = !query.trim();
  const runAt = (index: number) => {
    const command = results[index]?.command;
    if (!command) return;
    onClose();
    command.run();
  };

  return (
    <div className="overlay overlay--palette" role="presentation" onMouseDown={onClose}>
      <section className="palette" role="dialog" aria-modal="true" aria-label="Command palette" onMouseDown={(event) => event.stopPropagation()}>
        <div className="palette__search">
          <Search size={16} strokeWidth={1.8} aria-hidden="true" />
          <input
            ref={inputRef}
            value={query}
            placeholder="Type a command"
            spellCheck={false}
            role="combobox"
            aria-expanded="true"
            aria-controls="palette-results"
            aria-activedescendant={results[activeIndex] ? `palette-${results[activeIndex].command.id}` : undefined}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") { event.preventDefault(); onClose(); }
              else if (event.key === "ArrowDown") { event.preventDefault(); setActiveIndex((index) => Math.min(results.length - 1, index + 1)); }
              else if (event.key === "ArrowUp") { event.preventDefault(); setActiveIndex((index) => Math.max(0, index - 1)); }
              else if (event.key === "PageDown") { event.preventDefault(); setActiveIndex((index) => Math.min(results.length - 1, index + 8)); }
              else if (event.key === "PageUp") { event.preventDefault(); setActiveIndex((index) => Math.max(0, index - 8)); }
              else if (event.key === "Enter") { event.preventDefault(); runAt(activeIndex); }
            }}
          />
        </div>
        <div className="palette__results" id="palette-results" role="listbox" ref={listRef}>
          {results.map(({ command, positions }, index) => (
            <Fragment key={command.id}>
              {grouped && command.group !== results[index - 1]?.command.group && <div className="palette__group" role="presentation">{command.group}</div>}
              <div
                id={`palette-${command.id}`}
                data-index={index}
                role="option"
                aria-selected={index === activeIndex}
                className={`palette__item ${index === activeIndex ? "is-active" : ""}`}
                onMouseMove={() => index !== activeIndex && setActiveIndex(index)}
                onClick={() => runAt(index)}
              >
                <span className="palette__icon">{command.icon}</span>
                <span className="palette__label">
                  <Highlight text={command.label} positions={positions} />
                  {command.detail && <small>{command.detail}</small>}
                </span>
                {command.shortcut && <Keys shortcut={command.shortcut} subtle />}
              </div>
            </Fragment>
          ))}
          {results.length === 0 && <div className="palette__empty">No matching command</div>}
        </div>
        <footer className="palette__footer">
          <span><kbd>↑</kbd><kbd>↓</kbd> to navigate</span>
          <span><kbd><CornerDownLeft size={10} /></kbd> to run</span>
          <span><kbd>Esc</kbd> to close</span>
        </footer>
      </section>
    </div>
  );
}

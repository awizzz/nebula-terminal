import { useLayoutEffect, useRef, type ReactNode } from "react";
import { ArrowDown, ArrowUp, CaseSensitive, Regex, WholeWord, X } from "lucide-react";

export interface SearchOptions {
  caseSensitive: boolean;
  wholeWord: boolean;
  regex: boolean;
}

interface SearchBarProps {
  open: boolean;
  query: string;
  options: SearchOptions;
  result: { index: number; count: number } | null;
  onQueryChange: (query: string) => void;
  onOptionsChange: (options: SearchOptions) => void;
  onNext: () => void;
  onPrevious: () => void;
  onClose: () => void;
}

/** Whether `query` is a pattern JavaScript can compile, when regex search is on. */
export function validPattern(query: string): boolean {
  try {
    new RegExp(query);
    return true;
  } catch {
    return false;
  }
}

export default function SearchBar({ open, query, options, result, onQueryChange, onOptionsChange, onNext, onPrevious, onClose }: SearchBarProps) {
  const inputRef = useRef<HTMLInputElement | null>(null);

  useLayoutEffect(() => {
    if (open) inputRef.current?.select();
  }, [open]);

  if (!open) return null;

  const invalid = options.regex && !!query && !validPattern(query);
  const status = !query
    ? ""
    : invalid
      ? "Invalid pattern"
      : !result || result.count === 0
        ? "No results"
        : `${result.index + 1} of ${result.count >= 1000 ? "1000+" : result.count}`;
  const toggle = (key: keyof SearchOptions) => onOptionsChange({ ...options, [key]: !options[key] });
  const toggles: Array<{ key: keyof SearchOptions; label: string; shortcut: string; icon: ReactNode }> = [
    { key: "caseSensitive", label: "Match case", shortcut: "Alt+C", icon: <CaseSensitive size={15} /> },
    { key: "wholeWord", label: "Whole word", shortcut: "Alt+W", icon: <WholeWord size={15} /> },
    { key: "regex", label: "Regular expression", shortcut: "Alt+R", icon: <Regex size={15} /> },
  ];

  return (
    <div className={`find ${query && (invalid || result?.count === 0) ? "find--empty" : ""}`} role="search">
      <input
        ref={inputRef}
        value={query}
        placeholder="Find"
        spellCheck={false}
        aria-label="Find in terminal"
        aria-invalid={invalid}
        onChange={(event) => onQueryChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.altKey && !event.ctrlKey) {
            const key = { c: "caseSensitive", w: "wholeWord", r: "regex" }[event.key.toLowerCase()] as keyof SearchOptions | undefined;
            if (key) {
              event.preventDefault();
              toggle(key);
              return;
            }
          }
          if (event.key === "Enter" && event.shiftKey) onPrevious();
          else if (event.key === "Enter") onNext();
          else if (event.key === "Escape") onClose();
        }}
      />
      <div className="find__toggles" role="group" aria-label="Find options">
        {toggles.map(({ key, label, shortcut, icon }) => (
          <button
            key={key}
            type="button"
            className={`find__toggle ${options[key] ? "is-on" : ""}`}
            aria-pressed={options[key]}
            aria-label={label}
            title={`${label} (${shortcut})`}
            onClick={() => toggle(key)}
          >
            {icon}
          </button>
        ))}
      </div>
      <span className="find__status" aria-live="polite">{status}</span>
      <button type="button" onClick={onPrevious} aria-label="Previous match" title="Previous match (Shift+Enter)"><ArrowUp size={14} /></button>
      <button type="button" onClick={onNext} aria-label="Next match" title="Next match (Enter)"><ArrowDown size={14} /></button>
      <button type="button" onClick={onClose} aria-label="Close find" title="Close (Esc)"><X size={14} /></button>
    </div>
  );
}

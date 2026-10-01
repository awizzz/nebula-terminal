import { useLayoutEffect, useRef } from "react";
import { ArrowDown, ArrowUp, X } from "lucide-react";

interface SearchBarProps {
  open: boolean;
  query: string;
  result: { index: number; count: number } | null;
  onQueryChange: (query: string) => void;
  onNext: () => void;
  onPrevious: () => void;
  onClose: () => void;
}

export default function SearchBar({ open, query, result, onQueryChange, onNext, onPrevious, onClose }: SearchBarProps) {
  const inputRef = useRef<HTMLInputElement | null>(null);

  useLayoutEffect(() => {
    if (open) inputRef.current?.select();
  }, [open]);

  if (!open) return null;

  const status = !query ? "" : !result || result.count === 0 ? "No results" : `${result.index + 1} of ${result.count >= 1000 ? "1000+" : result.count}`;

  return (
    <div className={`find ${query && result?.count === 0 ? "find--empty" : ""}`} role="search">
      <input
        ref={inputRef}
        value={query}
        placeholder="Find"
        spellCheck={false}
        aria-label="Find in terminal"
        onChange={(event) => onQueryChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && event.shiftKey) onPrevious();
          else if (event.key === "Enter") onNext();
          else if (event.key === "Escape") onClose();
        }}
      />
      <span className="find__status" aria-live="polite">{status}</span>
      <button type="button" onClick={onPrevious} aria-label="Previous match" title="Previous match (Shift+Enter)"><ArrowUp size={14} /></button>
      <button type="button" onClick={onNext} aria-label="Next match" title="Next match (Enter)"><ArrowDown size={14} /></button>
      <button type="button" onClick={onClose} aria-label="Close find" title="Close (Esc)"><X size={14} /></button>
    </div>
  );
}

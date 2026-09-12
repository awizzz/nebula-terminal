import { useEffect, useRef } from "react";
import { ArrowDown, ArrowUp, Search, X } from "lucide-react";

interface SearchBarProps {
  open: boolean;
  query: string;
  onQueryChange: (query: string) => void;
  onNext: () => void;
  onPrevious: () => void;
  onClose: () => void;
}

export default function SearchBar({ open, query, onQueryChange, onNext, onPrevious, onClose }: SearchBarProps) {
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (open) requestAnimationFrame(() => inputRef.current?.focus());
  }, [open]);

  if (!open) return null;

  return (
    <div className="terminal-search" role="search">
      <Search size={15} aria-hidden="true" />
      <input
        ref={inputRef}
        value={query}
        placeholder="Find in terminal"
        onChange={(event) => onQueryChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && event.shiftKey) onPrevious();
          else if (event.key === "Enter") onNext();
          if (event.key === "Escape") onClose();
        }}
      />
      <button type="button" onClick={onPrevious} aria-label="Find previous" title="Previous match (Shift+Enter)"><ArrowUp size={14} /></button>
      <button type="button" onClick={onNext} aria-label="Find next" title="Next match (Enter)"><ArrowDown size={14} /></button>
      <button type="button" onClick={onClose} aria-label="Close search"><X size={14} /></button>
    </div>
  );
}

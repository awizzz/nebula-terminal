import { useEffect, useRef } from "react";

interface SearchBarProps {
  open: boolean;
  query: string;
  onQueryChange: (query: string) => void;
  onNext: () => void;
  onClose: () => void;
}

export default function SearchBar({ open, query, onQueryChange, onNext, onClose }: SearchBarProps) {
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (open) requestAnimationFrame(() => inputRef.current?.focus());
  }, [open]);

  if (!open) return null;

  return (
    <div className="terminal-search" role="search">
      <input
        ref={inputRef}
        value={query}
        placeholder="Find in terminal"
        onChange={(event) => onQueryChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") onNext();
          if (event.key === "Escape") onClose();
        }}
      />
      <button type="button" onClick={onNext} aria-label="Find next">↓</button>
      <button type="button" onClick={onClose} aria-label="Close search">×</button>
    </div>
  );
}

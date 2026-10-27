import type { CSSProperties, ReactNode } from "react";

/** A row of mutually exclusive options with a sliding thumb. */
export default function Segmented<T extends string>({ value, options, onChange, label }: { value: T; options: Array<{ value: T; label: ReactNode }>; onChange: (value: T) => void; label: string }) {
  const index = Math.max(0, options.findIndex((option) => option.value === value));
  return (
    <div className="segmented" role="radiogroup" aria-label={label} style={{ "--count": options.length, "--index": index } as CSSProperties}>
      <span className="segmented__thumb" aria-hidden="true" />
      {options.map((option) => (
        <button key={option.value} type="button" role="radio" aria-checked={option.value === value} onClick={() => onChange(option.value)}>{option.label}</button>
      ))}
    </div>
  );
}

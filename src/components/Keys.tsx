import { shortcutKeys } from "../keys";

export default function Keys({ shortcut, subtle = false }: { shortcut: string; subtle?: boolean }) {
  if (!shortcut) return null;
  return (
    <span className={`keys ${subtle ? "keys--subtle" : ""}`} aria-label={shortcut}>
      {shortcutKeys(shortcut).map((key, index) => <kbd key={`${key}-${index}`}>{key}</kbd>)}
    </span>
  );
}

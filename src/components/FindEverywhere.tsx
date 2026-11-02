import { Fragment, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { CornerDownLeft, Search } from "lucide-react";
import { getPane, type FoundLine } from "../paneRegistry";
import { tabTitle } from "./Titlebar";
import type { TerminalTab } from "../types";

interface Result {
  key: string;
  tab: TerminalTab;
  paneId: string;
  paneLabel?: string;
  found: FoundLine;
}

const MAX_RESULTS = 400;

/** Text before, in and after the match, the line cut down around it. */
function around(found: FoundLine): [string, string, string] {
  const start = Math.max(0, found.column - 40);
  const before = (start > 0 ? "…" : "") + found.text.slice(start, found.column).trimStart();
  const match = found.text.slice(found.column, found.column + found.length);
  return [before, match, found.text.slice(found.column + found.length, found.column + found.length + 80)];
}

/** Find in every tab: each matching line of every pane, grouped by tab. */
export default function FindEverywhere({ open, tabs, onClose, onReveal }: { open: boolean; tabs: TerminalTab[]; onClose: () => void; onReveal: (tabId: string, paneId: string, found: FoundLine) => void }) {
  const [query, setQuery] = useState("");
  const [searched, setSearched] = useState("");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);

  useLayoutEffect(() => {
    if (!open) return;
    setQuery("");
    setSearched("");
    inputRef.current?.focus();
  }, [open]);

  // Reading every buffer is cheap, but not on every keystroke.
  useEffect(() => {
    const timer = window.setTimeout(() => setSearched(query), 120);
    return () => window.clearTimeout(timer);
  }, [query]);

  const results = useMemo(() => {
    if (!open || searched.length < 2) return [];
    const list: Result[] = [];
    for (const tab of tabs) {
      tab.panes.forEach((pane, index) => {
        for (const found of getPane(pane.id)?.findAll(searched, caseSensitive) ?? []) {
          if (list.length >= MAX_RESULTS) return;
          list.push({ key: `${pane.id}:${found.line}`, tab, paneId: pane.id, paneLabel: tab.panes.length > 1 ? `pane ${index + 1}` : undefined, found });
        }
      });
    }
    return list;
  }, [caseSensitive, open, searched, tabs]);

  useEffect(() => setActiveIndex(0), [results]);
  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>(`[data-index="${activeIndex}"]`)?.scrollIntoView({ block: "nearest" });
  }, [activeIndex]);

  if (!open) return null;

  const reveal = (index: number) => {
    const result = results[index];
    if (!result) return;
    onClose();
    onReveal(result.tab.id, result.paneId, result.found);
  };
  const tabCount = new Set(results.map((result) => result.tab.id)).size;

  return (
    <div className="overlay overlay--palette" role="presentation" onMouseDown={onClose}>
      <section className="palette find-everywhere" role="dialog" aria-modal="true" aria-label="Find in every tab" onMouseDown={(event) => event.stopPropagation()}>
        <div className="palette__search">
          <Search size={16} strokeWidth={1.8} aria-hidden="true" />
          <input
            ref={inputRef}
            value={query}
            placeholder="Find in every tab"
            spellCheck={false}
            role="combobox"
            aria-expanded="true"
            aria-controls="find-everywhere-results"
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") { event.preventDefault(); onClose(); }
              else if (event.key === "ArrowDown") { event.preventDefault(); setActiveIndex((index) => Math.min(results.length - 1, index + 1)); }
              else if (event.key === "ArrowUp") { event.preventDefault(); setActiveIndex((index) => Math.max(0, index - 1)); }
              else if (event.key === "Enter") { event.preventDefault(); reveal(activeIndex); }
              else if (event.altKey && event.key.toLowerCase() === "c") { event.preventDefault(); setCaseSensitive((value) => !value); }
            }}
          />
          <button type="button" className={`find__toggle ${caseSensitive ? "is-on" : ""}`} aria-pressed={caseSensitive} title="Match case (Alt+C)" onClick={() => setCaseSensitive((value) => !value)}>Aa</button>
        </div>
        <div className="palette__results" id="find-everywhere-results" role="listbox" ref={listRef}>
          {results.map((result, index) => {
            const [before, match, after] = around(result.found);
            const firstOfTab = result.tab.id !== results[index - 1]?.tab.id;
            return (
              <Fragment key={result.key}>
                {firstOfTab && <div className="palette__group" role="presentation">{tabTitle(result.tab)}</div>}
                <div
                  data-index={index}
                  role="option"
                  aria-selected={index === activeIndex}
                  className={`palette__item find-everywhere__line ${index === activeIndex ? "is-active" : ""}`}
                  onMouseMove={() => index !== activeIndex && setActiveIndex(index)}
                  onClick={() => reveal(index)}
                >
                  <span className="find-everywhere__text">{before}<mark>{match}</mark>{after}</span>
                  {result.paneLabel && <small>{result.paneLabel}</small>}
                </div>
              </Fragment>
            );
          })}
          {searched.length >= 2 && results.length === 0 && <div className="palette__empty">Nothing found in any tab</div>}
          {searched.length < 2 && <div className="palette__empty">Type at least two characters</div>}
        </div>
        <footer className="palette__footer">
          <span>{results.length >= MAX_RESULTS ? `First ${MAX_RESULTS} lines` : `${results.length} line${results.length === 1 ? "" : "s"}`}{results.length ? ` in ${tabCount} tab${tabCount === 1 ? "" : "s"}` : ""}</span>
          <span><kbd><CornerDownLeft size={10} /></kbd> to show</span>
          <span><kbd>Esc</kbd> to close</span>
        </footer>
      </section>
    </div>
  );
}

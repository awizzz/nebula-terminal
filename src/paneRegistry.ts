import type { MovedPane } from "./types";

/** A line of a pane's buffer that matches, for Find in every tab. */
export interface FoundLine {
  line: number;
  column: number;
  length: number;
  text: string;
}

/** Imperative actions a mounted terminal pane exposes to menus and shortcuts. */
export interface PaneHandle {
  copy: () => void;
  /** Copies the selection as HTML with its colors, and as plain text. False without one. */
  copyWithColors: () => boolean;
  paste: () => void;
  selectAll: () => void;
  clear: () => void;
  hasSelection: () => boolean;
  focus: () => void;
  /** Sends text to the shell as if it was typed (broadcast input). */
  write: (data: string) => void;
  /** Scrolls to the previous or next command. False when the shell marks no commands. */
  jumpToCommand: (direction: -1 | 1) => boolean;
  /** Copies the output of the last finished command. False when there is none. */
  copyLastOutput: () => boolean;
  hasCommandOutput: () => boolean;
  /** Everything the pane shows, scrollback included, as text. */
  text: () => string;
  /** The same as a web page, with the terminal's colors. */
  html: (title: string) => string;
  /** The output of the last finished command, or undefined without one. */
  lastOutput: () => string | undefined;
  /** Every line that holds `query`, once each, 200 at most. */
  findAll: (query: string, caseSensitive: boolean) => FoundLine[];
  /** Scrolls to a found line and selects the match. */
  reveal: (found: FoundLine) => void;
  /** Keeps the shell running for another window and gives its screen. The pane then
   * unmounts without stopping the shell. */
  detach: () => Promise<MovedPane>;
  /** The screen and up to 1,000 lines of scrollback, with their colors, to show it again later. */
  screen: () => string;
  /** Takes the shell back after a move that failed. */
  undoDetach: () => void;
  /** Selecting with the keyboard: arrows, Shift to extend, Enter to copy. */
  markMode: () => void;
  /** Asks where, then writes what the pane prints there as plain text until stopped. */
  startLog: (name: string) => void;
  stopLog: () => void;
  logging: () => boolean;
}

const panes = new Map<string, PaneHandle>();

export function registerPane(id: string, handle: PaneHandle): () => void {
  panes.set(id, handle);
  return () => {
    if (panes.get(id) === handle) panes.delete(id);
  };
}

export function getPane(id: string | undefined): PaneHandle | undefined {
  return id ? panes.get(id) : undefined;
}

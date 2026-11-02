import type { MovedPane } from "./types";

/** Imperative actions a mounted terminal pane exposes to menus and shortcuts. */
export interface PaneHandle {
  copy: () => void;
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
  /** Keeps the shell running for another window and gives its screen. The pane then
   * unmounts without stopping the shell. */
  detach: () => Promise<MovedPane>;
  /** Takes the shell back after a move that failed. */
  undoDetach: () => void;
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

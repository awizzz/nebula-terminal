/** Imperative actions a mounted terminal pane exposes to menus and shortcuts. */
export interface PaneHandle {
  copy: () => void;
  paste: () => void;
  selectAll: () => void;
  clear: () => void;
  hasSelection: () => boolean;
  focus: () => void;
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

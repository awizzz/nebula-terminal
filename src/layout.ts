import type { SplitDirection } from "./types";

/**
 * The panes of a tab as a tree. A split lays its children out side by side
 * ("vertical", split right) or stacked ("horizontal", split down). `sizes` are the
 * children's shares of the split and add up to 1.
 *
 * The tree only holds pane ids. A tab keeps its pane models in a flat list, which is
 * also the order the panes are rendered in, so a pane never remounts (and never
 * restarts its shell) when the tree around it changes.
 */
export type LayoutNode = PaneLeaf | SplitNode;

export interface PaneLeaf {
  type: "pane";
  id: string;
}

export interface SplitNode {
  type: "split";
  direction: SplitDirection;
  children: LayoutNode[];
  sizes: number[];
}

/** A rectangle in whatever unit the caller uses: fractions of the tab, or pixels. */
export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Divider {
  /** Child indexes leading from the root to the split this divider belongs to. */
  path: number[];
  /** The divider sits between `children[index]` and `children[index + 1]`. */
  index: number;
  direction: SplitDirection;
  /** Where the divider line is, in fractions of the tab. */
  rect: Rect;
  /** The whole split, in fractions of the tab. */
  splitRect: Rect;
}

export type FocusDirection = "left" | "right" | "up" | "down";

export const MAX_PANES = 8;
/** Deepest tree that 8 panes can need: seven nested splits and a pane. */
export const MAX_DEPTH = MAX_PANES;
/** Smallest share a child keeps, so a saved layout can never hide a pane. */
const MIN_SHARE = 0.02;

export function paneLeaf(id: string): PaneLeaf {
  return { type: "pane", id };
}

/** Pane ids in reading order: left to right, top to bottom. */
export function paneIds(node: LayoutNode): string[] {
  return node.type === "pane" ? [node.id] : node.children.flatMap(paneIds);
}

export function nodeAt(root: LayoutNode, path: readonly number[]): LayoutNode | undefined {
  let node: LayoutNode | undefined = root;
  for (const index of path) node = node?.type === "split" ? node.children[index] : undefined;
  return node;
}

/** Shares that add up to 1; anything that isn't a positive number becomes an equal share. */
function normalizeSizes(sizes: readonly unknown[], count: number): number[] {
  const valid = sizes.length === count && sizes.every((size) => typeof size === "number" && Number.isFinite(size) && size > 0);
  const raw = valid ? (sizes as number[]) : new Array<number>(count).fill(1);
  const total = raw.reduce((sum, size) => sum + size, 0);
  const clamped = raw.map((size) => Math.max(MIN_SHARE, size / total));
  const clampedTotal = clamped.reduce((sum, size) => sum + size, 0);
  return clamped.map((size) => size / clampedTotal);
}

/**
 * Puts a tree in its canonical form: no empty or single-child splits, and no split
 * nested directly in a split of the same direction (its children join the parent).
 */
export function normalizeLayout(node: LayoutNode): LayoutNode | null {
  if (node.type === "pane") return node;
  const children: LayoutNode[] = [];
  const sizes: number[] = [];
  const shares = normalizeSizes(node.sizes, node.children.length);
  node.children.forEach((child, index) => {
    const normalized = normalizeLayout(child);
    if (!normalized) return;
    const share = shares[index]!;
    if (normalized.type === "split" && normalized.direction === node.direction) {
      normalized.children.forEach((grandchild, grandIndex) => {
        children.push(grandchild);
        sizes.push(share * normalized.sizes[grandIndex]!);
      });
    } else {
      children.push(normalized);
      sizes.push(share);
    }
  });
  if (children.length === 0) return null;
  if (children.length === 1) return children[0]!;
  return { type: "split", direction: node.direction, children, sizes: normalizeSizes(sizes, children.length) };
}

/**
 * Splits pane `targetId` and puts `newId` right of it or below it. When the pane
 * already sits in a split of that direction, the new pane becomes its sibling and the
 * two share the space the target had; otherwise the target turns into a new split.
 */
export function splitPane(root: LayoutNode, targetId: string, newId: string, direction: SplitDirection): LayoutNode {
  const visit = (node: LayoutNode): LayoutNode => {
    if (node.type === "pane") {
      return node.id === targetId ? { type: "split", direction, children: [node, paneLeaf(newId)], sizes: [0.5, 0.5] } : node;
    }
    const index = node.children.findIndex((child) => child.type === "pane" && child.id === targetId);
    if (index >= 0 && node.direction === direction) {
      const half = node.sizes[index]! / 2;
      return {
        ...node,
        children: [...node.children.slice(0, index + 1), paneLeaf(newId), ...node.children.slice(index + 1)],
        sizes: [...node.sizes.slice(0, index), half, half, ...node.sizes.slice(index + 1)],
      };
    }
    const children = node.children.map(visit);
    return children.every((child, childIndex) => child === node.children[childIndex]) ? node : { ...node, children };
  };
  return visit(root);
}

/**
 * Removes a pane. Its space goes to the sibling before it (or after it, for a first
 * child), and splits left with one child collapse into that child. `focusId` is the
 * pane that took over the space, the natural one to focus next.
 */
export function removePane(root: LayoutNode, id: string): { layout: LayoutNode | null; focusId?: string } {
  if (root.type === "pane") return root.id === id ? { layout: null } : { layout: root };
  let focusId: string | undefined;
  const visit = (node: LayoutNode): LayoutNode => {
    if (node.type === "pane") return node;
    const index = node.children.findIndex((child) => child.type === "pane" && child.id === id);
    if (index < 0) {
      const children = node.children.map(visit);
      return children.every((child, childIndex) => child === node.children[childIndex]) ? node : { ...node, children };
    }
    const heir = index > 0 ? index - 1 : 1;
    const heirIds = paneIds(node.children[heir]!);
    focusId = index > 0 ? heirIds.at(-1) : heirIds[0];
    const sizes = node.sizes.map((size, sizeIndex) => sizeIndex === heir ? size + node.sizes[index]! : size);
    return {
      ...node,
      children: node.children.filter((_, childIndex) => childIndex !== index),
      sizes: sizes.filter((_, sizeIndex) => sizeIndex !== index),
    };
  };
  const next = visit(root);
  if (next === root) return { layout: root };
  return { layout: normalizeLayout(next), focusId };
}

/** Replaces the sizes of the split at `path`; anything else is left alone. */
export function setSplitSizes(root: LayoutNode, path: readonly number[], sizes: readonly number[]): LayoutNode {
  const visit = (node: LayoutNode, depth: number): LayoutNode => {
    if (node.type === "pane") return node;
    if (depth === path.length) {
      return node.children.length === sizes.length ? { ...node, sizes: normalizeSizes(sizes, sizes.length) } : node;
    }
    const target = path[depth]!;
    const child = node.children[target];
    if (!child) return node;
    const next = visit(child, depth + 1);
    return next === child ? node : { ...node, children: node.children.map((candidate, index) => index === target ? next : candidate) };
  };
  return visit(root, 0);
}

/**
 * Moves the divider between `sizes[index]` and `sizes[index + 1]` by `delta` (a share of
 * the split). Neither side gets smaller than its minimum share, or than it already is
 * when a small window has squeezed it below that.
 */
export function resizePair(sizes: readonly number[], index: number, delta: number, minFirst: number, minSecond: number): number[] {
  const first = sizes[index];
  const second = sizes[index + 1];
  if (first === undefined || second === undefined) return [...sizes];
  const pair = first + second;
  const low = Math.min(minFirst, first);
  const high = pair - Math.min(minSecond, second);
  const next = Math.min(high, Math.max(low, first + delta));
  const result = [...sizes];
  result[index] = next;
  result[index + 1] = pair - next;
  return result;
}

/**
 * The smallest a node can get along a split direction (width for "vertical", height
 * for "horizontal") when each pane needs `paneMinimum` and each divider `divider`.
 */
export function minimumExtent(node: LayoutNode, direction: SplitDirection, paneMinimum: number, divider = 1): number {
  if (node.type === "pane") return paneMinimum;
  const extents = node.children.map((child) => minimumExtent(child, direction, paneMinimum, divider));
  return node.direction === direction
    ? extents.reduce((sum, extent) => sum + extent, 0) + divider * (extents.length - 1)
    : Math.max(...extents);
}

/** Where every pane and divider goes, in fractions of the tab (0 to 1). */
export function computeLayout(root: LayoutNode): { panes: Map<string, Rect>; dividers: Divider[] } {
  const panes = new Map<string, Rect>();
  const dividers: Divider[] = [];
  const place = (node: LayoutNode, rect: Rect, path: number[]) => {
    if (node.type === "pane") {
      panes.set(node.id, rect);
      return;
    }
    const across = node.direction === "vertical";
    let offset = across ? rect.x : rect.y;
    const extent = across ? rect.width : rect.height;
    node.children.forEach((child, index) => {
      const length = node.sizes[index]! * extent;
      const childRect = across
        ? { x: offset, y: rect.y, width: length, height: rect.height }
        : { x: rect.x, y: offset, width: rect.width, height: length };
      place(child, childRect, [...path, index]);
      offset += length;
      if (index < node.children.length - 1) {
        dividers.push({
          path,
          index,
          direction: node.direction,
          rect: across ? { x: offset, y: rect.y, width: 0, height: rect.height } : { x: rect.x, y: offset, width: rect.width, height: 0 },
          splitRect: rect,
        });
      }
    });
  };
  place(root, { x: 0, y: 0, width: 1, height: 1 }, []);
  return { panes, dividers };
}

/**
 * The pane next to `fromId` in a direction, from the panes' on-screen rectangles: the
 * closest one on that side that overlaps it, preferring the largest overlap.
 */
export function findNeighbor(rects: ReadonlyMap<string, Rect>, fromId: string, direction: FocusDirection, tolerance = 1): string | undefined {
  const from = rects.get(fromId);
  if (!from) return undefined;
  const horizontal = direction === "left" || direction === "right";
  let best: { id: string; gap: number; overlap: number; start: number } | undefined;
  for (const [id, rect] of rects) {
    if (id === fromId) continue;
    const gap = direction === "right" ? rect.x - (from.x + from.width)
      : direction === "left" ? from.x - (rect.x + rect.width)
      : direction === "down" ? rect.y - (from.y + from.height)
      : from.y - (rect.y + rect.height);
    if (gap < -tolerance) continue;
    const overlap = horizontal
      ? Math.min(from.y + from.height, rect.y + rect.height) - Math.max(from.y, rect.y)
      : Math.min(from.x + from.width, rect.x + rect.width) - Math.max(from.x, rect.x);
    if (overlap <= tolerance) continue;
    const start = horizontal ? rect.y : rect.x;
    const better = !best
      || gap < best.gap - tolerance
      || (Math.abs(gap - best.gap) <= tolerance && (overlap > best.overlap + tolerance || (Math.abs(overlap - best.overlap) <= tolerance && start < best.start)));
    if (better) best = { id, gap, overlap, start };
  }
  return best?.id;
}

/** Builds the tree for a layout saved before mixed splits: one row or column of panes. */
export function layoutFromFlat(ids: readonly string[], direction: SplitDirection, sizes?: readonly unknown[]): LayoutNode | null {
  if (ids.length === 0) return null;
  if (ids.length === 1) return paneLeaf(ids[0]!);
  return { type: "split", direction, children: ids.map(paneLeaf), sizes: normalizeSizes(sizes ?? [], ids.length) };
}

/**
 * Validates a tree read from storage. `readLeaf` turns a saved pane into the id of a
 * new pane, or returns null to reject it. Anything malformed, deeper than MAX_DEPTH or
 * holding more than MAX_PANES panes makes the whole tree invalid (null).
 */
export function sanitizeLayout(raw: unknown, readLeaf: (saved: Record<string, unknown>) => string | null): LayoutNode | null {
  let count = 0;
  const visit = (value: unknown, depth: number): LayoutNode | null => {
    if (depth > MAX_DEPTH || typeof value !== "object" || value === null || Array.isArray(value)) return null;
    const saved = value as Record<string, unknown>;
    if (saved.type === "pane") {
      count += 1;
      if (count > MAX_PANES) return null;
      const id = readLeaf(saved);
      return id ? paneLeaf(id) : null;
    }
    if (saved.type !== "split" || (saved.direction !== "vertical" && saved.direction !== "horizontal")) return null;
    if (!Array.isArray(saved.children) || saved.children.length < 1 || saved.children.length > MAX_PANES) return null;
    const children: LayoutNode[] = [];
    for (const child of saved.children) {
      const node = visit(child, depth + 1);
      if (!node) return null;
      children.push(node);
    }
    const sizes = Array.isArray(saved.sizes) ? saved.sizes : [];
    return { type: "split", direction: saved.direction, children, sizes: normalizeSizes(sizes, children.length) };
  };
  const tree = visit(raw, 1);
  return tree ? normalizeLayout(tree) : null;
}

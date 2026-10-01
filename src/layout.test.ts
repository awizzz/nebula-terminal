import { describe, expect, it } from "vitest";
import {
  MAX_PANES,
  computeLayout,
  findNeighbor,
  layoutFromFlat,
  minimumExtent,
  normalizeLayout,
  paneIds,
  paneLeaf,
  removePane,
  resizePair,
  sanitizeLayout,
  setSplitSizes,
  splitPane,
  type LayoutNode,
  type Rect,
} from "./layout";

const split = (direction: "vertical" | "horizontal", children: LayoutNode[], sizes = children.map(() => 1 / children.length)): LayoutNode => ({ type: "split", direction, children, sizes });
const a = paneLeaf("a");
const b = paneLeaf("b");
const c = paneLeaf("c");
const d = paneLeaf("d");

/** Pixel rectangles the way the workspace draws them: 1px dividers between panes. */
function screenRects(root: LayoutNode, width = 1200, height = 800): Map<string, Rect> {
  const rects = new Map<string, Rect>();
  for (const [id, rect] of computeLayout(root).panes) {
    const left = rect.x * width + (rect.x > 0 ? 1 : 0);
    const top = rect.y * height + (rect.y > 0 ? 1 : 0);
    rects.set(id, { x: left, y: top, width: (rect.x + rect.width) * width - left, height: (rect.y + rect.height) * height - top });
  }
  return rects;
}

describe("splitPane", () => {
  it("turns a single pane into a split", () => {
    expect(splitPane(a, "a", "b", "vertical")).toEqual(split("vertical", [a, b]));
  });

  it("nests a split of the other direction around the active pane", () => {
    const right = splitPane(a, "a", "b", "vertical");
    expect(splitPane(right, "b", "c", "horizontal")).toEqual(split("vertical", [a, split("horizontal", [b, c])]));
  });

  it("adds a sibling when the parent already splits that way", () => {
    const tree = split("vertical", [a, b], [0.6, 0.4]);
    const next = splitPane(tree, "a", "c", "vertical");
    expect(paneIds(next)).toEqual(["a", "c", "b"]);
    expect(next.type === "split" && next.sizes).toEqual([0.3, 0.3, 0.4]);
  });

  it("leaves the tree alone for an unknown pane", () => {
    const tree = split("vertical", [a, b]);
    expect(splitPane(tree, "zz", "c", "vertical")).toBe(tree);
  });
});

describe("removePane", () => {
  it("gives the space to the previous sibling", () => {
    const { layout, focusId } = removePane(split("vertical", [a, b, c], [0.2, 0.3, 0.5]), "b");
    expect(layout).toEqual(split("vertical", [a, c], [0.5, 0.5]));
    expect(focusId).toBe("a");
  });

  it("gives a first child's space to the next sibling", () => {
    const { layout, focusId } = removePane(split("vertical", [a, split("horizontal", [b, c])]), "a");
    expect(layout).toEqual(split("horizontal", [b, c]));
    expect(focusId).toBe("b");
  });

  it("collapses a split left with one pane and merges same-direction splits", () => {
    const tree = split("vertical", [a, split("horizontal", [b, split("vertical", [c, d])])], [0.5, 0.5]);
    const { layout, focusId } = removePane(tree, "b");
    expect(paneIds(layout!)).toEqual(["a", "c", "d"]);
    expect(layout).toMatchObject({ type: "split", direction: "vertical", sizes: [0.5, 0.25, 0.25] });
    expect(focusId).toBe("c");
  });

  it("returns null when the last pane closes", () => {
    expect(removePane(a, "a").layout).toBeNull();
    expect(removePane(a, "b").layout).toBe(a);
  });
});

describe("resizing", () => {
  it("moves one divider and keeps the shares adding up", () => {
    expect(resizePair([0.5, 0.5], 0, 0.1, 0.1, 0.1)).toEqual([0.6, 0.4]);
    const sizes = resizePair([0.25, 0.25, 0.5], 1, 0.2, 0.1, 0.1);
    expect(sizes[0]).toBe(0.25);
    expect(sizes[1]! + sizes[2]!).toBeCloseTo(0.75);
  });

  it("stops at the minimum on both sides", () => {
    const grown = resizePair([0.5, 0.5], 0, 0.9, 0.1, 0.2);
    expect(grown[0]).toBeCloseTo(0.8);
    expect(grown[1]).toBeCloseTo(0.2);
    expect(resizePair([0.5, 0.5], 0, -0.9, 0.1, 0.2)).toEqual([0.1, 0.9]);
    expect(resizePair([0.1, 0.1, 0.8], 0, 0.05, 0.15, 0.15)).toEqual([0.1, 0.1, 0.8]);
  });

  it("lets a pane squeezed below the minimum grow but not shrink", () => {
    const grown = resizePair([0.1, 0.9], 0, 0.2, 0.15, 0.15);
    expect(grown[0]).toBeCloseTo(0.3);
    expect(grown[1]).toBeCloseTo(0.7);
    expect(resizePair([0.1, 0.9], 0, -0.05, 0.15, 0.15)).toEqual([0.1, 0.9]);
  });

  it("sets the sizes of a nested split by path", () => {
    const tree = split("vertical", [a, split("horizontal", [b, c])]);
    const next = setSplitSizes(tree, [1], [0.7, 0.3]);
    expect(next).toEqual(split("vertical", [a, split("horizontal", [b, c], [0.7, 0.3])]));
    expect(setSplitSizes(tree, [0], [0.7, 0.3])).toBe(tree);
    expect(setSplitSizes(tree, [1], [1, 1, 1])).toBe(tree);
  });

  it("measures how small a subtree can get", () => {
    const tree = split("vertical", [a, split("horizontal", [b, split("vertical", [c, d])])]);
    expect(minimumExtent(tree, "vertical", 100)).toBe(302);
    expect(minimumExtent(tree, "horizontal", 50)).toBe(101);
  });
});

describe("computeLayout", () => {
  it("places panes and dividers in fractions of the tab", () => {
    const tree = split("vertical", [a, split("horizontal", [b, c], [0.25, 0.75])], [0.5, 0.5]);
    const { panes, dividers } = computeLayout(tree);
    expect(panes.get("a")).toEqual({ x: 0, y: 0, width: 0.5, height: 1 });
    expect(panes.get("c")).toEqual({ x: 0.5, y: 0.25, width: 0.5, height: 0.75 });
    expect(dividers).toEqual([
      { path: [], index: 0, direction: "vertical", rect: { x: 0.5, y: 0, width: 0, height: 1 }, splitRect: { x: 0, y: 0, width: 1, height: 1 } },
      { path: [1], index: 0, direction: "horizontal", rect: { x: 0.5, y: 0.25, width: 0.5, height: 0 }, splitRect: { x: 0.5, y: 0, width: 0.5, height: 1 } },
    ]);
  });
});

describe("findNeighbor", () => {
  // a | b
  //   | c
  // ---------
  //     d
  const tree = split("horizontal", [split("vertical", [a, split("horizontal", [b, c])]), d], [0.7, 0.3]);
  const rects = screenRects(tree);

  it("moves to the adjacent pane on that side", () => {
    expect(findNeighbor(rects, "b", "left")).toBe("a");
    expect(findNeighbor(rects, "b", "down")).toBe("c");
    expect(findNeighbor(rects, "c", "up")).toBe("b");
    expect(findNeighbor(rects, "c", "down")).toBe("d");
    expect(findNeighbor(rects, "a", "down")).toBe("d");
  });

  it("prefers the pane with the most overlap", () => {
    const uneven = screenRects(split("vertical", [a, split("horizontal", [b, c], [0.3, 0.7])]));
    expect(findNeighbor(uneven, "a", "right")).toBe("c");
  });

  it("returns nothing at an edge", () => {
    expect(findNeighbor(rects, "a", "left")).toBeUndefined();
    expect(findNeighbor(rects, "d", "down")).toBeUndefined();
    expect(findNeighbor(rects, "missing", "up")).toBeUndefined();
  });

  it("skips past panes that only touch at a corner", () => {
    expect(findNeighbor(rects, "a", "right")).toBe("b");
    expect(findNeighbor(rects, "d", "up")).toBe("a");
  });
});

describe("layoutFromFlat", () => {
  it("migrates a one-direction layout", () => {
    expect(layoutFromFlat(["a"], "vertical")).toEqual(a);
    expect(layoutFromFlat(["a", "b"], "horizontal", [3, 1])).toEqual(split("horizontal", [a, b], [0.75, 0.25]));
    expect(layoutFromFlat(["a", "b", "c"], "vertical", [1, "x", 1])).toEqual(split("vertical", [a, b, c]));
    expect(layoutFromFlat([], "vertical")).toBeNull();
  });
});

describe("sanitizeLayout", () => {
  let next = 0;
  const readLeaf = (saved: Record<string, unknown>) => typeof saved.profileId === "string" ? `${saved.profileId}-${next++}` : null;
  const savedPane = { type: "pane", profileId: "pwsh" };

  it("accepts a valid tree", () => {
    next = 0;
    const tree = sanitizeLayout({ type: "split", direction: "vertical", sizes: [2, 2], children: [savedPane, { type: "split", direction: "horizontal", sizes: [1, 3], children: [savedPane, savedPane] }] }, readLeaf);
    expect(tree).toEqual(split("vertical", [paneLeaf("pwsh-0"), split("horizontal", [paneLeaf("pwsh-1"), paneLeaf("pwsh-2")], [0.25, 0.75])], [0.5, 0.5]));
  });

  it("rejects malformed trees", () => {
    for (const raw of [null, 42, "pane", [], { type: "split", direction: "diagonal", children: [savedPane, savedPane] }, { type: "split", direction: "vertical", children: [] }, { type: "split", direction: "vertical", children: [savedPane, 7] }, { type: "pane" }]) {
      expect(sanitizeLayout(raw, readLeaf)).toBeNull();
    }
  });

  it("repairs sizes and collapses needless splits", () => {
    expect(sanitizeLayout({ type: "split", direction: "vertical", sizes: [-1, Infinity], children: [savedPane, savedPane] }, readLeaf)).toMatchObject({ sizes: [0.5, 0.5] });
    expect(sanitizeLayout({ type: "split", direction: "vertical", children: [savedPane] }, readLeaf)).toMatchObject({ type: "pane" });
    expect(sanitizeLayout({ type: "split", direction: "vertical", sizes: [1, 0.0001], children: [savedPane, savedPane] }, readLeaf)).toMatchObject({ sizes: [expect.closeTo(0.98, 2), expect.closeTo(0.02, 2)] });
  });

  it("caps the number of panes and the depth", () => {
    const wide = { type: "split", direction: "vertical", children: new Array(MAX_PANES + 1).fill(savedPane) };
    expect(sanitizeLayout(wide, readLeaf)).toBeNull();
    let deep: unknown = savedPane;
    for (let level = 0; level < 12; level += 1) deep = { type: "split", direction: level % 2 ? "vertical" : "horizontal", children: [deep] };
    expect(sanitizeLayout(deep, readLeaf)).toBeNull();
    const many = { type: "split", direction: "vertical", children: [wide.children.slice(0, 5), wide.children.slice(0, 4)].map((children) => ({ type: "split", direction: "horizontal", children })) };
    expect(sanitizeLayout(many, readLeaf)).toBeNull();
  });
});

describe("normalizeLayout", () => {
  it("flattens a split nested in one of the same direction", () => {
    const tree = split("vertical", [a, split("vertical", [b, c])], [0.5, 0.5]);
    expect(normalizeLayout(tree)).toEqual(split("vertical", [a, b, c], [0.5, 0.25, 0.25]));
  });
});

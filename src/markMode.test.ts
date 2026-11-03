import { describe, expect, it } from "vitest";
import { moveMark, selectionBetween } from "./markMode";

const lines = ["$ git status", "On branch main", ""];
const text = (y: number) => (lines[y] ?? "").padEnd(20);
const size = { cols: 20, rows: 2, lines: lines.length };
const move = (x: number, y: number, key: string, ctrl = false) => moveMark({ x, y }, key, ctrl, size, text);

describe("mark mode", () => {
  it("moves by a cell, wrapping at the ends of lines", () => {
    expect(move(0, 1, "ArrowLeft")).toEqual({ x: 19, y: 0 });
    expect(move(19, 0, "ArrowRight")).toEqual({ x: 0, y: 1 });
    expect(move(3, 0, "ArrowUp")).toEqual({ x: 3, y: 0 });
    expect(move(3, 2, "ArrowDown")).toEqual({ x: 3, y: 2 });
  });

  it("moves by a word with Ctrl", () => {
    expect(move(2, 0, "ArrowRight", true)).toEqual({ x: 6, y: 0 });
    expect(move(8, 0, "ArrowLeft", true)).toEqual({ x: 6, y: 0 });
    expect(move(6, 0, "ArrowLeft", true)).toEqual({ x: 2, y: 0 });
    expect(move(6, 0, "ArrowRight", true)).toEqual({ x: 0, y: 1 });
  });

  it("goes to the ends of a line, or of the buffer", () => {
    expect(move(4, 1, "End")).toEqual({ x: 13, y: 1 });
    expect(move(4, 1, "Home")).toEqual({ x: 0, y: 1 });
    expect(move(4, 1, "Home", true)).toEqual({ x: 0, y: 0 });
    expect(move(4, 0, "PageDown")).toEqual({ x: 4, y: 2 });
    expect(move(4, 0, "a")).toBeNull();
  });

  it("selects from either end", () => {
    expect(selectionBetween({ x: 5, y: 1 }, { x: 2, y: 0 }, 20)).toEqual({ column: 2, row: 0, length: 24 });
    expect(selectionBetween({ x: 2, y: 0 }, { x: 2, y: 0 }, 20)).toEqual({ column: 2, row: 0, length: 1 });
  });
});

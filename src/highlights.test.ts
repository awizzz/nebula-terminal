import { describe, expect, it } from "vitest";
import { findMatches, highlightPattern, sanitizeHighlights } from "./highlights";

const rules = (...texts: string[]) => texts.map((text) => ({ pattern: highlightPattern(text)! }));

describe("highlights", () => {
  it("find text in any case, and regular expressions between slashes", () => {
    expect(findMatches("Error: 2 errors, ERROR", rules("error"))).toEqual([
      { rule: 0, start: 0, end: 5 }, { rule: 0, start: 9, end: 14 }, { rule: 0, start: 17, end: 22 },
    ]);
    expect(findMatches("error[E0308]: mismatched types", rules("/E\\d{4}/"))).toEqual([{ rule: 0, start: 6, end: 11 }]);
    expect(findMatches("a.b axb", rules("a.b"))).toEqual([{ rule: 0, start: 0, end: 3 }]);
  });

  it("let the first highlight win where two overlap", () => {
    expect(findMatches("test FAILED", rules("failed", "/FAIL\\w*/"))).toEqual([{ rule: 0, start: 5, end: 11 }]);
  });

  it("skip empty and broken patterns", () => {
    expect(highlightPattern("  ")).toBeNull();
    expect(highlightPattern("/(/")).toBeNull();
    expect(highlightPattern("/x/g")?.flags).toBe("g");
  });

  it("are checked when read back", () => {
    expect(sanitizeHighlights([{ id: "a", text: "error", color: "plaid", notify: "yes" }, { text: "no id" }, null])).toEqual([{ id: "a", text: "error", color: "red", notify: false }]);
  });
});

import { describe, expect, it } from "vitest";
import { findPathCandidates } from "./filePaths";

const paths = (text: string) => findPathCandidates(text).map(({ path, line, column }) => [path, line, column]);

describe("findPathCandidates", () => {
  it("reads grep, compiler and linter locations", () => {
    expect(paths("src/main.rs:2:    // TODO: parse arguments")).toEqual([["src/main.rs", 2, undefined]]);
    expect(paths("  --> src/ui/window.rs:41:9")).toEqual([["src/ui/window.rs", 41, 9]]);
    expect(paths("C:\\Users\\me\\app\\main.ts(12,5): error TS2322")).toEqual([["C:\\Users\\me\\app\\main.ts", 12, 5]]);
    expect(paths("main.go:7: undefined: x")).toEqual([["main.go", 7, undefined]]);
  });

  it("reads quoted paths, with spaces and Python's line", () => {
    expect(paths('  File "C:\\Program Files\\app\\run.py", line 42, in <module>')).toEqual([["C:\\Program Files\\app\\run.py", 42, undefined]]);
    expect(paths("open 'docs/My notes.md' later")).toEqual([["docs/My notes.md", undefined, undefined]]);
  });

  it("reads home, relative and Git Bash paths", () => {
    expect(paths("cd ~/projects/app && ls ./build/out.log ../x/y")).toEqual([
      ["~/projects/app", undefined, undefined],
      ["./build/out.log", undefined, undefined],
      ["../x/y", undefined, undefined],
    ]);
    expect(paths("wrote /c/Users/me/notes.txt.")).toEqual([["/c/Users/me/notes.txt", undefined, undefined]]);
  });

  it("gives the offsets of the path and its location", () => {
    const [candidate] = findPathCandidates("error at lib/a.ts:3:4 here");
    expect(candidate).toMatchObject({ start: 9, end: 21 });
  });

  it("leaves URLs, versions, lone dots and slashes alone", () => {
    expect(paths("see https://example.com/a/b for more")).toEqual([]);
    expect(paths("version 1.2.3 released")).toEqual([]);
    expect(paths("cd .. && cd / && echo ./")).toEqual([]);
  });
});

import { describe, expect, it } from "vitest";
import { logText } from "./paneLog";

const lines = [
  { text: "$ npm test", isWrapped: false },
  { text: "a long line that wr", isWrapped: false },
  { text: "apped", isWrapped: true },
  { text: "done   ", isWrapped: false },
].map(({ text, isWrapped }) => ({ isWrapped, translateToString: (trim?: boolean) => trim ? text.trimEnd() : text }));

describe("pane logs", () => {
  it("keep wrapped lines whole and trim the rest", () => {
    expect(logText((index) => lines[index], 0, 4)).toBe("$ npm test\na long line that wrapped\ndone\n");
  });

  it("leave a line open when the next one continues it", () => {
    expect(logText((index) => lines[index], 1, 2)).toBe("a long line that wr");
    expect(logText((index) => lines[index], 2, 4)).toBe("apped\ndone\n");
  });
});

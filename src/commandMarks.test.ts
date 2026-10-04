import { describe, expect, it } from "vitest";
import { adjacentPrompt, joinLines } from "./commandMarks";

describe("adjacentPrompt", () => {
  const prompts = [0, 12, 30, 47];

  it("goes up to the nearest prompt above the screen", () => {
    expect(adjacentPrompt(prompts, 40, -1)).toBe(30);
    expect(adjacentPrompt(prompts, 30, -1)).toBe(12);
    expect(adjacentPrompt(prompts, 0, -1)).toBeUndefined();
  });

  it("goes down to the next prompt below the top line", () => {
    expect(adjacentPrompt(prompts, 12, 1)).toBe(30);
    expect(adjacentPrompt(prompts, 13, 1)).toBe(30);
    expect(adjacentPrompt(prompts, 47, 1)).toBeUndefined();
  });

  it("works whatever the order of the list", () => {
    expect(adjacentPrompt([47, 0, 30, 12], 31, -1)).toBe(30);
  });
});

describe("joinLines", () => {
  it("keeps line breaks and glues wrapped lines back together", () => {
    expect(joinLines([
      { text: "src/main.rs:2: // TODO: parse ar", wrapped: false },
      { text: "guments", wrapped: true },
      { text: "src/lib.rs:9: // TODO", wrapped: false },
      { text: "", wrapped: false },
    ])).toBe("src/main.rs:2: // TODO: parse arguments\nsrc/lib.rs:9: // TODO");
  });
});

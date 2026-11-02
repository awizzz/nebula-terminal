import { describe, expect, it } from "vitest";
import { commandInput, sanitizeSavedCommands, savedCommandLabel } from "./savedCommands";

describe("saved commands", () => {
  it("run each line as if typed", () => {
    expect(commandInput("git status")).toBe("git status\r");
    expect(commandInput("cd app\nnpm test\n")).toBe("cd app\rnpm test\r");
    expect(commandInput("make\r\n")).toBe("make\r");
  });

  it("show their name, or their first line", () => {
    expect(savedCommandLabel({ id: "a", name: "Deploy", command: "./deploy.sh" })).toBe("Deploy");
    expect(savedCommandLabel({ id: "b", name: " ", command: "cd app\nnpm test" })).toBe("cd app");
  });

  it("are checked when read back", () => {
    expect(sanitizeSavedCommands([{ id: "a", name: "x", command: "ls" }, { id: "b", command: "   " }, "nope", { name: "no id", command: "ls" }])).toEqual([{ id: "a", name: "x", command: "ls" }]);
    expect(sanitizeSavedCommands({})).toEqual([]);
  });
});

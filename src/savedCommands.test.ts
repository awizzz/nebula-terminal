import { describe, expect, it } from "vitest";
import { blanksIn, commandInput, fillBlanks, sanitizeSavedCommands, savedCommandLabel } from "./savedCommands";

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

  it("find their blanks, each once", () => {
    expect(blanksIn("ssh {{ user }}@{{host}} -p {{port}}; echo {{host}}")).toEqual(["user", "host", "port"]);
    expect(blanksIn("echo ${HOME} {a,b} '{print $1}'")).toEqual([]);
    expect(blanksIn("git checkout {{branch name}}")).toEqual(["branch name"]);
  });

  it("fill their blanks", () => {
    expect(fillBlanks("ssh {{user}}@{{ host }}; ping {{host}}", { user: "me", host: "box" })).toBe("ssh me@box; ping box");
    expect(fillBlanks("echo {{missing}}", {})).toBe("echo {{missing}}");
  });
});

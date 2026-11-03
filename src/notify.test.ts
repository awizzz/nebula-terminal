import { describe, expect, it } from "vitest";
import { commandMarkdown, finishedMessage, formatDuration, formatElapsed } from "./notify";

describe("notifications", () => {
  it("formats durations", () => {
    expect(formatDuration(4.4)).toBe("4 s");
    expect(formatDuration(125)).toBe("2 min 5 s");
    expect(formatDuration(120)).toBe("2 min");
    expect(formatDuration(3_780)).toBe("1 h 3 min");
  });

  it("says whether the command failed", () => {
    expect(finishedMessage({ command: "cargo build", code: 0, seconds: 42 }, "app")).toEqual({ title: "cargo build finished", body: "42 s · app" });
    expect(finishedMessage({ command: "npm test", code: 1, seconds: 61 }, "web")).toEqual({ title: "npm test failed", body: "1 min 1 s · web · exit code 1" });
    expect(finishedMessage({ command: "", code: null, seconds: 10 }, "x").title).toBe("Command finished");
  });
});

describe("command blocks", () => {
  it("say how long a command took", () => {
    expect(formatElapsed(0.0004)).toBe("1 ms");
    expect(formatElapsed(0.25)).toBe("250 ms");
    expect(formatElapsed(75)).toBe("1 min 15 s");
  });

  it("copy as Markdown, fenced past the output's own backticks", () => {
    expect(commandMarkdown("git status", "On branch main\n\n")).toBe("```console\n$ git status\nOn branch main\n```\n");
    expect(commandMarkdown("cat README.md", "```sh\nls\n```")).toBe("````console\n$ cat README.md\n```sh\nls\n```\n````\n");
  });
});

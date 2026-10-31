import { describe, expect, it } from "vitest";
import { drawsText } from "./splash";

describe("drawsText", () => {
  it("ignores what the console sends before the shell", () => {
    // ConPTY's first chunk: input modes, focus events, then a cleared screen.
    expect(drawsText("\x1b[?9001h\x1b[?1004h")).toBe(false);
    expect(drawsText("\x1b[?25l\x1b[2J\x1b[m\x1b[H\r\n\r\n\x1b]0;C:\\WINDOWS\\system32\\cmd.exe\x07\x1b[?25h")).toBe(false);
    expect(drawsText("\x1b]133;A\x1b\\\x1b]7;file://pc/C:/Users/me\x07")).toBe(false);
    expect(drawsText("")).toBe(false);
  });

  it("sees a prompt or a banner", () => {
    expect(drawsText("\x1b[32mPS C:\\Users\\me>\x1b[0m ")).toBe(true);
    expect(drawsText("\x1b]133;A\x07~/projects/app on main\r\n\x1b[33m❯\x1b[0m ")).toBe(true);
    expect(drawsText("Microsoft Windows [Version 10.0.26100.4061]")).toBe(true);
  });
});

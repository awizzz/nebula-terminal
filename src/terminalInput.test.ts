import { describe, expect, it } from "vitest";
import { isPaste, isTerminalReport, pasteLineCount, quoteDroppedPath, trimSingleLinePaste } from "./terminalInput";
import type { ProfileKind, TerminalProfile } from "./types";

const profile = (kind: ProfileKind, executable?: string): TerminalProfile => ({
  id: kind, name: kind, kind, available: true, accent: "#888888", executable,
});

describe("quoteDroppedPath", () => {
  const nasty = "C:\\Downloads\\a$(calc)`id`'s.txt";

  it("keeps command substitutions literal in every shell", () => {
    expect(quoteDroppedPath(nasty, profile("nebula"))).toBe("'C:\\Downloads\\a$(calc)`id`'\\''s.txt'");
    expect(quoteDroppedPath(nasty, profile("pwsh"))).toBe("'C:\\Downloads\\a$(calc)`id`''s.txt'");
    expect(quoteDroppedPath(nasty, profile("gitbash"))).toBe("'C:\\Downloads\\a$(calc)`id`'\\''s.txt'");
    expect(quoteDroppedPath("C:\\My Files\\x.txt", profile("cmd"))).toBe("\"C:\\My Files\\x.txt\"");
  });

  it("translates drive paths for WSL", () => {
    expect(quoteDroppedPath("D:\\Projects\\app", profile("wsl"))).toBe("'/mnt/d/Projects/app'");
  });

  it("guesses the shell of a custom profile from its program", () => {
    expect(quoteDroppedPath("x'y", profile("custom", "C:\\Program Files\\PowerShell\\7\\pwsh.exe"))).toBe("'x''y'");
    expect(quoteDroppedPath("x", profile("custom", "C:\\Windows\\System32\\cmd.exe"))).toBe("\"x\"");
    expect(quoteDroppedPath("x", profile("custom", "py.exe"))).toBe("'x'");
  });
});

describe("pastes", () => {
  it("tells pastes from typing", () => {
    expect(isPaste("\r")).toBe(false);
    expect(isPaste("a")).toBe(false);
    expect(isPaste("ls\r")).toBe(true);
    expect(isPaste("\x1b[200~ls\x1b[201~")).toBe(true);
  });

  it("counts lines without the bracketed-paste markers or one trailing newline", () => {
    expect(pasteLineCount("\x1b[200~ls\r\x1b[201~")).toBe(1);
    expect(pasteLineCount("curl x | sh\n")).toBe(1);
    expect(pasteLineCount("a\rb\r")).toBe(2);
    expect(pasteLineCount("\x1b[200~a\rb\x1b[201~")).toBe(2);
  });

  it("drops the newline that would run a single pasted line", () => {
    expect(trimSingleLinePaste("curl x | sh\r")).toBe("curl x | sh");
    expect(trimSingleLinePaste("\x1b[200~curl x | sh\r\x1b[201~")).toBe("\x1b[200~curl x | sh\x1b[201~");
    expect(trimSingleLinePaste("a\rb\r")).toBe("a\rb\r");
  });
});

describe("isTerminalReport", () => {
  it("recognizes what xterm answers on its own", () => {
    for (const report of ["\x1b[I", "\x1b[O", "\x1b[<0;12;4M", "\x1b[<0;12;4m", "\x1b[32;12;4M", "\x1b[M #!", "\x1b[24;80R", "\x1b[?24;80R", "\x1b[?2004;2$y", "\x1b[?1;2c", "\x1b[>0;276;0c", "\x1b[0n", "\x1b[8;24;80t", "\x1b]11;rgb:1e1e/1e1e/2e2e\x07", "\x1b]10;rgb:ffff/ffff/ffff\x1b\\", "\x1bP1$r0m\x1b\\"]) {
      expect(isTerminalReport(report), JSON.stringify(report)).toBe(true);
    }
  });

  it("leaves keys and pastes alone", () => {
    for (const input of ["a", "\r", "\x7f", "\x03", "\x1b", "\x1b[A", "\x1bOA", "\x1b[1;5C", "\x1b[3~", "\x1b[15~", "\x1b[Z", "\x1bOR", "\x1bP", "\x1b]", "\x1bc", "\x1b[200~echo \x1b[I\x1b[201~", "ls -la\r"]) {
      expect(isTerminalReport(input), JSON.stringify(input)).toBe(false);
    }
  });
});

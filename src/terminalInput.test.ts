import { describe, expect, it } from "vitest";
import { isPaste, pasteLineCount, quoteDroppedPath, trimSingleLinePaste } from "./terminalInput";
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

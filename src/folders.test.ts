import { describe, expect, it } from "vitest";
import { folderFromReport, isStartableFolder } from "./folders";

describe("folderFromReport", () => {
  it("reads drive folders", () => {
    expect(folderFromReport("file://PC/C:/Users/me")).toBe("C:\\Users\\me");
    expect(folderFromReport("file://PC/D:")).toBe("D:\\");
    expect(folderFromReport("file://PC/C:/Users/me/My%20Projects%20%231")).toBe("C:\\Users\\me\\My Projects #1");
    expect(folderFromReport("file://localhost/C:/Users/Zo%C3%A9")).toBe("C:\\Users\\Zoé");
  });

  it("reads WSL shares", () => {
    expect(folderFromReport("file://wsl.localhost/Ubuntu/home/me")).toBe("\\\\wsl.localhost\\Ubuntu\\home\\me");
    expect(folderFromReport("file://wsl$/Debian/tmp")).toBe("\\\\wsl$\\Debian\\tmp");
  });

  it("ignores folders a Windows tab can't open in", () => {
    expect(folderFromReport("file://ubuntu-box/home/me")).toBeNull();
    expect(folderFromReport("file://server/share/project")).toBeNull();
    expect(folderFromReport("file://PC/C:/bad%ZZ")).toBeNull();
    expect(folderFromReport("https://example.com/C:/x")).toBeNull();
    expect(folderFromReport("file://PC/C:/a%0Ab")).toBeNull();
  });
});

describe("isStartableFolder", () => {
  it("accepts drive paths and WSL shares only", () => {
    expect(isStartableFolder("C:\\Users\\me")).toBe(true);
    expect(isStartableFolder("\\\\wsl.localhost\\Ubuntu")).toBe(true);
    expect(isStartableFolder("\\\\server\\share")).toBe(false);
    expect(isStartableFolder("relative\\path")).toBe(false);
    expect(isStartableFolder(`C:\\${"a".repeat(2000)}`)).toBe(false);
    expect(isStartableFolder(42)).toBe(false);
  });
});

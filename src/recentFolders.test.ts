import { describe, expect, it } from "vitest";
import { folderParts, MAX_RECENT_FOLDERS, rememberFolder } from "./recentFolders";

describe("recent folders", () => {
  it("keep the newest first, once each", () => {
    let list = rememberFolder([], "C:\\Users\\me\\app");
    list = rememberFolder(list, "C:\\Users\\me\\site");
    list = rememberFolder(list, "c:\\users\\ME\\app");
    expect(list).toEqual(["c:\\users\\ME\\app", "C:\\Users\\me\\site"]);
    expect(rememberFolder(list, "  ")).toEqual(list);
    const many = Array.from({ length: 30 }, (_, index) => `C:\\f${index}`).reduce(rememberFolder, [] as string[]);
    expect(many).toHaveLength(MAX_RECENT_FOLDERS);
    expect(many[0]).toBe("C:\\f29");
  });

  it("split a folder into its name and where it is", () => {
    expect(folderParts("C:\\Users\\me\\projects\\app")).toEqual({ name: "app", parent: "C:\\Users\\me\\projects" });
    expect(folderParts("\\\\wsl.localhost\\Ubuntu\\home\\me\\")).toEqual({ name: "me", parent: "\\\\wsl.localhost\\Ubuntu\\home" });
    expect(folderParts("D:")).toEqual({ name: "D:\\", parent: "" });
  });
});

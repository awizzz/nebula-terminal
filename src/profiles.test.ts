import { describe, expect, it } from "vitest";
import { pickProfile, previewProfiles } from "./profiles";

describe("pickProfile", () => {
  it("honors an installed choice", () => {
    expect(pickProfile(previewProfiles, "cmd")?.id).toBe("cmd");
  });

  it("falls back to the first installed shell", () => {
    expect(pickProfile(previewProfiles, "wsl")?.id).toBe("nebula");
    expect(pickProfile(previewProfiles, "")?.id).toBe("nebula");
    expect(pickProfile(previewProfiles.map((profile) => ({ ...profile, available: profile.id === "gitbash" })))?.id).toBe("gitbash");
  });

  it("returns nothing when no shell is installed", () => {
    expect(pickProfile(previewProfiles.map((profile) => ({ ...profile, available: false })))).toBeUndefined();
  });
});

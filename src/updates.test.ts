import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { automaticCheckDue, dismiss, isDismissed, markChecked } from "./updates";

beforeEach(() => {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
  });
});

afterEach(() => vi.unstubAllGlobals());

describe("update checks", () => {
  it("checks at most once a day", () => {
    const now = Date.UTC(2026, 9, 1, 12);
    expect(automaticCheckDue(now)).toBe(true);
    markChecked(now);
    expect(automaticCheckDue(now + 60 * 60 * 1000)).toBe(false);
    expect(automaticCheckDue(now + 21 * 60 * 60 * 1000)).toBe(true);
    // A clock set back still lets the next check happen.
    expect(automaticCheckDue(now - 1000)).toBe(true);
  });

  it("remembers the version the user put off", () => {
    dismiss("1.1.0");
    expect(isDismissed("1.1.0")).toBe(true);
    expect(isDismissed("1.2.0")).toBe(false);
  });
});

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SILENCE_SECONDS, TabWatcher, type TabWatch } from "./tabWatch";

describe("tab watcher", () => {
  let alerts: [string, TabWatch][];
  let seen: Set<string>;
  let watcher: TabWatcher;

  beforeEach(() => {
    vi.useFakeTimers();
    alerts = [];
    seen = new Set();
    watcher = new TabWatcher((tabId, watch) => alerts.push([tabId, watch]), (tabId) => seen.has(tabId));
  });
  afterEach(() => vi.useRealTimers());

  it("tells once about output in a tab out of sight", () => {
    watcher.start("a", "output");
    seen.add("a");
    watcher.output("a");
    expect(alerts).toEqual([]);
    seen.clear();
    watcher.output("a");
    watcher.output("a");
    expect(alerts).toEqual([["a", "output"]]);
  });

  it("tells when a tab has been quiet long enough", () => {
    watcher.start("a", "silence");
    vi.advanceTimersByTime((SILENCE_SECONDS - 1) * 1000);
    watcher.output("a");
    vi.advanceTimersByTime((SILENCE_SECONDS - 1) * 1000);
    expect(alerts).toEqual([]);
    vi.advanceTimersByTime(1000);
    expect(alerts).toEqual([["a", "silence"]]);
    vi.advanceTimersByTime(SILENCE_SECONDS * 1000);
    expect(alerts).toHaveLength(1);
  });

  it("forgets a tab it stopped watching", () => {
    watcher.start("a", "silence");
    watcher.start("b", "output");
    watcher.stop("a");
    watcher.stop("b");
    watcher.output("b");
    vi.advanceTimersByTime(SILENCE_SECONDS * 1000);
    expect(alerts).toEqual([]);
  });
});

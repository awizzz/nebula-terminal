import { describe, expect, it } from "vitest";
import { parseProgress, windowProgress, type Progress } from "./progress";

describe("OSC 9;4 progress", () => {
  it("reads every state", () => {
    expect(parseProgress("4;1;42")).toEqual({ state: "normal", value: 42 });
    expect(parseProgress("4;1;250")).toEqual({ state: "normal", value: 100 });
    expect(parseProgress("4;3")).toEqual({ state: "indeterminate", value: 0 });
    expect(parseProgress("4;0")).toBeNull();
    expect(parseProgress("4;0;0")).toBeNull();
  });

  it("keeps the value an error or a pause doesn't give", () => {
    const previous: Progress = { state: "normal", value: 70 };
    expect(parseProgress("4;2", previous)).toEqual({ state: "error", value: 70 });
    expect(parseProgress("4;4;", previous)).toEqual({ state: "paused", value: 70 });
    expect(parseProgress("4;1", previous)).toEqual({ state: "normal", value: 0 });
  });

  it("leaves notifications alone", () => {
    expect(parseProgress("Build finished")).toBeUndefined();
    expect(parseProgress("4;9;10")).toBeUndefined();
    expect(parseProgress("4;1;10;extra")).toBeUndefined();
  });

  it("shows the tab in front on the taskbar, else an error, else the latest", () => {
    const progress = new Map<string, Progress>([["a", { state: "normal", value: 10 }], ["b", { state: "error", value: 50 }], ["c", { state: "normal", value: 90 }]]);
    expect(windowProgress(progress, "c")).toEqual({ state: "normal", value: 90 });
    expect(windowProgress(progress, "z")).toEqual({ state: "error", value: 50 });
    progress.delete("b");
    expect(windowProgress(progress, "z")).toEqual({ state: "normal", value: 90 });
    expect(windowProgress(new Map(), "z")).toBeUndefined();
  });
});

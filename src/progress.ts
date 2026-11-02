/**
 * Progress that programs report with OSC 9;4 (ConEmu's sequence, which Windows Terminal,
 * winget and others use): `ESC ] 9 ; 4 ; state ; percent ST`.
 */

export type ProgressState = "normal" | "error" | "indeterminate" | "paused";

export interface Progress {
  state: ProgressState;
  /** 0 to 100. */
  value: number;
}

const STATES: Record<string, ProgressState | null> = { "0": null, "1": "normal", "2": "error", "3": "indeterminate", "4": "paused" };

/**
 * The progress an OSC 9 payload reports: null to clear it, undefined when the payload is
 * something else (a notification). Error and paused keep the last value without one.
 */
export function parseProgress(data: string, previous?: Progress): Progress | null | undefined {
  const match = /^4;([0-4])(?:;(\d{0,3}))?$/.exec(data);
  if (!match) return undefined;
  const state = STATES[match[1]!];
  if (!state) return null;
  const given = match[2] ? Math.min(100, Number(match[2])) : undefined;
  return { state, value: given ?? (state === "normal" ? 0 : previous?.value ?? 0) };
}

/** What the taskbar button shows: the tab in front, else an error, else the latest report. */
export function windowProgress(progress: ReadonlyMap<string, Progress>, activeTabId: string): Progress | undefined {
  const front = progress.get(activeTabId);
  if (front) return front;
  const all = [...progress.values()];
  return all.find((report) => report.state === "error") ?? all.at(-1);
}

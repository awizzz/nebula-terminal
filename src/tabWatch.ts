import type { TabWatch } from "./types";

export type { TabWatch };

export const SILENCE_SECONDS = 30;

export class TabWatcher {
  private readonly watching = new Map<string, TabWatch>();
  private readonly timers = new Map<string, ReturnType<typeof setTimeout>>();

  /**
   * `alert` runs when a watch is over: output in a tab out of sight (output in front is
   * yours to see), or no output at all for {@link SILENCE_SECONDS}.
   */
  constructor(
    private readonly alert: (tabId: string, watch: TabWatch) => void,
    private readonly seen: (tabId: string) => boolean,
  ) {}

  start(tabId: string, watch: TabWatch): void {
    this.stop(tabId);
    this.watching.set(tabId, watch);
    if (watch === "silence") this.arm(tabId);
  }

  stop(tabId: string): void {
    this.watching.delete(tabId);
    clearTimeout(this.timers.get(tabId));
    this.timers.delete(tabId);
  }

  output(tabId: string): void {
    const watch = this.watching.get(tabId);
    if (watch === "silence") this.arm(tabId);
    else if (watch === "output" && !this.seen(tabId)) this.end(tabId, watch);
  }

  private arm(tabId: string): void {
    clearTimeout(this.timers.get(tabId));
    this.timers.set(tabId, setTimeout(() => this.end(tabId, "silence"), SILENCE_SECONDS * 1000));
  }

  private end(tabId: string, watch: TabWatch): void {
    this.stop(tabId);
    this.alert(tabId, watch);
  }
}

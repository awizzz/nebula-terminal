import type { TerminalTab } from "./types";

/**
 * The last screen of each pane, so a restored session shows what its panes printed.
 * IndexedDB rather than localStorage: a thousand colored lines per pane don't fit there.
 * Keys are `<window label>:<pane id>`, so each window tidies its own.
 */
const DATABASE = "nebula-terminal.screens";
const STORE = "screens";
/** Screens of windows that never came back are dropped after this long. */
const KEEP_FOR_MS = 30 * 24 * 60 * 60 * 1000;

interface SavedScreen {
  key: string;
  screen: string;
  savedAt: number;
}

let database: Promise<IDBDatabase> | undefined;

function open(): Promise<IDBDatabase> {
  database ??= new Promise((resolve, reject) => {
    const request = indexedDB.open(DATABASE, 1);
    request.onupgradeneeded = () => request.result.createObjectStore(STORE, { keyPath: "key" }).createIndex("savedAt", "savedAt");
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
  database.catch(() => { database = undefined; });
  return database;
}

/** Runs `work` in one transaction and resolves once it's on disk. */
async function transaction(mode: IDBTransactionMode, work: (store: IDBObjectStore) => void): Promise<void> {
  const db = await open();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(STORE, mode);
    work(tx.objectStore(STORE));
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  });
}

export function screenKey(windowLabel: string, paneId: string): string {
  return `${windowLabel}:${paneId}`;
}

/** Saves these screens, and drops this window's screens of panes that are gone. */
export async function saveScreens(windowLabel: string, screens: ReadonlyMap<string, string>, keep: ReadonlySet<string>): Promise<void> {
  const prefix = `${windowLabel}:`;
  const savedAt = Date.now();
  await transaction("readwrite", (store) => {
    for (const [key, screen] of screens) store.put({ key, screen, savedAt } satisfies SavedScreen);
    const own = store.getAllKeys(IDBKeyRange.bound(prefix, `${prefix}￿`));
    own.onsuccess = () => {
      for (const key of own.result as string[]) if (!keep.has(key) && !screens.has(key)) store.delete(key);
    };
  });
}

/** A saved screen, which is then forgotten: the pane saves it again under its new key. */
export async function takeScreen(key: string): Promise<{ screen: string; savedAt: number } | undefined> {
  let found: { screen: string; savedAt: number } | undefined;
  await transaction("readwrite", (store) => {
    const request = store.get(key);
    request.onsuccess = () => {
      const saved = request.result as Partial<SavedScreen> | undefined;
      if (typeof saved?.screen === "string") found = { screen: saved.screen, savedAt: typeof saved.savedAt === "number" ? saved.savedAt : Date.now() };
      store.delete(key);
    };
  });
  return found;
}

/** Forgets a window's screens (it closed while others stay open), or every screen. */
export async function forgetScreens(windowLabel?: string): Promise<void> {
  await transaction("readwrite", (store) => {
    if (windowLabel === undefined) store.clear();
    else store.delete(IDBKeyRange.bound(`${windowLabel}:`, `${windowLabel}:￿`));
  });
}

/** Drops screens nobody came back for in a month. */
export async function pruneScreens(now = Date.now()): Promise<void> {
  await transaction("readwrite", (store) => {
    store.index("savedAt").openCursor(IDBKeyRange.upperBound(now - KEEP_FOR_MS)).onsuccess = function () {
      const cursor = this.result;
      if (!cursor) return;
      cursor.delete();
      cursor.continue();
    };
  });
}

/** The line written under a restored screen, before the new shell's prompt. */
export function restoredLine(savedAt: number): string {
  const when = new Date(savedAt).toLocaleString(undefined, { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
  return `\x1b[0m\r\n\x1b[2m── Restored · ${when} ──\x1b[0m\r\n\r\n`;
}

/** How long a window waits for its saved screens before it opens without them. */
const LOAD_FOR_AT_MOST_MS = 800;

/** Gives each restored pane the screen it showed last time, read from the store. */
export async function withScreens<T extends { tabs: TerminalTab[] } | null>(restored: T): Promise<T> {
  const panes = restored?.tabs.flatMap((tab) => tab.panes).filter((pane) => pane.restoreScreen) ?? [];
  if (!panes.length) return restored;
  const loaded = await Promise.race([
    Promise.all(panes.map(async (pane) => [pane, await takeScreen(pane.restoreScreen!).catch(() => undefined)] as const)),
    new Promise<[]>((resolve) => window.setTimeout(() => resolve([]), LOAD_FOR_AT_MOST_MS)),
  ]);
  for (const [pane, saved] of loaded) if (saved) pane.screen = saved.screen + restoredLine(saved.savedAt);
  return restored;
}

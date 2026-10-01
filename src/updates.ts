import { invoke, isTauri } from "@tauri-apps/api/core";
import { version } from "../package.json";

export type InstallKind = "installer" | "msi" | "manual";

export interface UpdateInfo {
  version: string;
  current: string;
  notes: string;
  url: string;
  install: InstallKind;
}

const CHECKED_KEY = "nebula-terminal.update-checked";
const DISMISSED_KEY = "nebula-terminal.update-dismissed";
/** Automatic checks happen at most this often. */
const CHECK_INTERVAL = 20 * 60 * 60 * 1000;

/** The browser preview shows a made-up update with `#update` in the address. */
function previewUpdate(): UpdateInfo | null {
  if (!location.hash.includes("update")) return null;
  return {
    version: "1.2.0",
    current: version,
    notes: "A made-up release for the browser preview.",
    url: "https://github.com/awizzz/nebula-shell/releases/latest",
    install: "installer",
  };
}

export async function checkForUpdate(): Promise<UpdateInfo | null> {
  if (!isTauri()) return previewUpdate();
  return (await invoke<UpdateInfo | null>("check_for_update")) ?? null;
}

/** Downloads and verifies the new version, starts its installer and quits. */
export async function installUpdate(): Promise<void> {
  if (!isTauri()) {
    await new Promise((resolve) => window.setTimeout(resolve, 1500));
    throw new Error("Updates install from the desktop app.");
  }
  await invoke("install_update");
}

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Remembering the last check is only a convenience.
  }
}

export function automaticCheckDue(now = Date.now()): boolean {
  const last = Number(read(CHECKED_KEY));
  return !Number.isFinite(last) || now - last >= CHECK_INTERVAL || last > now;
}

export function markChecked(now = Date.now()): void {
  write(CHECKED_KEY, String(now));
}

/** "Later" hides one version; the next one shows up again. */
export function isDismissed(version: string): boolean {
  return read(DISMISSED_KEY) === version;
}

export function dismiss(version: string): void {
  write(DISMISSED_KEY, version);
}

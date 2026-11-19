import { invoke, isTauri } from "@tauri-apps/api/core";

declare global {
  interface Window {
    /** "windows", "linux" or "macos", set by the desktop host (src-tauri/src/platform.rs). */
    __NEBULA_PLATFORM__?: string;
    /** This machine's name, to tell its folders from a server's in OSC 7 reports. */
    __NEBULA_HOST__?: string;
  }
}

export type Platform = "windows" | "linux" | "macos";

function detect(): Platform {
  if (typeof window === "undefined") return "windows";
  // The browser preview shows the Windows app unless asked for another (?platform=linux).
  const value = window.__NEBULA_PLATFORM__ ?? new URLSearchParams(window.location.search).get("platform");
  return value === "linux" || value === "macos" ? value : "windows";
}

export const platform: Platform = detect();
export const isWindows = platform === "windows";

export const hostName = typeof window !== "undefined" ? window.__NEBULA_HOST__ ?? "" : "";

/** What the system calls the app that shows folders. */
export const fileManagerName = isWindows ? "File Explorer" : "the file manager";

let build: Promise<number | undefined> | undefined;

/** The Windows build number, read once; undefined elsewhere and outside the desktop app. */
export function windowsBuild(): Promise<number | undefined> {
  build ??= isTauri() && isWindows
    ? invoke<number | null>("windows_build").then((value) => value ?? undefined).catch(() => undefined)
    : Promise.resolve(undefined);
  return build;
}

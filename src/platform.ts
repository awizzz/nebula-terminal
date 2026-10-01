import { invoke, isTauri } from "@tauri-apps/api/core";

let build: Promise<number | undefined> | undefined;

/** The Windows build number, read once; undefined outside the desktop app. */
export function windowsBuild(): Promise<number | undefined> {
  build ??= isTauri() && navigator.userAgent.includes("Windows")
    ? invoke<number | null>("windows_build").then((value) => value ?? undefined).catch(() => undefined)
    : Promise.resolve(undefined);
  return build;
}

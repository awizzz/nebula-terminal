import { invoke } from "@tauri-apps/api/core";
import type { AppearancePreferences } from "./types";

/** What an admin window opens first. See src-tauri/src/elevation.rs. */
export interface AdministratorLaunch {
  profileId?: string | null;
  folder?: string | null;
  /** The settings of the window that asked; checked like saved ones before use. */
  preferences?: unknown;
}

declare global {
  interface Window {
    /** Set before the page loads, in admin windows only. */
    __NEBULA_ADMIN__?: AdministratorLaunch;
  }
}

/** The request this admin window was opened for; undefined in an ordinary window. */
export function administratorLaunch(): AdministratorLaunch | undefined {
  return typeof window === "undefined" ? undefined : window.__NEBULA_ADMIN__;
}

/** Every shell in this window runs as administrator. */
export const isAdministrator = administratorLaunch() !== undefined;

/**
 * Opens an admin window on a profile once the user accepts UAC. Resolves to false when
 * they decline, and rejects when Windows couldn't start it.
 */
export async function openAsAdministrator(profileId: string, folder: string | undefined, preferences: AppearancePreferences): Promise<boolean> {
  try {
    await invoke("open_as_administrator", { profileId, folder: folder ?? null, preferences });
    return true;
  } catch (error) {
    if (error === "cancelled") return false;
    throw error;
  }
}

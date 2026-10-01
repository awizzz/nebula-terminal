import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

/** Opens an http(s) link in the default browser. Anything else is ignored. */
export function openExternal(url: string): void {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return;
  }
  if (parsed.protocol !== "https:" && parsed.protocol !== "http:") return;
  if (isTauri()) void openUrl(parsed.href).catch(() => undefined);
  else window.open(parsed.href, "_blank", "noopener,noreferrer");
}

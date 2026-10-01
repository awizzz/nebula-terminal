import { isTauri } from "@tauri-apps/api/core";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import type { FinishedCommand } from "./components/TerminalPane";

/** `4 s`, `2 min 5 s`, `1 h 3 min`. */
export function formatDuration(seconds: number): string {
  const total = Math.max(0, Math.round(seconds));
  if (total < 60) return `${total} s`;
  const minutes = Math.floor(total / 60);
  if (minutes < 60) return total % 60 ? `${minutes} min ${total % 60} s` : `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  return minutes % 60 ? `${hours} h ${minutes % 60} min` : `${hours} h`;
}

/** The text of the notification for a command that finished. */
export function finishedMessage(finished: FinishedCommand, tabTitle: string): { title: string; body: string } {
  const command = finished.command.trim() || "Command";
  const failed = finished.code !== null && finished.code !== 0;
  const title = failed ? `${command} failed` : `${command} finished`;
  const details = [formatDuration(finished.seconds), tabTitle];
  if (failed) details.push(`exit code ${finished.code}`);
  return { title, body: details.join(" · ") };
}

let permission: Promise<boolean> | undefined;

/** Shows a Windows notification. Outside the desktop app it does nothing. */
export function notify(title: string, body: string): void {
  if (!isTauri()) return;
  permission ??= isPermissionGranted()
    .then(async (granted) => granted || (await requestPermission()) === "granted")
    .catch(() => false);
  void permission.then((granted) => {
    if (granted) sendNotification({ title, body });
  });
}

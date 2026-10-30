import { invoke, isTauri } from "@tauri-apps/api/core";
import { previewProfiles } from "./profiles";
import type { CustomProfile, CustomProfileDraft, ProfileFieldError, SshConnection, SshDraft, TerminalProfile } from "./types";

/*
 * Custom profiles are stored and validated by the desktop host (src-tauri/src/custom.rs).
 * The browser preview keeps a few in memory instead, so the editor can be tried there.
 */

const previewStore: CustomProfile[] = [
  {
    id: "custom:6f1c2a7e-3b7d-4c55-9d0e-8a41f2b6c901",
    name: "Developer PowerShell",
    executable: "pwsh.exe",
    args: ["-NoExit", "-Command", "&{Import-Module \"C:\\Program Files\\Microsoft Visual Studio\\2022\\Community\\Common7\\Tools\\Microsoft.VisualStudio.DevShell.dll\"; Enter-VsDevShell 4c1a7e2b}"],
    arguments: "-NoExit -Command \"&{Import-Module \\\"C:\\Program Files\\Microsoft Visual Studio\\2022\\Community\\Common7\\Tools\\Microsoft.VisualStudio.DevShell.dll\\\"; Enter-VsDevShell 4c1a7e2b}\"",
    cwd: "%USERPROFILE%\\source\\repos",
    accent: "#8b8cf0",
  },
  {
    id: "custom:0b9d4e63-58a2-4f1b-a7c4-2e5d9f8a6b10",
    name: "Python 3.13",
    executable: "py.exe",
    args: ["-3.13"],
    arguments: "-3.13",
    cwd: null,
    accent: "#3fb27f",
  },
];

const HEX_COLOR = /^#[0-9a-f]{6}$/i;
// The same shapes custom.rs accepts. None can start with "-", so ssh never reads one as an option.
const HOST = /^(?!-)[A-Za-z0-9.\-_:[\]]{1,253}$/;
const USER = /^(?!-)[A-Za-z0-9.\-_\\@]{1,64}$/;
const PORT = "([1-9]\\d{0,4})";
const ADDRESS = "(\\*|\\[[0-9A-Fa-f:.]+\\]|[A-Za-z0-9._][A-Za-z0-9.\\-_]*)";
const FORWARD = new RegExp(`^(?:${ADDRESS}:)?${PORT}:${ADDRESS}:${PORT}$`);

export const emptySsh: SshDraft = { host: "", user: "", port: "", identityFile: "", localForwards: "", remoteForwards: "" };

/** A new, empty draft. */
export function newDraft(accent: string, kind: CustomProfileDraft["kind"] = "program"): CustomProfileDraft {
  return { name: "", executable: "", arguments: "", cwd: "", accent, kind, ssh: { ...emptySsh } };
}

/** The editor's draft for a saved profile. */
export function draftOf(profile: CustomProfile): CustomProfileDraft {
  const { ssh } = profile;
  if (!ssh) return { ...newDraft(profile.accent), id: profile.id, name: profile.name, executable: profile.executable, arguments: profile.arguments, cwd: profile.cwd ?? "" };
  return {
    ...newDraft(profile.accent, "ssh"),
    id: profile.id,
    name: profile.name,
    ssh: {
      host: ssh.host,
      user: ssh.user ?? "",
      port: ssh.port ? String(ssh.port) : "",
      identityFile: ssh.identityFile ?? "",
      localForwards: ssh.localForwards.join("\n"),
      remoteForwards: ssh.remoteForwards.join("\n"),
    },
  };
}

export function forwardList(text: string): string[] {
  return text.split(/[\n,]/).map((line) => line.trim()).filter(Boolean);
}

function previewPort(value: string): number | null {
  const port = Number(value.trim());
  return value.trim() && Number.isInteger(port) && port > 0 && port < 65536 ? port : null;
}

/** The arguments ssh will get, in the order custom.rs builds them. */
export function sshArguments(ssh: SshDraft): string[] {
  const args: string[] = [];
  if (ssh.port.trim()) args.push("-p", ssh.port.trim());
  const key = ssh.identityFile.trim().replace(/^"|"$/g, "").trim();
  if (key) args.push("-i", key);
  for (const forward of forwardList(ssh.localForwards)) args.push("-L", forward);
  for (const forward of forwardList(ssh.remoteForwards)) args.push("-R", forward);
  const host = ssh.host.trim();
  const user = ssh.user.trim();
  if (host) args.push(user ? `${user}@${host}` : host);
  return args;
}

/** The command line, for the editor and the list of profiles. */
export function sshCommand(ssh: SshDraft): string {
  return ["ssh", ...sshArguments(ssh).map(quotePreviewArgument)].join(" ");
}

function validateSshPreview(ssh: SshDraft): ProfileFieldError | null {
  const host = ssh.host.trim();
  if (!host) return { field: "host", message: "Enter the server's name or address." };
  if (!HOST.test(host)) return { field: "host", message: "Use a host name or an IP address, like server.example.com or 192.168.1.20." };
  if (ssh.user.trim() && !USER.test(ssh.user.trim())) return { field: "user", message: "Use letters, digits, dots, dashes and underscores." };
  if (ssh.port.trim() && previewPort(ssh.port) === null) return { field: "port", message: "Use a port from 1 to 65535." };
  for (const field of ["localForwards", "remoteForwards"] as const) {
    const list = forwardList(ssh[field]);
    if (list.length > 16) return { field, message: "Use at most 16 forwards." };
    const bad = list.find((forward) => !FORWARD.test(forward));
    if (bad) return { field, message: `\u201c${bad}\u201d isn't a forward: use port:host:port, like 8080:localhost:80.` };
  }
  return null;
}

/**
 * Splits like the desktop host for everyday input: whitespace outside double quotes
 * separates arguments, and `\"` is a literal quote. The host applies the complete
 * Windows rules; this is only for the browser preview.
 */
export function previewSplitArguments(line: string): string[] {
  const args: string[] = [];
  let current = "";
  let quoted = false;
  let started = false;
  for (let index = 0; index < line.length; index += 1) {
    const char = line[index]!;
    if (char === "\\" && line[index + 1] === "\"") {
      current += "\"";
      started = true;
      index += 1;
    } else if (char === "\"") {
      quoted = !quoted;
      started = true;
    } else if (/\s/.test(char) && !quoted) {
      if (started) args.push(current);
      current = "";
      started = false;
    } else {
      current += char;
      started = true;
    }
  }
  if (started) args.push(current);
  return args;
}

function quotePreviewArgument(arg: string): string {
  return arg && !/[\s"]/.test(arg) ? arg : `"${arg.replace(/"/g, "\\\"")}"`;
}

function fieldError(error: unknown): ProfileFieldError {
  if (error && typeof error === "object" && "message" in error) {
    const { field, message } = error as Partial<ProfileFieldError>;
    return { field: field ?? null, message: String(message) };
  }
  return { field: null, message: String(error) };
}

/** Preview counterpart of the checks in custom.rs that need no file system. */
function validatePreview(draft: CustomProfileDraft): ProfileFieldError | null {
  if (!draft.name.trim()) return { field: "name", message: "Enter a name." };
  if (draft.name.trim().length > 60) return { field: "name", message: "Use up to 60 characters, on one line." };
  if (draft.kind === "ssh") {
    const problem = validateSshPreview(draft.ssh);
    if (problem) return problem;
  } else if (!draft.executable.trim()) return { field: "executable", message: "Enter the program to run." };
  if (!HEX_COLOR.test(draft.accent.trim())) return { field: "accent", message: "Use a color like #5b8def." };
  return null;
}

export async function listCustomProfiles(): Promise<CustomProfile[]> {
  if (isTauri()) return invoke<CustomProfile[]>("list_custom_profiles");
  return previewStore.map((profile) => ({ ...profile, args: [...profile.args] }));
}

/** Saves a profile. Rejects with a {@link ProfileFieldError} the editor can show. */
export async function saveCustomProfile(draft: CustomProfileDraft): Promise<CustomProfile> {
  if (isTauri()) {
    try {
      const { kind, ssh, ...fields } = draft;
      return await invoke<CustomProfile>("save_custom_profile", { profile: { ...fields, id: draft.id ?? null, ssh: kind === "ssh" ? ssh : null } });
    } catch (error) {
      throw fieldError(error);
    }
  }

  const problem = validatePreview(draft);
  if (problem) throw problem;
  const ssh = draft.kind === "ssh" ? previewConnection(draft.ssh) : null;
  const args = ssh ? sshArguments(draft.ssh) : previewSplitArguments(draft.arguments);
  const saved: CustomProfile = {
    id: draft.id ?? `custom:${crypto.randomUUID()}`,
    name: draft.name.trim(),
    executable: ssh ? "ssh" : draft.executable.trim(),
    args,
    arguments: args.map(quotePreviewArgument).join(" "),
    cwd: ssh ? null : draft.cwd.trim() || null,
    accent: draft.accent.trim().toLowerCase(),
    ssh,
  };
  const index = previewStore.findIndex((profile) => profile.id === saved.id);
  if (index >= 0) previewStore[index] = saved;
  else previewStore.push(saved);
  return saved;
}

function previewConnection(ssh: SshDraft): SshConnection {
  return {
    host: ssh.host.trim(),
    user: ssh.user.trim() || null,
    port: previewPort(ssh.port),
    identityFile: ssh.identityFile.trim().replace(/^"|"$/g, "").trim() || null,
    localForwards: forwardList(ssh.localForwards),
    remoteForwards: forwardList(ssh.remoteForwards),
  };
}

export async function deleteCustomProfile(id: string): Promise<void> {
  if (isTauri()) return invoke("delete_custom_profile", { id });
  const index = previewStore.findIndex((profile) => profile.id === id);
  if (index >= 0) previewStore.splice(index, 1);
}

/** How the host will split an arguments line, for the editor's preview. */
export async function splitArguments(line: string): Promise<string[]> {
  if (isTauri()) return invoke<string[]>("split_arguments", { line });
  return previewSplitArguments(line);
}

function previewTerminalProfile(profile: CustomProfile): TerminalProfile {
  return {
    id: profile.id,
    name: profile.name,
    kind: profile.ssh ? "ssh" : "custom",
    available: true,
    executable: profile.executable,
    commandLine: [quotePreviewArgument(profile.executable), profile.arguments].filter(Boolean).join(" "),
    accent: profile.accent,
  };
}

/** Every profile that can be opened: built-in shells, WSL, SSH and custom profiles. */
export async function detectProfiles(): Promise<TerminalProfile[]> {
  if (isTauri()) return invoke<TerminalProfile[]>("detect_profiles");
  return [...previewProfiles, ...previewStore.map(previewTerminalProfile)];
}

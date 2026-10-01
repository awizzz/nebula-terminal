import type { TerminalProfile, TerminalTab } from "./types";

/** Shown in the browser preview, where no PTY backend is available. */
export const previewProfiles: TerminalProfile[] = [
  { id: "nebula", name: "Nebula", kind: "nebula", available: true, executable: "nebula-sh.exe", commandLine: "C:\\Program Files\\Nebula Terminal\\nebula-sh.exe", accent: "#e8a33d" },
  { id: "pwsh", name: "PowerShell", kind: "pwsh", available: true, executable: "pwsh.exe", commandLine: "\"C:\\Program Files\\PowerShell\\7\\pwsh.exe\" -NoLogo", accent: "#5b8def" },
  { id: "powershell", name: "Windows PowerShell", kind: "powershell", available: true, executable: "powershell.exe", commandLine: "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe -NoLogo", accent: "#3f7cc4" },
  { id: "cmd", name: "Command Prompt", kind: "cmd", available: true, executable: "cmd.exe", commandLine: "C:\\Windows\\System32\\cmd.exe", accent: "#9aa3ab" },
  { id: "gitbash", name: "Git Bash", kind: "gitbash", available: false, accent: "#e5734a" },
  { id: "wsl", name: "WSL", kind: "wsl", available: true, executable: "wsl.exe", commandLine: "C:\\Windows\\System32\\wsl.exe --cd ~", accent: "#e0a040" },
  { id: "wsl:Ubuntu-24.04", name: "Ubuntu-24.04", kind: "wsl", available: true, executable: "wsl.exe", commandLine: "C:\\Windows\\System32\\wsl.exe -d Ubuntu-24.04 --cd ~", accent: "#e0a040" },
  { id: "wsl:Debian", name: "Debian", kind: "wsl", available: true, executable: "wsl.exe", commandLine: "C:\\Windows\\System32\\wsl.exe -d Debian --cd ~", accent: "#e0a040" },
  { id: "ssh:build-server", name: "build-server", kind: "ssh", available: true, executable: "ssh.exe", commandLine: "C:\\Windows\\System32\\OpenSSH\\ssh.exe build-server", accent: "#4f9d8f" },
  { id: "ssh:pi", name: "pi", kind: "ssh", available: true, executable: "ssh.exe", commandLine: "C:\\Windows\\System32\\OpenSSH\\ssh.exe pi", accent: "#4f9d8f" },
];

export type ProfileGroup = "shell" | "wsl" | "ssh" | "custom";

export const profileGroupLabels: Record<ProfileGroup, string> = {
  shell: "Shells",
  wsl: "WSL distributions",
  ssh: "SSH hosts",
  custom: "Custom profiles",
};

const groupOrder: ProfileGroup[] = ["shell", "wsl", "ssh", "custom"];

/** Built-in shells keep plain ids; the others carry their source as a prefix. */
export function profileGroup(profile: Pick<TerminalProfile, "id">): ProfileGroup {
  if (profile.id.startsWith("wsl:")) return "wsl";
  if (profile.id.startsWith("ssh:")) return "ssh";
  if (profile.id.startsWith("custom:")) return "custom";
  return "shell";
}

/** Profiles by group, in display order. Empty groups are left out. */
export function groupProfiles(profiles: TerminalProfile[]): Array<{ group: ProfileGroup; profiles: TerminalProfile[] }> {
  return groupOrder
    .map((group) => ({ group, profiles: profiles.filter((profile) => profileGroup(profile) === group) }))
    .filter((section) => section.profiles.length > 0);
}

/** Above this many entries, the new-tab menu gets a heading per group. */
export const MENU_GROUPING_THRESHOLD = 8;

export interface ProfileMenuSection {
  heading?: string;
  profiles: TerminalProfile[];
}

/**
 * Sections of the new-tab menu. A short list stays flat; a long one is split into
 * shells, WSL distributions, SSH hosts and custom profiles, each with a heading.
 */
export function profileMenuSections(profiles: TerminalProfile[]): ProfileMenuSection[] {
  if (profiles.length <= MENU_GROUPING_THRESHOLD) return profiles.length ? [{ profiles: groupProfiles(profiles).flatMap((section) => section.profiles) }] : [];
  return groupProfiles(profiles).map(({ group, profiles: members }) => ({
    heading: group === "shell" ? undefined : profileGroupLabels[group],
    profiles: members,
  }));
}

/** A short tag that tells a WSL distribution or an SSH host apart in a flat list. */
export function profileTag(profile: TerminalProfile): string | undefined {
  const group = profileGroup(profile);
  if (group === "wsl") return "WSL";
  if (group === "ssh") return "SSH";
  return undefined;
}

/** Label of the command palette entry that opens a profile. */
export function profileCommandLabel(profile: TerminalProfile): string {
  return profileGroup(profile) === "ssh" ? `Connect to ${profile.name}` : `New ${profile.name} tab`;
}

/**
 * Picks the profile to open: the requested one when it is installed, otherwise
 * the first available profile. The backend lists profiles in order of preference.
 */
export function pickProfile(profiles: TerminalProfile[], requestedId?: string): TerminalProfile | undefined {
  return profiles.find((profile) => profile.id === requestedId && profile.available)
    ?? profiles.find((profile) => profile.available);
}

/**
 * Brings open panes up to date after profiles were detected again, for example after
 * a custom profile was renamed. A pane whose profile is gone keeps it: its shell is
 * still running, and changing the profile would restart it.
 */
export function refreshTabProfiles(tabs: TerminalTab[], profiles: TerminalProfile[]): TerminalTab[] {
  const byId = new Map(profiles.map((profile) => [profile.id, profile]));
  return tabs.map((tab) => ({
    ...tab,
    panes: tab.panes.map((pane) => ({ ...pane, profile: byId.get(pane.profile.id) ?? pane.profile })),
  }));
}

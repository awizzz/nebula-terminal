import type { TerminalProfile } from "./types";

/** Shown in the browser preview, where no PTY backend is available. */
export const previewProfiles: TerminalProfile[] = [
  { id: "nebula", name: "Nebula", kind: "nebula", available: true, executable: "nebula-sh.exe", accent: "#e8a33d" },
  { id: "pwsh", name: "PowerShell", kind: "pwsh", available: true, executable: "pwsh.exe", accent: "#5b8def" },
  { id: "powershell", name: "Windows PowerShell", kind: "powershell", available: true, executable: "powershell.exe", accent: "#3f7cc4" },
  { id: "cmd", name: "Command Prompt", kind: "cmd", available: true, executable: "cmd.exe", accent: "#9aa3ab" },
  { id: "gitbash", name: "Git Bash", kind: "gitbash", available: true, executable: "bash.exe", accent: "#e5734a" },
  { id: "wsl", name: "WSL", kind: "wsl", available: false, accent: "#e0a040" },
];

/**
 * Picks the profile to open: the requested one when it is installed, otherwise
 * the first available profile. The backend lists profiles in order of preference.
 */
export function pickProfile(profiles: TerminalProfile[], requestedId?: string): TerminalProfile | undefined {
  return profiles.find((profile) => profile.id === requestedId && profile.available)
    ?? profiles.find((profile) => profile.available);
}

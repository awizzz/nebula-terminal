export type ProfileKind = "nebula" | "cmd" | "powershell" | "pwsh" | "wsl";

export interface TerminalProfile {
  id: string;
  name: string;
  kind: ProfileKind;
  available: boolean;
  executable?: string;
  accent: string;
}

export type BackgroundMode = "mica" | "solid";
export type AnimationLevel = "full" | "reduced" | "off";
export type CursorStyle = "block" | "bar" | "underline";
export type TabDensity = "comfortable" | "compact";

export interface AppearancePreferences {
  accent: string;
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  cursorStyle: CursorStyle;
  cursorBlink: boolean;
  terminalPadding: number;
  terminalOpacity: number;
  backgroundMode: BackgroundMode;
  animationLevel: AnimationLevel;
  tabDensity: TabDensity;
}

export interface TerminalTab {
  id: string;
  title: string;
  profile: TerminalProfile;
}

export interface PtyEvent {
  event: "output" | "exit" | "error";
  data: {
    sessionId: string;
    chunk?: string;
    code?: number;
    message?: string;
  };
}

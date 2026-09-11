export type ProfileKind = "nebula" | "cmd" | "powershell" | "pwsh" | "wsl" | "ssh" | "custom";

export interface TerminalProfile {
  id: string;
  name: string;
  kind: ProfileKind;
  available: boolean;
  executable?: string;
  accent: string;
}

export type BackgroundMode = "mica" | "solid" | "image";
export type AnimationLevel = "full" | "reduced" | "off";
export type CursorStyle = "block" | "bar" | "underline";
export type TabDensity = "comfortable" | "compact";
export type SplitDirection = "horizontal" | "vertical";

export interface KeybindingPreferences {
  newTab: string;
  closeTab: string;
  commandPalette: string;
  settings: string;
  find: string;
  splitVertical: string;
  splitHorizontal: string;
  closePane: string;
}

export interface AppearancePreferences {
  accent: string;
  themeId: string;
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  cursorStyle: CursorStyle;
  cursorBlink: boolean;
  terminalPadding: number;
  terminalOpacity: number;
  backgroundMode: BackgroundMode;
  backgroundImage?: string;
  backgroundImageOpacity: number;
  animationLevel: AnimationLevel;
  tabDensity: TabDensity;
  defaultProfileId: string;
  restoreSession: boolean;
  copyOnSelect: boolean;
  confirmCloseMultipleTabs: boolean;
  keybindings: KeybindingPreferences;
}

export interface TerminalPaneModel {
  id: string;
  profile: TerminalProfile;
}

export interface TerminalTab {
  id: string;
  title: string;
  panes: TerminalPaneModel[];
  activePaneId: string;
  splitDirection: SplitDirection;
}

export interface PersistedPane {
  profileId: string;
}

export interface PersistedTab {
  title: string;
  panes: PersistedPane[];
  activePaneIndex: number;
  splitDirection: SplitDirection;
}

export interface SessionSnapshot {
  version: 1;
  tabs: PersistedTab[];
  activeTabIndex: number;
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

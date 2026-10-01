import type { LayoutNode } from "./layout";

export type ProfileKind = "nebula" | "pwsh" | "powershell" | "cmd" | "gitbash" | "wsl";

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
/** "vertical" puts panes side by side (split right), "horizontal" stacks them (split down). */
export type SplitDirection = "horizontal" | "vertical";
export type TabColor = "red" | "orange" | "yellow" | "green" | "teal" | "blue" | "purple" | "pink";

export interface KeybindingPreferences {
  newTab: string;
  closeTab: string;
  nextTab: string;
  previousTab: string;
  splitVertical: string;
  splitHorizontal: string;
  closePane: string;
  find: string;
  commandPalette: string;
  settings: string;
  zoomIn: string;
  zoomOut: string;
  zoomReset: string;
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
  workingDirectory: string;
  restoreSession: boolean;
  copyOnSelect: boolean;
  confirmMultilinePaste: boolean;
  confirmCloseMultipleTabs: boolean;
  scrollback: number;
  gpuAcceleration: boolean;
  keybindings: KeybindingPreferences;
}

export interface TerminalPaneModel {
  id: string;
  profile: TerminalProfile;
}

export interface TerminalTab {
  id: string;
  /** The shell's latest title, or the profile name. */
  title: string;
  /** A name the user gave the tab. It wins over the shell's title until cleared. */
  customTitle?: string;
  color?: TabColor;
  /** Pane models in the order they were opened, which is also the order they render in. */
  panes: TerminalPaneModel[];
  layout: LayoutNode;
  activePaneId: string;
}

export interface PersistedPane {
  profileId: string;
}

export type PersistedLayout =
  | { type: "pane"; profileId: string }
  | { type: "split"; direction: SplitDirection; sizes: number[]; children: PersistedLayout[] };

export interface PersistedTab {
  title: string;
  customTitle?: string;
  color?: TabColor;
  /** Index of the active pane in reading order. */
  activePaneIndex: number;
  /** Version 3. */
  layout?: PersistedLayout;
  /** Versions 1 and 2 kept one row or column of panes. */
  panes?: PersistedPane[];
  splitDirection?: SplitDirection;
  paneSizes?: number[];
}

export interface SessionSnapshot {
  version: 1 | 2 | 3;
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

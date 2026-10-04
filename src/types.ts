import type { LayoutNode } from "./layout";

export type ProfileKind = "nebula" | "pwsh" | "powershell" | "cmd" | "gitbash" | "wsl" | "ssh" | "custom";

/**
 * Something a tab can open. Ids are `nebula`, `pwsh`… for the built-in shells,
 * `wsl:<distribution>`, `ssh:<host>` and `custom:<uuid>` for the others.
 */
export interface TerminalProfile {
  id: string;
  name: string;
  kind: ProfileKind;
  available: boolean;
  executable?: string | null;
  /** Program and arguments, for display only. */
  commandLine?: string | null;
  accent: string;
}

/** A profile added in Settings, as stored by the desktop host. */
export interface CustomProfile {
  id: string;
  name: string;
  executable: string;
  args: string[];
  /** `args` as one command line, for editing. */
  arguments: string;
  cwd?: string | null;
  accent: string;
}

/** What the profile editor sends to be saved. Without an id, a new profile is added. */
export interface CustomProfileDraft {
  id?: string;
  name: string;
  executable: string;
  arguments: string;
  cwd: string;
  accent: string;
}

export type CustomProfileField = "name" | "executable" | "arguments" | "cwd" | "accent";

export interface ProfileFieldError {
  field: CustomProfileField | null;
  message: string;
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
  previousCommand: string;
  nextCommand: string;
}

/** Where new shells start when they have no folder of their own. */
export type StartingFolder = "home" | "desktop" | "documents" | "custom";

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
  startingFolder: StartingFolder;
  /** The folder typed in Settings, used when `startingFolder` is "custom". */
  workingDirectory: string;
  /** New tabs and splits open where the focused pane's shell is. */
  openInCurrentFolder: boolean;
  /** PowerShell, Git Bash and WSL get a script that marks their commands. */
  shellIntegration: boolean;
  restoreSession: boolean;
  /** "Open in Nebula Terminal" in File Explorer. Null until the user decides: installed copies add it. */
  explorerMenu: boolean | null;
  /** The `nebula-terminal` command on the user's PATH. Null until the user decides, like `explorerMenu`. */
  pathCommand: boolean | null;
  copyOnSelect: boolean;
  /** Find options, kept from one search to the next. */
  searchCaseSensitive: boolean;
  searchWholeWord: boolean;
  searchRegex: boolean;
  confirmMultilinePaste: boolean;
  confirmCloseMultipleTabs: boolean;
  scrollback: number;
  gpuAcceleration: boolean;
  notifyLongCommands: boolean;
  longCommandSeconds: number;
  checkForUpdates: boolean;
  keybindings: KeybindingPreferences;
}

export interface TerminalPaneModel {
  id: string;
  profile: TerminalProfile;
  /** The last folder the shell reported, or the one the pane should start in. */
  cwd?: string;
  /** A console Windows handed over (Nebula as the default terminal). The pane shows it
   * instead of starting its profile, which only runs if the pane is restarted. */
  handoffId?: string;
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
  | { type: "pane"; profileId: string; cwd?: string }
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

/** What Windows offers to open the app in a folder (Rust's `sync_launchers`). */
export interface Launchers {
  explorerMenu: boolean;
  command: boolean;
}

/** A console Windows handed over, waiting for a tab (Rust's `take_handoffs`). */
export interface HandoffInfo {
  id: string;
  title: string;
}

/** Whether Nebula is the default terminal of Windows (Rust's `default_terminal`). */
export interface DefaultTerminal {
  supported: boolean;
  enabled: boolean;
}

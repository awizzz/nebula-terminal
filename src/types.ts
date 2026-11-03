import type { TerminalTheme } from "./themes";
import type { LayoutNode } from "./layout";

export type FileOpener = "auto" | "vscode" | "cursor" | "default" | "custom";

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
  /** Set when the profile is an SSH connection made in the editor. */
  ssh?: SshConnection | null;
}

export interface SshConnection {
  host: string;
  user?: string | null;
  port?: number | null;
  identityFile?: string | null;
  localForwards: string[];
  remoteForwards: string[];
}

/** The SSH fields as typed in the editor; forwards go one per line. */
export interface SshDraft {
  host: string;
  user: string;
  port: string;
  identityFile: string;
  localForwards: string;
  remoteForwards: string;
}

/** What the profile editor sends to be saved. Without an id, a new profile is added. */
export interface CustomProfileDraft {
  id?: string;
  name: string;
  executable: string;
  arguments: string;
  cwd: string;
  accent: string;
  /** Only `ssh` is saved for an SSH connection; switching back keeps the program fields. */
  kind: "program" | "ssh";
  ssh: SshDraft;
}

export type CustomProfileField = "name" | "executable" | "arguments" | "cwd" | "accent" | keyof SshDraft;

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
  newAdministratorTab: string;
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
  toggleBroadcast: string;
  previousCommand: string;
  togglePaneZoom: string;
  findEverywhere: string;
  switchTab: string;
  nextCommand: string;
}

/** Where new shells start when they have no folder of their own. */
export type StartingFolder = "home" | "desktop" | "documents" | "custom";

/** A command kept to run again from the palette. */
export interface SavedCommand {
  id: string;
  name: string;
  command: string;
}

/** A profile's own look; anything it leaves out is the app's. */
export interface ProfileLook {
  themeId?: string;
  fontFamily?: string;
  fontSize?: number;
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
  startingFolder: StartingFolder;
  /** The folder typed in Settings, used when `startingFolder` is "custom". */
  workingDirectory: string;
  /** New tabs and splits open where the focused pane's shell is. */
  openInCurrentFolder: boolean;
  /** PowerShell, Git Bash and WSL get a script that marks their commands. */
  shellIntegration: boolean;
  restoreSession: boolean;
  /** Restored panes show what they last printed. */
  restoreScreens: boolean;
  /** "Open in Nebula Terminal" in File Explorer. Null until the user decides: installed copies add it. */
  explorerMenu: boolean | null;
  /** The `nebula-terminal` command on the user's PATH. Null until the user decides, like `explorerMenu`. */
  pathCommand: boolean | null;
  /** Nebula in Windows Terminal's profiles. Null until the user decides: then it's off. */
  windowsTerminalProfile: boolean | null;
  copyOnSelect: boolean;
  /** What Ctrl+Click on a file path in the output opens it with. */
  fileOpener: FileOpener;
  /** For `fileOpener: "custom"`: a command with {file}, {line} and {column}. */
  fileOpenerCommand: string;
  /** Find options, kept from one search to the next. */
  searchCaseSensitive: boolean;
  searchWholeWord: boolean;
  searchRegex: boolean;
  /** Color schemes imported from Windows Terminal or iTerm2. */
  customThemes: TerminalTheme[];
  confirmMultilinePaste: boolean;
  confirmCloseMultipleTabs: boolean;
  scrollback: number;
  gpuAcceleration: boolean;
  notifyLongCommands: boolean;
  longCommandSeconds: number;
  checkForUpdates: boolean;
  /** A window that drops down from the top of the screen on a global shortcut. */
  quickTerminal: boolean;
  /** Physical keys, like "Win+Backquote" (see quickShortcut.ts). */
  quickTerminalShortcut: string;
  /** Share of the screen's height, from 0.25 to 0.9. */
  quickTerminalHeight: number;
  quickTerminalHideOnBlur: boolean;
  /** With the quick terminal on, the app stays in the notification area after its last window. */
  quickTerminalKeepRunning: boolean;
  /** Profiles with a theme or font of their own, by profile id. */
  profileLooks: Record<string, ProfileLook>;
  /** Wear `darkThemeId` or `lightThemeId`, whichever matches Windows' app mode. */
  followSystemTheme: boolean;
  /** The theme last picked for each scheme, which "Follow Windows" goes back to. */
  darkThemeId: string;
  lightThemeId: string;
  /** Admin windows get a red accent. */
  markAdministratorWindows: boolean;
  /** Sixel and iTerm2 images that programs print. */
  terminalImages: boolean;
  /** OSC 52: programs may put text on the clipboard (never read it). */
  clipboardWrite: boolean;
  /** Draw =>, != and the like as one symbol in fonts that have ligatures. */
  fontLigatures: boolean;
  savedCommands: SavedCommand[];
  /** xterm's accessibility tree, which Narrator and NVDA read. */
  screenReaderMode: boolean;
  /** With a Windows contrast theme on, terminals wear High Contrast Dark or Light. */
  followContrastThemes: boolean;
  /** A .json file the settings are synced through (sync.ts), or "" when they aren't. */
  syncFile: string;
  /** Pointing at a finished command shows its block and actions (shell integration). */
  commandBlocks: boolean;
  /** The workspace the main window opens at startup, instead of the last session; "" for none. */
  startupWorkspace: string;
  keybindings: KeybindingPreferences;
}

/** A pane on its way from another window: its screen, and its shell if it still runs. */
export interface MovedPane {
  sessionId?: string;
  screen: string;
}

export interface TerminalPaneModel {
  id: string;
  profile: TerminalProfile;
  /** Set while the pane arrives from another window; it then attaches to that shell. */
  moved?: MovedPane;
  /** The last folder the shell reported, or the one the pane should start in. */
  cwd?: string;
  /** A console Windows handed over (Nebula as the default terminal). The pane shows it
   * instead of starting its profile, which only runs if the pane is restarted. */
  handoffId?: string;
  /** The key of the screen it showed in the last session, read before the pane mounts. */
  restoreScreen?: string;
  /** That screen, written before the shell starts. */
  screen?: string;
}

export type TabWatch = "output" | "silence";

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
  /** What is typed in one pane goes to every pane of the tab. Not saved. */
  broadcast?: boolean;
  /** The pane that fills the tab for now, the others hidden. Not saved. */
  zoomedPaneId?: string;
  /** Tell the user when the tab prints something, or goes quiet. Not saved. */
  watch?: TabWatch;
}

export interface PersistedPane {
  profileId: string;
}

export type PersistedLayout =
  | { type: "pane"; profileId: string; cwd?: string; screen?: string }
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
  windowsTerminal: boolean;
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

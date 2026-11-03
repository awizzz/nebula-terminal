import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { Terminal, type IMarker } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { validPattern } from "./SearchBar";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { ClipboardAddon, type IClipboardProvider } from "@xterm/addon-clipboard";
import { SerializeAddon } from "@xterm/addon-serialize";
import { adjacentPrompt, commandOnPromptLine, joinLines } from "../commandMarks";
import { ClipboardCheck, Copy, FileText, Lock, RotateCcw, X } from "lucide-react";
import { openExternal } from "../external";
import { folderFromReport } from "../folders";
import { openPath, registerFileLinks } from "../fileLinks";
import { registerPane, type FoundLine } from "../paneRegistry";
import { windowsBuild } from "../platform";
import { isPaste, isTerminalReport, pasteLineCount, pastePreview as previewText, quoteDroppedPath, trimSingleLinePaste } from "../terminalInput";
import { previewSession } from "../preview-session";
import { startLog, type PaneLog } from "../paneLog";
import { commandInput } from "../savedCommands";
import { commandMarkdown, formatElapsed } from "../notify";
import { startHighlights } from "../highlights";
import { startMarkMode, type MarkMode } from "../markMode";
import { folderParts } from "../recentFolders";
import { drawsText } from "../splash";
import { parseProgress, type Progress } from "../progress";
import { resolveTheme, xtermTheme } from "../themes";
import type { AppearancePreferences, Highlight, MovedPane, PtyEvent, TerminalProfile } from "../types";

interface TerminalPaneProps {
  paneId: string;
  profile: TerminalProfile;
  /** Where the shell starts when it has no folder of its own (its last one, or the pane it was split from). */
  startIn?: string;
  /** A console Windows handed over. The pane shows it first; a restart runs the profile. */
  handoffId?: string;
  /** The screen and shell of a pane that comes from another window. */
  moved?: MovedPane;
  /** What the pane showed in the last session, written before its shell starts. */
  restoredScreen?: string;
  /** Typed into the shell when it starts (and restarts), from a workspace. */
  startCommand?: string;
  /** Nothing typed, pasted or broadcast reaches the shell. */
  readOnly?: boolean;
  /** Points added to the font size for this pane alone. */
  fontDelta?: number;
  preferences: AppearancePreferences;
  focused: boolean;
  visible: boolean;
  searchRequest?: { query: string; nonce: number; backwards?: boolean; caseSensitive?: boolean; wholeWord?: boolean; regex?: boolean };
  onFocus: () => void;
  onFontSizeDelta: (delta: number) => void;
  /** Ctrl+Shift+Wheel: this pane's text alone. */
  onPaneZoom?: (delta: number) => void;
  onTitleChange: (title: string) => void;
  onActivity: () => void;
  /** Every bit of output from the shell, seen or not. */
  onOutput?: () => void;
  onSearchResult: (result: { index: number; count: number }) => void;
  /** Input the user typed or pasted here, for panes that type together. */
  onInput?: (data: string) => void;
  onContextMenu: (x: number, y: number) => void;
  onCommandFinished: (finished: FinishedCommand) => void;
  onNotify: (title: string, body: string) => void;
  onFolderChange: (folder: string) => void;
  /** A short message for the window, like a file that couldn't be opened. */
  onNotice?: (text: string) => void;
  /** Called once, when the shell first draws something or fails to start. */
  onReady?: () => void;
  /** OSC 9;4 progress from a program, or null when it's done. */
  onProgress?: (progress: Progress | null) => void;
  /** The file this pane started logging to, or null once it stopped. */
  onLog?: (path: string | null) => void;
  /** A highlight that asks for a notification showed up in this line. */
  onHighlight?: (highlight: Highlight, line: string) => void;
  onClose: () => void;
}

/** A command that ran between shell-integration marks (OSC 133;C and 133;D). */
export interface FinishedCommand {
  command: string;
  code: number | null;
  seconds: number;
}

type ConnectionState = "starting" | "ready" | "closed" | "preview";

/** Waits before each new try at a dropped SSH connection; after the last, it stays closed. */
const RECONNECT_DELAYS = [2, 5, 10, 20, 30];
/** A connection that held this long starts the tries over when it drops. */
const RECONNECTED_AFTER_MS = 60_000;

/** One command as the shell marked it: its prompt, where its output starts, and where it ended. */
interface MarkedCommand {
  prompt?: IMarker;
  output?: IMarker;
  end?: IMarker;
  /** The output stopped mid-line, so the end mark's line is part of it. */
  endsMidLine?: boolean;
  startedAt?: number;
  /** How long it ran and how it ended, once it has. */
  seconds?: number;
  code?: number | null;
}

/** Commands kept per pane; older ones are forgotten (their lines usually left the scrollback too). */
const MAX_MARKED_COMMANDS = 500;

/** Falls back to the bundled Nerd Font icons for glyphs the user's font lacks. */
function withSymbols(fontFamily: string): string {
  return `${fontFamily}, "Nebula Symbols"`;
}

/** Ctrl+<letter> on any keyboard layout: Cyrillic or Greek layouts report another `key`. */
function isControlLetter(event: KeyboardEvent, letter: "c" | "v" | "z"): boolean {
  if (!event.ctrlKey || event.altKey) return false;
  const key = event.key.toLowerCase();
  if (key === letter) return true;
  return !/^[a-z]$/.test(key) && event.code === `Key${letter.toUpperCase()}`;
}

/** Nebula's prompt as the real interpreter draws it (see crates/nebula-sh/src/prompt.rs). */
const NEBULA_PROMPT = "\x1b[1;34m~/projects/app\x1b[0m \x1b[2mon\x1b[0m \x1b[1;35m\ue725 main\x1b[0m \x1b[33m!1\x1b[0m \x1b[2m?1\x1b[0m\r\n\x1b[1;32m❯\x1b[0m ";
const POWERSHELL_PROMPT = "\x1b[34mPS\x1b[0m C:\\Users\\you> ";

function previewPrompt(profile: TerminalProfile): string {
  return profile.kind === "nebula" ? NEBULA_PROMPT : POWERSHELL_PROMPT;
}

/** Colors a command line the way Nebula's highlighter does. */
function highlightCommand(line: string): string {
  return line.split(" ").map((word, index) => {
    if (index === 0) return `\x1b[1;32m${word}\x1b[0m`;
    if (word.startsWith("-")) return `\x1b[36m${word}\x1b[0m`;
    return word;
  }).join(" ");
}

/**
 * Browser-only stand-in for a shell: replays real Nebula output recorded by
 * scripts/record-preview.py. `#screenshot` drops the notice for documentation images.
 */
/** OSC 52: programs may put text on the clipboard, never read what's there. */
function writeOnlyClipboard(allowed: () => boolean): IClipboardProvider {
  return {
    readText: () => "",
    writeText: async (_selection, text) => {
      if (!allowed() || text.length > 1_000_000) return;
      await navigator.clipboard?.writeText(text).catch(() => undefined);
    },
  };
}

function writePreview(terminal: Terminal, profile: TerminalProfile) {
  if (location.hash !== "#screenshot") {
    terminal.writeln(`\x1b[2m${profile.name} · browser preview. Shells only run inside the desktop app.\x1b[0m`);
    terminal.writeln("");
  }
  if (profile.kind === "nebula") {
    // Marked like the real shell, so command jumps work in the preview too.
    for (const { command, output } of previewSession) {
      terminal.write(`\x1b]133;A\x07${NEBULA_PROMPT}${highlightCommand(command)}\r\n\x1b]133;C\x07`);
      terminal.write(output.replace(/\n/g, "\r\n"));
      terminal.write("\x1b]133;D;0\x07\r\n");
    }
    terminal.write("\x1b]133;A\x07");
  }
  terminal.write(previewPrompt(profile));
}

export default function TerminalPane({
  paneId,
  profile,
  startIn,
  handoffId,
  moved,
  restoredScreen,
  startCommand,
  readOnly = false,
  fontDelta = 0,
  preferences,
  focused,
  visible,
  searchRequest,
  onFocus,
  onFontSizeDelta,
  onPaneZoom,
  onTitleChange,
  onActivity,
  onOutput,
  onSearchResult,
  onInput,
  onContextMenu,
  onCommandFinished,
  onNotify,
  onFolderChange,
  onNotice,
  onReady,
  onProgress,
  onLog,
  onHighlight,
  onClose,
}: TerminalPaneProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const searchRef = useRef<SearchAddon | null>(null);
  const sessionRef = useRef<string | null>(null);
  const writeRef = useRef<(data: string) => void>(() => undefined);
  const focusedRef = useRef(focused);
  const visibleRef = useRef(visible);
  const preferencesRef = useRef(preferences);
  const startInRef = useRef(startIn);
  const handoffRef = useRef(handoffId);
  const movedRef = useRef(moved);
  const restoredRef = useRef(restoredScreen);
  const startCommandRef = useRef(startCommand);
  startCommandRef.current = startCommand;
  const readOnlyRef = useRef(readOnly);
  readOnlyRef.current = readOnly;
  const fontSize = Math.min(40, Math.max(6, preferences.fontSize + fontDelta));
  const fontSizeRef = useRef(fontSize);
  fontSizeRef.current = fontSize;
  const callbacksRef = useRef({ onFontSizeDelta, onPaneZoom, onTitleChange, onActivity, onOutput, onSearchResult, onContextMenu, onCommandFinished, onNotify, onFolderChange, onInput, onNotice, onReady, onProgress, onLog, onHighlight });
  /** The shell's folder, from its OSC 7 reports; relative paths in its output start there. */
  const folderRef = useRef(startIn);
  const [connectionState, setConnectionState] = useState<ConnectionState>("starting");
  const [exitCode, setExitCode] = useState<number | null>(null);
  /** Seconds before a dropped SSH connection starts again, while it counts down. */
  const [reconnectIn, setReconnectIn] = useState<number | null>(null);
  const reconnectAttemptsRef = useRef(0);
  /** The pane's screen as it is, written again above the shell when it restarts. */
  const screenRef = useRef<() => string>(() => "");
  const [pendingPaste, setPendingPaste] = useState<string | null>(null);
  const [restartNonce, setRestartNonce] = useState(0);
  // The command block under the pointer, in pixels from the top of the pane.
  const [block, setBlock] = useState<{ top: number; height: number; seconds?: number; code?: number | null } | null>(null);
  /** The file this pane logs to, while it does. */
  const [logFile, setLogFile] = useState<{ name: string; path: string } | null>(null);
  const [marking, setMarking] = useState(false);
  const stopLogRef = useRef<() => void>(() => undefined);
  const blockActionsRef = useRef<{ command: () => string; output: () => string; markdown: () => string; rerun: () => void }>({ command: () => "", output: () => "", markdown: () => "", rerun: () => undefined });
  const sectionRef = useRef<HTMLElement | null>(null);

  focusedRef.current = focused;
  visibleRef.current = visible;
  preferencesRef.current = preferences;
  startInRef.current = startIn;
  callbacksRef.current = { onFontSizeDelta, onPaneZoom, onTitleChange, onActivity, onOutput, onSearchResult, onContextMenu, onCommandFinished, onNotify, onFolderChange, onInput, onNotice, onReady, onProgress, onLog, onHighlight };

  const theme = useMemo(() => resolveTheme(preferences.themeId, preferences.customThemes), [preferences.customThemes, preferences.themeId]);
  const translucent = preferences.terminalOpacity < 1;
  // A translucent terminal lets the workspace paint the background; an opaque one keeps
  // xterm's own background so glyphs get subpixel antialiasing.
  const colors = useMemo(() => {
    const base = xtermTheme(theme, preferences.accent);
    return translucent ? { ...base, background: "#00000000" } : base;
  }, [preferences.accent, theme, translucent]);
  const colorsRef = useRef(colors);
  colorsRef.current = colors;

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    let disposed = false;

    const terminal = new Terminal({
      allowProposedApi: true,
      allowTransparency: preferencesRef.current.terminalOpacity < 1,
      screenReaderMode: preferencesRef.current.screenReaderMode,
      cursorBlink: preferencesRef.current.cursorBlink,
      cursorStyle: preferencesRef.current.cursorStyle,
      cursorInactiveStyle: "outline",
      fontFamily: withSymbols(preferencesRef.current.fontFamily),
      fontSize: fontSizeRef.current,
      lineHeight: preferencesRef.current.lineHeight,
      wordSeparator: preferencesRef.current.wordSeparators,
      scrollback: preferencesRef.current.scrollback,
      smoothScrollDuration: preferencesRef.current.animationLevel === "full" ? 80 : 0,
      // Failed commands and search results are marked along the scroll bar.
      overviewRuler: { width: 8 },
      drawBoldTextInBrightColors: false,
      minimumContrastRatio: 1,
      theme: colors,
      // OSC 8 hyperlinks follow the same Ctrl+click rule as detected links.
      linkHandler: {
        activate: (event, uri) => {
          if (event.ctrlKey || !isTauri()) openExternal(uri);
        },
      },
    });
    // ConPTY reflows wrapped lines itself; xterm only stays out of its way when it
    // knows the Windows build.
    void windowsBuild().then((buildNumber) => {
      if (!disposed && buildNumber) terminal.options.windowsPty = { backend: "conpty", buildNumber };
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    const serializer = new SerializeAddon();
    terminal.loadAddon(fit);
    terminal.loadAddon(search);
    terminal.loadAddon(serializer);
    // Emoji and symbols measured as today's programs measure them.
    terminal.loadAddon(new Unicode11Addon());
    terminal.unicode.activeVersion = "11";
    terminal.loadAddon(new ClipboardAddon(undefined, writeOnlyClipboard(() => preferencesRef.current.clipboardWrite)));
    // Links open with Ctrl+click so a plain click can still start a selection.
    terminal.loadAddon(new WebLinksAddon((event, uri) => {
      if (event.ctrlKey || !isTauri()) openExternal(uri);
    }));
    // File paths that exist, the same way: Ctrl+click opens them at their line.
    const fileLinks = registerFileLinks(terminal, () => folderRef.current, (path, candidate) => {
      openPath(path, candidate.line, candidate.column, preferencesRef.current)
        .catch((error) => callbacksRef.current.onNotice?.(error instanceof Error ? error.message : String(error)));
    });
    terminal.open(host);
    // A restart or profile change replaces the focused textarea; take focus back.
    if (focusedRef.current) terminal.focus();

    terminalRef.current = terminal;
    fitRef.current = fit;
    searchRef.current = search;
    setConnectionState("starting");
    setExitCode(null);

    const resize = () => {
      if (disposed || host.clientWidth === 0 || host.clientHeight === 0) return;
      try {
        fit.fit();
      } catch {
        // The pane can disappear between a layout event and the next frame.
      }
    };

    const observer = new ResizeObserver(resize);
    observer.observe(host);
    const resizeDisposable = terminal.onResize(({ cols, rows }) => {
      const sessionId = sessionRef.current;
      if (isTauri() && sessionId) void invoke("resize_session", { sessionId, cols, rows }).catch(() => undefined);
    });
    let lastTitle = "";
    const titleDisposable = terminal.onTitleChange((title) => {
      const clean = title.trim().slice(0, 120);
      if (!clean) return;
      lastTitle = clean;
      callbacksRef.current.onTitleChange(clean);
    });

    // Shell integration (OSC 133): A starts a prompt, C the command's output, D;code its end.
    let running: { command: string; startedAt: number } | null = null;
    const commands: MarkedCommand[] = [];
    const remember = (command: MarkedCommand) => {
      commands.push(command);
      for (const old of commands.splice(0, Math.max(0, commands.length - MAX_MARKED_COMMANDS))) {
        old.prompt?.dispose();
        old.output?.dispose();
        old.end?.dispose();
      }
    };
    const integration = terminal.parser.registerOscHandler(133, (data) => {
      const [mark, code] = data.split(";");
      const cursorMarker = () => terminal.registerMarker(0);
      if (mark === "A") {
        remember({ prompt: cursorMarker() });
        // Back at the prompt, whatever reported progress is over, even if it never said so.
        if (progress) reportProgress(null);
      } else if (mark === "C") {
        running = { command: lastTitle, startedAt: Date.now() };
        const current = commands.at(-1);
        if (current && !current.output && !current.end) Object.assign(current, { output: cursorMarker(), startedAt: running.startedAt });
        else remember({ output: cursorMarker(), startedAt: running.startedAt });
      } else if (mark === "D") {
        const parsed = code === undefined ? Number.NaN : Number.parseInt(code, 10);
        const current = commands.at(-1);
        if (current?.output && !current.end) {
          current.end = cursorMarker();
          current.endsMidLine = terminal.buffer.active.cursorX > 0;
          current.code = Number.isFinite(parsed) ? parsed : null;
          if (current.startedAt) current.seconds = (Date.now() - current.startedAt) / 1000;
          const anchor = current.prompt ?? current.output;
          if (Number.isFinite(parsed) && parsed !== 0 && !anchor.isDisposed) {
            terminal.registerDecoration({ marker: anchor, overviewRulerOptions: { color: colorsRef.current.red ?? "#ee6f78", position: "full" } });
          }
        }
        if (running) {
          callbacksRef.current.onCommandFinished({
            command: running.command,
            code: Number.isFinite(parsed) ? parsed : null,
            seconds: (Date.now() - running.startedAt) / 1000,
          });
          running = null;
        }
      }
      return true;
    });
    const liveCommands = () => commands.filter((command) => !(command.prompt ?? command.output)?.isDisposed);
    const outputLines = (command: MarkedCommand): [number, number] | null => {
      if (!command.output || !command.end || command.output.isDisposed || command.end.isDisposed) return null;
      const last = command.endsMidLine ? command.end.line : command.end.line - 1;
      return last >= command.output.line ? [command.output.line, last] : null;
    };
    const jumpToCommand = (direction: -1 | 1) => {
      const buffer = terminal.buffer.active;
      if (buffer.type !== "normal") return false;
      const prompts = liveCommands().map((command) => (command.prompt ?? command.output)!.line);
      if (prompts.length === 0) return false;
      const target = adjacentPrompt(prompts, buffer.viewportY, direction);
      if (target !== undefined) terminal.scrollToLine(target);
      return true;
    };
    const lastOutputLines = () => liveCommands().reverse().map(outputLines).find((range) => range !== null);
    /** Buffer lines as text, wrapped lines joined back into one. */
    const textOf = (first: number, last: number) => {
      const buffer = terminal.buffer.active;
      return joinLines(Array.from({ length: last - first + 1 }, (_, index) => {
        const line = buffer.getLine(first + index);
        return { text: line?.translateToString(true) ?? "", wrapped: line?.isWrapped ?? false };
      }));
    };
    const lastOutputText = () => {
      const lines = lastOutputLines();
      return lines ? textOf(lines[0], lines[1]) : undefined;
    };
    /** Everything in the pane, scrollback included, without the empty lines below the cursor. */
    const allText = () => textOf(0, terminal.buffer.active.length - 1).replace(/\s+$/, "\n");
    // Command blocks: pointing at a finished command shows it as one block, with what
    // can be done with it. Listened to on the pane, so the actions stay reachable.
    let shownBlock: MarkedCommand | undefined;
    const blockStart = (command: MarkedCommand) => (command.prompt ?? command.output)!.line;
    const blockOf = (row: number) => liveCommands().reverse().find((command) => {
      const range = outputLines(command);
      return range !== null && row >= blockStart(command) && row <= range[1];
    });
    const placeBlock = () => {
      const range = shownBlock && outputLines(shownBlock);
      const screen = host.querySelector<HTMLElement>(".xterm-screen");
      const section = sectionRef.current;
      if (!shownBlock || !range || !screen || !section || !preferencesRef.current.commandBlocks) {
        setBlock(null);
        return;
      }
      const cell = screen.clientHeight / terminal.rows;
      const viewport = terminal.buffer.active.viewportY;
      const top = Math.max(0, (blockStart(shownBlock) - viewport) * cell);
      const bottom = Math.min(screen.clientHeight, (range[1] + 1 - viewport) * cell);
      if (bottom <= top) {
        setBlock(null);
        return;
      }
      const offset = screen.getBoundingClientRect().top - section.getBoundingClientRect().top;
      setBlock({ top: offset + top, height: bottom - top, seconds: shownBlock.seconds, code: shownBlock.code });
    };
    const pointAt = (event: MouseEvent) => {
      // Not while selecting text, nor without shell integration's marks.
      if (event.buttons || !preferencesRef.current.commandBlocks) return;
      const screen = host.querySelector<HTMLElement>(".xterm-screen");
      if (!screen) return;
      const rect = screen.getBoundingClientRect();
      const row = Math.floor((event.clientY - rect.top) / (rect.height / terminal.rows)) + terminal.buffer.active.viewportY;
      const command = event.clientY >= rect.top && event.clientY <= rect.bottom ? blockOf(row) : shownBlock;
      if (command === shownBlock) return;
      shownBlock = command;
      placeBlock();
    };
    const leaveBlock = () => {
      shownBlock = undefined;
      setBlock(null);
    };
    const section = sectionRef.current;
    section?.addEventListener("mousemove", pointAt);
    section?.addEventListener("mouseleave", leaveBlock);
    const blockScroll = terminal.onScroll(placeBlock);
    const blockCommand = () => {
      if (!shownBlock?.output) return "";
      const lines = textOf(blockStart(shownBlock), Math.max(blockStart(shownBlock), shownBlock.output.line - 1)).split("\n");
      return commandOnPromptLine(lines.at(-1) ?? "");
    };
    const blockOutput = () => {
      const range = shownBlock && outputLines(shownBlock);
      return range ? textOf(range[0], range[1]) : "";
    };
    blockActionsRef.current = {
      command: blockCommand,
      output: blockOutput,
      markdown: () => commandMarkdown(blockCommand(), blockOutput()),
      rerun: () => {
        const command = blockCommand();
        if (command && !running) writeRef.current(`${command}\r`);
        terminal.focus();
      },
    };

    const copyLastOutput = () => {
      const text = lastOutputText();
      if (text === undefined) return false;
      void navigator.clipboard?.writeText(text).catch(() => undefined);
      return true;
    };
    // A click on a prompt or on the command typed after it selects that command's output,
    // unless the click only clears a selection.
    let hadSelection = false;
    const noteSelection = () => { hadSelection = terminal.hasSelection(); };
    const selectOutputAt = (event: MouseEvent) => {
      if (event.button !== 0 || event.detail !== 1 || event.ctrlKey || event.shiftKey || event.altKey) return;
      if (hadSelection || terminal.hasSelection() || terminal.modes.mouseTrackingMode !== "none" || terminal.buffer.active.type !== "normal") return;
      const screen = host.querySelector<HTMLElement>(".xterm-screen")?.getBoundingClientRect();
      if (!screen || screen.height === 0) return;
      const row = Math.floor((event.clientY - screen.top) / (screen.height / terminal.rows));
      const line = terminal.buffer.active.viewportY + row;
      const command = liveCommands().find(({ prompt, output }) => prompt && !prompt.isDisposed
        && (line === prompt.line || (output !== undefined && !output.isDisposed && line > prompt.line && line < output.line)));
      const lines = command && outputLines(command);
      if (lines) terminal.selectLines(lines[0], lines[1]);
    };
    host.addEventListener("mousedown", noteSelection, true);
    host.addEventListener("click", selectOutputAt);
    // Programs can ask for a notification: OSC 9;text (iTerm2) and OSC 777;notify;title;body.
    let progress: Progress | undefined;
    const reportProgress = (report: Progress | null) => {
      progress = report ?? undefined;
      callbacksRef.current.onProgress?.(report);
    };
    const notification = terminal.parser.registerOscHandler(9, (data) => {
      // OSC 9;4;… is a progress report (ConEmu, Windows Terminal), not a message.
      const report = parseProgress(data, progress);
      if (report !== undefined) {
        reportProgress(report);
        return true;
      }
      if (/^\d;/.test(data)) return false;
      callbacksRef.current.onNotify(profile.name, data.slice(0, 300));
      return true;
    });
    const titledNotification = terminal.parser.registerOscHandler(777, (data) => {
      const [kind, title = "", ...body] = data.split(";");
      if (kind !== "notify") return false;
      callbacksRef.current.onNotify(title.slice(0, 120) || profile.name, body.join(";").slice(0, 300));
      return true;
    });
    // OSC 7: the shell's current folder, where splits and the next launch start.
    const folderReport = terminal.parser.registerOscHandler(7, (data) => {
      const folder = folderFromReport(data);
      if (folder) {
        folderRef.current = folder;
        callbacksRef.current.onFolderChange(folder);
      }
      return true;
    });
    const searchDisposable = search.onDidChangeResults(({ resultIndex, resultCount }) => {
      callbacksRef.current.onSearchResult({ index: resultIndex, count: resultCount });
    });

    // Input typed while the shell starts waits here instead of being lost.
    let queued: string[] = [];
    let exited = false;
    let startedAt = Date.now();
    // Set when the shell moved to another window: it must outlive this pane.
    let keepSession = false;
    let reattach: (() => void) | undefined;
    const send = (sessionId: string, data: string) => {
      void invoke("write_session", { sessionId, data }).catch((error) => {
        if (!disposed) terminal.writeln(`\r\n\x1b[31m${String(error)}\x1b[0m`);
      });
    };
    // A read-only pane still answers the program's own questions (cursor position and the like).
    const locked = (data: string) => readOnlyRef.current && !isTerminalReport(data);
    let toldLocked = 0;
    const writeRaw = (data: string) => {
      if (locked(data)) return;
      const sessionId = sessionRef.current;
      if (isTauri()) {
        if (sessionId) send(sessionId, data);
        else if (!exited) queued.push(data);
        return;
      }
      if (data === "\r") terminal.write(`\r\n${previewPrompt(profile)}`);
      else if (data === "\u007f") terminal.write("\b \b");
      else if (!data.startsWith("\x1b") && data >= " ") terminal.write(data.replace(/\r?\n/g, `\r\n${previewPrompt(profile)}`));
    };

    writeRef.current = writeRaw;

    const typed = (data: string) => {
      if (locked(data)) {
        if (Date.now() - toldLocked > 3000) callbacksRef.current.onNotice?.("This pane is read-only: unlock it from its right-click menu");
        toldLocked = Date.now();
        return;
      }
      writeRaw(data);
      callbacksRef.current.onInput?.(data);
    };
    const writeInput = (data: string) => {
      if (isTerminalReport(data)) {
        writeRaw(data);
        return;
      }
      if (!isPaste(data)) {
        typed(data);
        return;
      }
      if (pasteLineCount(data) > 1) {
        if (preferencesRef.current.confirmMultilinePaste) setPendingPaste(data);
        else typed(data);
        return;
      }
      typed(trimSingleLinePaste(data));
    };

    const paste = () => {
      void navigator.clipboard?.readText().then((text) => {
        if (text) terminal.paste(text);
      }).catch(() => undefined);
    };
    const copy = () => {
      if (!terminal.hasSelection()) return;
      void navigator.clipboard?.writeText(terminal.getSelection()).catch(() => undefined);
    };

    let drawn = false;
    const ready = () => {
      if (drawn) return;
      drawn = true;
      callbacksRef.current.onReady?.();
    };

    // The last session's screen, once: a restart starts blank. Development runs this effect
    // twice, so it's let go only once the pane stays.
    const takeRestored = () => {
      const restored = restoredRef.current;
      queueMicrotask(() => { if (!disposed) restoredRef.current = undefined; });
      return restored;
    };

    if (isTauri()) {
      const channel = new Channel<PtyEvent>();
      channel.onmessage = (message) => {
        if (disposed) return;
        if (message.event === "output" && message.data.chunk) {
          // Ready once xterm has taken the text in; it is drawn on the next frame.
          if (!drawn && drawsText(message.data.chunk)) terminal.write(message.data.chunk, ready);
          else terminal.write(message.data.chunk);
          if (!visibleRef.current) callbacksRef.current.onActivity();
          callbacksRef.current.onOutput?.();
        } else if (message.event === "exit") {
          ready();
          if (progress) reportProgress(null);
          setExitCode(message.data.code ?? null);
          // ssh says 255 when the connection fails or drops; anything else came from the server.
          if (message.data.code === 255 && profile.kind === "ssh" && preferencesRef.current.reconnectSsh) {
            if (Date.now() - startedAt > RECONNECTED_AFTER_MS) reconnectAttemptsRef.current = 0;
            const delay = RECONNECT_DELAYS[reconnectAttemptsRef.current++];
            if (delay !== undefined) setReconnectIn(delay);
          }
          setConnectionState("closed");
          exited = true;
          queued = [];
          sessionRef.current = null;
        } else if (message.event === "error" && message.data.message) {
          terminal.writeln(`\r\n\x1b[31m${message.data.message}\x1b[0m`, ready);
        }
      };

      resize();
      reattach = () => {
        const sessionId = sessionRef.current;
        if (!sessionId) return;
        keepSession = false;
        void invoke("attach_session", { sessionId, cols: terminal.cols, rows: terminal.rows, onEvent: channel }).catch(() => undefined);
      };
      const { startingFolder, workingDirectory, shellIntegration } = preferencesRef.current;
      const handoff = handoffRef.current;
      // A handed-over console, or a shell from another window, can only be attached once.
      handoffRef.current = undefined;
      const arriving = movedRef.current;
      movedRef.current = undefined;
      const restored = takeRestored();
      // The shell then starts under it: ConPTY takes the cursor where it is.
      if (arriving) terminal.write(arriving.screen);
      else if (restored) terminal.write(restored);
      const started = arriving
        ? arriving.sessionId
          ? invoke("attach_session", { sessionId: arriving.sessionId, cols: terminal.cols, rows: terminal.rows, onEvent: channel }).then(() => arriving.sessionId!)
          : Promise.reject(new Error("its shell had already exited"))
        : handoff
        ? invoke<string>("attach_handoff", { handoffId: handoff, cols: terminal.cols, rows: terminal.rows, onEvent: channel })
        : invoke<string>("start_session", {
          profileId: profile.id,
          cols: terminal.cols,
          rows: terminal.rows,
          cwd: startingFolder === "custom" ? workingDirectory.trim() || null : null,
          knownFolder: startingFolder === "desktop" || startingFolder === "documents" ? startingFolder : null,
          startIn: startInRef.current ?? null,
          shellIntegration,
          onEvent: channel,
        });
      void started.then((sessionId) => {
        startedAt = Date.now();
        if (disposed) {
          void invoke("close_session", { sessionId }).catch(() => undefined);
          return;
        }
        // The shell may already have exited (a missing WSL distribution, for example).
        if (exited) return;
        sessionRef.current = sessionId;
        setConnectionState((state) => state === "closed" ? state : "ready");
        void invoke("resize_session", { sessionId, cols: terminal.cols, rows: terminal.rows }).catch(() => undefined);
        // A workspace's command goes first, as if typed while the shell starts.
        if (startCommandRef.current && !arriving && !handoff) send(sessionId, commandInput(startCommandRef.current));
        for (const data of queued) send(sessionId, data);
        queued = [];
      }).catch((error) => {
        if (disposed) return;
        if (arriving && !arriving.sessionId) {
          terminal.write("", ready);
        } else {
          terminal.writeln(`\x1b[31mCould not ${handoff ? "open this console" : arriving ? "move this shell" : `start ${profile.name}`}: ${String(error)}\x1b[0m`, ready);
        }
        setConnectionState("closed");
      });
    } else {
      setConnectionState("preview");
      const restored = takeRestored();
      if (restored) terminal.write(restored);
      writePreview(terminal, profile);
      for (const line of startCommandRef.current?.split(/\r?\n/) ?? []) {
        writeRaw(line);
        writeRaw("\r");
      }
      terminal.write("", ready);
    }

    // terminal.paste() routes through onData with bracketed-paste markers when the shell asks for them.
    const inputDisposable = terminal.onData(writeInput);
    const selectionDisposable = terminal.onSelectionChange(() => {
      if (preferencesRef.current.copyOnSelect) copy();
    });

    // Mark mode has the keyboard to itself: nothing reaches the shell meanwhile.
    let mark: MarkMode | undefined;
    const startMarking = () => {
      if (mark) return;
      mark = startMarkMode(terminal, copy, () => {
        mark = undefined;
        if (!disposed) setMarking(false);
      });
      setMarking(true);
      terminal.focus();
    };
    const leaveMark = () => mark?.end();
    host.addEventListener("mousedown", leaveMark, true);
    const blurDisposable = terminal.textarea ? (() => {
      const textarea = terminal.textarea!;
      textarea.addEventListener("blur", leaveMark);
      return { dispose: () => textarea.removeEventListener("blur", leaveMark) };
    })() : undefined;

    terminal.attachCustomKeyEventHandler((event) => {
      if (mark) {
        if (event.type === "keydown") mark.key(event);
        return false;
      }
      if (event.type !== "keydown") return true;
      // Windows conventions: Ctrl+C copies when text is selected and interrupts otherwise; Ctrl+V pastes.
      if (isControlLetter(event, "c") && terminal.hasSelection()) {
        copy();
        if (!event.shiftKey) terminal.clearSelection();
        return false;
      }
      if (isControlLetter(event, "v")) {
        event.preventDefault();
        paste();
        return false;
      }
      if (event.shiftKey && !event.ctrlKey && event.key === "Insert") {
        event.preventDefault();
        paste();
        return false;
      }
      // Ctrl+Z while Nebula runs a command pauses it, as in bash. The program has the
      // keyboard then, so the shell hears it through the host instead of the key.
      const sessionId = sessionRef.current;
      if (isControlLetter(event, "z") && !event.shiftKey && running && sessionId && isTauri() && profile.id === "nebula" && !readOnlyRef.current) {
        event.preventDefault();
        void invoke<boolean>("suspend_foreground", { sessionId })
          .then((paused) => { if (!paused) writeRef.current("\x1a"); })
          .catch(() => writeRef.current("\x1a"));
        return false;
      }
      return true;
    });

    // xterm's scroll area swallows wheel events whenever it can scroll, so Ctrl+Wheel
    // is caught on the way down, before it gets there.
    const handleWheel = (event: WheelEvent) => {
      if (!event.ctrlKey) return;
      event.preventDefault();
      event.stopPropagation();
      const delta = event.deltaY < 0 ? 1 : -1;
      if (event.shiftKey) callbacksRef.current.onPaneZoom?.(delta);
      else callbacksRef.current.onFontSizeDelta(delta);
    };
    host.addEventListener("wheel", handleWheel, { passive: false, capture: true });

    const handleContextMenu = (event: MouseEvent) => {
      event.preventDefault();
      callbacksRef.current.onContextMenu(event.clientX, event.clientY);
    };
    host.addEventListener("contextmenu", handleContextMenu);

    const insertDropped = (event: Event) => {
      if (!focusedRef.current) return;
      const paths = (event as CustomEvent<string[]>).detail;
      if (!Array.isArray(paths)) return;
      const text = paths.map((path) => quoteDroppedPath(path, profile)).join(" ");
      if (text) writeRaw(text);
    };
    window.addEventListener("nebula:insert-paths", insertDropped);

    const highlights = startHighlights(terminal, () => preferencesRef.current.highlights, (highlight, line) => callbacksRef.current.onHighlight?.(highlight, line));

    // Logging to a file: Rust keeps the file, this side sends it finished lines.
    let log: { id: number; writer: PaneLog } | undefined;
    const stopLogging = () => {
      if (!log) return;
      const { id, writer } = log;
      log = undefined;
      writer.stop();
      void invoke("stop_log", { id }).catch(() => undefined);
      if (!disposed) setLogFile(null);
      callbacksRef.current.onLog?.(null);
    };
    stopLogRef.current = stopLogging;
    const startLogging = async (name: string) => {
      if (log || !isTauri()) return;
      const started = await invoke<{ id: number; path: string } | null>("start_log", { name })
        .catch((error: unknown) => {
          callbacksRef.current.onNotice?.(String(error));
          return null;
        });
      if (!started) return;
      if (disposed || log) {
        void invoke("stop_log", { id: started.id }).catch(() => undefined);
        return;
      }
      log = {
        id: started.id,
        writer: startLog(terminal, (text) => {
          void invoke("append_log", { id: started.id, text }).catch((error: unknown) => {
            stopLogging();
            callbacksRef.current.onNotice?.(String(error));
          });
        }),
      };
      setLogFile({ name: folderParts(started.path).name, path: started.path });
      callbacksRef.current.onLog?.(started.path);
    };

    screenRef.current = () => serializer.serialize({ scrollback: 1_000, excludeAltBuffer: true, excludeModes: true });

    const unregister = registerPane(paneId, {
      markMode: startMarking,
      screen: () => screenRef.current(),
      startLog: (name: string) => void startLogging(name),
      stopLog: stopLogging,
      logging: () => log !== undefined,
      copy,
      paste,
      selectAll: () => terminal.selectAll(),
      clear: () => {
        log?.writer.flush();
        terminal.clear();
      },
      hasSelection: () => terminal.hasSelection(),
      focus: () => terminal.focus(),
      write: (data: string) => writeRef.current(data),
      jumpToCommand,
      copyLastOutput,
      hasCommandOutput: () => lastOutputLines() !== undefined,
      copyWithColors: () => {
        if (!terminal.hasSelection()) return false;
        const html = serializer.serializeAsHTML({ onlySelection: true, includeGlobalBackground: true });
        const plain = terminal.getSelection();
        const item = new ClipboardItem({ "text/html": new Blob([html], { type: "text/html" }), "text/plain": new Blob([plain], { type: "text/plain" }) });
        void navigator.clipboard?.write([item]).catch(() => navigator.clipboard?.writeText(plain));
        return true;
      },
      text: allText,
      html: (title) => {
        // The addon wraps its fragment in a page of its own, for the clipboard.
        const body = serializer.serializeAsHTML({ includeGlobalBackground: true })
          .replace(/^[\s\S]*?<!--StartFragment-->/, "")
          .replace(/<!--EndFragment-->[\s\S]*$/, "");
        const name = title.replace(/[&<>"]/g, (char) => `&#${char.charCodeAt(0)};`);
        const background = String(terminal.options.theme?.background ?? "#000000").replace(/[^#\w(),.\s%]/g, "");
        return `<!doctype html>\n<html lang="en">\n<head>\n<meta charset="utf-8">\n<title>${name}</title>\n<style>body { margin: 0; background: ${background}; } pre { margin: 0; padding: 16px; }</style>\n</head>\n<body>\n${body}\n</body>\n</html>\n`;
      },
      lastOutput: lastOutputText,
      findAll: (query, caseSensitive) => {
        const buffer = terminal.buffer.active;
        const needle = caseSensitive ? query : query.toLowerCase();
        const found: FoundLine[] = [];
        for (let line = 0; line < buffer.length && found.length < 200; line++) {
          const text = buffer.getLine(line)?.translateToString(true) ?? "";
          const column = (caseSensitive ? text : text.toLowerCase()).indexOf(needle);
          if (column >= 0) found.push({ line, column, length: query.length, text });
        }
        return found;
      },
      reveal: ({ line, column, length }) => {
        terminal.scrollToLine(Math.max(0, line - Math.floor(terminal.rows / 2)));
        terminal.select(column, line, length);
        terminal.focus();
      },
      detach: async () => {
        const screen = serializer.serialize({ scrollback: Math.min(preferencesRef.current.scrollback, 5_000) });
        const sessionId = sessionRef.current;
        if (!sessionId || !isTauri()) return { screen };
        await invoke("park_session", { sessionId });
        keepSession = true;
        return { sessionId, screen };
      },
      undoDetach: () => reattach?.(),
    });

    return () => {
      stopLogging();
      highlights.dispose();
      leaveMark();
      host.removeEventListener("mousedown", leaveMark, true);
      blurDisposable?.dispose();
      disposed = true;
      unregister();
      observer.disconnect();
      resizeDisposable.dispose();
      titleDisposable.dispose();
      integration.dispose();
      notification.dispose();
      titledNotification.dispose();
      folderReport.dispose();
      fileLinks.dispose();
      searchDisposable.dispose();
      inputDisposable.dispose();
      selectionDisposable.dispose();
      host.removeEventListener("wheel", handleWheel, { capture: true });
      host.removeEventListener("contextmenu", handleContextMenu);
      host.removeEventListener("click", selectOutputAt);
      host.removeEventListener("mousedown", noteSelection, true);
      window.removeEventListener("nebula:insert-paths", insertDropped);
      section?.removeEventListener("mousemove", pointAt);
      section?.removeEventListener("mouseleave", leaveBlock);
      blockScroll.dispose();
      const sessionId = sessionRef.current;
      sessionRef.current = null;
      if (isTauri() && sessionId && !keepSession) void invoke("close_session", { sessionId }).catch(() => undefined);
      terminal.dispose();
      terminalRef.current = null;
      fitRef.current = null;
      searchRef.current = null;
    };
    // The session restarts only when the profile changes or the user asks for it.
  }, [paneId, profile.id, restartNonce]);

  useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal) return;
    terminal.options.fontFamily = withSymbols(preferences.fontFamily);
    terminal.options.fontSize = fontSize;
    terminal.options.lineHeight = preferences.lineHeight;
    terminal.options.cursorStyle = preferences.cursorStyle;
    terminal.options.cursorBlink = preferences.cursorBlink;
    terminal.options.scrollback = preferences.scrollback;
    terminal.options.smoothScrollDuration = preferences.animationLevel === "full" ? 80 : 0;
    terminal.options.allowTransparency = translucent;
    terminal.options.theme = colors;
    terminal.options.screenReaderMode = preferences.screenReaderMode;
    terminal.options.wordSeparator = preferences.wordSeparators;
    requestAnimationFrame(() => {
      try {
        fitRef.current?.fit();
      } catch {
        // Hidden panes are fitted again when they become visible.
      }
    });
  }, [colors, translucent, preferences.animationLevel, preferences.cursorBlink, preferences.cursorStyle, preferences.fontFamily, fontSize, preferences.lineHeight, preferences.scrollback, preferences.screenReaderMode, preferences.wordSeparators]);

  // Sixel and iTerm2 images. The decoder is loaded the first time a pane wants it.
  useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal || !preferences.terminalImages) return;
    let addon: { dispose(): void } | undefined;
    let cancelled = false;
    void import("@xterm/addon-image")
      .then(({ ImageAddon }) => {
        if (cancelled || terminalRef.current !== terminal) return;
        const images = new ImageAddon({ sixelSupport: true, iipSupport: true, storageLimit: 64, showPlaceholder: true });
        terminal.loadAddon(images);
        addon = images;
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
      try {
        addon?.dispose();
      } catch {
        // The terminal may already be disposed.
      }
    };
  }, [paneId, profile.id, restartNonce, preferences.terminalImages]);

  // Ligatures: the page can't read font files, so the addon joins the usual sequences
  // (=>, !=, ===…) and the font draws them as one glyph if it has one.
  useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal || !preferences.fontLigatures) return;
    let addon: { dispose(): void } | undefined;
    let cancelled = false;
    void import("@xterm/addon-ligatures")
      .then(({ LigaturesAddon }) => {
        if (cancelled || terminalRef.current !== terminal) return;
        const ligatures = new LigaturesAddon();
        terminal.loadAddon(ligatures);
        addon = ligatures;
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
      try {
        addon?.dispose();
      } catch {
        // The terminal may already be disposed.
      }
    };
  }, [paneId, profile.id, restartNonce, preferences.fontLigatures]);

  // GPU rendering can be switched off live; xterm falls back to its DOM renderer.
  useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal || !preferences.gpuAcceleration) return;
    let webgl: WebglAddon | undefined;
    try {
      webgl = new WebglAddon();
      webgl.onContextLoss(() => webgl?.dispose());
      terminal.loadAddon(webgl);
    } catch {
      webgl = undefined;
    }
    return () => {
      try {
        webgl?.dispose();
      } catch {
        // The terminal may already be disposed.
      }
    };
  }, [paneId, profile.id, restartNonce, preferences.gpuAcceleration]);

  useEffect(() => {
    if (!visible) return;
    requestAnimationFrame(() => {
      try {
        fitRef.current?.fit();
      } catch {
        // Ignore a pane that unmounted before the frame.
      }
    });
  }, [visible]);

  useEffect(() => {
    if (focused) terminalRef.current?.focus();
  }, [focused]);

  useEffect(() => {
    const search = searchRef.current;
    if (!search) return;
    if (!searchRequest?.query) {
      search.clearDecorations();
      return;
    }
    const accent = preferences.accent;
    const options = {
      incremental: !searchRequest.backwards,
      caseSensitive: searchRequest.caseSensitive,
      wholeWord: searchRequest.wholeWord,
      regex: searchRequest.regex,
      decorations: {
        matchBackground: `${accent}55`,
        matchBorder: `${accent}00`,
        matchOverviewRuler: accent,
        activeMatchBackground: accent,
        activeMatchBorder: accent,
        activeMatchColorOverviewRuler: accent,
      },
    };
    if (searchRequest.regex && !validPattern(searchRequest.query)) {
      search.clearDecorations();
      return;
    }
    if (searchRequest.backwards) search.findPrevious(searchRequest.query, options);
    else search.findNext(searchRequest.query, options);
  }, [preferences.accent, searchRequest?.backwards, searchRequest?.nonce, searchRequest?.query, searchRequest?.caseSensitive, searchRequest?.wholeWord, searchRequest?.regex]);

  /** Starts the shell again, under what the pane showed. */
  const restart = useCallback((manual: boolean) => {
    if (manual) reconnectAttemptsRef.current = 0;
    restoredRef.current = `${screenRef.current()}\x1b[0m\r\n`;
    setReconnectIn(null);
    setRestartNonce((value) => value + 1);
  }, []);

  useEffect(() => {
    if (reconnectIn === null) return;
    if (reconnectIn <= 0) return restart(false);
    const timer = window.setTimeout(() => setReconnectIn((value) => value === null ? null : value - 1), 1000);
    return () => window.clearTimeout(timer);
  }, [reconnectIn, restart]);

  useEffect(() => {
    if (connectionState !== "closed" || !focused) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.ctrlKey || event.altKey || event.metaKey) return;
      if (event.key === "Enter") { event.preventDefault(); restart(true); }
      else if (event.key === "Escape") { event.preventDefault(); if (reconnectIn === null) onClose(); else setReconnectIn(null); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [connectionState, focused, onClose, reconnectIn, restart]);

  const commitPaste = () => {
    const data = pendingPaste;
    setPendingPaste(null);
    if (!data) return;
    if (isTauri()) writeRef.current(data);
    else terminalRef.current?.write(previewText(data).replace(/\n/g, `\r\n${previewPrompt(profile)}`));
    callbacksRef.current.onInput?.(data);
    requestAnimationFrame(() => terminalRef.current?.focus());
  };

  // xterm sends pasted newlines as \r and may wrap them in bracketed-paste markers.
  const pastePreview = pendingPaste ? previewText(pendingPaste) : "";
  const pasteLines = pendingPaste ? pasteLineCount(pendingPaste) : 0;

  return (
    <section
      ref={sectionRef}
      className={`terminal ${focused ? "is-focused" : ""}`}
      style={{ padding: preferences.terminalPadding }}
      data-state={connectionState}
      data-mark-mode={marking || undefined}
      aria-label={`${profile.name} terminal`}
      onMouseDown={onFocus}
    >
      <div ref={hostRef} className="terminal__host" />
      {readOnly && (
        <div className="terminal__chip terminal__locked" role="status" title="Keys, pastes and broadcasts don't reach this shell">
          <Lock size={12} strokeWidth={2} aria-hidden="true" />Read-only
        </div>
      )}
      {marking && (
        <div className="terminal__chip terminal__mark" role="status">
          Mark mode: arrows move, Shift selects, <kbd>Enter</kbd> copies, <kbd>Esc</kbd> leaves
        </div>
      )}
      {logFile && (
        <div className="terminal__chip terminal__log" role="status" title={`Logging to ${logFile.path}`} onMouseDown={(event) => event.stopPropagation()}>
          <span className="terminal__log-dot" aria-hidden="true" />
          <span className="terminal__log-name">{logFile.name}</span>
          <button type="button" title="Stop logging" aria-label="Stop logging" onClick={() => { stopLogRef.current(); terminalRef.current?.focus(); }}><X size={12} strokeWidth={2} /></button>
        </div>
      )}
      {block && (
        <>
          <div className="command-block" style={{ top: block.top, height: block.height }} aria-hidden="true" />
          <div className="command-block__actions" style={{ top: block.top }} onMouseDown={(event) => { event.preventDefault(); event.stopPropagation(); }}>
            {block.seconds !== undefined && (
              <span className={`command-block__meta ${block.code ? "is-failed" : ""}`} title={block.code ? `Failed with exit code ${block.code}` : "Finished"}>
                {block.code ? `exit ${block.code} · ` : ""}{formatElapsed(block.seconds)}
              </span>
            )}
            <button type="button" title="Copy command" aria-label="Copy command" onClick={() => { void navigator.clipboard?.writeText(blockActionsRef.current.command()); terminalRef.current?.focus(); }}><Copy size={13} strokeWidth={1.8} /></button>
            <button type="button" title="Copy output" aria-label="Copy output" onClick={() => { void navigator.clipboard?.writeText(blockActionsRef.current.output()); terminalRef.current?.focus(); }}><ClipboardCheck size={13} strokeWidth={1.8} /></button>
            <button type="button" title="Copy as Markdown" aria-label="Copy as Markdown" onClick={() => { void navigator.clipboard?.writeText(blockActionsRef.current.markdown()); terminalRef.current?.focus(); }}><FileText size={13} strokeWidth={1.8} /></button>
            <button type="button" title="Run again" aria-label="Run again" onClick={() => blockActionsRef.current.rerun()}><RotateCcw size={13} strokeWidth={1.8} /></button>
          </div>
        </>
      )}
      {connectionState === "starting" && <div className="terminal__progress" role="progressbar" aria-label={`Starting ${profile.name}`} />}
      {connectionState === "closed" && reconnectIn !== null && (
        <div className="terminal__exit" role="status">
          <span className="terminal__exit-dot is-waiting" />
          <span>Disconnected. Trying again in {reconnectIn} s</span>
          <button type="button" onClick={() => restart(true)}>Reconnect now <kbd>Enter</kbd></button>
          <button type="button" onClick={() => setReconnectIn(null)}>Cancel <kbd>Esc</kbd></button>
        </div>
      )}
      {connectionState === "closed" && reconnectIn === null && (
        <div className="terminal__exit" role="status">
          <span className={`terminal__exit-dot ${exitCode === 0 ? "is-ok" : ""}`} />
          <span>{exitCode === null ? `${profile.name} could not start` : `${profile.name} exited with code ${exitCode}`}</span>
          <button type="button" onClick={() => restart(true)}>Restart <kbd>Enter</kbd></button>
          <button type="button" onClick={onClose}>Close <kbd>Esc</kbd></button>
        </div>
      )}
      {pendingPaste && (
        <div className="dialog-layer" onMouseDown={(event) => event.stopPropagation()}>
          <div className="dialog" role="alertdialog" aria-modal="true" aria-labelledby={`paste-${paneId}`}>
            <h2 id={`paste-${paneId}`}>Paste {pasteLines} lines?</h2>
            <p>Each line may run as a separate command as soon as it reaches the shell.</p>
            <pre className="dialog__preview">{pastePreview.length > 600 ? `${pastePreview.slice(0, 600)}…` : pastePreview}</pre>
            <div className="dialog__actions">
              <button className="button" type="button" onClick={() => { setPendingPaste(null); terminalRef.current?.focus(); }}>Cancel</button>
              <button className="button button--primary" type="button" autoFocus onClick={commitPaste}>Paste</button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

import { useEffect, useMemo, useRef, useState } from "react";
import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { Terminal, type IMarker } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { adjacentPrompt, joinLines } from "../commandMarks";
import { openExternal } from "../external";
import { folderFromReport } from "../folders";
import { registerPane } from "../paneRegistry";
import { windowsBuild } from "../platform";
import { isPaste, pasteLineCount, pastePreview as previewText, quoteDroppedPath, trimSingleLinePaste } from "../terminalInput";
import { previewSession } from "../preview-session";
import { resolveTheme, xtermTheme } from "../themes";
import type { AppearancePreferences, PtyEvent, TerminalProfile } from "../types";

interface TerminalPaneProps {
  paneId: string;
  profile: TerminalProfile;
  /** Where the shell starts when it has no folder of its own (its last one, or the pane it was split from). */
  startIn?: string;
  preferences: AppearancePreferences;
  focused: boolean;
  visible: boolean;
  searchRequest?: { query: string; nonce: number; backwards?: boolean };
  onFocus: () => void;
  onFontSizeDelta: (delta: number) => void;
  onTitleChange: (title: string) => void;
  onActivity: () => void;
  onSearchResult: (result: { index: number; count: number }) => void;
  onContextMenu: (x: number, y: number) => void;
  onCommandFinished: (finished: FinishedCommand) => void;
  onNotify: (title: string, body: string) => void;
  onFolderChange: (folder: string) => void;
  onClose: () => void;
}

/** A command that ran between shell-integration marks (OSC 133;C and 133;D). */
export interface FinishedCommand {
  command: string;
  code: number | null;
  seconds: number;
}

type ConnectionState = "starting" | "ready" | "closed" | "preview";

/** One command as the shell marked it: its prompt, where its output starts, and where it ended. */
interface MarkedCommand {
  prompt?: IMarker;
  output?: IMarker;
  end?: IMarker;
  /** The output stopped mid-line, so the end mark's line is part of it. */
  endsMidLine?: boolean;
}

/** Commands kept per pane; older ones are forgotten (their lines usually left the scrollback too). */
const MAX_MARKED_COMMANDS = 500;

/** Falls back to the bundled Nerd Font icons for glyphs the user's font lacks. */
function withSymbols(fontFamily: string): string {
  return `${fontFamily}, "Nebula Symbols"`;
}

/** Ctrl+<letter> on any keyboard layout: Cyrillic or Greek layouts report another `key`. */
function isControlLetter(event: KeyboardEvent, letter: "c" | "v"): boolean {
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
  preferences,
  focused,
  visible,
  searchRequest,
  onFocus,
  onFontSizeDelta,
  onTitleChange,
  onActivity,
  onSearchResult,
  onContextMenu,
  onCommandFinished,
  onNotify,
  onFolderChange,
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
  const callbacksRef = useRef({ onFontSizeDelta, onTitleChange, onActivity, onSearchResult, onContextMenu, onCommandFinished, onNotify, onFolderChange });
  const [connectionState, setConnectionState] = useState<ConnectionState>("starting");
  const [exitCode, setExitCode] = useState<number | null>(null);
  const [pendingPaste, setPendingPaste] = useState<string | null>(null);
  const [restartNonce, setRestartNonce] = useState(0);

  focusedRef.current = focused;
  visibleRef.current = visible;
  preferencesRef.current = preferences;
  startInRef.current = startIn;
  callbacksRef.current = { onFontSizeDelta, onTitleChange, onActivity, onSearchResult, onContextMenu, onCommandFinished, onNotify, onFolderChange };

  const theme = useMemo(() => resolveTheme(preferences.themeId), [preferences.themeId]);
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
      cursorBlink: preferencesRef.current.cursorBlink,
      cursorStyle: preferencesRef.current.cursorStyle,
      cursorInactiveStyle: "outline",
      fontFamily: withSymbols(preferencesRef.current.fontFamily),
      fontSize: preferencesRef.current.fontSize,
      lineHeight: preferencesRef.current.lineHeight,
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
    terminal.loadAddon(fit);
    terminal.loadAddon(search);
    // Links open with Ctrl+click so a plain click can still start a selection.
    terminal.loadAddon(new WebLinksAddon((event, uri) => {
      if (event.ctrlKey || !isTauri()) openExternal(uri);
    }));
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
      } else if (mark === "C") {
        running = { command: lastTitle, startedAt: Date.now() };
        const current = commands.at(-1);
        if (current && !current.output && !current.end) current.output = cursorMarker();
        else remember({ output: cursorMarker() });
      } else if (mark === "D") {
        const parsed = code === undefined ? Number.NaN : Number.parseInt(code, 10);
        const current = commands.at(-1);
        if (current?.output && !current.end) {
          current.end = cursorMarker();
          current.endsMidLine = terminal.buffer.active.cursorX > 0;
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
    const copyLastOutput = () => {
      const lines = lastOutputLines();
      if (!lines) return false;
      const buffer = terminal.buffer.active;
      const text = joinLines(Array.from({ length: lines[1] - lines[0] + 1 }, (_, index) => {
        const line = buffer.getLine(lines[0] + index);
        return { text: line?.translateToString(true) ?? "", wrapped: line?.isWrapped ?? false };
      }));
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
    const notification = terminal.parser.registerOscHandler(9, (data) => {
      // OSC 9;4;… is a progress report (ConEmu, Windows Terminal), not a message.
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
      if (folder) callbacksRef.current.onFolderChange(folder);
      return true;
    });
    const searchDisposable = search.onDidChangeResults(({ resultIndex, resultCount }) => {
      callbacksRef.current.onSearchResult({ index: resultIndex, count: resultCount });
    });

    // Input typed while the shell starts waits here instead of being lost.
    let queued: string[] = [];
    let exited = false;
    const send = (sessionId: string, data: string) => {
      void invoke("write_session", { sessionId, data }).catch((error) => {
        if (!disposed) terminal.writeln(`\r\n\x1b[31m${String(error)}\x1b[0m`);
      });
    };
    const writeRaw = (data: string) => {
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

    const writeInput = (data: string) => {
      if (!isPaste(data)) {
        writeRaw(data);
        return;
      }
      if (pasteLineCount(data) > 1) {
        if (preferencesRef.current.confirmMultilinePaste) setPendingPaste(data);
        else writeRaw(data);
        return;
      }
      writeRaw(trimSingleLinePaste(data));
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

    if (isTauri()) {
      const channel = new Channel<PtyEvent>();
      channel.onmessage = (message) => {
        if (disposed) return;
        if (message.event === "output" && message.data.chunk) {
          terminal.write(message.data.chunk);
          if (!visibleRef.current) callbacksRef.current.onActivity();
        } else if (message.event === "exit") {
          setExitCode(message.data.code ?? null);
          setConnectionState("closed");
          exited = true;
          queued = [];
          sessionRef.current = null;
        } else if (message.event === "error" && message.data.message) {
          terminal.writeln(`\r\n\x1b[31m${message.data.message}\x1b[0m`);
        }
      };

      resize();
      const { startingFolder, workingDirectory, shellIntegration } = preferencesRef.current;
      void invoke<string>("start_session", {
        profileId: profile.id,
        cols: terminal.cols,
        rows: terminal.rows,
        cwd: startingFolder === "custom" ? workingDirectory.trim() || null : null,
        knownFolder: startingFolder === "desktop" || startingFolder === "documents" ? startingFolder : null,
        startIn: startInRef.current ?? null,
        shellIntegration,
        onEvent: channel,
      }).then((sessionId) => {
        if (disposed) {
          void invoke("close_session", { sessionId }).catch(() => undefined);
          return;
        }
        // The shell may already have exited (a missing WSL distribution, for example).
        if (exited) return;
        sessionRef.current = sessionId;
        setConnectionState((state) => state === "closed" ? state : "ready");
        void invoke("resize_session", { sessionId, cols: terminal.cols, rows: terminal.rows }).catch(() => undefined);
        for (const data of queued) send(sessionId, data);
        queued = [];
      }).catch((error) => {
        if (disposed) return;
        terminal.writeln(`\x1b[31mCould not start ${profile.name}: ${String(error)}\x1b[0m`);
        setConnectionState("closed");
      });
    } else {
      setConnectionState("preview");
      writePreview(terminal, profile);
    }

    // terminal.paste() routes through onData with bracketed-paste markers when the shell asks for them.
    const inputDisposable = terminal.onData(writeInput);
    const selectionDisposable = terminal.onSelectionChange(() => {
      if (preferencesRef.current.copyOnSelect) copy();
    });

    terminal.attachCustomKeyEventHandler((event) => {
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
      return true;
    });

    // xterm's scroll area swallows wheel events whenever it can scroll, so Ctrl+Wheel
    // is caught on the way down, before it gets there.
    const handleWheel = (event: WheelEvent) => {
      if (!event.ctrlKey) return;
      event.preventDefault();
      event.stopPropagation();
      callbacksRef.current.onFontSizeDelta(event.deltaY < 0 ? 1 : -1);
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

    const unregister = registerPane(paneId, {
      copy,
      paste,
      selectAll: () => terminal.selectAll(),
      clear: () => terminal.clear(),
      hasSelection: () => terminal.hasSelection(),
      focus: () => terminal.focus(),
      jumpToCommand,
      copyLastOutput,
      hasCommandOutput: () => lastOutputLines() !== undefined,
    });

    return () => {
      disposed = true;
      unregister();
      observer.disconnect();
      resizeDisposable.dispose();
      titleDisposable.dispose();
      integration.dispose();
      notification.dispose();
      titledNotification.dispose();
      folderReport.dispose();
      searchDisposable.dispose();
      inputDisposable.dispose();
      selectionDisposable.dispose();
      host.removeEventListener("wheel", handleWheel, { capture: true });
      host.removeEventListener("contextmenu", handleContextMenu);
      host.removeEventListener("click", selectOutputAt);
      host.removeEventListener("mousedown", noteSelection, true);
      window.removeEventListener("nebula:insert-paths", insertDropped);
      const sessionId = sessionRef.current;
      sessionRef.current = null;
      if (isTauri() && sessionId) void invoke("close_session", { sessionId }).catch(() => undefined);
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
    terminal.options.fontSize = preferences.fontSize;
    terminal.options.lineHeight = preferences.lineHeight;
    terminal.options.cursorStyle = preferences.cursorStyle;
    terminal.options.cursorBlink = preferences.cursorBlink;
    terminal.options.scrollback = preferences.scrollback;
    terminal.options.smoothScrollDuration = preferences.animationLevel === "full" ? 80 : 0;
    terminal.options.allowTransparency = translucent;
    terminal.options.theme = colors;
    requestAnimationFrame(() => {
      try {
        fitRef.current?.fit();
      } catch {
        // Hidden panes are fitted again when they become visible.
      }
    });
  }, [colors, translucent, preferences.animationLevel, preferences.cursorBlink, preferences.cursorStyle, preferences.fontFamily, preferences.fontSize, preferences.lineHeight, preferences.scrollback]);

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
      decorations: {
        matchBackground: `${accent}55`,
        matchBorder: `${accent}00`,
        matchOverviewRuler: accent,
        activeMatchBackground: accent,
        activeMatchBorder: accent,
        activeMatchColorOverviewRuler: accent,
      },
    };
    if (searchRequest.backwards) search.findPrevious(searchRequest.query, options);
    else search.findNext(searchRequest.query, options);
  }, [preferences.accent, searchRequest?.backwards, searchRequest?.nonce, searchRequest?.query]);

  useEffect(() => {
    if (connectionState !== "closed" || !focused) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.ctrlKey || event.altKey || event.metaKey) return;
      if (event.key === "Enter") { event.preventDefault(); setRestartNonce((value) => value + 1); }
      else if (event.key === "Escape") { event.preventDefault(); onClose(); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [connectionState, focused, onClose]);

  const commitPaste = () => {
    const data = pendingPaste;
    setPendingPaste(null);
    if (!data) return;
    if (isTauri()) writeRef.current(data);
    else terminalRef.current?.write(previewText(data).replace(/\n/g, `\r\n${previewPrompt(profile)}`));
    requestAnimationFrame(() => terminalRef.current?.focus());
  };

  // xterm sends pasted newlines as \r and may wrap them in bracketed-paste markers.
  const pastePreview = pendingPaste ? previewText(pendingPaste) : "";
  const pasteLines = pendingPaste ? pasteLineCount(pendingPaste) : 0;

  return (
    <section
      className={`terminal ${focused ? "is-focused" : ""}`}
      style={{ padding: preferences.terminalPadding }}
      data-state={connectionState}
      aria-label={`${profile.name} terminal`}
      onMouseDown={onFocus}
    >
      <div ref={hostRef} className="terminal__host" />
      {connectionState === "starting" && <div className="terminal__progress" role="progressbar" aria-label={`Starting ${profile.name}`} />}
      {connectionState === "closed" && (
        <div className="terminal__exit" role="status">
          <span className={`terminal__exit-dot ${exitCode === 0 ? "is-ok" : ""}`} />
          <span>{exitCode === null ? `${profile.name} could not start` : `${profile.name} exited with code ${exitCode}`}</span>
          <button type="button" onClick={() => setRestartNonce((value) => value + 1)}>Restart <kbd>Enter</kbd></button>
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

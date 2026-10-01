import { useEffect, useMemo, useRef, useState } from "react";
import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { ClipboardPaste, RotateCcw, ShieldAlert, X } from "lucide-react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { resolveTheme } from "../preferences";
import type { AppearancePreferences, PtyEvent, TerminalProfile } from "../types";

interface TerminalPaneProps {
  profile: TerminalProfile;
  preferences: AppearancePreferences;
  focused: boolean;
  searchRequest?: { query: string; nonce: number; backwards?: boolean };
  onFocus: () => void;
  onFontSizeDelta: (delta: number) => void;
  onTitleChange: (title: string) => void;
}

type ConnectionState = "starting" | "ready" | "closed" | "preview";

function quoteDroppedPath(path: string): string {
  return `"${path.replaceAll('"', '\\"')}"`;
}

function isMultiline(text: string): boolean {
  return text.replaceAll("\r\n", "\n").split(/[\r\n]/).filter(Boolean).length > 1;
}

export default function TerminalPane({
  profile,
  preferences,
  focused,
  searchRequest,
  onFocus,
  onFontSizeDelta,
  onTitleChange,
}: TerminalPaneProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const searchRef = useRef<SearchAddon | null>(null);
  const sessionRef = useRef<string | null>(null);
  const focusedRef = useRef(focused);
  const preferencesRef = useRef(preferences);
  const onFontSizeDeltaRef = useRef(onFontSizeDelta);
  const onTitleChangeRef = useRef(onTitleChange);
  const [connectionState, setConnectionState] = useState<ConnectionState>("starting");
  const [exitCode, setExitCode] = useState<number | null>(null);
  const [pendingPaste, setPendingPaste] = useState<string | null>(null);
  const [restartNonce, setRestartNonce] = useState(0);

  focusedRef.current = focused;
  preferencesRef.current = preferences;
  onFontSizeDeltaRef.current = onFontSizeDelta;
  onTitleChangeRef.current = onTitleChange;

  const preset = useMemo(() => resolveTheme(preferences.themeId), [preferences.themeId]);
  const theme = useMemo(() => ({
    background: preset.background,
    foreground: preset.foreground,
    cursor: preferences.accent,
    cursorAccent: preset.background,
    selectionBackground: `${preferences.accent}42`,
    black: preset.black,
    red: preset.red,
    green: preset.green,
    yellow: preset.yellow,
    blue: preset.blue,
    magenta: preset.magenta,
    cyan: preset.cyan,
    white: preset.white,
    brightBlack: "#687277",
    brightRed: preset.red,
    brightGreen: preset.green,
    brightYellow: preset.yellow,
    brightBlue: preset.blue,
    brightMagenta: preset.magenta,
    brightCyan: preset.cyan,
    brightWhite: preset.foreground,
  }), [preferences.accent, preset]);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    let disposed = false;

    const terminal = new Terminal({
      allowProposedApi: false,
      convertEol: false,
      cursorBlink: preferencesRef.current.cursorBlink,
      cursorStyle: preferencesRef.current.cursorStyle,
      fontFamily: preferencesRef.current.fontFamily,
      fontSize: preferencesRef.current.fontSize,
      lineHeight: preferencesRef.current.lineHeight,
      scrollback: preferencesRef.current.scrollback,
      theme,
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    terminal.loadAddon(fit);
    terminal.loadAddon(search);
    terminal.loadAddon(new WebLinksAddon());
    terminal.open(host);

    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => webgl.dispose());
      terminal.loadAddon(webgl);
    } catch {
      // xterm keeps its canvas renderer when WebGL is unavailable.
    }

    terminalRef.current = terminal;
    fitRef.current = fit;
    searchRef.current = search;
    setConnectionState("starting");
    setExitCode(null);

    const resize = () => {
      if (disposed) return;
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
      if (isTauri() && sessionId) {
        void invoke("resize_session", { sessionId, cols, rows }).catch(() => undefined);
      }
    });
    const titleDisposable = terminal.onTitleChange((title) => {
      const clean = title.trim().slice(0, 120);
      if (clean) onTitleChangeRef.current(clean);
    });

    const writeRaw = (data: string) => {
      const sessionId = sessionRef.current;
      if (isTauri() && sessionId) {
        void invoke("write_session", { sessionId, data }).catch((error) => {
          if (!disposed) terminal.writeln(`\r\n\x1b[31m${String(error)}\x1b[0m`);
        });
        return;
      }
      if (!isTauri()) {
        if (data === "\r") terminal.write("\r\n\x1b[38;2;242;169;59m❯\x1b[0m ");
        else if (data === "\u007f") terminal.write("\b \b");
        else terminal.write(data);
      }
    };

    const writeInput = (data: string) => {
      if (preferencesRef.current.confirmMultilinePaste && isMultiline(data)) {
        setPendingPaste(data);
        return;
      }
      writeRaw(data);
    };

    if (isTauri()) {
      const channel = new Channel<PtyEvent>();
      channel.onmessage = (message) => {
        if (disposed) return;
        if (message.event === "output" && message.data.chunk) terminal.write(message.data.chunk);
        if (message.event === "exit") {
          setExitCode(message.data.code ?? null);
          setConnectionState("closed");
        }
        if (message.event === "error" && message.data.message) {
          terminal.writeln(`\r\n\x1b[31m${message.data.message}\x1b[0m`);
        }
      };

      resize();
      void invoke<string>("start_session", {
        profileId: profile.id,
        cols: terminal.cols,
        rows: terminal.rows,
        cwd: preferencesRef.current.workingDirectory.trim() || null,
        onEvent: channel,
      }).then((sessionId) => {
        if (disposed) {
          void invoke("close_session", { sessionId }).catch(() => undefined);
          return;
        }
        sessionRef.current = sessionId;
        setConnectionState("ready");
        void invoke("resize_session", { sessionId, cols: terminal.cols, rows: terminal.rows }).catch(() => undefined);
      }).catch((error) => {
        if (disposed) return;
        terminal.writeln(`\x1b[31mCould not start ${profile.name}: ${String(error)}\x1b[0m`);
        setConnectionState("closed");
      });
    } else {
      setConnectionState("preview");
      terminal.writeln("\x1b[38;2;242;169;59mNebula Terminal\x1b[0m  \x1b[38;2;111;154;148msolar-noir workspace\x1b[0m");
      terminal.writeln("\x1b[38;2;126;137;140mA native PTY opens automatically in the desktop app.\x1b[0m\r\n");
      terminal.writeln("\x1b[38;2;126;137;140mTry the command palette with Ctrl+Shift+P.\x1b[0m\r\n");
      terminal.write("\x1b[38;2;242;169;59m❯\x1b[0m ");
    }

    const inputDisposable = terminal.onData(writeInput);
    const selectionDisposable = terminal.onSelectionChange(() => {
      if (!preferencesRef.current.copyOnSelect || !terminal.hasSelection()) return;
      void navigator.clipboard?.writeText(terminal.getSelection()).catch(() => undefined);
    });

    terminal.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown") return true;
      if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "c" && terminal.hasSelection()) {
        void navigator.clipboard?.writeText(terminal.getSelection()).catch(() => undefined);
        return false;
      }
      if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "v") {
        void navigator.clipboard?.readText().then(writeInput).catch(() => undefined);
        return false;
      }
      return true;
    });

    const handleWheel = (event: WheelEvent) => {
      if (!event.ctrlKey) return;
      event.preventDefault();
      onFontSizeDeltaRef.current(event.deltaY < 0 ? 1 : -1);
    };
    host.addEventListener("wheel", handleWheel, { passive: false });

    const insertDropped = (event: Event) => {
      if (!focusedRef.current) return;
      const paths = (event as CustomEvent<string[]>).detail;
      if (!Array.isArray(paths)) return;
      const text = paths.map(quoteDroppedPath).join(" ");
      if (text) writeInput(text);
    };
    window.addEventListener("nebula:insert-paths", insertDropped);

    return () => {
      disposed = true;
      observer.disconnect();
      resizeDisposable.dispose();
      titleDisposable.dispose();
      inputDisposable.dispose();
      selectionDisposable.dispose();
      host.removeEventListener("wheel", handleWheel);
      window.removeEventListener("nebula:insert-paths", insertDropped);
      const sessionId = sessionRef.current;
      sessionRef.current = null;
      if (isTauri() && sessionId) void invoke("close_session", { sessionId }).catch(() => undefined);
      terminal.dispose();
      terminalRef.current = null;
      fitRef.current = null;
      searchRef.current = null;
    };
  }, [profile.id, restartNonce]);

  useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal) return;
    terminal.options.fontFamily = preferences.fontFamily;
    terminal.options.fontSize = preferences.fontSize;
    terminal.options.lineHeight = preferences.lineHeight;
    terminal.options.cursorStyle = preferences.cursorStyle;
    terminal.options.cursorBlink = preferences.cursorBlink;
    terminal.options.scrollback = preferences.scrollback;
    terminal.options.theme = theme;
    requestAnimationFrame(() => fitRef.current?.fit());
  }, [preferences.cursorBlink, preferences.cursorStyle, preferences.fontFamily, preferences.fontSize, preferences.lineHeight, preferences.scrollback, theme]);

  useEffect(() => {
    if (focused) terminalRef.current?.focus();
  }, [focused]);

  useEffect(() => {
    if (!searchRequest?.query) {
      searchRef.current?.clearDecorations();
      return;
    }
    const options = { incremental: true, decorations: { matchBackground: "#6b5528", activeMatchBackground: "#f2a93b", matchOverviewRuler: "#f2a93b", activeMatchColorOverviewRuler: "#fff2d6" } };
    if (searchRequest.backwards) searchRef.current?.findPrevious(searchRequest.query, options);
    else searchRef.current?.findNext(searchRequest.query, options);
  }, [searchRequest?.backwards, searchRequest?.nonce, searchRequest?.query]);

  const commitPaste = () => {
    const data = pendingPaste;
    setPendingPaste(null);
    if (!data) return;
    const sessionId = sessionRef.current;
    if (isTauri() && sessionId) void invoke("write_session", { sessionId, data }).catch(() => undefined);
    else terminalRef.current?.write(data);
    requestAnimationFrame(() => terminalRef.current?.focus());
  };

  return (
    <section
      className={`terminal-pane ${focused ? "terminal-pane--focused" : ""}`}
      style={{
        padding: preferences.terminalPadding,
        backgroundColor: `color-mix(in srgb, ${preset.background} ${Math.round(preferences.terminalOpacity * 100)}%, transparent)`,
      }}
      data-state={connectionState}
      aria-label={`${profile.name} terminal`}
      onMouseDown={onFocus}
    >
      <div ref={hostRef} className="terminal-host" />
      {connectionState === "starting" && <div className="terminal-state terminal-state--starting"><span />Opening {profile.name}</div>}
      {connectionState === "closed" && (
        <div className="terminal-state terminal-state--closed">
          <span>{exitCode === null ? "Session ended" : `Process exited with code ${exitCode}`}</span>
          <button type="button" onClick={() => setRestartNonce((value) => value + 1)}><RotateCcw size={13} />Restart</button>
        </div>
      )}
      {pendingPaste && (
        <div className="paste-guard" role="dialog" aria-modal="true" aria-label="Confirm multiline paste" onMouseDown={(event) => event.stopPropagation()}>
          <button className="paste-guard__close" type="button" onClick={() => setPendingPaste(null)} aria-label="Cancel paste"><X size={15} /></button>
          <span className="paste-guard__icon"><ShieldAlert size={20} /></span>
          <div><strong>Paste {pendingPaste.replaceAll("\r\n", "\n").split("\n").length} lines?</strong><p>Review the commands before they run in this session.</p></div>
          <div className="paste-guard__actions"><button type="button" onClick={() => setPendingPaste(null)}>Cancel</button><button className="primary-button" type="button" onClick={commitPaste}><ClipboardPaste size={14} />Paste</button></div>
        </div>
      )}
    </section>
  );
}

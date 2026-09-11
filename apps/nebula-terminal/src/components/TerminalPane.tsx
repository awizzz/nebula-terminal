import { useEffect, useMemo, useRef, useState } from "react";
import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { resolveTheme } from "../preferences";
import type { AppearancePreferences, PtyEvent, TerminalProfile } from "../types";

interface TerminalPaneProps {
  paneId: string;
  profile: TerminalProfile;
  preferences: AppearancePreferences;
  focused: boolean;
  searchRequest?: { query: string; nonce: number };
  onFocus: () => void;
  onFontSizeDelta: (delta: number) => void;
}

function quoteDroppedPath(path: string): string {
  return /\s/.test(path) ? `"${path.replaceAll('"', '\\"')}"` : path;
}

export default function TerminalPane({
  profile,
  preferences,
  focused,
  searchRequest,
  onFocus,
  onFontSizeDelta,
}: TerminalPaneProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const searchRef = useRef<SearchAddon | null>(null);
  const sessionRef = useRef<string | null>(null);
  const [connectionState, setConnectionState] = useState<"starting" | "ready" | "closed" | "preview">("starting");

  const preset = useMemo(() => resolveTheme(preferences.themeId), [preferences.themeId]);
  const theme = useMemo(() => ({
    background: preset.background,
    foreground: preset.foreground,
    cursor: preferences.accent,
    cursorAccent: preset.background,
    selectionBackground: `${preferences.accent}4d`,
    black: preset.black,
    red: preset.red,
    green: preset.green,
    yellow: preset.yellow,
    blue: preset.blue,
    magenta: preset.magenta,
    cyan: preset.cyan,
    white: preset.white,
    brightBlack: "#5b5f6b",
    brightRed: preset.red,
    brightGreen: preset.green,
    brightYellow: preset.yellow,
    brightBlue: preset.blue,
    brightMagenta: preset.magenta,
    brightCyan: preset.cyan,
    brightWhite: preset.foreground,
  }), [preferences.accent, preset]);

  useEffect(() => {
    if (!hostRef.current) return;

    const terminal = new Terminal({
      allowProposedApi: false,
      convertEol: false,
      cursorBlink: preferences.cursorBlink,
      cursorStyle: preferences.cursorStyle,
      fontFamily: preferences.fontFamily,
      fontSize: preferences.fontSize,
      lineHeight: preferences.lineHeight,
      scrollback: 10_000,
      theme,
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    terminal.loadAddon(fit);
    terminal.loadAddon(search);
    terminal.loadAddon(new WebLinksAddon());
    terminal.open(hostRef.current);

    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => webgl.dispose());
      terminal.loadAddon(webgl);
    } catch {
      // Canvas renderer remains active when WebGL is unavailable.
    }

    terminalRef.current = terminal;
    fitRef.current = fit;
    searchRef.current = search;

    const resize = () => {
      try {
        fit.fit();
        if (isTauri() && sessionRef.current) {
          void invoke("resize_session", {
            sessionId: sessionRef.current,
            cols: terminal.cols,
            rows: terminal.rows,
          });
        }
      } catch {
        // Ignore resize attempts while the pane is being unmounted.
      }
    };

    const observer = new ResizeObserver(resize);
    observer.observe(hostRef.current);

    if (isTauri()) {
      const channel = new Channel<PtyEvent>();
      channel.onmessage = (message) => {
        if (message.event === "output" && message.data.chunk) terminal.write(message.data.chunk);
        if (message.event === "exit") {
          terminal.writeln(`\r\n\x1b[38;2;125;130;145m[process exited${message.data.code !== undefined ? `: ${message.data.code}` : ""}]\x1b[0m`);
          setConnectionState("closed");
        }
        if (message.event === "error" && message.data.message) terminal.writeln(`\r\n\x1b[31m${message.data.message}\x1b[0m`);
      };

      fit.fit();
      void invoke<string>("start_session", {
        profileId: profile.id,
        cols: terminal.cols,
        rows: terminal.rows,
        onEvent: channel,
      }).then((sessionId) => {
        sessionRef.current = sessionId;
        setConnectionState("ready");
        resize();
      }).catch((error) => {
        terminal.writeln(`\x1b[31mUnable to start ${profile.name}: ${String(error)}\x1b[0m`);
        setConnectionState("closed");
      });
    } else {
      setConnectionState("preview");
      terminal.writeln("\x1b[38;2;139;124;246mNebula Terminal\x1b[0m  \x1b[38;2;125;130;145mUI preview\x1b[0m");
      terminal.writeln("\x1b[38;2;125;130;145mThe native PTY connects automatically inside the Tauri desktop host.\x1b[0m\r\n");
      terminal.write("\x1b[38;2;139;124;246m❯\x1b[0m ");
    }

    const inputDisposable = terminal.onData((data) => {
      if (isTauri() && sessionRef.current) {
        void invoke("write_session", { sessionId: sessionRef.current, data });
      } else if (!isTauri()) {
        if (data === "\r") terminal.write("\r\n\x1b[38;2;139;124;246m❯\x1b[0m ");
        else if (data === "\u007f") terminal.write("\b \b");
        else terminal.write(data);
      }
    });

    const selectionDisposable = terminal.onSelectionChange(() => {
      if (!preferences.copyOnSelect || !terminal.hasSelection()) return;
      void navigator.clipboard?.writeText(terminal.getSelection()).catch(() => undefined);
    });

    const keyDisposable = terminal.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown") return true;
      if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "c" && terminal.hasSelection()) {
        void navigator.clipboard?.writeText(terminal.getSelection()).catch(() => undefined);
        return false;
      }
      if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "v") {
        void navigator.clipboard?.readText().then((text) => {
          if (text && isTauri() && sessionRef.current) void invoke("write_session", { sessionId: sessionRef.current, data: text });
        }).catch(() => undefined);
        return false;
      }
      return true;
    });

    const handleWheel = (event: WheelEvent) => {
      if (!event.ctrlKey) return;
      event.preventDefault();
      onFontSizeDelta(event.deltaY < 0 ? 1 : -1);
    };
    hostRef.current.addEventListener("wheel", handleWheel, { passive: false });

    const insertDropped = (event: Event) => {
      if (!focused || !isTauri() || !sessionRef.current) return;
      const paths = (event as CustomEvent<string[]>).detail;
      const text = paths.map(quoteDroppedPath).join(" ");
      if (text) void invoke("write_session", { sessionId: sessionRef.current, data: text });
    };
    window.addEventListener("nebula:insert-paths", insertDropped);

    return () => {
      observer.disconnect();
      inputDisposable.dispose();
      selectionDisposable.dispose();
      keyDisposable.dispose();
      hostRef.current?.removeEventListener("wheel", handleWheel);
      window.removeEventListener("nebula:insert-paths", insertDropped);
      if (isTauri() && sessionRef.current) void invoke("close_session", { sessionId: sessionRef.current });
      sessionRef.current = null;
      terminal.dispose();
      terminalRef.current = null;
      fitRef.current = null;
      searchRef.current = null;
    };
  }, [profile.id]);

  useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal) return;
    terminal.options.fontFamily = preferences.fontFamily;
    terminal.options.fontSize = preferences.fontSize;
    terminal.options.lineHeight = preferences.lineHeight;
    terminal.options.cursorStyle = preferences.cursorStyle;
    terminal.options.cursorBlink = preferences.cursorBlink;
    terminal.options.theme = theme;
    requestAnimationFrame(() => fitRef.current?.fit());
  }, [preferences, theme]);

  useEffect(() => {
    if (focused) terminalRef.current?.focus();
  }, [focused]);

  useEffect(() => {
    if (!focused || !searchRequest?.query) return;
    searchRef.current?.findNext(searchRequest.query, { incremental: true });
  }, [focused, searchRequest]);

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
      {connectionState === "starting" && <div className="terminal-connecting">Starting {profile.name}…</div>}
    </section>
  );
}

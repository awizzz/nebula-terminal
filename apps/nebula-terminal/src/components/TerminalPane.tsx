import { useEffect, useMemo, useRef, useState } from "react";
import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import type { AppearancePreferences, PtyEvent, TerminalProfile } from "../types";

interface TerminalPaneProps {
  tabId: string;
  profile: TerminalProfile;
  preferences: AppearancePreferences;
  focused: boolean;
}

export default function TerminalPane({ profile, preferences, focused }: TerminalPaneProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const sessionRef = useRef<string | null>(null);
  const [connectionState, setConnectionState] = useState<"starting" | "ready" | "closed" | "preview">("starting");

  const theme = useMemo(() => ({
    background: "#0a0b0e",
    foreground: "#e8e9ed",
    cursor: preferences.accent,
    cursorAccent: "#0a0b0e",
    selectionBackground: `${preferences.accent}4d`,
    black: "#17181d",
    red: "#ef6b73",
    green: "#8ccf7e",
    yellow: "#e5c07b",
    blue: "#7aa2f7",
    magenta: "#bb9af7",
    cyan: "#7dcfff",
    white: "#c7c9d1",
    brightBlack: "#5b5f6b",
    brightRed: "#ff7a85",
    brightGreen: "#9fe38f",
    brightYellow: "#f2d28c",
    brightBlue: "#8db0ff",
    brightMagenta: "#c8a5ff",
    brightCyan: "#8bdfff",
    brightWhite: "#f4f4f6",
  }), [preferences.accent]);

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
    const links = new WebLinksAddon();
    terminal.loadAddon(fit);
    terminal.loadAddon(search);
    terminal.loadAddon(links);
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
        if (message.event === "error" && message.data.message) {
          terminal.writeln(`\r\n\x1b[31m${message.data.message}\x1b[0m`);
        }
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
        return;
      }
      if (!isTauri()) {
        if (data === "\r") terminal.write("\r\n\x1b[38;2;139;124;246m❯\x1b[0m ");
        else if (data === "\u007f") terminal.write("\b \b");
        else terminal.write(data);
      }
    });

    return () => {
      observer.disconnect();
      inputDisposable.dispose();
      if (isTauri() && sessionRef.current) {
        void invoke("close_session", { sessionId: sessionRef.current });
      }
      sessionRef.current = null;
      terminal.dispose();
      terminalRef.current = null;
      fitRef.current = null;
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

  return (
    <section
      className={`terminal-pane ${focused ? "terminal-pane--focused" : ""}`}
      style={{
        padding: preferences.terminalPadding,
        backgroundColor: `rgba(10, 11, 14, ${preferences.terminalOpacity})`,
      }}
      data-state={connectionState}
      aria-label={`${profile.name} terminal`}
    >
      <div ref={hostRef} className="terminal-host" />
      {connectionState === "starting" && <div className="terminal-connecting">Starting {profile.name}…</div>}
    </section>
  );
}

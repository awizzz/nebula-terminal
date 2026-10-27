import type { TerminalTheme } from "../themes";

/** A few lines of a shell session in the theme's colors, for theme cards. */
export default function ThemePreview({ theme }: { theme: TerminalTheme }) {
  const [, red, green, yellow, blue, magenta] = theme.ansi;
  return (
    <span className="theme-card__preview" style={{ background: theme.background, color: theme.foreground }} aria-hidden="true">
      <span><b style={{ color: blue }}>~/app</b> on <b style={{ color: magenta }}>main</b> <span style={{ color: yellow }}>!1</span></span>
      <span><b style={{ color: green }}>❯ git</b> status --short</span>
      <span><span style={{ color: red }}> M</span> src/theme.ts</span>
      <span><span style={{ color: red }}>??</span> notes.txt</span>
    </span>
  );
}

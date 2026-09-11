# Nebula Terminal

Nebula Terminal is the graphical desktop companion to Nebula Shell. The shell remains usable as a standalone CLI, while the terminal adds the native window, rendering, tabs, panes, profiles, clickable settings and visual customization.

The goal is not to wrap the existing shell in a decorative frame. The terminal should feel like a complete product: fast to launch, visually distinctive, keyboard-first, pleasant to use with a mouse, and configurable without editing files unless the user wants to.

## Product principles

1. **Nebula first, compatibility always.** Nebula Shell is the default profile, but CMD, Windows PowerShell, PowerShell 7, WSL, SSH and arbitrary executable profiles can be hosted without turning them into Nebula syntax.
2. **The shell and terminal stay separate.** `nebula.exe` must continue to work in Windows Terminal, ConHost or another emulator. The graphical terminal is an additional binary, not a runtime dependency of the shell.
3. **Native execution stays native.** The shell's normal command path must not regress to CMD or PowerShell just because it is hosted in the terminal.
4. **A real terminal surface.** Interactive applications, full-screen TUIs, colors, mouse reporting, Unicode, resize events and Ctrl+C must behave like they do in a mature terminal emulator.
5. **Visual polish without visual noise.** The interface should have one coherent design language, strong typography, restrained motion and purposeful depth rather than a dashboard full of cards and effects.
6. **Customization is a product feature.** Most appearance and profile settings should have a clickable editor with live preview. The underlying config remains text-based and portable.
7. **Fast by default.** Effects are optional, terminal rendering is GPU accelerated where available, expensive UI is lazy-loaded and the terminal surface is never blocked by settings or decorative animation.
8. **Safe IPC boundary.** The webview does not receive a generic unrestricted process-spawn API. PTY/process creation and profile validation live in Rust.

## Chosen desktop stack

### Host: Tauri 2

Tauri keeps the desktop host and privileged process logic in Rust while allowing a modern frontend for the product UI. Nebula can use a custom titlebar, transparent window configuration, native window controls and platform-specific effects without replacing the shell core.

Windows 11 should prefer Mica when available. Acrylic/blur can remain optional fallbacks because resize and drag performance varies by Windows version and graphics stack. A solid background fallback is mandatory.

### UI: React + TypeScript + Vite

The UI is an application/editor surface rather than a static page. React gives us strong composition for tabs, settings, profile editors, command palettes and split-pane state while keeping the terminal renderer isolated from normal React re-renders.

The design system should be built from semantic tokens rather than hard-coded colors. Accessible primitives may use Radix/shadcn-style source components, but Nebula must not ship with an identifiable default shadcn appearance. Components are primitives; the visual language belongs to Nebula.

### Terminal renderer: xterm.js

Use `@xterm/xterm` as the terminal viewport. It is already proven in products such as VS Code, Tabby and Hyper and supports the terminal behavior Nebula needs.

Initial addon set:

- WebGL renderer, with DOM/canvas fallback if initialization fails
- Fit for reliable rows/columns after resize
- Search
- Web links
- Clipboard integration
- Ligatures as an optional preference
- Unicode/grapheme support when stable enough for the selected xterm release

The renderer should be wrapped behind a small `TerminalViewport` component so the rest of the UI never depends directly on xterm internals.

### PTY/process layer: Rust

The desktop backend owns pseudo-terminal sessions. On Windows this means ConPTY. Start with a maintained Rust PTY abstraction such as `portable-pty`, then isolate it behind Nebula's own `PtySession` trait so the dependency can be replaced later without rewriting the UI.

A PTY session exposes only the operations the frontend needs:

- create session from a validated profile
- write input bytes
- receive output bytes
- resize rows/columns
- send interrupt/control events
- report exit status
- terminate session

No generic JavaScript-side process spawning is required.

## Repository direction

The repository should evolve toward a Cargo workspace without breaking the existing CLI release:

```text
nebula-shell/
  crates/
    nebula-core/          reusable shell/config/i18n logic
    nebula-pty/           PTY/session abstraction
  apps/
    nebula-cli/           standalone nebula.exe
    nebula-terminal/      Tauri desktop application
      src/                React/TypeScript UI
      src-tauri/          native desktop host
  locales/
  docs/
```

This migration should be incremental. The current root package should not be reorganized until tests cover the behavior being moved.

The first terminal prototype may launch the existing `nebula.exe` inside a PTY rather than linking the shell engine in-process. That gives correct interactive semantics early and keeps the CLI independently testable. Once the boundaries are stable, shared configuration, localization and profile metadata can move into `nebula-core`.

## Window and application chrome

Nebula Terminal should own its complete application chrome.

### Titlebar

Use a custom, compact titlebar integrated with the tab strip. It should contain:

- Nebula mark and optional workspace/profile indicator
- draggable empty region
- tab strip with title, shell/profile icon and dirty/activity state only when meaningful
- new-tab action
- compact overflow/profile menu
- native-feeling minimize, maximize/restore and close controls

Avoid making the titlebar a second toolbar. Terminal actions that are rarely used belong in the command palette or context menus.

### Tabs and panes

Tabs are first-class sessions. A tab can contain one or more split panes. Splits should support horizontal/vertical layout, keyboard focus movement and drag resizing.

Each pane keeps independent:

- PTY session
- profile
- cwd/title
- font zoom override if the user chooses per-pane zoom
- search state
- scrollback position

Closing a pane with a live foreground process should follow a configurable confirmation policy.

### Status surface

Keep the bottom area optional and minimal. Useful information can include current profile, admin/elevated state, cwd/Git context when supplied by Nebula, encoding or connection state. Do not duplicate prompt information by default.

## Clickable customization

The settings experience is part of the reason to use Nebula Terminal instead of a generic emulator. Every appearance change should preview immediately and write back to a versioned config model.

### Appearance

Support at minimum:

- light, dark and system application theme
- terminal color palette
- accent color
- font family, fallback list, size, weight and line height
- font ligatures
- cursor shape, thickness and blink
- terminal padding
- background opacity
- Mica/solid background mode on Windows
- optional background image with fit, position, opacity and blur controls
- tab density and compact mode
- corner radius and separator intensity where supported by the window design
- animation level: full, reduced, off

A theme is a portable object that can be exported/imported without containing machine-specific paths or secrets.

### Prompt editor

Nebula Shell's prompt should gain a visual editor later. The first version can expose the existing template and booleans in a friendly settings form. A later version can use a reorderable prompt composer for status, identity, cwd, Git, duration and exit-code segments.

Changes must still serialize to the normal Nebula config so the same prompt works when `nebula.exe` is launched outside Nebula Terminal.

### Profiles

Profiles should cover Nebula, CMD, Windows PowerShell, PowerShell 7, WSL distributions, SSH presets and custom executables.

A profile editor should expose executable, arguments, starting directory, environment overrides, icon, color and whether the profile may request elevation. Secrets such as SSH private-key contents are never copied into the theme/profile export.

### Keybindings

Provide a searchable keybinding editor with conflict detection. Keyboard actions should include tab/pane operations, command palette, search, copy/paste, font zoom, settings, profile launch and focus movement.

## Primary interaction surfaces

### Command palette

`Ctrl+Shift+P` opens a fast searchable command palette. It replaces a large persistent toolbar for infrequent actions. Commands include profile creation, split actions, theme switching, settings navigation, reload config, copy path, clear buffer and developer diagnostics.

### Context menu

Right-click behavior should be terminal-aware. Selection context prioritizes copy/search; empty terminal context prioritizes paste, split, new tab and profile actions. Do not overload the menu with settings that belong elsewhere.

### Settings

Settings should open as a dedicated app surface rather than a tiny modal. The terminal can remain visible behind it or in a preview pane. Appearance changes should be visible without pressing Save; persistence can still be explicit for destructive profile changes.

### First-run experience

Keep onboarding short. Detect available shells, select Nebula as default, show the essential shortcuts and let the user enter the terminal immediately. Do not force account creation or a long wizard.

## Design language

Nebula should look recognizable without becoming a neon gaming terminal.

Direction:

- dark-first but excellent in light mode
- near-black neutral base rather than blue-black everywhere
- one restrained accent family chosen by the user
- subtle Mica depth on Windows 11
- crisp 1px separators instead of nested bordered cards
- compact chrome and generous terminal breathing room
- carefully tuned monospace typography paired with a neutral UI sans
- soft hover/focus transitions around 120–180 ms
- motion for tab creation, pane focus and settings transitions only where it improves orientation
- no permanent glow, rainbow gradients or animated backgrounds by default

Themes may be expressive, but the default theme should feel credible for daily professional use.

## Design-system tokens

The frontend should define semantic tokens for:

- app/window background
- terminal background
- elevated surface
- hover/active surface
- primary/secondary text
- muted text
- separators
- accent and accent-foreground
- success/warning/error/info
- focus ring
- terminal ANSI palette
- radii
- spacing scale
- typography scale
- motion durations/easing

UI components consume semantic tokens only. Theme import/export maps into these tokens instead of overriding arbitrary CSS selectors.

## Performance rules

The terminal must remain responsive even when the surrounding React application is busy.

- terminal output writes directly to the xterm instance instead of entering global React state
- batch backend output events to avoid IPC overhead for every tiny chunk
- keep scrollback bounded and configurable
- use WebGL when available and recover cleanly from context loss
- lazy-load settings, theme gallery and non-terminal routes
- virtualize long profile/theme/keybinding lists if they become large
- avoid continuous backdrop filters over the terminal viewport when they cause measurable resize/render regressions
- animations never run on every terminal output frame

A smooth terminal under heavy output is more important than decorative effects.

## Security rules

- frontend IPC commands are explicit and capability-scoped
- no unrestricted `exec(command: string)` Tauri command exposed to the webview
- profile creation is data validation, not string concatenation into a host shell
- environment values and arguments are passed as structured arrays/maps
- elevated sessions are visibly marked
- links detected in terminal output require the normal user click; never auto-open
- pasted multiline commands can optionally show a paste-protection confirmation
- config imports validate schema/version before writing
- theme imports cannot execute code or reference remote scripts
- terminal webview content never renders arbitrary HTML from process output

## Accessibility

- keyboard navigation must cover every app control
- visible focus states are mandatory
- all icon-only buttons receive accessible names/tooltips
- terminal contrast follows xterm accessibility options where practical
- respect Windows reduced-motion and application `prefers-reduced-motion`
- do not encode shell/admin/error state using color alone
- UI zoom and terminal font zoom are separate controls

## Delivery phases

### Phase A — design and host shell

Create the complete desktop visual concept first, including the main terminal screen, settings/appearance screen, profile editor, command palette and split-pane state. Build the design system from the accepted concept before implementing UI.

Then scaffold Tauri + React/TypeScript and reproduce the approved application chrome with a fake/static terminal viewport. This phase proves the visual system and native window behavior without mixing PTY debugging into design work.

### Phase B — real PTY terminal

Integrate xterm.js and the Rust PTY service. Launch Nebula as the default profile and validate interactive programs, Unicode, resize, Ctrl+C, copy/paste, search, heavy output and window resizing.

### Phase C — product workflows

Add tabs, splits, profile management, settings persistence, theme import/export, command palette and keybinding editor.

### Phase D — deeper Nebula integration

Expose structured events from Nebula Shell for cwd, Git/status, elevation and other metadata so the graphical chrome can react without scraping terminal text. Move stable shared code into reusable crates.

### Phase E — distribution quality

Add icon/version resources, signed installer strategy, auto-update design, crash-safe config migration, release channels, ARM64 build and package-manager distribution.

## Visual implementation workflow

For the graphical application, follow a concept-first workflow:

1. Generate the full primary screen and important states before coding.
2. Extract design tokens, typography, icons, spacing, component families and interaction rules from the accepted concept.
3. Implement application chrome from those tokens rather than ad-hoc CSS.
4. Verify the rendered app against the concept at matching viewport dimensions.
5. Fix visual drift before adding the next feature surface.
6. Re-run visual QA for desktop scaling, reduced motion, light/dark themes and at least one compact window size.

The UI is not considered complete because it compiles. It is complete when the rendered product is faithful, readable and responsive under real terminal use.

## Non-goals for the first graphical release

- replacing Windows Explorer or becoming a full IDE
- cloud accounts or mandatory synchronization
- an extension marketplace before the core security model is established
- custom shell scripting language changes required only for the GUI
- shipping every visual effect enabled by default
- sacrificing terminal compatibility for custom rendering tricks

Nebula Terminal should win first by being a fast, polished terminal built around Nebula Shell, while remaining a good host for the command-line tools Windows users already rely on.

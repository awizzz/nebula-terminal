# Roadmap

Nebula is still pre-1.0. The priorities below describe direction rather than fixed release promises.

## Near term

- improve completion so it understands command context instead of only generic words and paths
- make native pipelines stream concurrently instead of buffering each stage in memory
- add richer Git status information without slowing down the prompt
- expand automated tests around native parsing, Windows path handling, aliases and configuration migration
- improve executable metadata and Windows packaging
- publish package-manager manifests after the release format is stable
- harden the Nebula Terminal preview with real-world daily use before a stable desktop line

## Shell runtime

Nebula 0.5 introduces a native execution engine. Ordinary executables are launched directly by Nebula and core shell syntax such as pipelines, redirection and conditional command chains no longer require CMD or PowerShell.

CMD, Windows PowerShell and PowerShell 7 remain compatibility layers for shell-specific syntax, scripts and commands. They can be invoked explicitly from native mode or selected as the session backend.

Runtime work after 0.5 should focus on:

- streaming native pipelines with concurrent child processes
- process groups, interruption and job-control behavior
- richer native expansion and quoting rules
- explicit compatibility boundaries for `.bat`, `.cmd` and PowerShell script execution
- optional persistent ConPTY sessions for users who deliberately select a compatibility backend and need backend-specific state

The native engine should remain independent from ConPTY. A pseudoconsole is useful for compatibility sessions and interactive terminal applications, but it should not become a requirement for normal Nebula command execution.

## Completion and editing

Planned areas include:

- command-aware completion
- Git branch/ref completion
- environment-variable completion
- better quoted-path completion
- interactive history search improvements
- configurable key bindings

## Distribution

Once the release pipeline and command surface are stable:

- ARM64 builds
- Winget package
- Scoop manifest
- richer Windows executable metadata
- signed releases when a trusted code-signing setup is available

## Nebula Terminal

Nebula Shell remains usable independently in any compatible terminal emulator. Nebula Terminal is a separate graphical desktop application that makes Nebula the default experience while still hosting CMD, Windows PowerShell, PowerShell 7 and WSL profiles.

The architecture is documented in [`terminal-ui.md`](terminal-ui.md).

### Implemented preview foundation

- Tauri 2 + React/TypeScript desktop host with custom application chrome
- xterm.js backed by native Rust PTY/ConPTY sessions
- draggable tabs and profile picker
- vertical and horizontal split panes
- drag-resizable panes with restored proportions
- keyboard-driven command palette and previous/next terminal search
- persistent tab/split session restoration
- Solar Noir settings with six original palettes, background images and live terminal typography
- editable keyboard shortcuts
- visual-only theme JSON import/export
- starting directory, configurable scrollback and multiline paste protection
- live session titles, safe path insertion and process restart controls
- incremental UTF-8 decoding across PTY reads
- Windows MSI/NSIS and portable ZIP release workflow
- bundled Nebula Shell resource in Terminal release builds

### Next desktop milestones

- collect feedback from daily use and harden PTY edge cases
- structured Nebula Shell metadata for cwd, Git state, elevation and command status without scraping terminal text
- richer profile editing for custom executables, SSH hosts and WSL distributions
- more flexible split trees and keyboard focus movement between panes
- Windows notifications for completed long-running commands in inactive tabs
- automated update UX once the preview release format has proven stable
- accessibility review, keyboard-only QA and performance profiling on low-power hardware
- ARM64 packaging and package-manager distribution

The terminal must not become a dependency of `nebula.exe`, and the graphical host must not move normal Nebula command execution back through CMD or PowerShell.

## 1.0 criteria

A 1.0 shell release should mean:

- stable configuration format with documented migration behavior
- reliable update/release process
- meaningful automated coverage of the native parser, core built-ins and configuration
- documented compatibility boundaries for CMD and PowerShell
- streaming pipelines and predictable interruption behavior
- no known high-severity dependency advisories
- predictable behavior on supported Windows versions

Nebula Terminal can follow its own versioning maturity until its PTY behavior, settings format and desktop update path are similarly stable.

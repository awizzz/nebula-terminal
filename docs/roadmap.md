# Roadmap

Nebula is still pre-1.0. The priorities below describe direction rather than fixed release promises.

## Near term

- improve completion so it understands command context instead of only generic words and paths
- add richer Git status information without slowing down the prompt
- expand automated tests around Windows path handling, aliases and configuration migration
- improve executable metadata and Windows packaging
- publish package-manager manifests after the release format is stable

## Shell runtime

The current backend model starts a fresh CMD, Windows PowerShell or PowerShell 7 process for each external command. This keeps command compatibility simple but does not preserve backend-specific process state.

A future runtime should investigate a persistent Windows pseudoconsole/ConPTY backend. The goal is to preserve interactive backend state without turning the shell layer into a terminal emulator.

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
- Windows executable icon and version resources
- signed releases when a trusted code-signing setup is available

## Terminal UI

Nebula Shell deliberately remains separate from terminal-emulator concerns such as tabs, panes, blur and GPU rendering.

A future Nebula Terminal could host Nebula and other shells through ConPTY while providing those graphical features. That would be a separate component rather than a dependency of the shell.

## 1.0 criteria

A 1.0 release should mean:

- stable configuration format with documented migration behavior
- reliable update/release process
- meaningful automated coverage of core built-ins and configuration
- documented compatibility boundaries for CMD and PowerShell backends
- no known high-severity dependency advisories
- predictable behavior on supported Windows versions

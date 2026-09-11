# Roadmap

Nebula is still pre-1.0. The priorities below describe direction rather than fixed release promises.

## Near term

- improve completion so it understands command context instead of only generic words and paths
- make native pipelines stream concurrently instead of buffering each stage in memory
- add richer Git status information without slowing down the prompt
- expand automated tests around native parsing, Windows path handling, aliases and configuration migration
- improve executable metadata and Windows packaging
- publish package-manager manifests after the release format is stable

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
- Windows executable icon and version resources
- signed releases when a trusted code-signing setup is available

## Terminal UI

Nebula Shell deliberately remains separate from terminal-emulator concerns such as tabs, panes, blur and GPU rendering.

A future Nebula Terminal could host Nebula and other shells through ConPTY while providing those graphical features. That would be a separate component rather than a dependency of the shell.

## 1.0 criteria

A 1.0 release should mean:

- stable configuration format with documented migration behavior
- reliable update/release process
- meaningful automated coverage of the native parser, core built-ins and configuration
- documented compatibility boundaries for CMD and PowerShell
- streaming pipelines and predictable interruption behavior
- no known high-severity dependency advisories
- predictable behavior on supported Windows versions

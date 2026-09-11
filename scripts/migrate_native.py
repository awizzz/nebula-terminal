from pathlib import Path


def replace(path, old, new, count=None):
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    if old not in text:
        raise SystemExit(f"Expected text not found in {path}: {old[:100]!r}")
    text = text.replace(old, new) if count is None else text.replace(old, new, count)
    p.write_text(text, encoding="utf-8", newline="\n")


replace("Cargo.toml", 'version = "0.4.0"', 'version = "0.5.0"', 1)
replace(
    "Cargo.lock",
    'name = "nebula-shell"\nversion = "0.4.0"',
    'name = "nebula-shell"\nversion = "0.5.0"',
    1,
)

# Main process entry point and private elevated one-shot mode.
p = Path("src/main.rs")
text = p.read_text(encoding="utf-8")
text = text.replace("mod i18n;\nmod platform;", "mod i18n;\nmod native;\nmod platform;", 1)
old = '''fn validate_cli_args(args: &[String]) -> Result<bool, String> {
    match args {
        [] => Ok(false),
        [arg] if matches!(arg.as_str(), "-h" | "--help") => {
            print_cli_help();
            Ok(true)
        }
        [arg] if matches!(arg.as_str(), "-V" | "--version") => {
            println!("{APP_NAME} {VERSION}");
            Ok(true)
        }
        [arg] if matches!(arg.as_str(), "--admin" | "--elevated-child") => Ok(false),
        [arg] => Err(format!("unknown option: {arg}")),
        _ => Err("Nebula accepts at most one startup option".into()),
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match validate_cli_args(&args) {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("Nebula: {error}");
            eprintln!("Try 'nebula --help' for usage information.");
            process::exit(2);
        }
    }

    if let Err(error) = shell::run() {
        eprintln!("Nebula: {error}");
        process::exit(1);
    }
}'''
new = '''enum CliAction {
    Interactive,
    Exit,
    ElevatedRun(String),
}

fn decode_hex(value: &str) -> Result<String, String> {
    if value.len() % 2 != 0 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid internal command payload".into());
    }
    let bytes = (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "invalid internal command payload".to_string())?;
    String::from_utf8(bytes).map_err(|_| "invalid internal command payload".into())
}

fn validate_cli_args(args: &[String]) -> Result<CliAction, String> {
    match args {
        [] => Ok(CliAction::Interactive),
        [arg] if matches!(arg.as_str(), "-h" | "--help") => {
            print_cli_help();
            Ok(CliAction::Exit)
        }
        [arg] if matches!(arg.as_str(), "-V" | "--version") => {
            println!("{APP_NAME} {VERSION}");
            Ok(CliAction::Exit)
        }
        [arg] if matches!(arg.as_str(), "--admin" | "--elevated-child") => {
            Ok(CliAction::Interactive)
        }
        [flag, payload] if flag == "--elevated-run" => {
            Ok(CliAction::ElevatedRun(decode_hex(payload)?))
        }
        [arg] => Err(format!("unknown option: {arg}")),
        _ => Err("invalid startup arguments".into()),
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let action = match validate_cli_args(&args) {
        Ok(action) => action,
        Err(error) => {
            eprintln!("Nebula: {error}");
            eprintln!("Try 'nebula --help' for usage information.");
            process::exit(2);
        }
    };

    let result = match action {
        CliAction::Exit => return,
        CliAction::Interactive => shell::run().map(|_| 0),
        CliAction::ElevatedRun(command) => shell::run_once(&command),
    };

    match result {
        Ok(code) => process::exit(code),
        Err(error) => {
            eprintln!("Nebula: {error}");
            process::exit(1);
        }
    }
}'''
if old not in text:
    raise SystemExit("main.rs CLI block did not match")
text = text.replace(old, new, 1)
start = text.index("#[cfg(test)]\nmod tests {")
text = text[:start] + '''#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_mode_accepts_no_arguments() {
        assert!(matches!(validate_cli_args(&[]), Ok(CliAction::Interactive)));
    }

    #[test]
    fn admin_modes_are_forwarded_to_the_shell() {
        assert!(matches!(validate_cli_args(&["--admin".into()]), Ok(CliAction::Interactive)));
        assert!(matches!(
            validate_cli_args(&["--elevated-child".into()]),
            Ok(CliAction::Interactive)
        ));
    }

    #[test]
    fn elevated_payload_is_decoded() {
        match validate_cli_args(&["--elevated-run".into(), "6563686f206869".into()]).unwrap() {
            CliAction::ElevatedRun(command) => assert_eq!(command, "echo hi"),
            _ => panic!("unexpected CLI action"),
        }
    }

    #[test]
    fn invalid_options_are_rejected() {
        assert!(validate_cli_args(&["--wat".into()]).is_err());
        assert!(validate_cli_args(&["--elevated-run".into(), "xyz".into()]).is_err());
    }
}
'''
p.write_text(text, encoding="utf-8", newline="\n")

# Native backend defaults.
replace("src/config.rs", 'backend: "cmd".into(),', 'backend: "native".into(),', 1)
replace(
    "src/config.rs",
    '"cmd" | "powershell" | "pwsh"',
    '"native" | "cmd" | "powershell" | "pwsh"',
    1,
)
replace(
    "config.example.toml",
    '# Supported command backends: cmd, powershell, pwsh\nbackend = "cmd"',
    '# Supported command backends: native, cmd, powershell, pwsh\nbackend = "native"',
    1,
)

# Shell runtime integration.
replace(
    "src/shell.rs",
    "    i18n::Translator,\n    platform, ui,",
    "    i18n::Translator,\n    native, platform, ui,",
    1,
)
replace(
    "src/shell.rs",
    '''    let executable = match requested.as_str() {
        "cmd" => "cmd.exe",
        "powershell" => "powershell.exe",
        "pwsh" => "pwsh.exe",
        _ => {''',
    '''    let executable = match requested.as_str() {
        "native" => None,
        "cmd" => Some("cmd.exe"),
        "powershell" => Some("powershell.exe"),
        "pwsh" => Some("pwsh.exe"),
        _ => {''',
    1,
)
replace(
    "src/shell.rs",
    '''    if !platform::command_exists(executable) {
        println!(
            "{}",
            ui::paint(
                &config.theme.error,
                &translator.value("msg.backend_unavailable", requested),
                false,
            )
        );
        state.last_code = 1;
        return LoopAction::Continue;
    }''',
    '''    if executable.is_some_and(|program| !platform::command_exists(program)) {
        println!(
            "{}",
            ui::paint(
                &config.theme.error,
                &translator.value("msg.backend_unavailable", requested),
                false,
            )
        );
        state.last_code = 1;
        return LoopAction::Continue;
    }''',
    1,
)
replace(
    "src/shell.rs",
    '''    let backend_exe = match config.general.backend.as_str() {
        "powershell" => "powershell.exe",
        "pwsh" => "pwsh.exe",
        _ => "cmd.exe",
    };
    let backend_ok = platform::command_exists(backend_exe);
    print_check(
        backend_ok,
        &translator.text("doctor.backend"),
        backend_exe,
        config,
        translator,
    );''',
    '''    let (backend_ok, backend_detail) = match config.general.backend.as_str() {
        "native" => (true, "Nebula native engine"),
        "powershell" => (platform::command_exists("powershell.exe"), "powershell.exe"),
        "pwsh" => (platform::command_exists("pwsh.exe"), "pwsh.exe"),
        _ => (platform::command_exists("cmd.exe"), "cmd.exe"),
    };
    print_check(
        backend_ok,
        &translator.text("doctor.backend"),
        backend_detail,
        config,
        translator,
    );''',
    1,
)
replace(
    "src/shell.rs",
    '''    let status = match backend.as_str() {
        "cmd" => Command::new("cmd.exe")''',
    '''    if backend == "native" {
        return match native::execute(line, cwd) {
            Ok(code) => code,
            Err(error) => {
                eprintln!("{}", translator.value("msg.native_error", error));
                1
            }
        };
    }

    let status = match backend.as_str() {
        "cmd" => Command::new("cmd.exe")''',
    1,
)
replace(
    "src/shell.rs",
    '''        _ => {
            eprintln!(
                "{}",
                translator.value("msg.backend_unknown", &config.general.backend)
            );
            Command::new("cmd.exe")
                .args(["/d", "/s", "/c", line])
                .current_dir(cwd)
                .status()
        }''',
    '''        _ => {
            eprintln!(
                "{}",
                translator.value("msg.backend_unknown", &config.general.backend)
            );
            return match native::execute(line, cwd) {
                Ok(code) => code,
                Err(error) => {
                    eprintln!("{}", translator.value("msg.native_error", error));
                    1
                }
            };
        }''',
    1,
)
marker = "fn execute_external(line: &str, cwd: &Path, config: &Config, translator: &Translator) -> i32 {"
replace(
    "src/shell.rs",
    marker,
    '''pub fn run_once(command: &str) -> Result<i32, String> {
    let mut config = Config::load().unwrap_or_default();
    config.apply_environment();
    let mut translator = Translator::from_config(&config);
    let mut state = ShellState::new();
    let action = dispatch(command, &mut state, &mut config, &mut translator);
    if matches!(action, LoopAction::Exit) {
        return Ok(0);
    }
    Ok(state.last_code)
}

''' + marker,
    1,
)

# Native sudo launches Nebula itself through UAC.
p = Path("src/platform.rs")
text = p.read_text(encoding="utf-8")
old = '''        let (program, parameters) = match backend.to_ascii_lowercase().as_str() {
            "powershell" => {'''
new = '''        let (program, parameters) = match backend.to_ascii_lowercase().as_str() {
            "native" => {
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                let payload = command
                    .as_bytes()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                (
                    exe.to_string_lossy().into_owned(),
                    format!("--elevated-run {payload}"),
                )
            }
            "powershell" => {'''
if old not in text:
    raise SystemExit("platform elevation block did not match")
text = text.replace(old, new, 1)
if "        let file = wide_str(program);\n" not in text:
    raise SystemExit("platform file encoding block did not match")
text = text.replace("        let file = wide_str(program);\n", "        let file = wide_str(&program);\n", 1)
p.write_text(text, encoding="utf-8", newline="\n")

# Completion list.
p = Path("src/editor.rs")
text = p.read_text(encoding="utf-8")
for old, new in [
    ('        "backend",\n        "cd",', '        "backend",\n        "cat",\n        "cd",'),
    ('        "config",\n        "dir",', '        "config",\n        "copy",\n        "cp",\n        "dir",'),
    ('        "language",\n        "netsh",', '        "language",\n        "ls",\n        "md",\n        "mkdir",\n        "move",\n        "mv",\n        "netsh",'),
    ('        "powershell",\n        "pushd",', '        "powershell",\n        "pushd",\n        "pwsh",'),
    ('        "theme",\n        "ui",', '        "theme",\n        "touch",\n        "type",\n        "ui",'),
    ('        "where",\n        "winget",', '        "where",\n        "which",\n        "winget",'),
]:
    if old not in text:
        raise SystemExit(f"editor completion block not found: {old!r}")
    text = text.replace(old, new, 1)
p.write_text(text, encoding="utf-8", newline="\n")

# Locale strings; parity is tested by the existing suite.
for locale, replacements in {
    "locales/en-US.toml": [
        (
            '"banner.hint" = "Type help for Nebula commands. Other commands are passed to your configured backend."',
            '"banner.hint" = "Type help for Nebula commands. Native mode runs programs directly; CMD and PowerShell remain available when needed."',
        ),
        (
            '"msg.backend_unknown" = "Unknown backend \'{value}\', falling back to cmd."',
            '"msg.backend_unknown" = "Unknown backend \'{value}\', falling back to the native engine."',
        ),
        (
            '"msg.backend_unavailable" = "Backend is not installed or unavailable: {value}"',
            '"msg.backend_unavailable" = "Backend is not installed or unavailable: {value}"\n"msg.native_error" = "Native command error: {value}"',
        ),
        (
            '"help.backend" = "backend [name]        Show or change cmd / powershell / pwsh"',
            '"help.backend" = "backend [name]        Show or change native / cmd / powershell / pwsh"',
        ),
        (
            '"help.external" = "Everything else is executed by the configured shell backend, including pipes, redirects, .bat/.cmd files, Git, Python, PowerShell, SSH and Windows utilities."',
            '"help.external" = "Native mode launches programs directly and supports pipes, redirects, &&, || and ;. Use cmd <command>, powershell <command> or pwsh <command> for shell-specific syntax and scripts."',
        ),
    ],
    "locales/fr-FR.toml": [
        (
            '"banner.hint" = "Tape help pour les commandes Nebula. Les autres commandes sont envoyées au backend configuré."',
            '"banner.hint" = "Tape help pour les commandes Nebula. Le mode natif lance directement les programmes ; CMD et PowerShell restent disponibles si besoin."',
        ),
        (
            '"msg.backend_unknown" = "Backend \'{value}\' inconnu, utilisation de cmd."',
            '"msg.backend_unknown" = "Backend \'{value}\' inconnu, utilisation du moteur natif."',
        ),
        (
            '"msg.backend_unavailable" = "Le backend n\'est pas installé ou disponible : {value}"',
            '"msg.backend_unavailable" = "Le backend n\'est pas installé ou disponible : {value}"\n"msg.native_error" = "Erreur de commande native : {value}"',
        ),
        (
            '"help.backend" = "backend [nom]         Affiche ou change cmd / powershell / pwsh"',
            '"help.backend" = "backend [nom]         Affiche ou change native / cmd / powershell / pwsh"',
        ),
        (
            '"help.external" = "Tout le reste est exécuté par le backend configuré : pipes, redirections, fichiers .bat/.cmd, Git, Python, PowerShell, SSH et outils Windows compris."',
            '"help.external" = "Le mode natif lance directement les programmes et gère pipes, redirections, &&, || et ;. Utilise cmd <commande>, powershell <commande> ou pwsh <commande> pour la syntaxe propre à ces shells."',
        ),
    ],
}.items():
    p = Path(locale)
    text = p.read_text(encoding="utf-8")
    for old, new in replacements:
        if old not in text:
            raise SystemExit(f"locale text not found in {locale}: {old}")
        text = text.replace(old, new, 1)
    p.write_text(text, encoding="utf-8", newline="\n")

# Native parser improvements and safe compatibility command construction.
p = Path("src/native.rs")
text = p.read_text(encoding="utf-8")
text = text.replace("    Or,\n}", "    Or,\n    Sequence,\n}", 1)
text = text.replace("    Or,\n    In,", "    Or,\n    Sequence,\n    In,", 1)
text = text.replace("            Token::And | Token::Or => {", "            Token::And | Token::Or | Token::Sequence => {", 1)
text = text.replace(
    '''                gate = Some(if matches!(tokens[index], Token::And) {
                    Connector::And
                } else {
                    Connector::Or
                });''',
    '''                gate = match tokens[index] {
                    Token::And => Some(Connector::And),
                    Token::Or => Some(Connector::Or),
                    Token::Sequence => None,
                    _ => unreachable!(),
                };''',
    1,
)
text = text.replace(
    "            '|' => {\n                flush_word(&mut tokens, &mut word);\n                tokens.push(Token::Pipe);\n            }",
    "            '|' => {\n                flush_word(&mut tokens, &mut word);\n                tokens.push(Token::Pipe);\n            }\n            ';' => {\n                flush_word(&mut tokens, &mut word);\n                tokens.push(Token::Sequence);\n            }",
    1,
)
old = '''        "cmd" => {
            let mut command = Command::new("cmd.exe");
            if argv.len() > 1 {
                command.args(["/d", "/s", "/c", &join_command(&argv[1..])]);
            }
            Some(command)
        }
        "powershell" => {
            let mut command = Command::new("powershell.exe");
            if argv.len() > 1 {
                command.args(["-NoLogo", "-NoProfile", "-Command", &join_command(&argv[1..])]);
            }
            Some(command)
        }
        "pwsh" => {
            let mut command = Command::new("pwsh.exe");
            if argv.len() > 1 {
                command.args(["-NoLogo", "-NoProfile", "-Command", &join_command(&argv[1..])]);
            }
            Some(command)
        }'''
new = '''        "cmd" => {
            let mut command = Command::new("cmd.exe");
            if argv.len() > 1 {
                let joined = join_command(&argv[1..]);
                command.args(["/d", "/s", "/c", joined.as_str()]);
            }
            Some(command)
        }
        "powershell" => {
            let mut command = Command::new("powershell.exe");
            if argv.len() > 1 {
                let joined = join_command(&argv[1..]);
                command.args(["-NoLogo", "-NoProfile", "-Command", joined.as_str()]);
            }
            Some(command)
        }
        "pwsh" => {
            let mut command = Command::new("pwsh.exe");
            if argv.len() > 1 {
                let joined = join_command(&argv[1..]);
                command.args(["-NoLogo", "-NoProfile", "-Command", joined.as_str()]);
            }
            Some(command)
        }'''
if old not in text:
    raise SystemExit("native compatibility block did not match")
text = text.replace(old, new, 1)
p.write_text(text, encoding="utf-8", newline="\n")

# Match the protected-branch required check name.
replace(
    ".github/workflows/ci.yml",
    "  windows:\n    runs-on:",
    "  windows:\n    name: CI / windows\n    runs-on:",
    1,
)

# Public docs and release notes.
p = Path("CHANGELOG.md")
text = p.read_text(encoding="utf-8")
entry = '''## 0.5.0 — 2026-09-11

### Added

- native command engine that launches executables without routing through CMD or PowerShell
- native parsing for `&&`, `||`, `;`, pipelines and standard input/output/error redirection
- native built-ins for `echo`, `dir`/`ls`, `type`/`cat`, `mkdir`, `touch`, `where`/`which`, `copy`/`cp` and `move`/`mv`
- explicit `cmd <command>`, `powershell <command>` and `pwsh <command>` compatibility from native mode
- native administrator elevation for `sudo` without using CMD as an intermediary

### Changed

- `native` is now the default command backend
- CMD, Windows PowerShell and PowerShell 7 remain selectable compatibility backends
- repository references now use `awizzz/nebula-shell`

### Notes

- `.bat` and `.cmd` scripts require CMD compatibility (`cmd script.cmd` or `backend cmd`)
- native pipelines are currently buffered between stages rather than streamed concurrently

'''
text = text.replace(
    "Notable user-facing changes are listed here.\n\n",
    "Notable user-facing changes are listed here.\n\n" + entry,
    1,
)
p.write_text(text, encoding="utf-8", newline="\n")

p = Path("README.md")
text = p.read_text(encoding="utf-8")
text = text.replace(
    "Nebula is a native Windows shell frontend written in Rust. It keeps normal Windows command execution available while adding a modern prompt, persistent shell state, history controls, completion, themes, localization and UAC helpers.",
    "Nebula is a native Windows shell written in Rust. Its default execution engine launches programs directly without routing every command through CMD or PowerShell, while keeping both shells available for compatibility when their syntax is needed.",
)
text = text.replace(
    "- normal CMD command execution, pipes, redirects and `.bat` / `.cmd` files\n- optional Windows PowerShell and PowerShell 7 backends",
    "- native executable launch without a CMD or PowerShell intermediary\n- native `&&`, `||`, `;`, pipelines and input/output/error redirection\n- native filesystem and utility built-ins (`ls`, `cat`, `mkdir`, `touch`, `which`, `cp`, `mv` and Windows aliases)\n- explicit CMD, Windows PowerShell and PowerShell 7 compatibility modes",
)
text = text.replace("backend pwsh\nalias gs=git status", "backend native\nalias gs=git status")
old = '''### Command backends

```text
backend cmd
backend powershell
backend pwsh
```

Nebula currently launches each external command through a fresh backend process. Backend-specific process state such as PowerShell variables, functions and imported modules therefore does not persist between separate commands. Nebula-owned state such as the working directory, aliases and environment variables does persist.'''
new = '''### Command execution

Native mode is the default:

```text
backend native
git status
python app.py
ping 1.1.1.1
dir | findstr src
```

Programs are launched directly by Nebula. CMD and PowerShell are still available when you need shell-specific commands or syntax:

```text
cmd dir /b
powershell Get-Process
pwsh Get-ChildItem
```

You can also delegate the whole session command path to a compatibility backend:

```text
backend cmd
backend powershell
backend pwsh
```

Compatibility backends still use a fresh shell process for each command, so backend-specific variables, functions and imported modules do not persist between separate commands. Nebula-owned state does persist.'''
if old not in text:
    raise SystemExit("README backend section did not match")
text = text.replace(old, new, 1)
text = text.replace(
    "- backend-specific PowerShell state is not persistent between commands\n- completion is generic rather than command-aware",
    "- native pipelines are currently buffered between stages rather than streamed concurrently\n- `.bat` / `.cmd` scripts require explicit CMD compatibility\n- compatibility-backend PowerShell state is not persistent between commands\n- completion is generic rather than command-aware",
)
p.write_text(text, encoding="utf-8", newline="\n")

p = Path("docs/architecture.md")
text = p.read_text(encoding="utf-8")
text += '''\n\n## Native execution engine\n\nStarting with 0.5.0, `native` is the default backend. Nebula parses command chains itself, launches ordinary executables directly with the Windows process model, and implements core pipelines, conditional execution and redirection without invoking CMD or PowerShell.\n\nCMD, Windows PowerShell and PowerShell 7 are compatibility layers, not runtime dependencies of the native engine. Users can call them explicitly (`cmd <command>`, `powershell <command>`, `pwsh <command>`) or select one as the session backend. Batch files remain a CMD format and therefore require CMD compatibility.\n\nNative pipelines currently buffer one stage before feeding the next. Streaming pipelines and richer job control remain future work.\n'''
p.write_text(text, encoding="utf-8", newline="\n")

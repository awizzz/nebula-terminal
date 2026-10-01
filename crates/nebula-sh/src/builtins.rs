//! Commands that run inside the shell process.

use crate::commands;
use crate::exec::{Resolution, Shell};
use crate::style::{self, Paint};
use crate::sys;
use std::env;
use std::io::Write;
use std::path::PathBuf;

pub fn run(shell: &mut Shell, argv: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let args = &argv[1..];
    let result = match argv[0].as_str() {
        "cd" => cd(shell, args, out),
        "pushd" => pushd(shell, args, out),
        "popd" => popd(shell, out),
        "dirs" => dirs(shell, out),
        "exit" => exit(shell, args),
        "export" => export(args, out),
        "unset" => {
            for name in args {
                env::remove_var(name);
            }
            Ok(0)
        }
        "alias" => alias(shell, args, out),
        "unalias" => unalias(shell, args),
        "history" => history(shell, args, out),
        "source" | "." => source(shell, args),
        "type" => describe(shell, args, out, false),
        "which" => describe(shell, args, out, true),
        "clear" => {
            let _ = write!(out, "\x1b[H\x1b[2J\x1b[3J");
            Ok(0)
        }
        "help" => help(out),
        other => Err(format!("{other}: not a builtin")),
    };
    match result {
        Ok(code) => code,
        Err(message) => {
            let _ = writeln!(err, "{}: {message}", argv[0]);
            1
        }
    }
}

type Outcome = Result<i32, String>;

fn resolve_dir(target: &str) -> PathBuf {
    let translated = sys::translate_path(target);
    if translated == "~" {
        return sys::home().unwrap_or_default();
    }
    if let Some(rest) = translated
        .strip_prefix("~/")
        .or_else(|| translated.strip_prefix("~\\"))
    {
        return sys::home().unwrap_or_default().join(rest);
    }
    PathBuf::from(translated)
}

fn cd(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> Outcome {
    let target = match args.first().map(String::as_str) {
        None | Some("~") => sys::home().ok_or("HOME is not set")?,
        Some("-") => {
            let previous = env::var_os("OLDPWD").ok_or("OLDPWD is not set")?;
            let path = PathBuf::from(previous);
            let _ = writeln!(out, "{}", sys::display_path(&path));
            path
        }
        Some(target) => resolve_dir(target),
    };
    if args.len() > 1 {
        return Err("too many arguments".into());
    }
    shell
        .change_dir(&target)
        .map_err(|error| format!("{}: {error}", args.first().map_or("~", String::as_str)))?;
    Ok(0)
}

fn pushd(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> Outcome {
    let current = env::current_dir().map_err(|error| error.to_string())?;
    let target = match args.first() {
        Some(target) => resolve_dir(target),
        None => shell.dir_stack().pop().ok_or("no other directory")?,
    };
    shell
        .change_dir(&target)
        .map_err(|error| format!("{}: {error}", target.display()))?;
    shell.dir_stack().push(current);
    dirs(shell, out)
}

fn popd(shell: &mut Shell, out: &mut dyn Write) -> Outcome {
    let target = shell.dir_stack().pop().ok_or("directory stack empty")?;
    shell
        .change_dir(&target)
        .map_err(|error| format!("{}: {error}", target.display()))?;
    dirs(shell, out)
}

fn dirs(shell: &mut Shell, out: &mut dyn Write) -> Outcome {
    let mut entries = vec![sys::display_path(&env::current_dir().unwrap_or_default())];
    entries.extend(
        shell
            .dir_stack()
            .iter()
            .rev()
            .map(|path| sys::display_path(path)),
    );
    let _ = writeln!(out, "{}", entries.join(" "));
    Ok(0)
}

fn exit(shell: &mut Shell, args: &[String]) -> Outcome {
    let code = match args.first() {
        Some(value) => value
            .parse::<i32>()
            .map_err(|_| format!("{value}: numeric argument required"))?,
        None => shell.last_status,
    };
    shell.exit_code = Some(code);
    Ok(code)
}

fn export(args: &[String], out: &mut dyn Write) -> Outcome {
    if args.is_empty() || args == ["-p"] {
        let mut vars: Vec<(String, String)> = env::vars().collect();
        vars.sort_by_key(|(name, _)| name.to_lowercase());
        for (name, value) in vars {
            let _ = writeln!(out, "declare -x {name}=\"{}\"", value.replace('"', "\\\""));
        }
        return Ok(0);
    }
    for arg in args {
        if let Some((name, value)) = arg.split_once('=') {
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(format!("`{arg}`: not a valid identifier"));
            }
            env::set_var(name, value);
        }
    }
    Ok(0)
}

fn alias(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> Outcome {
    if args.is_empty() {
        for (name, value) in &shell.aliases {
            let _ = writeln!(out, "alias {name}={}", crate::parse::quote(value));
        }
        return Ok(0);
    }
    let mut status = 0;
    for arg in args {
        match arg.split_once('=') {
            Some((name, value)) if !name.is_empty() => {
                shell.aliases.insert(name.to_owned(), value.to_owned());
            }
            _ => match shell.aliases.get(arg) {
                Some(value) => {
                    let _ = writeln!(out, "alias {arg}={}", crate::parse::quote(value));
                }
                None => {
                    status = 1;
                    let _ = writeln!(out, "alias: {arg}: not found");
                }
            },
        }
    }
    Ok(status)
}

fn unalias(shell: &mut Shell, args: &[String]) -> Outcome {
    if args.first().map(String::as_str) == Some("-a") {
        shell.aliases.clear();
        return Ok(0);
    }
    for name in args {
        if shell.aliases.remove(name).is_none() {
            return Err(format!("{name}: not found"));
        }
    }
    Ok(0)
}

fn history(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> Outcome {
    if args.first().map(String::as_str) == Some("-c") {
        shell.history.clear();
        return Ok(0);
    }
    let count = args
        .first()
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(shell.history.len());
    let start = shell.history.len().saturating_sub(count);
    let width = shell.history.len().to_string().len();
    for (index, line) in shell.history.iter().enumerate().skip(start) {
        let _ = writeln!(out, "  {}  {line}", format!("{:>width$}", index + 1).dim());
    }
    Ok(0)
}

fn source(shell: &mut Shell, args: &[String]) -> Outcome {
    let path = args.first().ok_or("filename argument required")?;
    let text = std::fs::read_to_string(resolve_dir(path))
        .map_err(|error| format!("{path}: {}", crate::exec::describe_io_error(&error)))?;
    Ok(shell.run_line(&text))
}

fn describe(shell: &mut Shell, args: &[String], out: &mut dyn Write, which: bool) -> Outcome {
    let mut status = 0;
    for name in args {
        match shell.resolve(name) {
            Resolution::Alias(value) => {
                let _ = writeln!(out, "{name}: aliased to {value}");
            }
            Resolution::Builtin => {
                let _ = writeln!(out, "{name}: shell built-in");
            }
            Resolution::Util => {
                let _ = writeln!(out, "{name}: Nebula command");
            }
            Resolution::External(path) => {
                if which {
                    let _ = writeln!(out, "{}", path.display());
                } else {
                    let _ = writeln!(out, "{name} is {}", path.display());
                }
            }
            Resolution::Missing => {
                status = 1;
                let _ = writeln!(out, "{name} not found");
            }
        }
    }
    Ok(status)
}

fn help(out: &mut dyn Write) -> Outcome {
    let groups: &[(&str, &[&str])] = &[
        (
            "Files",
            &[
                "ls", "tree", "cd", "pwd", "cp", "mv", "rm", "mkdir", "rmdir", "touch", "ln",
                "find", "du", "df", "realpath",
            ],
        ),
        (
            "Text",
            &[
                "cat", "less", "head", "tail", "grep", "wc", "sort", "uniq", "cut", "tr", "tee",
                "echo", "printf", "xargs",
            ],
        ),
        (
            "System",
            &[
                "ps", "kill", "open", "env", "export", "whoami", "hostname", "uname", "date",
                "sleep",
            ],
        ),
        (
            "Shell",
            &[
                "alias", "history", "type", "which", "source", "pushd", "popd", "clear", "exit",
            ],
        ),
    ];
    let _ = writeln!(out, "{}", "Nebula — Linux commands on Windows".bold());
    let _ = writeln!(
        out,
        "Every command supports --help. Windows programs (git, node, python…) run as usual.\n"
    );
    for (title, names) in groups {
        let _ = writeln!(out, "{}", title.paint(style::ACCENT).bold());
        for name in *names {
            let _ = writeln!(
                out,
                "  {:<10} {}",
                name.paint(style::COMMAND),
                commands::describe(name).unwrap_or("").dim()
            );
        }
        let _ = writeln!(out);
    }
    let mut others: Vec<&str> = crate::coreutils::NAMES
        .iter()
        .copied()
        .filter(|name| !groups.iter().any(|(_, names)| names.contains(name)))
        .collect();
    others.sort_unstable();
    let _ = writeln!(out, "{}", "Also available".paint(style::ACCENT).bold());
    let _ = writeln!(out, "  {}", others.join(" ").dim());
    Ok(0)
}

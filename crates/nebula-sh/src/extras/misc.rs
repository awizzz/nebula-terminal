//! `open`, `less` and `xargs`, plus running a command on behalf of `find -exec`.

use std::ffi::OsString;
use std::io::{self, Read};
use std::process::Command;

/// Runs `argv` the way the shell would resolve it (Nebula commands first, then PATH)
/// and returns its exit code.
pub fn run_command(argv: &[String]) -> i32 {
    let Some(name) = argv.first() else { return 0 };
    let mut command = if crate::commands::is_util(name) {
        let mut command =
            Command::new(std::env::current_exe().unwrap_or_else(|_| "nebula-sh".into()));
        command.arg(name);
        command
    } else {
        match crate::sys::find_executable(name) {
            Some(path) => Command::new(path),
            None => {
                eprintln!("nebula: command not found: {name}");
                return 127;
            }
        }
    };
    match command.args(&argv[1..]).status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("nebula: {name}: {}", crate::exec::describe_io_error(&error));
            126
        }
    }
}

pub fn open(name: &str, args: &[String]) -> i32 {
    if args.is_empty() || args[0] == "--help" {
        println!("Usage: {name} FILE|FOLDER|URL…\nOpen with the default application.");
        return i32::from(args.is_empty());
    }
    let mut status = 0;
    for target in args {
        let target = crate::sys::translate_path(target);
        let result = if cfg!(windows) {
            // `start` treats its first quoted argument as a window title, hence the empty one.
            Command::new("cmd")
                .args(["/d", "/c", "start", ""])
                .arg(&target)
                .status()
        } else if cfg!(target_os = "macos") {
            Command::new("open").arg(&target).status()
        } else {
            Command::new("xdg-open").arg(&target).status()
        };
        if !result.is_ok_and(|status| status.success()) {
            eprintln!("{name}: cannot open {target}");
            status = 1;
        }
    }
    status
}

/// `less` is provided by the uutils `more` pager.
pub fn less(args: &[String]) -> i32 {
    let forwarded: Vec<OsString> = std::iter::once(OsString::from("more"))
        .chain(
            args[1..]
                .iter()
                .filter(|arg| !matches!(arg.as_str(), "-R" | "-r" | "-S" | "-X" | "-F"))
                .map(OsString::from),
        )
        .collect();
    crate::coreutils::run("more", forwarded).unwrap_or(1)
}

pub fn xargs(args: &[String]) -> i32 {
    let mut null = false;
    let mut per_call: Option<usize> = None;
    let mut replace: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-0" | "--null" => null = true,
            "-n" => {
                index += 1;
                per_call = args.get(index).and_then(|n| n.parse().ok());
            }
            "-I" => {
                index += 1;
                replace = args.get(index).cloned();
            }
            "--help" => {
                println!("Usage: xargs [-0] [-n N] [-I REPLACE] [COMMAND [ARGS…]]\nBuild and run commands from standard input (default command: echo).");
                return 0;
            }
            arg if arg.starts_with("-n") && arg.len() > 2 => per_call = arg[2..].parse().ok(),
            _ => break,
        }
        index += 1;
    }
    let mut command: Vec<String> = args[index..].to_vec();
    if command.is_empty() {
        command.push("echo".into());
    }

    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        eprintln!("xargs: cannot read standard input");
        return 1;
    }
    let items: Vec<String> = if null || replace.is_some() {
        let separator = if null { '\0' } else { '\n' };
        input
            .split(separator)
            .map(|item| item.trim_end_matches('\r').to_owned())
            .filter(|item| !item.is_empty())
            .collect()
    } else {
        split_words(&input)
    };

    let mut status = 0;
    if let Some(placeholder) = replace {
        for item in &items {
            let argv: Vec<String> = command
                .iter()
                .map(|part| part.replace(&placeholder, item))
                .collect();
            status = status.max(code_for(run_command(&argv)));
        }
        return status;
    }
    let size = per_call.filter(|n| *n > 0).unwrap_or(items.len().max(1));
    if items.is_empty() {
        return code_for(run_command(&command));
    }
    for chunk in items.chunks(size) {
        let argv: Vec<String> = command
            .iter()
            .cloned()
            .chain(chunk.iter().cloned())
            .collect();
        status = status.max(code_for(run_command(&argv)));
    }
    status
}

/// GNU xargs exits with 123 when any invocation failed.
fn code_for(code: i32) -> i32 {
    match code {
        0 => 0,
        127 | 126 => code,
        _ => 123,
    }
}

/// Splits xargs input on whitespace, honoring simple quotes.
fn split_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut in_word = false;
    for c in input.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => current.push(c),
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                in_word = true;
            }
            None if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            None => {
                current.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(current);
    }
    words
}

#[cfg(test)]
mod tests {
    #[test]
    fn splits_xargs_input() {
        assert_eq!(
            super::split_words("a  b\n'c d' \"e\"\n"),
            vec!["a", "b", "c d", "e"]
        );
    }
}

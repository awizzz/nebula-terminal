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

/// Leaves room under the Windows limit of 32,767 characters per command line.
const MAX_COMMAND_LINE: usize = if cfg!(windows) { 30_000 } else { 120_000 };

/// Splits `items` into groups that each fit on one command line after `fixed`,
/// with at most `max_items` per group.
pub fn batches(fixed: &[String], items: Vec<String>, max_items: Option<usize>) -> Vec<Vec<String>> {
    let base: usize = fixed.iter().map(|arg| arg.len() + 3).sum();
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut length = base;
    for item in items {
        let cost = item.len() + 3;
        let full = max_items.is_some_and(|max| current.len() >= max)
            || (!current.is_empty() && length + cost > MAX_COMMAND_LINE);
        if full {
            groups.push(std::mem::take(&mut current));
            length = base;
        }
        length += cost;
        current.push(item);
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

/// Asks a yes/no question on the console, even when standard input is a pipe
/// (`find -ok`).
pub fn confirm(question: &str) -> bool {
    use std::io::{BufRead, Write};
    eprint!("{question} ");
    let _ = io::stderr().flush();
    let console = if cfg!(windows) { "CONIN$" } else { "/dev/tty" };
    let Ok(file) = std::fs::File::open(console) else {
        return false;
    };
    let mut answer = String::new();
    if io::BufReader::new(file).read_line(&mut answer).is_err() {
        return false;
    }
    matches!(answer.trim_start().chars().next(), Some('y' | 'Y'))
}

#[cfg(windows)]
fn shell_open(target: &str) -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() };
    let verb = wide("open");
    let file = wide(target);
    // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the call.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecute reports success with a value greater than 32.
    result as isize > 32
}

#[cfg(not(windows))]
fn shell_open(target: &str) -> bool {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(opener)
        .arg(target)
        .status()
        .is_ok_and(|status| status.success())
}

pub fn open(name: &str, args: &[String]) -> i32 {
    if args.is_empty() || args[0] == "--help" {
        println!("Usage: {name} FILE|FOLDER|URL…\nOpen with the default application.");
        return i32::from(args.is_empty());
    }
    let mut status = 0;
    for target in args {
        let target = crate::sys::translate_path(target);
        if !shell_open(&target) {
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
    let mut separator: Option<char> = None;
    let mut per_call: Option<usize> = None;
    let mut replace: Option<String> = None;
    let mut skip_empty = false;
    let mut trace = false;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        let value = |index: &mut usize| -> Option<String> {
            if arg.len() > 2 && !arg.starts_with("--") {
                Some(arg[2..].to_owned())
            } else {
                *index += 1;
                args.get(*index).cloned()
            }
        };
        match arg {
            "-0" | "--null" => separator = Some('\0'),
            "-r" | "--no-run-if-empty" => skip_empty = true,
            "-t" | "--verbose" => trace = true,
            _ if arg.starts_with("-n") || arg.starts_with("-L") => {
                match value(&mut index)
                    .and_then(|n| n.parse().ok())
                    .filter(|n| *n > 0)
                {
                    Some(n) => per_call = Some(n),
                    None => {
                        eprintln!("xargs: {arg}: a positive number is required");
                        return 1;
                    }
                }
                if arg.starts_with("-L") {
                    separator.get_or_insert('\n');
                }
            }
            _ if arg.starts_with("-I") => replace = value(&mut index),
            _ if arg.starts_with("-d") => {
                let delimiter = value(&mut index).unwrap_or_default();
                separator = Some(match delimiter.as_str() {
                    "\\n" => '\n',
                    "\\t" => '\t',
                    "\\0" => '\0',
                    other => other.chars().next().unwrap_or('\n'),
                });
            }
            "--help" => {
                println!("Usage: xargs [-0] [-d DELIM] [-n N] [-L N] [-I REPLACE] [-r] [-t] [COMMAND [ARGS…]]\nBuild and run commands from standard input (default command: echo).");
                return 0;
            }
            "--" => {
                index += 1;
                break;
            }
            _ if arg.starts_with('-') && arg.len() > 1 => {
                eprintln!("xargs: {arg}: unsupported option");
                return 1;
            }
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
    let items: Vec<String> = if separator.is_some() || replace.is_some() {
        let separator = separator.unwrap_or('\n');
        input
            .split(separator)
            .map(|item| item.trim_end_matches('\r').to_owned())
            .filter(|item| !item.is_empty())
            .collect()
    } else {
        split_words(&input)
    };

    let run = |argv: &[String]| {
        if trace {
            eprintln!("{}", argv.join(" "));
        }
        code_for(run_command(argv))
    };
    let mut status = 0;
    if let Some(placeholder) = replace {
        for item in &items {
            let argv: Vec<String> = command
                .iter()
                .map(|part| part.replace(&placeholder, item))
                .collect();
            status = status.max(run(&argv));
        }
        return status;
    }
    if items.is_empty() {
        return if skip_empty { 0 } else { run(&command) };
    }
    for chunk in batches(&command, items, per_call) {
        let argv: Vec<String> = command.iter().cloned().chain(chunk).collect();
        status = status.max(run(&argv));
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

    #[test]
    fn batches_respect_count_and_length() {
        let items: Vec<String> = (0..10).map(|n| n.to_string()).collect();
        let groups = super::batches(&["echo".into()], items, Some(4));
        assert_eq!(
            groups.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![4, 4, 2]
        );
        let long: Vec<String> = (0..300).map(|_| "x".repeat(1000)).collect();
        let groups = super::batches(&["rm".into()], long, None);
        assert!(groups.len() > 1);
        assert!(groups
            .iter()
            .all(|group| group.len() * 1003 <= super::MAX_COMMAND_LINE));
    }
}

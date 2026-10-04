//! Commands that run inside the shell process.

use crate::commands;
use crate::exec::{Flow, Io, Resolution, Shell};
use crate::parse;
use crate::style::{self, Paint};
use crate::sys;
use std::env;
use std::io::{Read, Write};
use std::path::PathBuf;

pub fn run(shell: &mut Shell, argv: &[String], io: &Io) -> i32 {
    let mut out = io.stdout.writer();
    let mut err = io.stderr.writer();
    let args = &argv[1..];
    let result = match argv[0].as_str() {
        "cd" => cd(shell, args, &mut out),
        "pushd" => pushd(shell, args, &mut out),
        "popd" => popd(shell, &mut out),
        "dirs" => dirs(shell, &mut out),
        "exit" => exit(shell, args),
        "export" => export(args, &mut out),
        "unset" => unset(shell, args),
        "alias" => alias(shell, args, &mut out),
        "unalias" => unalias(shell, args),
        "history" => history(shell, args, &mut out),
        "source" | "." => source(shell, args, io),
        "type" => describe(shell, args, &mut out),
        "which" => which(shell, args, &mut out),
        "clear" => {
            let _ = write!(out, "\x1b[H\x1b[2J\x1b[3J");
            Ok(0)
        }
        "help" => help(&mut out),
        "echo" => echo(args, &mut out),
        "true" | ":" => Ok(0),
        "false" => Ok(1),
        "test" | "[" => crate::test::builtin(&argv[0], args, &|name| env::var_os(name).is_some())
            .map(|value| i32::from(!value))
            .map_err(|message| (message, 2))
            .or_else(|(message, code)| {
                let _ = writeln!(err, "{}: {message}", argv[0]);
                Ok::<i32, String>(code)
            }),
        "local" => local(shell, args),
        "declare" | "typeset" => declare(shell, args, &mut out),
        "return" => flow(shell, args, "return"),
        "break" => flow(shell, args, "break"),
        "continue" => flow(shell, args, "continue"),
        "shift" => shift(shell, args),
        "set" => set(shell, args, &mut out),
        "read" => read(shell, args, io, &mut err),
        "mapfile" | "readarray" => mapfile(shell, args, io),
        "eval" => Ok(shell.run_source(&args.join(" "), io, "eval: ")),
        "command" => command(shell, args, io, &mut out),
        "let" => let_(shell, args),
        "jobs" => jobs(shell, args, &mut out),
        "wait" => wait(shell, args),
        "fg" => fg(shell, args, &mut out),
        "bg" => bg(shell, args),
        "disown" => disown(shell, args),
        "trap" => trap(shell, args, &mut out),
        "z" => z(shell, args, &mut out),
        "getopts" => getopts(shell, args, &mut err),
        other => Err(format!("{other}: not a builtin")),
    };
    let _ = out.flush();
    match result {
        Ok(code) => code,
        Err(message) => {
            let _ = writeln!(err, "{}: {message}", argv[0]);
            1
        }
    }
}

type Outcome = Result<i32, String>;

/// `z words…` goes to the folder you visit most that matches, `z -l words…` lists the
/// matches, `z` alone lists every folder it knows, best last.
fn z(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> Outcome {
    let list = args.first().is_some_and(|arg| arg == "-l");
    let words: Vec<String> = args.iter().filter(|arg| *arg != "-l").cloned().collect();
    if words.is_empty() || list {
        let ranked = crate::frecency::ranked(&words);
        for (score, path) in ranked.iter().rev() {
            let _ = writeln!(out, "{score:<10.1} {path}");
        }
        return Ok(i32::from(ranked.is_empty() && !words.is_empty()));
    }
    // A real folder needs no ranking.
    if words.len() == 1 {
        let target = resolve_dir(&words[0]);
        if target.is_dir() {
            shell.change_dir(&target)?;
            return Ok(0);
        }
    }
    match crate::frecency::ranked(&words).into_iter().next() {
        Some((_, path)) => {
            shell.change_dir(std::path::Path::new(&path))?;
            Ok(0)
        }
        None => Err(format!("no folder you visited matches {}", words.join(" "))),
    }
}

/// `jobs [-l | -p]`: the background jobs, newest last, with `+` on the current one.
fn jobs(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> Outcome {
    let long = args.iter().any(|arg| arg == "-l");
    let pids_only = args.iter().any(|arg| arg == "-p");
    let pipefail = shell.options.pipefail;
    let mut index = 0;
    while index < shell.jobs.len() {
        let marker = shell.job_marker(index);
        let job = &mut shell.jobs[index];
        let state = job.state(pipefail);
        if pids_only {
            for pid in &job.pids {
                let _ = writeln!(out, "{pid}");
            }
        } else if long {
            let pid = job
                .pids
                .last()
                .map_or(String::new(), |pid| format!("{pid} "));
            let _ = writeln!(out, "[{}]{marker} {pid}{state:<24}{} &", job.id, job.text);
        } else {
            let _ = writeln!(out, "[{}]{marker}  {state:<24}{} &", job.id, job.text);
        }
        // A finished job is shown once, then forgotten.
        if state == "Running" {
            index += 1;
        } else {
            shell.jobs.remove(index);
        }
    }
    Ok(0)
}

/// `wait [-n] [%job | pid]...`: without operands, waits for every job and returns 0.
fn wait(shell: &mut Shell, args: &[String]) -> Outcome {
    if args.first().is_some_and(|arg| arg == "-n") {
        // The next job to finish, whichever it is.
        loop {
            if shell.jobs.is_empty() {
                return Ok(127);
            }
            for index in 0..shell.jobs.len() {
                if let Some(status) = shell.poll_job(index) {
                    shell.jobs.remove(index);
                    return Ok(status);
                }
            }
            if shell.interrupted.load(std::sync::atomic::Ordering::SeqCst) {
                return Ok(130);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
    if args.is_empty() {
        while !shell.jobs.is_empty() {
            if shell.wait_job(0, false) == 130
                && shell.interrupted.load(std::sync::atomic::Ordering::SeqCst)
            {
                return Ok(130);
            }
        }
        return Ok(0);
    }
    let mut status = 0;
    for spec in args {
        match shell.find_job(spec) {
            Some(index) => status = shell.wait_job(index, false),
            None if spec.starts_with('%') => return Err(format!("{spec}: no such job")),
            None => {
                eprintln!("wait: pid {spec} is not a child of this shell");
                status = 127;
            }
        }
    }
    Ok(status)
}

fn job_index(shell: &Shell, args: &[String]) -> Result<usize, String> {
    let spec = args.first().map_or("%+", String::as_str);
    shell.find_job(spec).ok_or_else(|| {
        if args.is_empty() {
            "no current job".to_owned()
        } else {
            format!("{spec}: no such job")
        }
    })
}

/// `fg [%job]`: waits for the job as if it ran in the foreground; Ctrl+C stops it.
fn fg(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> Outcome {
    let index = job_index(shell, args)?;
    let _ = writeln!(out, "{}", shell.jobs[index].text);
    let _ = out.flush();
    Ok(shell.wait_job(index, true))
}

/// Jobs never stop in Nebula, so there is nothing to resume.
fn bg(shell: &mut Shell, args: &[String]) -> Outcome {
    let index = job_index(shell, args)?;
    let job = &shell.jobs[index];
    Err(format!(
        "job {} is already running in the background",
        job.id
    ))
}

/// `disown [%job]`: lets the job run on, out of the job list.
fn disown(shell: &mut Shell, args: &[String]) -> Outcome {
    if args.first().is_some_and(|arg| arg == "-a") {
        shell.jobs.clear();
        return Ok(0);
    }
    let index = job_index(shell, args)?;
    shell.jobs.remove(index);
    Ok(0)
}

/// The conditions `trap` knows, by the names and numbers scripts use.
/// `getopts optstring name [arg…]`: puts the next option of the arguments (or of `$@`)
/// in `name` and its value in `OPTARG`, as bash does. A leading `:` in `optstring`
/// reports problems through `name` and `OPTARG` instead of messages.
fn getopts(shell: &mut Shell, args: &[String], err: &mut dyn Write) -> Outcome {
    let [spec, name, explicit @ ..] = args else {
        let _ = writeln!(err, "getopts: usage: getopts optstring name [arg ...]");
        return Ok(2);
    };
    valid_name(name)?;
    let words = if explicit.is_empty() {
        shell.positional.clone()
    } else {
        explicit.to_vec()
    };
    let (silent, spec) = match spec.strip_prefix(':') {
        Some(spec) => (true, spec),
        None => (false, spec.as_str()),
    };
    let mut index = env::var("OPTIND")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|&index| index > 0)
        .unwrap_or(1);
    let mut offset = match shell.getopts_next {
        Some((at, offset)) if at == index => offset,
        _ => 0,
    };
    let word: Vec<char> = words
        .get(index - 1)
        .map(|word| word.chars().collect())
        .unwrap_or_default();
    if offset == 0 || offset >= word.len() {
        // Options end at the first word that isn't one, or after `--`.
        let dashes = word == ['-', '-'];
        if dashes || word.len() < 2 || word[0] != '-' {
            shell.getopts_next = None;
            env::set_var("OPTIND", (index + usize::from(dashes)).to_string());
            env::remove_var("OPTARG");
            shell.assign_scalar(name, "?".to_owned(), false);
            return Ok(1);
        }
        offset = 1;
    }
    let option = word[offset];
    let rest: String = word[offset + 1..].iter().collect();
    // The next call goes on in this word, or at the next one.
    let mut next = if rest.is_empty() {
        (index + 1, 0)
    } else {
        (index, offset + 1)
    };
    let report = !silent && env::var("OPTERR").map_or(true, |value| value != "0");
    let mut found = option.to_string();
    let mut argument = None;
    match spec.find(option).filter(|_| option != ':') {
        None => {
            if report {
                let _ = writeln!(err, "{}: illegal option -- {option}", shell.script_name);
            }
            found = "?".to_owned();
            argument = silent.then(|| option.to_string());
        }
        Some(at) if spec[at + option.len_utf8()..].starts_with(':') => {
            if !rest.is_empty() {
                argument = Some(rest);
                next = (index + 1, 0);
            } else if let Some(value) = words.get(index) {
                argument = Some(value.clone());
                next = (index + 2, 0);
            } else {
                if report {
                    let _ = writeln!(
                        err,
                        "{}: option requires an argument -- {option}",
                        shell.script_name
                    );
                }
                found = if silent { ":" } else { "?" }.to_owned();
                argument = silent.then(|| option.to_string());
            }
        }
        Some(_) => {}
    }
    index = next.0;
    shell.getopts_next = (next.1 > 0).then_some(next);
    env::set_var("OPTIND", index.to_string());
    match argument {
        Some(value) => env::set_var("OPTARG", value),
        None => env::remove_var("OPTARG"),
    }
    shell.assign_scalar(name, found, false);
    Ok(0)
}

fn trap_condition(name: &str) -> Option<&'static str> {
    let upper = name.to_ascii_uppercase();
    let bare = upper.strip_prefix("SIG").unwrap_or(&upper);
    Some(match bare {
        "0" | "EXIT" => "EXIT",
        "ERR" => "ERR",
        "1" | "HUP" => "HUP",
        "2" | "INT" => "INT",
        "3" | "QUIT" => "QUIT",
        "15" | "TERM" => "TERM",
        _ => return None,
    })
}

/// `trap [action] condition...`, `trap - condition...`, `trap -p`, `trap -l`.
/// EXIT runs when the shell ends, ERR after a command fails, INT on Ctrl+C. HUP,
/// QUIT and TERM are accepted for scripts written for Linux but never happen here.
fn trap(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> Outcome {
    let args: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .skip_while(|arg| *arg == "--")
        .collect();
    let print = |shell: &Shell, out: &mut dyn Write, only: &[&str]| {
        for (condition, action) in &shell.traps {
            if only.is_empty() || only.contains(&condition.as_str()) {
                let _ = writeln!(out, "trap -- {} {condition}", parse::quote(action));
            }
        }
    };
    match args.first().copied() {
        None => {
            print(shell, out, &[]);
            Ok(0)
        }
        Some("-l") => {
            let _ = writeln!(out, " 0) EXIT	 1) HUP	 2) INT	 3) QUIT	15) TERM	ERR");
            Ok(0)
        }
        Some("-p") => {
            let mut only = Vec::new();
            for name in &args[1..] {
                only.push(
                    trap_condition(name).ok_or_else(|| format!("{name}: unknown condition"))?,
                );
            }
            print(shell, out, &only);
            Ok(0)
        }
        Some(first) => {
            // `trap INT` alone resets, like `trap - INT`.
            let (action, conditions) = if args.len() == 1 || first == "-" {
                (None, if first == "-" { &args[1..] } else { &args[..] })
            } else {
                (Some(first), &args[1..])
            };
            for name in conditions {
                let condition = trap_condition(name).ok_or_else(|| {
                    format!(
                        "{name}: not a condition Nebula can trap (EXIT, ERR, INT, TERM, HUP, QUIT)"
                    )
                })?;
                match action {
                    Some(action) => {
                        shell.traps.insert(condition.to_owned(), action.to_owned());
                    }
                    None => {
                        shell.traps.remove(condition);
                    }
                }
            }
            Ok(0)
        }
    }
}

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

fn valid_name(name: &str) -> Result<(), String> {
    if parse::is_name(name) {
        Ok(())
    } else {
        Err(format!("`{name}`: not a valid identifier"))
    }
}

fn export(args: &[String], out: &mut dyn Write) -> Outcome {
    let names: Vec<&String> = args.iter().filter(|arg| !arg.starts_with('-')).collect();
    if names.is_empty() {
        let mut vars: Vec<(String, String)> = env::vars().collect();
        vars.sort_by_key(|(name, _)| name.to_lowercase());
        for (name, value) in vars {
            let _ = writeln!(out, "declare -x {name}=\"{}\"", value.replace('"', "\\\""));
        }
        return Ok(0);
    }
    for arg in names {
        // Every Nebula variable is already an environment variable.
        match arg.split_once('=') {
            Some((name, value)) => {
                valid_name(name)?;
                env::set_var(name, value);
            }
            None => valid_name(arg)?,
        }
    }
    Ok(0)
}

fn unset(shell: &mut Shell, args: &[String]) -> Outcome {
    let mut functions = false;
    let mut variables = false;
    for arg in args {
        match arg.as_str() {
            "-f" => functions = true,
            "-v" => variables = true,
            name => {
                if let Some((base, sub)) = parse::split_subscript(name) {
                    shell.unset_element(base, sub);
                    continue;
                }
                if !functions {
                    shell.arrays.remove(name);
                }
                if functions {
                    shell.functions.remove(name);
                } else if variables
                    || env::var_os(name).is_some()
                    || !shell.functions.contains_key(name)
                {
                    if !name.is_empty() && !name.contains('=') {
                        env::remove_var(name);
                    }
                } else {
                    shell.functions.remove(name);
                }
            }
        }
    }
    Ok(0)
}

fn alias(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> Outcome {
    if args.is_empty() {
        for (name, value) in &shell.aliases {
            let _ = writeln!(out, "alias {name}={}", parse::quote(value));
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
                    let _ = writeln!(out, "alias {arg}={}", parse::quote(value));
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

fn source(shell: &mut Shell, args: &[String], io: &Io) -> Outcome {
    let path = args.first().ok_or("filename argument required")?;
    let text = std::fs::read_to_string(resolve_dir(path))
        .map_err(|error| format!("{path}: {}", crate::exec::describe_io_error(&error)))?;
    let saved =
        (args.len() > 1).then(|| std::mem::replace(&mut shell.positional, args[1..].to_vec()));
    let status = shell.run_source(&text, io, &format!("{path}: "));
    if let Some(saved) = saved {
        shell.positional = saved;
    }
    if shell.flow == Some(Flow::Return) {
        shell.flow = None;
    }
    Ok(status)
}

fn describe(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> Outcome {
    let (kind_only, names) = match args.first().map(String::as_str) {
        Some("-t") => (true, &args[1..]),
        _ => (false, args),
    };
    let mut status = 0;
    for name in names {
        if parse::KEYWORDS.contains(&name.as_str()) {
            let _ = if kind_only {
                writeln!(out, "keyword")
            } else {
                writeln!(out, "{name} is a shell keyword")
            };
            continue;
        }
        match shell.resolve(name) {
            Resolution::Alias(value) => {
                let _ = if kind_only {
                    writeln!(out, "alias")
                } else {
                    writeln!(out, "{name} is aliased to `{value}`")
                };
            }
            Resolution::Function(function) => {
                let _ = if kind_only {
                    writeln!(out, "function")
                } else {
                    writeln!(out, "{name} is a function\n{}", function.source)
                };
            }
            Resolution::Builtin => {
                let _ = if kind_only {
                    writeln!(out, "builtin")
                } else {
                    writeln!(out, "{name} is a shell builtin")
                };
            }
            Resolution::Util => {
                let _ = if kind_only {
                    writeln!(out, "file")
                } else {
                    writeln!(out, "{name} is a Nebula command")
                };
            }
            Resolution::External(path) => {
                let _ = if kind_only {
                    writeln!(out, "file")
                } else {
                    writeln!(out, "{name} is {}", path.display())
                };
            }
            Resolution::Missing => {
                status = 1;
                if !kind_only {
                    let _ = writeln!(out, "{name} not found");
                }
            }
        }
    }
    Ok(status)
}

fn which(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> Outcome {
    let mut status = 0;
    for name in args.iter().filter(|arg| !arg.starts_with('-')) {
        match shell.resolve(name) {
            Resolution::External(path) => {
                let _ = writeln!(out, "{}", path.display());
            }
            Resolution::Alias(value) => {
                let _ = writeln!(out, "{name}: aliased to {value}");
            }
            Resolution::Function(_) => {
                let _ = writeln!(out, "{name}: shell function");
            }
            Resolution::Builtin => {
                let _ = writeln!(out, "{name}: shell built-in");
            }
            Resolution::Util => {
                let _ = writeln!(out, "{name}: Nebula command");
            }
            Resolution::Missing => {
                status = 1;
                let _ = writeln!(out, "{name} not found");
            }
        }
    }
    Ok(status)
}

/// `echo [-neE] args…`, with the escapes of `echo -e`.
fn echo(args: &[String], out: &mut dyn Write) -> Outcome {
    let mut newline = true;
    let mut escapes = false;
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        let Some(flags) = arg.strip_prefix('-') else {
            break;
        };
        if flags.is_empty() || !flags.chars().all(|c| matches!(c, 'n' | 'e' | 'E')) {
            break;
        }
        for flag in flags.chars() {
            match flag {
                'n' => newline = false,
                'e' => escapes = true,
                _ => escapes = false,
            }
        }
        index += 1;
    }
    let text = args[index..].join(" ");
    if !escapes {
        let _ = out.write_all(text.as_bytes());
        if newline {
            let _ = out.write_all(b"\n");
        }
        return Ok(0);
    }
    let (text, stop) = unescape(&text);
    let _ = out.write_all(text.as_bytes());
    if newline && !stop {
        let _ = out.write_all(b"\n");
    }
    Ok(0)
}

/// Backslash escapes of `echo -e`. Returns the text and whether `\c` cut it short.
fn unescape(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(next) = chars.next() else {
            out.push('\\');
            break;
        };
        match next {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'a' => out.push('\x07'),
            'b' => out.push('\x08'),
            'e' | 'E' => out.push('\x1b'),
            'f' => out.push('\x0c'),
            'v' => out.push('\x0b'),
            '\\' => out.push('\\'),
            'c' => return (out, true),
            '0' => {
                let mut value = 0u32;
                for _ in 0..3 {
                    match chars.peek().and_then(|d| d.to_digit(8)) {
                        Some(d) => {
                            value = value * 8 + d;
                            chars.next();
                        }
                        None => break,
                    }
                }
                out.extend(char::from_u32(value));
            }
            'x' => {
                let mut value = 0u32;
                let mut digits = 0;
                while digits < 2 {
                    match chars.peek().and_then(|d| d.to_digit(16)) {
                        Some(d) => {
                            value = value * 16 + d;
                            chars.next();
                            digits += 1;
                        }
                        None => break,
                    }
                }
                if digits == 0 {
                    out.push_str("\\x");
                } else {
                    out.extend(char::from_u32(value));
                }
            }
            other => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    (out, false)
}

fn local(shell: &mut Shell, args: &[String]) -> Outcome {
    let flags: String = args
        .iter()
        .filter_map(|arg| arg.strip_prefix('-'))
        .collect();
    let array = flags.contains('a') || flags.contains('A');
    for arg in args.iter().filter(|arg| !arg.starts_with('-')) {
        let (name, value) = match arg.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (arg.as_str(), None),
        };
        valid_name(name)?;
        if array && value.is_none() {
            shell.declare_local_array(name, flags.contains('A'))?;
        } else {
            shell.declare_local(name, value)?;
        }
    }
    Ok(0)
}

fn declare(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> Outcome {
    let mut global = false;
    let mut functions = None;
    let mut array: Option<bool> = None;
    let mut print = false;
    let mut names = Vec::new();
    for arg in args {
        match arg.strip_prefix('-') {
            Some(flags) if !flags.is_empty() && !arg.contains('=') => {
                for flag in flags.chars() {
                    match flag {
                        'g' => global = true,
                        'f' => functions = Some(true),
                        'F' => functions = Some(false),
                        'a' => array = Some(false),
                        'A' => array = Some(true),
                        'p' => print = true,
                        // -x, -i, -r, -p and the rest: every variable is already exported.
                        _ => {}
                    }
                }
            }
            _ => names.push(arg),
        }
    }
    if let Some(with_body) = functions {
        for function in shell.functions.values() {
            if names.is_empty() || names.iter().any(|name| **name == function.name) {
                let _ = if with_body {
                    writeln!(out, "{}", function.source)
                } else {
                    writeln!(out, "declare -f {}", function.name)
                };
            }
        }
        return Ok(0);
    }
    if print {
        return print_declarations(shell, &names, out);
    }
    if names.is_empty() {
        if let Some(assoc) = array {
            for (name, value) in &shell.arrays {
                if matches!(value, crate::exec::Array::Assoc(_)) == assoc {
                    let kind = if assoc { 'A' } else { 'a' };
                    let _ = writeln!(out, "declare -{kind} {name}={}", value.describe());
                }
            }
            return Ok(0);
        }
        return set(shell, &[], out);
    }
    for arg in names {
        let (name, value) = match arg.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (arg.as_str(), None),
        };
        valid_name(name)?;
        if let Some(assoc) = array {
            let local = shell.in_function() && !global;
            let exists = match shell.arrays.get(name) {
                Some(crate::exec::Array::Assoc(_)) => assoc,
                Some(crate::exec::Array::Indexed(map)) => !assoc || map.is_empty(),
                None => false,
            };
            if local {
                shell.declare_local_array(name, assoc)?;
            } else if !exists {
                let mut created = Shell::empty_array(assoc);
                // A plain variable becomes element 0 of the new indexed array.
                if let (false, Some(previous)) = (assoc, env::var_os(name)) {
                    if let crate::exec::Array::Indexed(map) = &mut created {
                        map.insert(0, previous.to_string_lossy().into_owned());
                    }
                    env::remove_var(name);
                }
                shell.arrays.insert(name.to_owned(), created);
            }
            if let Some(value) = value {
                shell.assign_scalar(name, value.to_owned(), false);
            }
            continue;
        }
        if shell.in_function() && !global {
            let value = value.map(str::to_owned).or_else(|| env::var(name).ok());
            shell.declare_local(name, value.as_deref())?;
        } else if let Some(value) = value {
            env::set_var(name, value);
        }
    }
    Ok(0)
}

/// `declare -p [name...]`: each variable as a `declare` line that recreates it.
fn print_declarations(shell: &Shell, names: &[&String], out: &mut dyn Write) -> Outcome {
    let mut status = 0;
    let all: Vec<String>;
    let names: Vec<&str> = if names.is_empty() {
        all = shell.arrays.keys().cloned().collect();
        all.iter().map(String::as_str).collect()
    } else {
        names.iter().map(|name| name.as_str()).collect()
    };
    for name in names {
        match (shell.arrays.get(name), env::var(name)) {
            (Some(array), _) => {
                let kind = if matches!(array, crate::exec::Array::Assoc(_)) {
                    'A'
                } else {
                    'a'
                };
                let _ = writeln!(out, "declare -{kind} {name}={}", array.describe());
            }
            (None, Ok(value)) => {
                let _ = writeln!(out, "declare -x {name}={}", parse::quote(&value));
            }
            (None, Err(_)) => {
                eprintln!("declare: {name}: not found");
                status = 1;
            }
        }
    }
    Ok(status)
}

fn flow(shell: &mut Shell, args: &[String], which: &str) -> Outcome {
    let number = match args.first() {
        Some(value) => Some(
            value
                .parse::<i32>()
                .map_err(|_| format!("{value}: numeric argument required"))?,
        ),
        None => None,
    };
    if which == "return" {
        shell.flow = Some(Flow::Return);
        return Ok(number.unwrap_or(shell.last_status));
    }
    if shell.loop_depth == 0 {
        return Err("only meaningful in a `for`, `while` or `until` loop".into());
    }
    let levels = usize::try_from(number.unwrap_or(1))
        .ok()
        .filter(|levels| *levels > 0)
        .ok_or("loop count out of range")?
        .min(shell.loop_depth);
    shell.flow = Some(if which == "break" {
        Flow::Break(levels)
    } else {
        Flow::Continue(levels)
    });
    Ok(0)
}

fn shift(shell: &mut Shell, args: &[String]) -> Outcome {
    let count = match args.first() {
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| format!("{value}: numeric argument required"))?,
        None => 1,
    };
    if count > shell.positional.len() {
        return Ok(1);
    }
    shell.positional.drain(..count);
    Ok(0)
}

fn set(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> Outcome {
    if args.is_empty() {
        let mut vars: Vec<(String, String)> = env::vars().collect();
        vars.sort_by_key(|(name, _)| name.to_lowercase());
        for (name, value) in vars {
            let _ = writeln!(out, "{name}={}", parse::quote(&value));
        }
        return Ok(0);
    }
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if arg == "--" {
            shell.positional = args[index + 1..].to_vec();
            return Ok(0);
        }
        let (enable, flags) = match (arg.strip_prefix('-'), arg.strip_prefix('+')) {
            (Some(flags), _) if !flags.is_empty() => (true, flags),
            (_, Some(flags)) if !flags.is_empty() => (false, flags),
            _ => {
                shell.positional = args[index..].to_vec();
                return Ok(0);
            }
        };
        for flag in flags.chars() {
            match flag {
                'e' => shell.options.errexit = enable,
                'u' => shell.options.nounset = enable,
                'x' => shell.options.xtrace = enable,
                'o' => {
                    index += 1;
                    match args.get(index).map(String::as_str) {
                        Some("errexit") => shell.options.errexit = enable,
                        Some("nounset") => shell.options.nounset = enable,
                        Some("xtrace") => shell.options.xtrace = enable,
                        Some("pipefail") => shell.options.pipefail = enable,
                        Some(other) => return Err(format!("{other}: unknown option name")),
                        None => {
                            let options = shell.options;
                            for (name, on) in [
                                ("errexit", options.errexit),
                                ("nounset", options.nounset),
                                ("pipefail", options.pipefail),
                                ("xtrace", options.xtrace),
                            ] {
                                let _ =
                                    writeln!(out, "{name:<12}{}", if on { "on" } else { "off" });
                            }
                        }
                    }
                }
                other => return Err(format!("-{other}: unsupported option")),
            }
        }
        index += 1;
    }
    Ok(0)
}

struct ReadOptions {
    raw: bool,
    silent: bool,
    prompt: Option<String>,
    delimiter: u8,
    count: Option<usize>,
}

fn read(shell: &mut Shell, args: &[String], io: &Io, err: &mut dyn Write) -> Outcome {
    let mut options = ReadOptions {
        raw: false,
        silent: false,
        prompt: None,
        delimiter: b'\n',
        count: None,
    };
    let mut names = Vec::new();
    let mut array = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-a" => {
                let name = iter.next().cloned().unwrap_or_default();
                valid_name(&name)?;
                array = Some(name);
            }
            "-r" => options.raw = true,
            "-s" => options.silent = true,
            "-p" => options.prompt = Some(iter.next().cloned().unwrap_or_default()),
            "-d" => {
                options.delimiter = iter.next().and_then(|d| d.bytes().next()).unwrap_or(0);
            }
            "-n" | "-N" => {
                let value = iter.next().cloned().unwrap_or_default();
                options.count = Some(
                    value
                        .parse()
                        .map_err(|_| format!("{value}: invalid count"))?,
                );
            }
            "-t" | "-u" => return Err(format!("{arg}: unsupported option")),
            flags if flags.starts_with('-') && flags.len() > 1 => {
                for flag in flags[1..].chars() {
                    match flag {
                        'r' => options.raw = true,
                        's' => options.silent = true,
                        'a' => {
                            let name = iter.next().cloned().unwrap_or_default();
                            valid_name(&name)?;
                            array = Some(name);
                        }
                        other => return Err(format!("-{other}: unsupported option")),
                    }
                }
            }
            name => {
                valid_name(name)?;
                names.push(name.to_owned());
            }
        }
    }
    if names.is_empty() {
        names.push("REPLY".to_owned());
    }

    let terminal = io.stdin.is_terminal();
    if let Some(prompt) = &options.prompt {
        if terminal {
            let _ = write!(err, "{prompt}");
            let _ = err.flush();
        }
    }
    let (line, complete) = if terminal && options.silent {
        read_silently(err)?
    } else {
        read_record(io, &options)?
    };
    let line = if options.raw {
        line
    } else {
        strip_backslashes(&line)
    };
    let ifs = env::var("IFS").unwrap_or_else(|_| " \t\n".to_owned());
    if let Some(name) = array {
        let items = split_all(&line, &ifs)
            .into_iter()
            .map(|value| (None, value))
            .collect();
        shell.assign_list(&name, items, false);
        return Ok(i32::from(!complete));
    }
    let values = split_fields(&line, &ifs, names.len());
    for (name, value) in names.iter().zip(values) {
        env::set_var(name, value);
    }
    Ok(i32::from(!complete))
}

/// Reads up to the delimiter. Returns the text and whether a delimiter was found
/// (`false` at end of input, which makes `read` fail and ends `while read` loops).
fn read_record(io: &Io, options: &ReadOptions) -> Result<(String, bool), String> {
    let mut reader = io.stdin.reader().map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    let mut byte = [0u8];
    loop {
        if options
            .count
            .is_some_and(|count| String::from_utf8_lossy(&bytes).chars().count() >= count)
        {
            return Ok((String::from_utf8_lossy(&bytes).into_owned(), true));
        }
        match reader.read(&mut byte) {
            Ok(0) => {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                let text = text.strip_suffix('\r').unwrap_or(&text).to_owned();
                return Ok((text, false));
            }
            Ok(_) => {
                if byte[0] == options.delimiter {
                    // A backslash before the newline continues the line (without -r).
                    if !options.raw
                        && options.delimiter == b'\n'
                        && bytes.last() == Some(&b'\\')
                        && trailing_backslashes(&bytes) % 2 == 1
                    {
                        bytes.pop();
                        if bytes.last() == Some(&b'\r') {
                            bytes.pop();
                        }
                        continue;
                    }
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    let text = if options.delimiter == b'\n' {
                        text.strip_suffix('\r').unwrap_or(&text).to_owned()
                    } else {
                        text
                    };
                    return Ok((text, true));
                }
                bytes.push(byte[0]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn trailing_backslashes(bytes: &[u8]) -> usize {
    bytes.iter().rev().take_while(|b| **b == b'\\').count()
}

/// `read -s`: reads a line from the console without echoing it.
fn read_silently(err: &mut dyn Write) -> Result<(String, bool), String> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    crossterm::terminal::enable_raw_mode().map_err(|error| error.to_string())?;
    let mut line = String::new();
    let result = loop {
        match event::read() {
            Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Enter => break Ok((line, true)),
                KeyCode::Backspace => {
                    line.pop();
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    break Err("interrupted".to_owned())
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    break Ok((line, false))
                }
                KeyCode::Char(c) => line.push(c),
                _ => {}
            },
            Ok(Event::Paste(text)) => line.push_str(&text),
            Ok(_) => {}
            Err(error) => break Err(error.to_string()),
        }
    };
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = writeln!(err);
    result
}

fn strip_backslashes(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Splits a line for `read` the way bash does with `IFS`: whitespace separators
/// collapse and are trimmed; the last variable gets the rest of the line.
/// Every field of a line, as `read -a` splits it.
fn split_all(line: &str, ifs: &str) -> Vec<String> {
    if ifs.is_empty() {
        return vec![line.to_owned()];
    }
    let is_space = |c: char| ifs.contains(c) && c.is_whitespace();
    let mut fields = Vec::new();
    let mut rest = line.trim_matches(is_space);
    while !rest.is_empty() {
        match rest.find(|c: char| ifs.contains(c)) {
            Some(index) => {
                fields.push(rest[..index].to_owned());
                rest = rest[index..].trim_start_matches(is_space);
                if let Some(c) = rest
                    .chars()
                    .next()
                    .filter(|c| !c.is_whitespace() && ifs.contains(*c))
                {
                    rest = rest[c.len_utf8()..].trim_start_matches(is_space);
                }
            }
            None => {
                fields.push(rest.to_owned());
                rest = "";
            }
        }
    }
    fields
}

/// `mapfile [-t] [-n count] [-s skip] [array]`, also called `readarray`: the lines of
/// standard input into an array (`MAPFILE` by default).
fn mapfile(shell: &mut Shell, args: &[String], io: &Io) -> Outcome {
    let mut trim = false;
    let mut count: Option<usize> = None;
    let mut skip = 0;
    let mut name = "MAPFILE".to_owned();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let number = |value: Option<&String>| -> Result<usize, String> {
            let value = value.cloned().unwrap_or_default();
            value
                .parse()
                .map_err(|_| format!("{value}: invalid number"))
        };
        match arg.as_str() {
            "-t" => trim = true,
            "-n" => count = Some(number(iter.next())?).filter(|n| *n > 0),
            "-s" => skip = number(iter.next())?,
            flag if flag.starts_with('-') => return Err(format!("{flag}: unsupported option")),
            other => {
                valid_name(other)?;
                name = other.to_owned();
            }
        }
    }
    let mut reader = io.stdin.reader().map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<String> = text
        .split_inclusive('\n')
        .skip(skip)
        .map(|line| {
            if trim {
                line.trim_end_matches('\n')
                    .trim_end_matches('\r')
                    .to_owned()
            } else {
                line.to_owned()
            }
        })
        .collect();
    if let Some(count) = count {
        lines.truncate(count);
    }
    shell.assign_list(
        &name,
        lines.into_iter().map(|line| (None, line)).collect(),
        false,
    );
    Ok(0)
}

fn split_fields(line: &str, ifs: &str, count: usize) -> Vec<String> {
    if ifs.is_empty() {
        let mut fields = vec![line.to_owned()];
        fields.resize(count, String::new());
        return fields;
    }
    let is_space = |c: char| ifs.contains(c) && c.is_whitespace();
    let is_separator = |c: char| ifs.contains(c);
    let mut fields = Vec::with_capacity(count);
    let mut rest = line.trim_matches(is_space);
    while fields.len() + 1 < count && !rest.is_empty() {
        match rest.find(is_separator) {
            Some(index) => {
                fields.push(rest[..index].to_owned());
                rest = rest[index..].trim_start_matches(is_space);
                if let Some(c) = rest
                    .chars()
                    .next()
                    .filter(|c| !c.is_whitespace() && is_separator(*c))
                {
                    rest = rest[c.len_utf8()..].trim_start_matches(is_space);
                }
            }
            None => {
                fields.push(rest.to_owned());
                rest = "";
            }
        }
    }
    if fields.len() < count {
        fields.push(rest.to_owned());
    }
    fields.resize(count, String::new());
    fields
}

fn command(shell: &mut Shell, args: &[String], io: &Io, out: &mut dyn Write) -> Outcome {
    let (mode, names) = match args.first().map(String::as_str) {
        Some("-v") => (Some(false), &args[1..]),
        Some("-V") => (Some(true), &args[1..]),
        _ => (None, args),
    };
    match mode {
        Some(true) => describe(shell, names, out),
        Some(false) => {
            let mut status = 0;
            for name in names {
                match shell.resolve(name) {
                    Resolution::Alias(value) => {
                        let _ = writeln!(out, "alias {name}={}", parse::quote(&value));
                    }
                    Resolution::External(path) => {
                        let _ = writeln!(out, "{}", path.display());
                    }
                    Resolution::Missing => status = 1,
                    _ => {
                        let _ = writeln!(out, "{name}");
                    }
                }
            }
            Ok(status)
        }
        None if names.is_empty() => Ok(0),
        None => {
            let _ = out.flush();
            Ok(shell.run_bypassing_functions(names.to_vec(), io))
        }
    }
}

fn let_(shell: &mut Shell, args: &[String]) -> Outcome {
    if args.is_empty() {
        return Err("expression expected".into());
    }
    let mut last = 0;
    for arg in args {
        match crate::expand::Context::arith(shell, arg) {
            Some(value) => last = value,
            None => return Ok(1),
        }
    }
    Ok(i32::from(last == 0))
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
            "Scripting",
            &[
                "test", "read", "local", "return", "shift", "getopts", "set", "eval", "let",
                "command", "source",
            ],
        ),
        (
            "Shell",
            &[
                "alias", "history", "type", "which", "pushd", "popd", "clear", "exit",
            ],
        ),
    ];
    let _ = writeln!(out, "{}", "Nebula — Linux commands on Windows".bold());
    let _ = writeln!(
        out,
        "Every command supports --help. Windows programs (git, node, python…) run as usual."
    );
    let _ = writeln!(
        out,
        "Scripts can use if, for, while, until, case, functions, $((…)) and [[ … ]].\n"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_read_fields_like_bash() {
        assert_eq!(
            split_fields("  a  b  c d ", " \t\n", 2),
            vec!["a", "b  c d"]
        );
        assert_eq!(split_fields("a", " \t\n", 3), vec!["a", "", ""]);
        assert_eq!(split_fields("  keep  ", "", 1), vec!["  keep  "]);
        assert_eq!(split_fields("x:y::z", ":", 4), vec!["x", "y", "", "z"]);
        assert_eq!(split_fields("root:x:0", ":", 2), vec!["root", "x:0"]);
    }

    #[test]
    fn unescapes_echo_e() {
        assert_eq!(unescape(r"a\tb\n"), ("a\tb\n".to_owned(), false));
        assert_eq!(unescape(r"\x41\0101"), ("AA".to_owned(), false));
        assert_eq!(unescape(r"stop\chere"), ("stop".to_owned(), true));
    }
}

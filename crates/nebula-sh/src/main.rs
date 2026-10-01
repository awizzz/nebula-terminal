//! Nebula: a Linux-style command interpreter for Windows.
//!
//! `nebula-sh` starts an interactive shell. `nebula-sh -c "line"` runs one line,
//! `nebula-sh script.sh args…` runs a script. `nebula-sh <command> [args]` runs one
//! of Nebula's commands (`ls`, `grep`…) directly; the shell uses that form to run
//! them in pipelines.

mod arith;
mod builtins;
mod commands;
mod coreutils;
mod editor;
mod exec;
mod expand;
mod extras;
mod icons;
mod parse;
mod prompt;
mod style;
mod suggest;
mod sys;
mod test;

use std::ffi::OsString;
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Functions can recurse deeply; give the interpreter more room than the default
/// 1 MB main-thread stack on Windows.
const STACK_SIZE: usize = 256 * 1024 * 1024;

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    let first = args.get(1).and_then(|arg| arg.to_str()).unwrap_or("");

    if commands::is_util(first) {
        sys::enable_ansi();
        let rest = args[1..].to_vec();
        let code = extras::run(first, rest.clone())
            .or_else(|| coreutils::run(first, rest))
            .unwrap_or(127);
        return exit(code);
    }

    let args: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    match std::thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(move || shell_main(&args))
    {
        Ok(handle) => handle.join().unwrap_or(ExitCode::FAILURE),
        Err(_) => ExitCode::FAILURE,
    }
}

fn shell_main(args: &[String]) -> ExitCode {
    let mut shell = exec::Shell::new();
    prepare_environment();
    ignore_interrupts(&shell);
    let first = args.get(1).map_or("", String::as_str);

    match first {
        "-c" => {
            let line = args.get(2).cloned().unwrap_or_default();
            if let Some(name) = args.get(3) {
                shell.script_name = name.clone();
            }
            shell.positional = args.get(4..).map(<[String]>::to_vec).unwrap_or_default();
            let status = shell.run_line(&line);
            exit(shell.exit_code.unwrap_or(status))
        }
        // Used by the shell itself to run pipeline stages and `( … )`.
        "--subshell" => {
            let path = args.get(2).cloned().unwrap_or_default();
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let _ = std::fs::remove_file(&path);
            shell.last_status = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
            if let Some(name) = args.get(4) {
                shell.script_name = name.clone();
            }
            let status = shell.run_line(&text);
            exit(shell.exit_code.unwrap_or(status))
        }
        "--version" | "-V" => {
            println!("nebula {VERSION}");
            ExitCode::SUCCESS
        }
        "--help" | "-h" => {
            println!(
                "Usage: nebula-sh [-c COMMAND [NAME [ARGS…]] | SCRIPT [ARGS…] | COMMAND [ARGS…]]"
            );
            ExitCode::SUCCESS
        }
        "" => {
            sys::enable_ansi();
            exit(editor::interactive(&mut shell))
        }
        script => match std::fs::read_to_string(sys::translate_path(script)) {
            Ok(text) => {
                shell.script_name = script.to_owned();
                shell.positional = args[2..].to_vec();
                let status = shell.run_line(&text);
                exit(shell.exit_code.unwrap_or(status))
            }
            Err(error) => {
                eprintln!("nebula: {script}: {}", exec::describe_io_error(&error));
                ExitCode::from(127)
            }
        },
    }
}

fn exit(code: i32) -> ExitCode {
    ExitCode::from(u8::try_from(code.rem_euclid(256)).unwrap_or(1))
}

/// Linux programs and scripts expect HOME, USER and SHELL.
fn prepare_environment() {
    if std::env::var_os("HOME").is_none() {
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            std::env::set_var("HOME", profile);
        }
    }
    if std::env::var_os("USER").is_none() {
        if let Some(user) = std::env::var_os("USERNAME") {
            std::env::set_var("USER", user);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        std::env::set_var("SHELL", exe);
    }
    if let Ok(cwd) = std::env::current_dir() {
        std::env::set_var("PWD", cwd);
    }
}

/// Ctrl+C stops the running program, never the shell itself.
fn ignore_interrupts(shell: &exec::Shell) {
    let flag = shell.interrupted.clone();
    let _ = ctrlc::set_handler(move || flag.store(true, std::sync::atomic::Ordering::SeqCst));
}

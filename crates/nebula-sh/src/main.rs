//! Nebula: a Linux-style command interpreter for Windows.
//!
//! `nebula-sh` starts an interactive shell. `nebula-sh -c "line"` runs one line.
//! `nebula-sh <command> [args]` runs one of Nebula's commands (`ls`, `grep`…)
//! directly; the shell uses that form to run them in pipelines.

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

use std::ffi::OsString;
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

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

    let mut shell = exec::Shell::new();
    prepare_environment();
    ignore_interrupts(&shell);

    match first {
        "-c" => {
            let line = args
                .get(2)
                .map(|arg| arg.to_string_lossy().into_owned())
                .unwrap_or_default();
            exit(shell.run_line(&line))
        }
        "--version" | "-V" => {
            println!("nebula {VERSION}");
            ExitCode::SUCCESS
        }
        "--help" | "-h" => {
            println!("Usage: nebula-sh [-c COMMAND | SCRIPT | COMMAND [ARGS…]]");
            ExitCode::SUCCESS
        }
        "" => {
            sys::enable_ansi();
            exit(editor::interactive(&mut shell))
        }
        script => match std::fs::read_to_string(script) {
            Ok(text) => exit(shell.run_line(&text)),
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

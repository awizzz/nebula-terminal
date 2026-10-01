//! `ps`, `kill`, `killall` and `pkill` on top of `sysinfo`, which works the same on
//! Windows and Linux.

use std::io::{self, Write};
use sysinfo::{
    Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, Signal, System, UpdateKind,
};

fn system() -> System {
    System::new_with_specifics(
        RefreshKind::nothing().with_processes(
            ProcessRefreshKind::nothing()
                .with_memory()
                .with_exe(UpdateKind::OnlyIfNotSet),
        ),
    )
}

fn human_memory(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.1}G", mb / 1024.0)
    } else {
        format!("{mb:.0}M")
    }
}

pub fn ps(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("--help") {
        println!(
            "Usage: ps [aux|-e|-ef]\nList running processes: PID, parent PID, memory and name."
        );
        return 0;
    }
    let mut system = system();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    let mut processes: Vec<_> = system.processes().values().collect();
    processes.sort_by_key(|process| process.pid());
    let color = super::stdout_is_terminal() && crate::style::enabled();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let header = format!("{:>7} {:>7} {:>7}  {}", "PID", "PPID", "MEM", "NAME");
    let _ = writeln!(
        out,
        "{}",
        if color {
            nu_ansi_term::Style::new().bold().paint(header).to_string()
        } else {
            header
        }
    );
    let me = std::process::id();
    for process in processes {
        let pid = process.pid().as_u32();
        let parent = process
            .parent()
            .map(|p| p.as_u32().to_string())
            .unwrap_or_else(|| "-".into());
        let name = process.name().to_string_lossy();
        let line = format!(
            "{pid:>7} {parent:>7} {:>7}  {name}",
            human_memory(process.memory())
        );
        let line = if color && pid == me {
            nu_ansi_term::Color::Green.paint(line).to_string()
        } else {
            line
        };
        let _ = writeln!(out, "{line}");
    }
    0
}

/// A signal by name (`TERM`, `SIGTERM`) or number. `Some(None)` is signal 0, which
/// only checks that the process exists.
fn signal(spec: &str) -> Option<Option<Signal>> {
    let name = spec.to_ascii_uppercase();
    let name = name.strip_prefix("SIG").unwrap_or(&name);
    Some(Some(match name {
        "0" => return Some(None),
        "1" | "HUP" => Signal::Hangup,
        "2" | "INT" => Signal::Interrupt,
        "3" | "QUIT" => Signal::Quit,
        "9" | "KILL" => Signal::Kill,
        "10" | "USR1" => Signal::User1,
        "12" | "USR2" => Signal::User2,
        "15" | "TERM" => Signal::Term,
        "18" | "CONT" => Signal::Continue,
        "19" | "STOP" => Signal::Stop,
        _ => return None,
    }))
}

/// Sends `signal`. Windows has no signals: every one of them ends the process.
fn send(process: &sysinfo::Process, signal: Signal) -> bool {
    process.kill_with(signal).unwrap_or_else(|| process.kill())
}

pub fn kill(args: &[String]) -> i32 {
    let mut pids = Vec::new();
    let mut chosen: Option<Signal> = Some(Signal::Term);
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-l" | "-L" => {
                println!("HUP INT QUIT KILL USR1 USR2 TERM CONT STOP");
                return 0;
            }
            "--help" => {
                println!("Usage: kill [-s SIGNAL | -SIGNAL] PID…\nStop processes. On Windows every signal except 0 ends the process.");
                return 0;
            }
            "-s" | "-n" => {
                let value = iter.next().map(String::as_str).unwrap_or_default();
                match signal(value) {
                    Some(signal) => chosen = signal,
                    None => {
                        eprintln!("kill: {value}: invalid signal specification");
                        return 1;
                    }
                }
            }
            other if other.starts_with('-') && other.len() > 1 => match signal(&other[1..]) {
                Some(signal) => chosen = signal,
                None => {
                    eprintln!("kill: {other}: invalid signal specification");
                    return 1;
                }
            },
            other => match other.parse::<u32>() {
                Ok(pid) => pids.push(pid),
                Err(_) => {
                    eprintln!("kill: {other}: arguments must be process IDs");
                    return 1;
                }
            },
        }
    }
    if pids.is_empty() {
        eprintln!("kill: usage: kill [-s SIGNAL | -SIGNAL] PID…");
        return 2;
    }
    let mut system = system();
    system.refresh_processes(ProcessesToUpdate::All, true);
    let mut status = 0;
    for pid in pids {
        match (system.process(Pid::from_u32(pid)), chosen) {
            (Some(_), None) => {}
            (Some(process), Some(signal)) if send(process, signal) => {}
            (Some(_), Some(_)) => {
                eprintln!("kill: ({pid}) - Operation not permitted");
                status = 1;
            }
            (None, _) => {
                if chosen.is_some() {
                    eprintln!("kill: ({pid}) - No such process");
                }
                status = 1;
            }
        }
    }
    status
}

pub fn killall(name: &str, args: &[String]) -> i32 {
    let targets: Vec<&String> = args.iter().filter(|arg| !arg.starts_with('-')).collect();
    if targets.is_empty() {
        eprintln!("Usage: {name} NAME…");
        return 2;
    }
    let mut system = system();
    system.refresh_processes(ProcessesToUpdate::All, true);
    let me = std::process::id();
    let mut killed = 0;
    for process in system.processes().values() {
        if process.pid().as_u32() == me {
            continue;
        }
        let process_name = process.name().to_string_lossy().to_lowercase();
        let stem = process_name.trim_end_matches(".exe");
        let matches = targets.iter().any(|target| {
            let target = target.to_lowercase();
            if name == "pkill" {
                process_name.contains(&target)
            } else {
                stem == target.trim_end_matches(".exe")
            }
        });
        if matches && send(process, Signal::Term) {
            killed += 1;
        }
    }
    if killed == 0 {
        eprintln!("{name}: no process found");
        return 1;
    }
    0
}

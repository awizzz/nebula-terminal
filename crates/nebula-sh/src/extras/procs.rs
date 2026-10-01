//! `ps`, `kill`, `killall` and `pkill` on top of `sysinfo`, which works the same on
//! Windows and Linux.

use std::io::{self, Write};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System, UpdateKind};

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

pub fn kill(args: &[String]) -> i32 {
    let mut pids = Vec::new();
    for arg in args {
        match arg.as_str() {
            "-l" => {
                println!("TERM KILL INT HUP");
                return 0;
            }
            "--help" => {
                println!("Usage: kill [-9|-s SIGNAL] PID…\nStop processes. On Windows every signal ends the process.");
                return 0;
            }
            "-s" => {}
            other if other.starts_with('-') => {}
            other => match other.parse::<u32>() {
                Ok(pid) => pids.push(pid),
                Err(_)
                    if matches!(
                        other,
                        "TERM" | "KILL" | "INT" | "HUP" | "SIGTERM" | "SIGKILL"
                    ) => {}
                Err(_) => {
                    eprintln!("kill: {other}: arguments must be process IDs");
                    return 1;
                }
            },
        }
    }
    if pids.is_empty() {
        eprintln!("kill: usage: kill [-9] PID…");
        return 2;
    }
    let mut system = system();
    system.refresh_processes(ProcessesToUpdate::All, true);
    let mut status = 0;
    for pid in pids {
        match system.process(Pid::from_u32(pid)) {
            Some(process) if process.kill() => {}
            Some(_) => {
                eprintln!("kill: ({pid}) - Operation not permitted");
                status = 1;
            }
            None => {
                eprintln!("kill: ({pid}) - No such process");
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
        if matches && process.kill() {
            killed += 1;
        }
    }
    if killed == 0 {
        eprintln!("{name}: no process found");
        return 1;
    }
    0
}

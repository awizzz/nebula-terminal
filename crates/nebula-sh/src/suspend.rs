//! Pausing programs, which Windows has no signal for: every thread of a process, and of
//! the processes it started, is suspended, and resumed the same way.
//!
//! Ctrl+Z comes from Nebula Terminal. While a command runs, the keyboard belongs to the
//! program, so the terminal doesn't send the key: it sets an event named after this
//! shell's process, `Local\NebulaTerminal.Suspend.<pid>`, which the shell waits on next
//! to the program it runs.

use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set from Ctrl+Z until the next prompt: everything still waited on stops too, so a
/// whole pipeline pauses, and the rest of the command line doesn't run.
static SUSPENDING: AtomicBool = AtomicBool::new(false);

pub fn suspending() -> bool {
    SUSPENDING.load(Ordering::SeqCst)
}

/// Back at the prompt.
pub fn settle() {
    SUSPENDING.store(false, Ordering::SeqCst);
}

pub enum Waited {
    Exited(std::process::ExitStatus),
    /// The program was paused; the caller keeps it as a stopped job.
    #[cfg_attr(not(windows), allow(dead_code))]
    Suspended,
}

#[cfg(windows)]
mod platform {
    use super::{Waited, SUSPENDING};
    use std::collections::{HashMap, HashSet};
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;
    use std::sync::atomic::Ordering;
    use std::sync::OnceLock;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, Thread32First,
                Thread32Next, PROCESSENTRY32W, TH32CS_SNAPPROCESS, TH32CS_SNAPTHREAD,
                THREADENTRY32,
            },
            Threading::{
                CreateEventW, OpenThread, ResumeThread, SuspendThread, WaitForMultipleObjects,
                WaitForSingleObject, INFINITE, THREAD_SUSPEND_RESUME,
            },
        },
    };

    struct Event(HANDLE);
    // SAFETY: an event handle can be waited on from any thread.
    unsafe impl Send for Event {}
    unsafe impl Sync for Event {}

    static EVENT: OnceLock<Event> = OnceLock::new();

    pub fn listen() {
        let name: Vec<u16> = format!("Local\\NebulaTerminal.Suspend.{}\0", std::process::id())
            .encode_utf16()
            .collect();
        // SAFETY: an unnamed security descriptor and a NUL-terminated name; auto-reset.
        let handle = unsafe { CreateEventW(std::ptr::null(), 0, 0, name.as_ptr()) };
        if handle.is_null() {
            return;
        }
        let event = EVENT.get_or_init(|| Event(handle));
        let handle = event.0 as usize;
        std::thread::spawn(move || loop {
            // SAFETY: the event lives as long as the process.
            if unsafe { WaitForSingleObject(handle as HANDLE, INFINITE) } != WAIT_OBJECT_0 {
                break;
            }
            SUSPENDING.store(true, Ordering::SeqCst);
        });
    }

    /// Waits for the program, or pauses it when Ctrl+Z comes first.
    pub fn wait(child: &mut Child) -> std::io::Result<Waited> {
        let Some(event) = EVENT.get() else {
            return child.wait().map(Waited::Exited);
        };
        loop {
            if SUSPENDING.load(Ordering::SeqCst) {
                if let Some(status) = child.try_wait()? {
                    return Ok(Waited::Exited(status));
                }
                suspend_tree(child.id());
                return Ok(Waited::Suspended);
            }
            let handles = [child.as_raw_handle() as HANDLE, event.0];
            // SAFETY: both handles stay open for the call. A signalled event wakes the
            // listener thread too, but this wait looks at SUSPENDING again either way.
            let woke = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, 250) };
            if woke == WAIT_OBJECT_0 {
                return child.wait().map(Waited::Exited);
            }
            if woke == WAIT_OBJECT_0 + 1 {
                SUSPENDING.store(true, Ordering::SeqCst);
            }
        }
    }

    /// The process and everything it started, children of children included.
    fn tree(pid: u32) -> HashSet<u32> {
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        // SAFETY: a snapshot owned and closed here; the entry has its size set.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot != INVALID_HANDLE_VALUE {
                let mut entry: PROCESSENTRY32W = std::mem::zeroed();
                entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
                let mut more = Process32FirstW(snapshot, &mut entry) != 0;
                while more {
                    children
                        .entry(entry.th32ParentProcessID)
                        .or_default()
                        .push(entry.th32ProcessID);
                    more = Process32NextW(snapshot, &mut entry) != 0;
                }
                CloseHandle(snapshot);
            }
        }
        let mut found = HashSet::from([pid]);
        let mut queue = vec![pid];
        while let Some(parent) = queue.pop() {
            for child in children.get(&parent).into_iter().flatten() {
                // Process ids are reused: a "child" can't be older than this walk's root, but
                // a loop in the ids must not run forever.
                if found.insert(*child) {
                    queue.push(*child);
                }
            }
        }
        found
    }

    fn each_thread(pid: u32, action: unsafe extern "system" fn(HANDLE) -> u32) -> bool {
        let processes = tree(pid);
        let mut touched = false;
        // SAFETY: as in `tree`; each thread handle is closed after use.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return false;
            }
            let mut entry: THREADENTRY32 = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
            let mut more = Thread32First(snapshot, &mut entry) != 0;
            while more {
                if processes.contains(&entry.th32OwnerProcessID) {
                    let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                    if !thread.is_null() {
                        touched |= action(thread) != u32::MAX;
                        CloseHandle(thread);
                    }
                }
                more = Thread32Next(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
        }
        touched
    }

    pub fn suspend_tree(pid: u32) -> bool {
        each_thread(pid, SuspendThread)
    }

    pub fn resume_tree(pid: u32) -> bool {
        each_thread(pid, ResumeThread)
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Waited;
    use std::process::Child;

    pub fn listen() {}

    pub fn wait(child: &mut Child) -> std::io::Result<Waited> {
        child.wait().map(Waited::Exited)
    }

    fn signal(pid: u32, name: &str) -> bool {
        std::process::Command::new("kill")
            .args([name, &pid.to_string()])
            .status()
            .is_ok_and(|status| status.success())
    }

    pub fn suspend_tree(pid: u32) -> bool {
        signal(pid, "-STOP")
    }

    pub fn resume_tree(pid: u32) -> bool {
        signal(pid, "-CONT")
    }
}

/// Starts listening for Ctrl+Z from Nebula Terminal (interactive shells only).
pub fn listen() {
    platform::listen();
}

pub fn wait(child: &mut Child) -> std::io::Result<Waited> {
    platform::wait(child)
}

pub fn suspend_tree(pid: u32) -> bool {
    platform::suspend_tree(pid)
}

pub fn resume_tree(pid: u32) -> bool {
    platform::resume_tree(pid)
}

use crate::{
    custom::CustomProfiles,
    integration::{Kind, Scripts},
    profiles,
};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
};
use tauri::{ipc::Channel, State};

#[derive(Default)]
struct Utf8StreamDecoder {
    pending: Vec<u8>,
}

impl Utf8StreamDecoder {
    fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut output = String::new();

        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(valid) => {
                    output.push_str(valid);
                    self.pending.clear();
                    break;
                }
                Err(error) => {
                    let valid_up_to = error.valid_up_to();
                    let invalid_length = error.error_len();
                    if valid_up_to > 0 {
                        output.push_str(
                            std::str::from_utf8(&self.pending[..valid_up_to])
                                .expect("validated UTF-8 prefix"),
                        );
                        self.pending.drain(..valid_up_to);
                    }
                    match invalid_length {
                        Some(length) => {
                            self.pending.drain(..length);
                            output.push('\u{fffd}');
                        }
                        None => break,
                    }
                }
            }
        }

        output
    }

    fn finish(&mut self) -> String {
        if self.pending.is_empty() {
            String::new()
        } else {
            self.pending.clear();
            "\u{fffd}".to_owned()
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum PtyEvent {
    Output { session_id: String, chunk: String },
    Exit { session_id: String, code: u32 },
    Error { session_id: String, message: String },
}

/// Work for a session's I/O thread. One thread per session applies input and
/// resizes in the order the UI sent them.
enum Request {
    Write(String),
    Resize(PtySize),
}

struct SessionHandle {
    requests: Mutex<mpsc::Sender<Request>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
}

/// Owns the PTY's writer and master. It ends when the session is dropped, and the
/// pseudo-console closes on this thread, never while a lock is held.
fn run_io(
    receiver: mpsc::Receiver<Request>,
    mut writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    channel: Channel<PtyEvent>,
    session_id: String,
) {
    for request in receiver {
        let result = match request {
            Request::Write(data) => writer
                .write_all(data.as_bytes())
                .and_then(|_| writer.flush())
                .map_err(|error| format!("PTY write failed: {error}")),
            Request::Resize(size) => master
                .resize(size)
                .map_err(|error| format!("PTY resize failed: {error}")),
        };
        if let Err(message) = result {
            let _ = channel.send(PtyEvent::Error {
                session_id: session_id.clone(),
                message,
            });
        }
    }
}

struct PtyStateInner {
    sessions: Mutex<HashMap<String, Arc<SessionHandle>>>,
    next_id: AtomicU64,
}

#[derive(Clone)]
pub struct PtyState {
    inner: Arc<PtyStateInner>,
}

impl Default for PtyState {
    fn default() -> Self {
        Self {
            inner: Arc::new(PtyStateInner {
                sessions: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(1),
            }),
        }
    }
}

fn lock_error(name: &str) -> String {
    format!("Internal PTY {name} lock is poisoned.")
}

/// A starting folder the user typed: it must exist, or the session doesn't start.
fn existing_directory(folder: &str) -> Result<PathBuf, String> {
    let directory = profiles::expand_directory(folder);
    if directory.is_dir() {
        Ok(directory)
    } else {
        Err(format!(
            "Starting directory '{}' does not exist or is not a directory.",
            directory.display()
        ))
    }
}

/// `cwd` is the folder typed in Settings and `known_folder` the Desktop or Documents
/// choice. `start_in` is where this pane should reopen: the folder of the pane it was
/// split from, or the one it was in when the app closed. It only applies while it
/// still exists. `shell_integration` lets PowerShell, Git Bash and WSL mark their commands.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn start_session(
    profile_id: String,
    cols: u16,
    rows: u16,
    cwd: Option<String>,
    known_folder: Option<String>,
    start_in: Option<String>,
    shell_integration: bool,
    on_event: Channel<PtyEvent>,
    state: State<'_, PtyState>,
    custom_profiles: State<'_, CustomProfiles>,
    scripts: State<'_, Scripts>,
) -> Result<String, String> {
    let cwd = cwd.filter(|value| !value.trim().is_empty());
    let start_in = start_in
        .map(|folder| profiles::expand_directory(&folder))
        .filter(|folder| folder.is_dir());
    let known_folder = known_folder.as_deref().and_then(profiles::known_folder);
    let custom_cwd = cwd.is_some() || known_folder.is_some() || start_in.is_some();
    let mut profile = profiles::resolve_profile(&profile_id, custom_cwd, &custom_profiles)?;
    if let Some(kind) = Kind::of(&profile_id).filter(|_| shell_integration) {
        scripts.apply(kind, &mut profile.args);
    }
    // A profile's own starting folder wins, then the pane's own, then Settings.
    let directory = match (&profile.cwd, start_in, &cwd) {
        (Some(folder), _, _) => Some(existing_directory(folder)?),
        (None, Some(folder), _) => Some(folder),
        (None, None, Some(folder)) => Some(existing_directory(folder)?),
        (None, None, None) => known_folder,
    };
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| format!("Unable to create PTY: {error}"))?;

    let mut command = CommandBuilder::new(&profile.executable);
    command.args(&profile.args);
    if let Some(directory) = directory {
        command.cwd(directory);
    } else if let Some(home) = profiles::home_directory() {
        command.cwd(home);
    } else if let Ok(current) = std::env::current_dir() {
        command.cwd(current);
    }
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TERM_PROGRAM", "NebulaTerminal");
    command.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
    command.env("NEBULA_TERMINAL", "1");

    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("Unable to start profile '{profile_id}': {error}"))?;
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("Unable to read PTY output: {error}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("Unable to open PTY input: {error}"))?;
    let killer = child.clone_killer();

    let session_id = format!("s{}", state.inner.next_id.fetch_add(1, Ordering::Relaxed));
    let (requests, receiver) = mpsc::channel();
    let io_channel = on_event.clone();
    let io_session_id = session_id.clone();
    let master = pair.master;
    thread::spawn(move || run_io(receiver, writer, master, io_channel, io_session_id));
    let handle = Arc::new(SessionHandle {
        requests: Mutex::new(requests),
        killer: Mutex::new(killer),
    });

    state
        .inner
        .sessions
        .lock()
        .map_err(|_| lock_error("session"))?
        .insert(session_id.clone(), handle);

    let output_channel = on_event.clone();
    let output_session_id = session_id.clone();
    thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        let mut decoder = Utf8StreamDecoder::default();
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => {
                    let chunk = decoder.finish();
                    if !chunk.is_empty() {
                        let _ = output_channel.send(PtyEvent::Output {
                            session_id: output_session_id.clone(),
                            chunk,
                        });
                    }
                    break;
                }
                Ok(count) => {
                    let chunk = decoder.push(&buffer[..count]);
                    if chunk.is_empty() {
                        continue;
                    }
                    if output_channel
                        .send(PtyEvent::Output {
                            session_id: output_session_id.clone(),
                            chunk,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                Err(error) => {
                    let _ = output_channel.send(PtyEvent::Error {
                        session_id: output_session_id.clone(),
                        message: format!("PTY read failed: {error}"),
                    });
                    break;
                }
            }
        }
    });

    let state_inner = state.inner.clone();
    let wait_channel = on_event;
    let wait_session_id = session_id.clone();
    thread::spawn(move || {
        let result = child.wait();
        let code = result
            .as_ref()
            .map(|status| status.exit_code())
            .unwrap_or(1);
        if let Err(error) = result {
            let _ = wait_channel.send(PtyEvent::Error {
                session_id: wait_session_id.clone(),
                message: format!("Process wait failed: {error}"),
            });
        }
        let _ = wait_channel.send(PtyEvent::Exit {
            session_id: wait_session_id.clone(),
            code,
        });
        // Dropped after the lock is released: the last handle closes the pseudo-console.
        let removed = state_inner
            .sessions
            .lock()
            .ok()
            .and_then(|mut sessions| sessions.remove(&wait_session_id));
        drop(removed);
    });

    Ok(session_id)
}

fn session(state: &PtyState, session_id: &str) -> Result<Arc<SessionHandle>, String> {
    state
        .inner
        .sessions
        .lock()
        .map_err(|_| lock_error("session"))?
        .get(session_id)
        .cloned()
        .ok_or_else(|| format!("Session '{session_id}' does not exist."))
}

fn send(state: &PtyState, session_id: &str, request: Request) -> Result<(), String> {
    session(state, session_id)?
        .requests
        .lock()
        .map_err(|_| lock_error("request"))?
        .send(request)
        .map_err(|_| format!("Session '{session_id}' has ended."))
}

// Input and resizes are plain (not async) commands: Tauri runs those in the order
// they arrive, and they only queue work for the session's I/O thread.
#[tauri::command]
pub fn write_session(
    session_id: String,
    data: String,
    state: State<'_, PtyState>,
) -> Result<(), String> {
    send(&state, &session_id, Request::Write(data))
}

#[tauri::command]
pub fn resize_session(
    session_id: String,
    cols: u16,
    rows: u16,
    state: State<'_, PtyState>,
) -> Result<(), String> {
    send(
        &state,
        &session_id,
        Request::Resize(PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        }),
    )
}

#[tauri::command]
pub async fn close_session(session_id: String, state: State<'_, PtyState>) -> Result<(), String> {
    let session = state
        .inner
        .sessions
        .lock()
        .map_err(|_| lock_error("session"))?
        .remove(&session_id);

    if let Some(session) = session {
        // portable-pty reports TerminateProcess the wrong way round on Windows, so the
        // result says nothing useful; the wait thread reports the exit either way.
        if let Ok(mut killer) = session.killer.lock() {
            let _ = killer.kill();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Utf8StreamDecoder;

    #[test]
    fn preserves_characters_split_across_reads() {
        let mut decoder = Utf8StreamDecoder::default();
        assert_eq!(decoder.push(&[0x61, 0xf0, 0x9f]), "a");
        assert_eq!(decoder.push(&[0x8c, 0x8c, 0x62]), "🌌b");
        assert_eq!(decoder.finish(), "");
    }

    #[test]
    fn replaces_invalid_sequences_without_losing_valid_text() {
        let mut decoder = Utf8StreamDecoder::default();
        assert_eq!(decoder.push(&[b'a', 0xff, b'b']), "a�b");
    }

    #[test]
    fn flushes_an_incomplete_character_at_end_of_stream() {
        let mut decoder = Utf8StreamDecoder::default();
        assert_eq!(decoder.push(&[0xe2, 0x82]), "");
        assert_eq!(decoder.finish(), "�");
    }
}

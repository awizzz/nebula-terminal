use crate::profiles;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
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
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum PtyEvent {
    Output { session_id: String, chunk: String },
    Exit { session_id: String, code: u32 },
    Error { session_id: String, message: String },
}

struct SessionHandle {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
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

#[tauri::command]
pub fn start_session(
    profile_id: String,
    cols: u16,
    rows: u16,
    cwd: Option<String>,
    on_event: Channel<PtyEvent>,
    state: State<'_, PtyState>,
) -> Result<String, String> {
    let profile = profiles::resolve_profile(&profile_id)?;
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
    if let Some(requested) = cwd.filter(|value| !value.trim().is_empty()) {
        let directory = profiles::expand_directory(&requested);
        if !directory.is_dir() {
            return Err(format!(
                "Starting directory '{}' does not exist or is not a directory.",
                directory.display()
            ));
        }
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
    let handle = Arc::new(SessionHandle {
        master: Mutex::new(pair.master),
        writer: Mutex::new(writer),
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
        if let Ok(mut sessions) = state_inner.sessions.lock() {
            sessions.remove(&wait_session_id);
        }
    });

    Ok(session_id)
}

#[tauri::command]
pub fn write_session(
    session_id: String,
    data: String,
    state: State<'_, PtyState>,
) -> Result<(), String> {
    let session = state
        .inner
        .sessions
        .lock()
        .map_err(|_| lock_error("session"))?
        .get(&session_id)
        .cloned()
        .ok_or_else(|| format!("Session '{session_id}' does not exist."))?;

    let mut writer = session.writer.lock().map_err(|_| lock_error("writer"))?;
    writer
        .write_all(data.as_bytes())
        .and_then(|_| writer.flush())
        .map_err(|error| format!("PTY write failed: {error}"))
}

#[tauri::command]
pub fn resize_session(
    session_id: String,
    cols: u16,
    rows: u16,
    state: State<'_, PtyState>,
) -> Result<(), String> {
    let session = state
        .inner
        .sessions
        .lock()
        .map_err(|_| lock_error("session"))?
        .get(&session_id)
        .cloned()
        .ok_or_else(|| format!("Session '{session_id}' does not exist."))?;

    let result = session
        .master
        .lock()
        .map_err(|_| lock_error("master"))?
        .resize(PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| format!("PTY resize failed: {error}"));

    result
}

#[tauri::command]
pub fn close_session(session_id: String, state: State<'_, PtyState>) -> Result<(), String> {
    let session = state
        .inner
        .sessions
        .lock()
        .map_err(|_| lock_error("session"))?
        .remove(&session_id);

    if let Some(session) = session {
        session
            .killer
            .lock()
            .map_err(|_| lock_error("killer"))?
            .kill()
            .map_err(|error| format!("Unable to terminate PTY process: {error}"))?;
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

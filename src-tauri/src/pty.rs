use crate::{
    custom::CustomProfiles,
    integration::{Kind, Scripts},
    profiles,
};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    io::{self, Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
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
    events: Arc<Events>,
}

/// How much output a session keeps while its tab moves to another window.
const MAX_PARKED_BYTES: usize = 4 * 1024 * 1024;
/// A parked session nobody attaches to within this time is stopped.
const PARKED_FOR_AT_MOST: Duration = Duration::from_secs(30);

/// Where a session's events go: the pane that shows it, or a queue while that pane moves
/// to another window. The shell keeps running either way.
enum Sink {
    Pane(Channel<PtyEvent>),
    Parked {
        queue: VecDeque<PtyEvent>,
        bytes: usize,
        generation: u64,
    },
}

struct Events {
    sink: Mutex<Sink>,
    parkings: AtomicU64,
}

impl Events {
    fn new(channel: Channel<PtyEvent>) -> Arc<Self> {
        Arc::new(Self {
            sink: Mutex::new(Sink::Pane(channel)),
            parkings: AtomicU64::new(0),
        })
    }

    /// False once the pane's window is gone, so the reader can stop.
    fn send(&self, event: PtyEvent) -> bool {
        let Ok(mut sink) = self.sink.lock() else {
            return false;
        };
        match &mut *sink {
            Sink::Pane(channel) => channel.send(event).is_ok(),
            Sink::Parked { queue, bytes, .. } => {
                *bytes += event.size();
                queue.push_back(event);
                // The oldest output goes first; the pane gets its screen from the old window anyway.
                while *bytes > MAX_PARKED_BYTES {
                    match queue.pop_front() {
                        Some(dropped) => *bytes -= dropped.size(),
                        None => break,
                    }
                }
                true
            }
        }
    }

    /// Queues events from now on, and says which parking this is.
    fn park(&self) -> Result<u64, String> {
        let generation = self.parkings.fetch_add(1, Ordering::Relaxed) + 1;
        *self.sink.lock().map_err(|_| lock_error("event"))? = Sink::Parked {
            queue: VecDeque::new(),
            bytes: 0,
            generation,
        };
        Ok(generation)
    }

    /// Sends what was queued to the new pane, then everything that follows.
    fn attach(&self, channel: Channel<PtyEvent>) -> Result<(), String> {
        let mut sink = self.sink.lock().map_err(|_| lock_error("event"))?;
        if let Sink::Parked { queue, .. } = &mut *sink {
            for event in queue.drain(..) {
                let _ = channel.send(event);
            }
        }
        *sink = Sink::Pane(channel);
        Ok(())
    }

    fn still_parked(&self, generation: u64) -> bool {
        self.sink
            .lock()
            .is_ok_and(|sink| matches!(&*sink, Sink::Parked { generation: current, .. } if *current == generation))
    }
}

impl PtyEvent {
    fn size(&self) -> usize {
        match self {
            PtyEvent::Output { chunk, .. } => chunk.len(),
            PtyEvent::Error { message, .. } => message.len(),
            PtyEvent::Exit { .. } => 0,
        }
    }
}

/// The console behind a session, which takes its resizes: the pseudo-console the app
/// created, or one Windows handed over when Nebula is the default terminal.
pub trait Console: Send {
    fn resize(&mut self, size: PtySize) -> io::Result<()>;
}

impl Console for Box<dyn MasterPty + Send> {
    fn resize(&mut self, size: PtySize) -> io::Result<()> {
        MasterPty::resize(&**self, size).map_err(|error| io::Error::other(error.to_string()))
    }
}

/// Everything a session runs on, however its console was made.
pub struct SessionParts {
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
    pub console: Box<dyn Console>,
    pub killer: Box<dyn ChildKiller + Send + Sync>,
    /// Blocks until the program exits, then gives its exit code.
    pub wait: Box<dyn FnOnce() -> io::Result<u32> + Send>,
}

/// A console Windows handed over, waiting for a tab to attach to it.
struct PendingHandoff {
    id: String,
    title: String,
    announced: bool,
    parts: SessionParts,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffInfo {
    id: String,
    title: String,
}

/// Owns the session's writer and console. It ends when the session is dropped, and
/// the console closes on this thread, never while a lock is held.
fn run_io(
    receiver: mpsc::Receiver<Request>,
    mut writer: Box<dyn Write + Send>,
    mut console: Box<dyn Console>,
    events: Arc<Events>,
    session_id: String,
) {
    for request in receiver {
        let result = match request {
            Request::Write(data) => writer
                .write_all(data.as_bytes())
                .and_then(|_| writer.flush())
                .map_err(|error| format!("PTY write failed: {error}")),
            Request::Resize(size) => console
                .resize(size)
                .map_err(|error| format!("PTY resize failed: {error}")),
        };
        if let Err(message) = result {
            events.send(PtyEvent::Error {
                session_id: session_id.clone(),
                message,
            });
        }
    }
}

struct PtyStateInner {
    sessions: Mutex<HashMap<String, Arc<SessionHandle>>>,
    handoffs: Mutex<Vec<PendingHandoff>>,
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
                handoffs: Mutex::new(Vec::new()),
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

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("Unable to read PTY output: {error}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("Unable to open PTY input: {error}"))?;
    let killer = child.clone_killer();
    let parts = SessionParts {
        reader,
        writer,
        console: Box::new(pair.master),
        killer,
        wait: Box::new(move || child.wait().map(|status| status.exit_code())),
    };
    spawn_session(&state, parts, on_event)
}

/// Starts the threads of a session and registers it: one for input and resizes, one
/// streaming output to the pane, one waiting for the program to exit.
fn spawn_session(
    state: &PtyState,
    parts: SessionParts,
    on_event: Channel<PtyEvent>,
) -> Result<String, String> {
    let SessionParts {
        mut reader,
        writer,
        console,
        killer,
        wait,
    } = parts;
    let session_id = format!("s{}", state.inner.next_id.fetch_add(1, Ordering::Relaxed));
    let (requests, receiver) = mpsc::channel();
    let events = Events::new(on_event);
    let io_events = events.clone();
    let io_session_id = session_id.clone();
    thread::spawn(move || run_io(receiver, writer, console, io_events, io_session_id));
    let handle = Arc::new(SessionHandle {
        requests: Mutex::new(requests),
        killer: Mutex::new(killer),
        events: events.clone(),
    });

    state
        .inner
        .sessions
        .lock()
        .map_err(|_| lock_error("session"))?
        .insert(session_id.clone(), handle);

    let output_events = events.clone();
    let output_session_id = session_id.clone();
    thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        let mut decoder = Utf8StreamDecoder::default();
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => {
                    let chunk = decoder.finish();
                    if !chunk.is_empty() {
                        output_events.send(PtyEvent::Output {
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
                    if !output_events.send(PtyEvent::Output {
                        session_id: output_session_id.clone(),
                        chunk,
                    }) {
                        break;
                    }
                }
                Err(error) => {
                    output_events.send(PtyEvent::Error {
                        session_id: output_session_id.clone(),
                        message: format!("PTY read failed: {error}"),
                    });
                    break;
                }
            }
        }
    });

    let state_inner = state.inner.clone();
    let wait_events = events;
    let wait_session_id = session_id.clone();
    thread::spawn(move || {
        let result = wait();
        let code = *result.as_ref().unwrap_or(&1);
        if let Err(error) = result {
            wait_events.send(PtyEvent::Error {
                session_id: wait_session_id.clone(),
                message: format!("Process wait failed: {error}"),
            });
        }
        wait_events.send(PtyEvent::Exit {
            session_id: wait_session_id.clone(),
            code,
        });
        // Dropped after the lock is released: the last handle closes the console.
        let removed = state_inner
            .sessions
            .lock()
            .ok()
            .and_then(|mut sessions| sessions.remove(&wait_session_id));
        drop(removed);
    });

    Ok(session_id)
}

impl PtyState {
    /// Keeps a console Windows handed over until a tab attaches to it. A console that
    /// asked to be hidden runs without a tab until its program exits.
    pub fn receive_handoff(&self, title: String, hidden: bool, parts: SessionParts) {
        if hidden {
            let _ = spawn_session(self, parts, Channel::new(|_| Ok(())));
            return;
        }
        let id = format!("h{}", self.inner.next_id.fetch_add(1, Ordering::Relaxed));
        if let Ok(mut handoffs) = self.inner.handoffs.lock() {
            handoffs.push(PendingHandoff {
                id,
                title,
                announced: false,
                parts,
            });
        }
    }
}

/// Consoles handed over since the last call, for the UI to open tabs for.
#[tauri::command]
pub fn take_handoffs(state: State<'_, PtyState>) -> Vec<HandoffInfo> {
    let Ok(mut handoffs) = state.inner.handoffs.lock() else {
        return Vec::new();
    };
    handoffs
        .iter_mut()
        .filter(|handoff| !handoff.announced)
        .map(|handoff| {
            handoff.announced = true;
            HandoffInfo {
                id: handoff.id.clone(),
                title: handoff.title.clone(),
            }
        })
        .collect()
}

/// Connects a tab to a console that was handed over, at the tab's size.
#[tauri::command]
pub fn attach_handoff(
    handoff_id: String,
    cols: u16,
    rows: u16,
    on_event: Channel<PtyEvent>,
    state: State<'_, PtyState>,
) -> Result<String, String> {
    let pending = {
        let mut handoffs = state
            .inner
            .handoffs
            .lock()
            .map_err(|_| lock_error("handoff"))?;
        let index = handoffs
            .iter()
            .position(|handoff| handoff.id == handoff_id)
            .ok_or_else(|| "This console is already open or has closed.".to_owned())?;
        handoffs.remove(index)
    };
    let session_id = spawn_session(&state, pending.parts, on_event)?;
    send(
        &state,
        &session_id,
        Request::Resize(PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        }),
    )?;
    Ok(session_id)
}

/// Holds a session's output while its tab moves to another window. If no window takes
/// it in time, the shell is stopped rather than left running out of sight.
#[tauri::command]
pub fn park_session(session_id: String, state: State<'_, PtyState>) -> Result<(), String> {
    let handle = session(&state, &session_id)?;
    let generation = handle.events.park()?;
    let inner = state.inner.clone();
    thread::spawn(move || {
        thread::sleep(PARKED_FOR_AT_MOST);
        if !handle.events.still_parked(generation) {
            return;
        }
        let removed = inner
            .sessions
            .lock()
            .ok()
            .and_then(|mut sessions| sessions.remove(&session_id));
        if let Some(removed) = removed {
            if let Ok(mut killer) = removed.killer.lock() {
                let _ = killer.kill();
            }
        }
    });
    Ok(())
}

/// Connects a parked session to the pane that shows it now, at that pane's size.
#[tauri::command]
pub fn attach_session(
    session_id: String,
    cols: u16,
    rows: u16,
    on_event: Channel<PtyEvent>,
    state: State<'_, PtyState>,
) -> Result<(), String> {
    session(&state, &session_id)?.events.attach(on_event)?;
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
    use super::{Events, PtyEvent, Utf8StreamDecoder, MAX_PARKED_BYTES};
    use std::sync::{Arc, Mutex};
    use tauri::ipc::{Channel, InvokeResponseBody};

    fn recording() -> (Channel<PtyEvent>, Arc<Mutex<Vec<String>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let channel = Channel::new(move |body| {
            if let InvokeResponseBody::Json(json) = body {
                log.lock().unwrap().push(json);
            }
            Ok(())
        });
        (channel, seen)
    }

    fn output(chunk: &str) -> PtyEvent {
        PtyEvent::Output {
            session_id: "s1".into(),
            chunk: chunk.into(),
        }
    }

    #[test]
    fn a_parked_session_catches_up_in_its_new_pane() {
        let (first, first_seen) = recording();
        let events = Events::new(first);
        assert!(events.send(output("one")));
        let generation = events.park().unwrap();
        assert!(events.still_parked(generation));
        assert!(events.send(output("two")));
        assert!(events.send(output("three")));
        assert_eq!(first_seen.lock().unwrap().len(), 1);

        let (second, second_seen) = recording();
        events.attach(second).unwrap();
        assert!(!events.still_parked(generation));
        assert!(events.send(output("four")));
        let seen = second_seen.lock().unwrap();
        assert_eq!(seen.len(), 3);
        assert!(seen[0].contains("two") && seen[1].contains("three") && seen[2].contains("four"));
    }

    #[test]
    fn a_parked_session_keeps_only_recent_output() {
        let (channel, _) = recording();
        let events = Events::new(channel);
        events.park().unwrap();
        let chunk = "x".repeat(1024 * 1024);
        for _ in 0..6 {
            events.send(output(&chunk));
        }
        events.send(output("last"));
        let (next, seen) = recording();
        events.attach(next).unwrap();
        let seen = seen.lock().unwrap();
        assert!(seen.len() <= MAX_PARKED_BYTES / chunk.len() + 1);
        assert!(seen.last().unwrap().contains("last"));
    }

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

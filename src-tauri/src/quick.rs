//! The quick terminal: a window that drops down from the top of the screen on a global
//! shortcut, over whatever is in front, and goes away on the same shortcut.
//!
//! Its shortcut names a physical key, as `KeyboardEvent.code` does: `Win+Backquote` is
//! the key under Escape on every layout (` in QWERTY, ² in AZERTY). It becomes the
//! layout's virtual key only when it's registered.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "quick";

/// Share of the screen's height the window takes, at most and at least.
const TALLEST: f64 = 0.9;
const SHORTEST: f64 = 0.25;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Key {
    /// A key that types a character, by scan code: its virtual key depends on the layout.
    Scan(u32),
    /// F1–F24 and the navigation keys, the same on every layout.
    Virtual(u32),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Shortcut {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub key: Key,
}

fn key(code: &str) -> Option<Key> {
    const ROWS: [(&str, u32); 3] = [("QWERTYUIOP", 0x10), ("ASDFGHJKL", 0x1e), ("ZXCVBNM", 0x2c)];
    if let Some(letter) = code.strip_prefix("Key").filter(|rest| rest.len() == 1) {
        return ROWS.iter().find_map(|(row, first)| {
            row.find(letter)
                .map(|index| Key::Scan(first + index as u32))
        });
    }
    if let Some(digit) = code
        .strip_prefix("Digit")
        .and_then(|rest| rest.parse::<u32>().ok())
    {
        return (digit <= 9).then(|| Key::Scan(if digit == 0 { 0x0b } else { digit + 1 }));
    }
    if let Some(number) = code
        .strip_prefix('F')
        .and_then(|rest| rest.parse::<u32>().ok())
    {
        // VK_F1 is 0x70.
        return (1..=24)
            .contains(&number)
            .then(|| Key::Virtual(0x6f + number));
    }
    Some(match code {
        "Backquote" => Key::Scan(0x29),
        "Minus" => Key::Scan(0x0c),
        "Equal" => Key::Scan(0x0d),
        "BracketLeft" => Key::Scan(0x1a),
        "BracketRight" => Key::Scan(0x1b),
        "Semicolon" => Key::Scan(0x27),
        "Quote" => Key::Scan(0x28),
        "Backslash" => Key::Scan(0x2b),
        "Comma" => Key::Scan(0x33),
        "Period" => Key::Scan(0x34),
        "Slash" => Key::Scan(0x35),
        "IntlBackslash" => Key::Scan(0x56),
        "Space" => Key::Scan(0x39),
        "PageUp" => Key::Virtual(0x21),
        "PageDown" => Key::Virtual(0x22),
        "End" => Key::Virtual(0x23),
        "Home" => Key::Virtual(0x24),
        "ArrowLeft" => Key::Virtual(0x25),
        "ArrowUp" => Key::Virtual(0x26),
        "ArrowRight" => Key::Virtual(0x27),
        "ArrowDown" => Key::Virtual(0x28),
        "Insert" => Key::Virtual(0x2d),
        "Delete" => Key::Virtual(0x2e),
        _ => return None,
    })
}

/// Reads "Ctrl+Alt+Win+Backquote". A global shortcut needs Ctrl, Alt or Win, so it
/// can't take a key away from every other program.
pub fn parse(text: &str) -> Option<Shortcut> {
    let mut parts: Vec<&str> = text.split('+').map(str::trim).collect();
    let key = key(parts.pop()?)?;
    let mut shortcut = Shortcut {
        ctrl: false,
        alt: false,
        shift: false,
        win: false,
        key,
    };
    for part in parts {
        let flag = match part {
            "Ctrl" => &mut shortcut.ctrl,
            "Alt" => &mut shortcut.alt,
            "Shift" => &mut shortcut.shift,
            "Win" => &mut shortcut.win,
            _ => return None,
        };
        if *flag {
            return None;
        }
        *flag = true;
    }
    (shortcut.ctrl || shortcut.alt || shortcut.win).then_some(shortcut)
}

pub struct QuickTerminal {
    height: Mutex<f64>,
    hotkey: hotkey::Listener,
    /// The app stays in the notification area when its last window closes.
    keep_running: AtomicBool,
}

impl QuickTerminal {
    pub fn new(app: AppHandle) -> Self {
        Self {
            keep_running: AtomicBool::new(false),
            height: Mutex::new(0.5),
            hotkey: hotkey::Listener::start(move || {
                let app = app.clone();
                let _ = app.clone().run_on_main_thread(move || toggle(&app));
            }),
        }
    }
}

impl QuickTerminal {
    /// Whether closing the last window leaves the app running, for its shortcut.
    pub fn keeps_running(&self) -> bool {
        self.keep_running.load(Ordering::Relaxed)
    }
}

fn height(app: &AppHandle) -> f64 {
    app.state::<QuickTerminal>()
        .height
        .lock()
        .map(|height| *height)
        .unwrap_or(0.5)
}

/// Puts the window along the top of the screen the pointer is on.
fn place(app: &AppHandle, window: &tauri::WebviewWindow) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|point| app.monitor_from_point(point.x, point.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let area = monitor.work_area();
    let height = (f64::from(area.size.height) * height(app)).round() as u32;
    let _ = window.set_position(area.position);
    let _ = window.set_size(tauri::PhysicalSize::new(area.size.width, height));
}

pub fn toggle(app: &AppHandle) {
    match app.get_webview_window(LABEL) {
        Some(window) if window.is_visible().unwrap_or(false) => {
            if window.is_focused().unwrap_or(false) {
                // The page slides the window away, then hides it.
                let _ = app.emit_to(LABEL, "quick-terminal-hide", ());
            } else {
                let _ = window.set_focus();
            }
        }
        Some(window) => {
            place(app, &window);
            let _ = window.show();
            let _ = window.set_focus();
            let _ = app.emit_to(LABEL, "quick-terminal-show", ());
        }
        None => {
            let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
                .title("Nebula Terminal")
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .visible(false)
                .initialization_script("window.__NEBULA_WINDOW__ = 'quick';")
                .build();
            if let Ok(window) = built {
                place(app, &window);
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
    }
}

/// Registers the shortcut, or removes it when `enabled` is false (which also closes the
/// window). Fails with a message the settings can show when another app owns the keys.
#[tauri::command]
pub async fn set_quick_terminal(
    app: AppHandle,
    state: State<'_, QuickTerminal>,
    enabled: bool,
    shortcut: String,
    height: f64,
    keep_running: bool,
) -> Result<(), String> {
    if let Ok(mut current) = state.height.lock() {
        *current = height.clamp(SHORTEST, TALLEST);
    }
    let in_tray = enabled && keep_running;
    state.keep_running.store(in_tray, Ordering::Relaxed);
    if in_tray {
        crate::tray::show(&app).map_err(|error| error.to_string())?;
    } else {
        crate::tray::hide(&app);
    }
    if !enabled {
        if let Some(window) = app.get_webview_window(LABEL) {
            let _ = window.destroy();
        }
        return state.hotkey.set(None);
    }
    let shortcut = parse(&shortcut).ok_or("Choose a shortcut with Ctrl, Alt or Win.")?;
    state.hotkey.set(Some(shortcut))?;
    // A new height applies at once to a window that's showing.
    if let Some(window) = app.get_webview_window(LABEL) {
        if window.is_visible().unwrap_or(false) {
            place(&app, &window);
        }
    }
    Ok(())
}

/// Called by the quick terminal's page once it has slid out of view.
#[tauri::command]
pub fn hide_quick_terminal(app: AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

#[cfg(windows)]
mod hotkey {
    //! RegisterHotKey on a thread of its own: Windows posts WM_HOTKEY to the thread
    //! that registered the keys, so that thread waits for messages and nothing else.

    use super::{Key, Shortcut};
    use std::sync::mpsc;
    use std::time::Duration;
    use windows_sys::Win32::{
        System::Threading::GetCurrentThreadId,
        UI::{
            Input::KeyboardAndMouse::{
                GetKeyboardLayout, MapVirtualKeyExW, RegisterHotKey, UnregisterHotKey,
                MAPVK_VSC_TO_VK_EX, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
            },
            WindowsAndMessaging::{
                GetMessageW, PeekMessageW, PostThreadMessageW, MSG, PM_NOREMOVE, WM_APP, WM_HOTKEY,
            },
        },
    };

    const SET: u32 = WM_APP + 1;
    const ID: i32 = 1;

    struct Request {
        shortcut: Option<Shortcut>,
        reply: mpsc::Sender<Result<(), String>>,
    }

    pub struct Listener {
        thread: Option<u32>,
    }

    impl Listener {
        pub fn start(on_press: impl Fn() + Send + 'static) -> Self {
            let (ready, started) = mpsc::channel();
            std::thread::spawn(move || {
                let mut msg: MSG = unsafe { std::mem::zeroed() };
                // SAFETY: plain Win32 calls on this thread's own message queue. The first
                // PeekMessage creates the queue, so PostThreadMessage can reach it.
                unsafe {
                    PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
                    let _ = ready.send(GetCurrentThreadId());
                    while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                        match msg.message {
                            WM_HOTKEY => on_press(),
                            SET => {
                                // SAFETY: `set` boxed this request and gave up ownership.
                                let request = Box::from_raw(msg.lParam as *mut Request);
                                let _ = request.reply.send(apply(request.shortcut));
                            }
                            _ => {}
                        }
                    }
                }
            });
            Self {
                thread: started.recv_timeout(Duration::from_secs(2)).ok(),
            }
        }

        pub fn set(&self, shortcut: Option<Shortcut>) -> Result<(), String> {
            let thread = self.thread.ok_or("The shortcut listener didn't start.")?;
            let (reply, answer) = mpsc::channel();
            let request = Box::into_raw(Box::new(Request { shortcut, reply }));
            // SAFETY: on success the thread takes the box back; on failure it's ours again.
            unsafe {
                if PostThreadMessageW(thread, SET, 0, request as isize) == 0 {
                    drop(Box::from_raw(request));
                    return Err("The shortcut listener has stopped.".to_owned());
                }
            }
            answer
                .recv_timeout(Duration::from_secs(2))
                .map_err(|_| "The shortcut listener didn't answer.".to_owned())?
        }
    }

    fn apply(shortcut: Option<Shortcut>) -> Result<(), String> {
        // SAFETY: the hotkey belongs to this thread; unregistering a missing one is harmless.
        unsafe { UnregisterHotKey(std::ptr::null_mut(), ID) };
        let Some(shortcut) = shortcut else {
            return Ok(());
        };
        let vk = match shortcut.key {
            Key::Virtual(vk) => vk,
            // SAFETY: reads the keyboard layout of this thread, which follows the user's.
            Key::Scan(scan) => unsafe {
                MapVirtualKeyExW(scan, MAPVK_VSC_TO_VK_EX, GetKeyboardLayout(0))
            },
        };
        if vk == 0 {
            return Err("This key doesn't exist on your keyboard layout.".to_owned());
        }
        let mut modifiers = MOD_NOREPEAT;
        for (on, flag) in [
            (shortcut.ctrl, MOD_CONTROL),
            (shortcut.alt, MOD_ALT),
            (shortcut.shift, MOD_SHIFT),
            (shortcut.win, MOD_WIN),
        ] {
            if on {
                modifiers |= flag;
            }
        }
        // SAFETY: a thread hotkey (no window); WM_HOTKEY comes to this thread's queue.
        if unsafe { RegisterHotKey(std::ptr::null_mut(), ID, modifiers, vk) } == 0 {
            return Err("Another app already uses this shortcut.".to_owned());
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod hotkey {
    use super::Shortcut;

    pub struct Listener;

    impl Listener {
        pub fn start(_on_press: impl Fn() + Send + 'static) -> Self {
            Self
        }

        pub fn set(&self, _shortcut: Option<Shortcut>) -> Result<(), String> {
            Err("The quick terminal exists on Windows only.".to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_physical_keys() {
        let shortcut = parse("Win+Backquote").unwrap();
        assert!(shortcut.win && !shortcut.ctrl && !shortcut.alt && !shortcut.shift);
        assert_eq!(shortcut.key, Key::Scan(0x29));
        assert_eq!(parse("Ctrl+Alt+KeyT").unwrap().key, Key::Scan(0x14));
        assert_eq!(parse("Ctrl+Alt+KeyM").unwrap().key, Key::Scan(0x32));
        assert_eq!(parse("Alt+Digit1").unwrap().key, Key::Scan(0x02));
        assert_eq!(parse("Alt+Digit0").unwrap().key, Key::Scan(0x0b));
        assert_eq!(parse("Ctrl+Shift+F12").unwrap().key, Key::Virtual(0x7b));
        assert_eq!(parse("Win+ArrowDown").unwrap().key, Key::Virtual(0x28));
        assert_eq!(parse("Ctrl+Space").unwrap().key, Key::Scan(0x39));
    }

    #[test]
    fn refuses_shortcuts_that_would_take_a_key_from_everyone() {
        assert_eq!(parse("Backquote"), None);
        assert_eq!(parse("Shift+KeyA"), None);
        assert_eq!(parse("Win+Win+KeyA"), None);
        assert_eq!(parse("Hyper+KeyA"), None);
        assert_eq!(parse("Win+NumpadEnter"), None);
        assert_eq!(parse("Win+KeyAB"), None);
        assert_eq!(parse("Win+F25"), None);
        assert_eq!(parse(""), None);
    }
}

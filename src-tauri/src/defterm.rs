//! Nebula Terminal as the default terminal of Windows 11.
//!
//! When a console program starts on its own (cmd from the Start menu, a script
//! double-clicked in Explorer), the console host looks at `HKCU\Console\%%Startup`.
//! `DelegationConsole` names the console server that takes over (Windows Terminal's
//! OpenConsole, which ships with Windows 11) and `DelegationTerminal` the window that
//! shows it. OpenConsole creates that class over COM and calls
//! `ITerminalHandoff3::EstablishPtyHandoff`, passing the program's console as pipes.
//! Nebula registers itself as that class and opens a tab on the pipes.

/// Nebula's handoff class. Never change it: Windows stores it in the registry.
#[cfg(windows)]
const TERMINAL_CLSID: &str = "{01EDD6E6-6F42-4E94-910B-C8D5B1F3231C}";
#[cfg(windows)]
const TERMINAL_GUID: u128 = 0x01EDD6E6_6F42_4E94_910B_C8D5B1F3231C;

/// Windows Terminal's console server (OpenConsole), which hands consoles to terminals.
#[cfg(windows)]
const CONSOLE_CLSID: &str = "{2EACA947-7F5F-4CFA-BA87-8F7FBEEFBE69}";

/// "Let Windows decide", what the Windows settings write when nothing is chosen.
#[cfg(windows)]
const UNDECIDED: &str = "{00000000-0000-0000-0000-000000000000}";

/// Whether this version of Windows can delegate consoles (Windows 11 22H2 and later).
#[cfg(windows)]
fn supported() -> bool {
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .and_then(|key| key.get_value::<String, _>("CurrentBuildNumber"))
        .ok()
        .and_then(|build| build.trim().parse::<u32>().ok())
        .is_some_and(|build| build >= 22621)
}

#[cfg(windows)]
mod registry {
    use super::{CONSOLE_CLSID, TERMINAL_CLSID, UNDECIDED};
    use std::{io, path::Path};
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};

    const STARTUP: &str = r"Console\%%Startup";

    fn class_key() -> String {
        format!(r"Software\Classes\CLSID\{TERMINAL_CLSID}")
    }

    pub fn is_default() -> bool {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(STARTUP)
            .and_then(|key| key.get_value::<String, _>("DelegationTerminal"))
            .is_ok_and(|value| value.eq_ignore_ascii_case(TERMINAL_CLSID))
    }

    /// COM starts `exe -Embedding` when a console arrives and the app isn't running.
    pub fn make_default(exe: &Path) -> io::Result<()> {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let (class, _) = root.create_subkey(class_key())?;
        class.set_value("", &"Nebula Terminal")?;
        let (server, _) = class.create_subkey("LocalServer32")?;
        server.set_value("", &format!("\"{}\"", exe.display()))?;
        let (startup, _) = root.create_subkey(STARTUP)?;
        startup.set_value("DelegationConsole", &CONSOLE_CLSID)?;
        startup.set_value("DelegationTerminal", &TERMINAL_CLSID)?;
        Ok(())
    }

    /// Hands the choice back to Windows, unless another terminal took over meanwhile.
    pub fn release_default() -> io::Result<()> {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        if is_default() {
            let (startup, _) = root.create_subkey(STARTUP)?;
            startup.set_value("DelegationConsole", &UNDECIDED)?;
            startup.set_value("DelegationTerminal", &UNDECIDED)?;
        }
        match root.delete_subkey_all(class_key()) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}

// The interface's names and arguments come from microsoft/terminal's ITerminalHandoff.idl.
#[cfg(windows)]
#[allow(non_snake_case, clippy::too_many_arguments)]
mod server {
    use crate::pty::{Console, PtyState, SessionParts};
    use portable_pty::{ChildKiller, PtySize};
    use std::{
        ffi::c_void,
        fs::File,
        io::{self, Write},
        os::windows::io::{AsRawHandle, FromRawHandle, IntoRawHandle, OwnedHandle, RawHandle},
        sync::Arc,
    };
    use tauri::{AppHandle, Emitter, Manager};
    use windows::{
        core::{
            implement, interface, IUnknown, IUnknown_Vtbl, Interface, Ref, BOOL, GUID, HRESULT,
        },
        Win32::{
            Foundation::{
                DuplicateHandle, CLASS_E_NOAGGREGATION, DUPLICATE_SAME_ACCESS, E_INVALIDARG,
                HANDLE, S_OK, WAIT_OBJECT_0,
            },
            System::{
                Com::{
                    CoInitializeEx, CoRegisterClassObject, IClassFactory, IClassFactory_Impl,
                    CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED, REGCLS_MULTIPLEUSE,
                },
                Pipes::CreatePipe,
                Threading::{
                    GetCurrentProcess, GetExitCodeProcess, TerminateProcess, WaitForSingleObject,
                    INFINITE,
                },
            },
        },
    };

    /// `TERMINAL_STARTUP_INFO` from microsoft/terminal's ITerminalHandoff.idl.
    #[repr(C)]
    pub struct TerminalStartupInfo {
        title: *const u16,
        icon_path: *const u16,
        icon_index: i32,
        x: u32,
        y: u32,
        x_size: u32,
        y_size: u32,
        x_count_chars: u32,
        y_count_chars: u32,
        fill_attribute: u32,
        flags: u32,
        show_window: u16,
    }

    /// What Windows Terminal before 1.22 asks for: the console made the pipes.
    #[interface("AA6B364F-4A50-4176-9002-0AE755E7B5EF")]
    unsafe trait ITerminalHandoff2: IUnknown {
        fn EstablishPtyHandoff(
            &self,
            input: HANDLE,
            output: HANDLE,
            signal: HANDLE,
            reference: HANDLE,
            server: HANDLE,
            client: HANDLE,
            startup_info: TerminalStartupInfo,
        ) -> HRESULT;
    }

    /// The current version: the terminal makes the pipes and hands them back.
    #[interface("6F23DA90-15C5-4203-9DB0-64E73F1B1B00")]
    unsafe trait ITerminalHandoff3: IUnknown {
        fn EstablishPtyHandoff(
            &self,
            input: *mut HANDLE,
            output: *mut HANDLE,
            signal: HANDLE,
            reference: HANDLE,
            server: HANDLE,
            client: HANDLE,
            startup_info: *const TerminalStartupInfo,
        ) -> HRESULT;
    }

    /// Reads a BSTR without taking it: COM frees it after the call.
    ///
    /// # Safety
    /// `text` is null or a valid BSTR, which stores its byte length just before it.
    unsafe fn read_bstr(text: *const u16) -> String {
        if text.is_null() {
            return String::new();
        }
        let bytes = unsafe { *text.cast::<u32>().sub(1) } as usize;
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, bytes / 2) })
    }

    /// Our own copy of a handle COM lends for the length of the call.
    fn own(handle: HANDLE) -> windows::core::Result<OwnedHandle> {
        let mut copy = HANDLE::default();
        // SAFETY: both handles belong to this process, and `copy` receives a new one.
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                handle,
                GetCurrentProcess(),
                &mut copy,
                0,
                false,
                DUPLICATE_SAME_ACCESS,
            )?;
            Ok(OwnedHandle::from_raw_handle(copy.0 as RawHandle))
        }
    }

    fn pipe() -> windows::core::Result<(OwnedHandle, OwnedHandle)> {
        let (mut read, mut write) = (HANDLE::default(), HANDLE::default());
        // SAFETY: CreatePipe fills both handles, which we then own.
        unsafe {
            CreatePipe(&mut read, &mut write, None, 0)?;
            Ok((
                OwnedHandle::from_raw_handle(read.0 as RawHandle),
                OwnedHandle::from_raw_handle(write.0 as RawHandle),
            ))
        }
    }

    /// Resizes go to the console server over its signal pipe, as ConPTY does. The
    /// reference and server handles keep the console alive until the session drops.
    struct HandedConsole {
        signal: File,
        _reference: OwnedHandle,
        _server: OwnedHandle,
    }

    impl Console for HandedConsole {
        fn resize(&mut self, size: PtySize) -> io::Result<()> {
            // PTY_SIGNAL_RESIZE_WINDOW, then the width and height.
            let packet = [8_u16, size.cols, size.rows];
            let bytes: Vec<u8> = packet
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect();
            self.signal.write_all(&bytes)
        }
    }

    #[derive(Debug, Clone)]
    struct ClientKiller(Arc<OwnedHandle>);

    impl ChildKiller for ClientKiller {
        fn kill(&mut self) -> io::Result<()> {
            // SAFETY: the handle stays open as long as `self`.
            unsafe { TerminateProcess(HANDLE(self.0.as_raw_handle()), 1) }.map_err(io::Error::other)
        }

        fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
            Box::new(self.clone())
        }
    }

    fn wait_for(client: Arc<OwnedHandle>) -> io::Result<u32> {
        let handle = HANDLE(client.as_raw_handle());
        // SAFETY: `client` keeps the handle open for both calls.
        unsafe {
            if WaitForSingleObject(handle, INFINITE) != WAIT_OBJECT_0 {
                return Err(io::Error::last_os_error());
            }
            let mut code = 0;
            GetExitCodeProcess(handle, &mut code).map_err(io::Error::other)?;
            Ok(code)
        }
    }

    /// The handles COM lends for one call, before we take our own copies.
    struct Lent {
        signal: HANDLE,
        reference: HANDLE,
        server: HANDLE,
        client: HANDLE,
    }

    /// Takes over a console: `writer` carries what the user types to it, `reader` brings
    /// its output back.
    fn adopt(
        app: &AppHandle,
        writer: OwnedHandle,
        reader: OwnedHandle,
        lent: Lent,
        info: &TerminalStartupInfo,
    ) -> windows::core::Result<()> {
        let signal = own(lent.signal)?;
        let reference = own(lent.reference)?;
        let server = own(lent.server)?;
        let client = Arc::new(own(lent.client)?);
        // SAFETY: the title is a BSTR that lives for the length of the call.
        let title = unsafe { read_bstr(info.title) };
        let hidden = info.show_window == 0;

        let parts = SessionParts {
            reader: Box::new(File::from(reader)),
            writer: Box::new(File::from(writer)),
            console: Box::new(HandedConsole {
                signal: File::from(signal),
                _reference: reference,
                _server: server,
            }),
            killer: Box::new(ClientKiller(client.clone())),
            wait: Box::new(move || wait_for(client)),
        };
        app.state::<PtyState>()
            .receive_handoff(title, hidden, parts);
        if !hidden {
            let _ = app.emit("handoff", ());
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        Ok(())
    }

    fn hresult(result: windows::core::Result<()>) -> HRESULT {
        match result {
            Ok(()) => S_OK,
            Err(error) => error.code(),
        }
    }

    #[implement(ITerminalHandoff3, ITerminalHandoff2)]
    struct Handoff(AppHandle);

    impl ITerminalHandoff3_Impl for Handoff_Impl {
        unsafe fn EstablishPtyHandoff(
            &self,
            input: *mut HANDLE,
            output: *mut HANDLE,
            signal: HANDLE,
            reference: HANDLE,
            server: HANDLE,
            client: HANDLE,
            startup_info: *const TerminalStartupInfo,
        ) -> HRESULT {
            if input.is_null() || output.is_null() || startup_info.is_null() {
                return E_INVALIDARG;
            }
            hresult((|| {
                // The console reads what we type from one pipe and writes its output to the other.
                let (console_input, our_input) = pipe()?;
                let (our_output, console_output) = pipe()?;
                let lent = Lent {
                    signal,
                    reference,
                    server,
                    client,
                };
                // SAFETY: OpenConsole passes a valid TERMINAL_STARTUP_INFO for the call.
                adopt(&self.0, our_input, our_output, lent, unsafe {
                    &*startup_info
                })?;
                // COM takes these two and closes them once they are marshaled.
                unsafe {
                    *input = HANDLE(console_input.into_raw_handle());
                    *output = HANDLE(console_output.into_raw_handle());
                }
                Ok(())
            })())
        }
    }

    impl ITerminalHandoff2_Impl for Handoff_Impl {
        unsafe fn EstablishPtyHandoff(
            &self,
            input: HANDLE,
            output: HANDLE,
            signal: HANDLE,
            reference: HANDLE,
            server: HANDLE,
            client: HANDLE,
            startup_info: TerminalStartupInfo,
        ) -> HRESULT {
            hresult((|| {
                let lent = Lent {
                    signal,
                    reference,
                    server,
                    client,
                };
                adopt(&self.0, own(input)?, own(output)?, lent, &startup_info)
            })())
        }
    }

    #[implement(IClassFactory)]
    struct Factory(AppHandle);

    impl IClassFactory_Impl for Factory_Impl {
        fn CreateInstance(
            &self,
            outer: Ref<IUnknown>,
            iid: *const GUID,
            object: *mut *mut c_void,
        ) -> windows::core::Result<()> {
            if outer.is_some() {
                return Err(CLASS_E_NOAGGREGATION.into());
            }
            if iid.is_null() || object.is_null() {
                return Err(E_INVALIDARG.into());
            }
            let handoff: IUnknown = Handoff(self.0.clone()).into();
            // SAFETY: `iid` and `object` were checked above and come from COM.
            unsafe { handoff.query(&*iid, object).ok() }
        }

        fn LockServer(&self, _lock: BOOL) -> windows::core::Result<()> {
            Ok(())
        }
    }

    /// Registers the handoff class for as long as the app runs, on a thread of its own
    /// in the multithreaded apartment, so calls never wait for the UI thread.
    pub fn listen(app: AppHandle) {
        std::thread::spawn(move || {
            let clsid = GUID::from_u128(super::TERMINAL_GUID);
            // SAFETY: plain COM calls on this thread; the factory lives until the process exits.
            let registered = unsafe {
                CoInitializeEx(None, COINIT_MULTITHREADED)
                    .ok()
                    .and_then(|_| {
                        let factory: IClassFactory = Factory(app).into();
                        CoRegisterClassObject(
                            &clsid,
                            &factory,
                            CLSCTX_LOCAL_SERVER,
                            REGCLS_MULTIPLEUSE,
                        )
                    })
            };
            if registered.is_err() {
                return;
            }
            // The registration lasts while this apartment does.
            loop {
                std::thread::park();
            }
        });
    }
}

#[cfg(windows)]
pub use server::listen;

#[cfg(not(windows))]
pub fn listen(_app: tauri::AppHandle) {}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultTerminal {
    /// Windows 11 22H2 or later, with consoles that can be handed over.
    supported: bool,
    enabled: bool,
}

/// Makes Nebula the default terminal, or gives the choice back to Windows. `None` only
/// reads the current state.
#[tauri::command]
pub fn default_terminal(enabled: Option<bool>) -> Result<DefaultTerminal, String> {
    #[cfg(windows)]
    {
        if !supported() {
            return Ok(DefaultTerminal {
                supported: false,
                enabled: false,
            });
        }
        let result = match enabled {
            Some(true) => std::env::current_exe().and_then(|exe| registry::make_default(&exe)),
            Some(false) => registry::release_default(),
            None => Ok(()),
        };
        result.map_err(|error| format!("Could not change the default terminal: {error}"))?;
        Ok(DefaultTerminal {
            supported: true,
            enabled: registry::is_default(),
        })
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Ok(DefaultTerminal {
            supported: false,
            enabled: false,
        })
    }
}

/// Part of `nebula-terminal --uninstall`.
pub fn uninstall() {
    #[cfg(windows)]
    let _ = registry::release_default();
}

/// Whether COM started this copy to receive a console. It then opens no tab of its own.
#[tauri::command]
pub fn launched_for_handoff() -> bool {
    std::env::args()
        .skip(1)
        .any(|arg| arg.eq_ignore_ascii_case("-Embedding") || arg.eq_ignore_ascii_case("/Embedding"))
}

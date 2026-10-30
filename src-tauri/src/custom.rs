//! Profiles the user adds in Settings. They live in a JSON file in the app's config
//! folder, written only here and validated on the way in and on the way out. The UI
//! edits them through the commands below and starts them by id, like any other profile.

use crate::{cmdline, profiles};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::State;

pub const ID_PREFIX: &str = "custom:";
const FILE_VERSION: u32 = 1;
const MAX_PROFILES: usize = 50;
const MAX_NAME: usize = 60;
const MAX_PATH: usize = 1024;
const MAX_ARGUMENTS: usize = 64;
const MAX_ARGUMENTS_LENGTH: usize = 4096;
const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_FORWARDS: usize = 16;
const MAX_HOST: usize = 253;
const MAX_USER: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomProfile {
    pub id: String,
    pub name: String,
    /// Absolute path or a program name looked up on PATH; `%VAR%` and `~` are expanded.
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub accent: String,
    /// Set for an SSH connection: `executable` is then `ssh`, and its arguments are
    /// always built from these fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<SshConnection>,
}

/// An SSH connection made in the profile editor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConnection {
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// A private key file; `~` and `%VAR%` are expanded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<String>,
    /// `[bind:]port:host:hostport`, as `-L` takes them: local ports that reach the remote side.
    #[serde(default)]
    pub local_forwards: Vec<String>,
    /// The same for `-R`: remote ports that reach this PC.
    #[serde(default)]
    pub remote_forwards: Vec<String>,
}

impl SshConnection {
    /// The arguments for `ssh`. Every value was checked, so none can pass for an option.
    pub fn arguments(&self) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(port) = self.port {
            args.extend(["-p".to_owned(), port.to_string()]);
        }
        if let Some(key) = &self.identity_file {
            args.extend([
                "-i".to_owned(),
                profiles::expand_directory(key).display().to_string(),
            ]);
        }
        for forward in &self.local_forwards {
            args.extend(["-L".to_owned(), forward.clone()]);
        }
        for forward in &self.remote_forwards {
            args.extend(["-R".to_owned(), forward.clone()]);
        }
        args.push(match &self.user {
            Some(user) => format!("{user}@{}", self.host),
            None => self.host.clone(),
        });
        args
    }

    fn is_well_formed(&self) -> bool {
        is_host(&self.host)
            && self.user.as_deref().is_none_or(is_user)
            && self.port.is_none_or(|port| port > 0)
            && self
                .identity_file
                .as_ref()
                .is_none_or(|path| path.len() <= MAX_PATH && !has_control_characters(path))
            && self.local_forwards.len() <= MAX_FORWARDS
            && self.remote_forwards.len() <= MAX_FORWARDS
            && self
                .local_forwards
                .iter()
                .chain(&self.remote_forwards)
                .all(|forward| is_forward(forward))
    }
}

impl CustomProfile {
    /// What the profile passes to its program.
    pub fn arguments(&self) -> Vec<String> {
        self.ssh
            .as_ref()
            .map_or_else(|| self.args.clone(), SshConnection::arguments)
    }
}

/// A host name, an IPv4 address or an IPv6 one (bare or in brackets). Never starts with
/// `-`, so ssh can't read it as an option.
fn is_host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_HOST
        && !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '[' | ']'))
}

/// A user name, possibly `DOMAIN\user` or `user@domain`.
fn is_user(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_USER
        && !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '\\' | '@'))
}

fn is_port(value: &str) -> bool {
    value.parse::<u16>().is_ok_and(|port| port > 0)
}

/// `[bind:]port:host:hostport`, where bind and host may be `[IPv6]`.
fn is_forward(value: &str) -> bool {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut bracket = false;
    for c in value.chars() {
        match c {
            '[' => bracket = true,
            ']' => bracket = false,
            ':' if !bracket => {
                parts.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    parts.push(current);
    let address = |text: &str| text == "*" || is_host(text);
    match parts.as_slice() {
        [port, host, host_port] => is_port(port) && address(host) && is_port(host_port),
        [bind, port, host, host_port] => {
            address(bind) && is_port(port) && address(host) && is_port(host_port)
        }
        _ => false,
    }
}

#[derive(Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    profiles: Vec<CustomProfile>,
}

/// What the editor sends: the arguments as one line, split here.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomProfileInput {
    pub id: Option<String>,
    pub name: String,
    pub executable: String,
    pub arguments: String,
    pub cwd: String,
    pub accent: String,
    /// Present for an SSH connection; `executable`, `arguments` and `cwd` are then unused.
    #[serde(default)]
    pub ssh: Option<SshInput>,
}

/// The SSH fields of the editor, as typed: forwards one per line.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshInput {
    pub host: String,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub port: String,
    #[serde(default)]
    pub identity_file: String,
    #[serde(default)]
    pub local_forwards: String,
    #[serde(default)]
    pub remote_forwards: String,
}

/// A stored profile plus its arguments as one line, for the editor.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomProfileView {
    #[serde(flatten)]
    pub profile: CustomProfile,
    pub arguments: String,
}

impl From<CustomProfile> for CustomProfileView {
    fn from(profile: CustomProfile) -> Self {
        let arguments = cmdline::join(&profile.arguments());
        Self { profile, arguments }
    }
}

/// A validation failure the editor can show next to the field it belongs to.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct FieldError {
    pub field: Option<&'static str>,
    pub message: String,
}

impl FieldError {
    fn on(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field: Some(field),
            message: message.into(),
        }
    }

    fn general(message: impl Into<String>) -> Self {
        Self {
            field: None,
            message: message.into(),
        }
    }
}

fn has_control_characters(value: &str) -> bool {
    value.chars().any(char::is_control)
}

pub fn is_valid_id(id: &str) -> bool {
    id.strip_prefix(ID_PREFIX).is_some_and(|rest| {
        rest.len() == 36
            && uuid::Uuid::try_parse(rest).is_ok_and(|uuid| uuid.hyphenated().to_string() == rest)
    })
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Finds the program a profile runs: an absolute path to an existing file, or a bare
/// name found on PATH. Relative paths with folders are refused, so what runs never
/// depends on the current directory.
pub fn resolve_executable(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    if value.is_empty() || has_control_characters(value) {
        return None;
    }
    let expanded = profiles::expand_directory(value);
    if expanded.is_absolute() {
        return expanded.is_file().then_some(expanded);
    }
    let text = expanded.to_string_lossy();
    if text.contains(['/', '\\', ':']) {
        return None;
    }
    profiles::find_executable(&text)
}

/// Checks the shape of a stored entry. Used when reading the file, where a program that
/// has since been uninstalled is not a reason to drop the profile.
fn is_well_formed(profile: &CustomProfile) -> bool {
    let name = profile.name.trim();
    is_valid_id(&profile.id)
        && !name.is_empty()
        && name.chars().count() <= MAX_NAME
        && !has_control_characters(&profile.name)
        && !profile.executable.trim().is_empty()
        && profile.executable.len() <= MAX_PATH
        && !has_control_characters(&profile.executable)
        && profile.args.len() <= MAX_ARGUMENTS
        && profile.args.iter().map(String::len).sum::<usize>() <= MAX_ARGUMENTS_LENGTH
        && !profile.args.iter().any(|arg| arg.contains('\0'))
        && profile
            .cwd
            .as_ref()
            .is_none_or(|cwd| cwd.len() <= MAX_PATH && !has_control_characters(cwd))
        && is_hex_color(&profile.accent)
        && profile
            .ssh
            .as_ref()
            .is_none_or(|ssh| profile.executable == "ssh" && ssh.is_well_formed())
}

fn forwards(field: &'static str, text: &str) -> Result<Vec<String>, FieldError> {
    let list: Vec<String> = text
        .split(['\n', ','])
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    if list.len() > MAX_FORWARDS {
        return Err(FieldError::on(
            field,
            format!("Use at most {MAX_FORWARDS} forwards."),
        ));
    }
    if let Some(bad) = list.iter().find(|forward| !is_forward(forward)) {
        return Err(FieldError::on(field, format!("\u{201c}{bad}\u{201d} isn't a forward: use port:host:port, like 8080:localhost:80.")));
    }
    Ok(list)
}

/// Checks the SSH fields. `find_program` must find `ssh` (the OpenSSH client).
fn validate_ssh(
    input: &SshInput,
    find_program: impl Fn(&str) -> Option<PathBuf>,
) -> Result<SshConnection, FieldError> {
    let host = input.host.trim();
    if host.is_empty() {
        return Err(FieldError::on(
            "host",
            "Enter the server's name or address.",
        ));
    }
    if !is_host(host) {
        return Err(FieldError::on(
            "host",
            "Use a host name or an IP address, like server.example.com or 192.168.1.20.",
        ));
    }
    let user = input.user.trim();
    if !user.is_empty() && !is_user(user) {
        return Err(FieldError::on(
            "user",
            "Use letters, digits, dots, dashes and underscores.",
        ));
    }
    let port = input.port.trim();
    let port = if port.is_empty() {
        None
    } else {
        Some(
            port.parse::<u16>()
                .ok()
                .filter(|port| *port > 0)
                .ok_or_else(|| FieldError::on("port", "Use a port from 1 to 65535."))?,
        )
    };
    let key = input.identity_file.trim().trim_matches('"').trim();
    let identity_file = if key.is_empty() {
        None
    } else {
        if key.len() > MAX_PATH || has_control_characters(key) {
            return Err(FieldError::on("identityFile", "This path is not valid."));
        }
        if !profiles::expand_directory(key).is_file() {
            return Err(FieldError::on(
                "identityFile",
                "This key file does not exist.",
            ));
        }
        Some(key.to_owned())
    };
    let connection = SshConnection {
        host: host.to_owned(),
        user: (!user.is_empty()).then(|| user.to_owned()),
        port,
        identity_file,
        local_forwards: forwards("localForwards", &input.local_forwards)?,
        remote_forwards: forwards("remoteForwards", &input.remote_forwards)?,
    };
    if find_program("ssh").is_none() {
        return Err(FieldError::general(
            "The OpenSSH client isn't installed. Add it in Windows Settings → System → Optional features.",
        ));
    }
    Ok(connection)
}

pub fn split_arguments_line(line: &str) -> Result<Vec<String>, String> {
    if line.len() > MAX_ARGUMENTS_LENGTH {
        return Err(format!(
            "Arguments can be at most {MAX_ARGUMENTS_LENGTH} characters."
        ));
    }
    if line.contains('\0') {
        return Err("Arguments cannot contain NUL characters.".into());
    }
    let args = cmdline::split(line);
    if args.len() > MAX_ARGUMENTS {
        return Err(format!("Use at most {MAX_ARGUMENTS} arguments."));
    }
    Ok(args)
}

/// Validates what the editor sent and turns it into a stored profile with the given id.
/// `find_program` decides whether the program exists (see [`resolve_executable`]).
pub fn validate(
    input: &CustomProfileInput,
    id: String,
    find_program: impl Fn(&str) -> Option<PathBuf>,
) -> Result<CustomProfile, FieldError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(FieldError::on("name", "Enter a name."));
    }
    if name.chars().count() > MAX_NAME || has_control_characters(name) {
        return Err(FieldError::on(
            "name",
            format!("Use up to {MAX_NAME} characters, on one line."),
        ));
    }

    let accent = input.accent.trim().to_ascii_lowercase();
    if let Some(ssh) = &input.ssh {
        let connection = validate_ssh(ssh, &find_program)?;
        if !is_hex_color(&accent) {
            return Err(FieldError::on("accent", "Use a color like #5b8def."));
        }
        return Ok(CustomProfile {
            id,
            name: name.to_owned(),
            executable: "ssh".to_owned(),
            args: connection.arguments(),
            cwd: None,
            accent,
            ssh: Some(connection),
        });
    }

    let executable = input.executable.trim().trim_matches('"').trim();
    if executable.is_empty() {
        return Err(FieldError::on("executable", "Enter the program to run."));
    }
    if executable.len() > MAX_PATH || has_control_characters(executable) {
        return Err(FieldError::on("executable", "This path is not valid."));
    }
    let expanded = profiles::expand_directory(executable);
    let bare = !expanded.to_string_lossy().contains(['/', '\\', ':']);
    if !expanded.is_absolute() && !bare {
        return Err(FieldError::on(
            "executable",
            "Use a full path, such as C:\\Tools\\app.exe, or a program name found on PATH.",
        ));
    }
    if find_program(executable).is_none() {
        return Err(FieldError::on(
            "executable",
            if bare {
                format!("No program named \u{201c}{executable}\u{201d} was found on PATH.")
            } else {
                "This file does not exist.".to_owned()
            },
        ));
    }

    let args = split_arguments_line(&input.arguments)
        .map_err(|message| FieldError::on("arguments", message))?;

    let cwd = input.cwd.trim();
    let cwd = if cwd.is_empty() {
        None
    } else {
        if cwd.len() > MAX_PATH || has_control_characters(cwd) {
            return Err(FieldError::on("cwd", "This path is not valid."));
        }
        if !profiles::expand_directory(cwd).is_dir() {
            return Err(FieldError::on("cwd", "This folder does not exist."));
        }
        Some(cwd.to_owned())
    };

    if !is_hex_color(&accent) {
        return Err(FieldError::on("accent", "Use a color like #5b8def."));
    }

    Ok(CustomProfile {
        id,
        name: name.to_owned(),
        executable: executable.to_owned(),
        args,
        cwd,
        accent,
        ssh: None,
    })
}

/// Adds or replaces a profile in the list and returns the saved entry.
pub fn upsert(
    profiles: &mut Vec<CustomProfile>,
    input: &CustomProfileInput,
    find_program: impl Fn(&str) -> Option<PathBuf>,
) -> Result<CustomProfile, FieldError> {
    let existing = match input.id.as_deref() {
        Some(id) => Some(
            profiles
                .iter()
                .position(|profile| profile.id == id)
                .ok_or_else(|| FieldError::general("This profile was removed in the meantime."))?,
        ),
        None if profiles.len() >= MAX_PROFILES => {
            return Err(FieldError::general(format!(
                "You can have up to {MAX_PROFILES} custom profiles."
            )));
        }
        None => None,
    };
    let id = match existing {
        Some(index) => profiles[index].id.clone(),
        None => format!("{ID_PREFIX}{}", uuid::Uuid::new_v4().hyphenated()),
    };
    let profile = validate(input, id, find_program)?;
    match existing {
        Some(index) => profiles[index] = profile.clone(),
        None => profiles.push(profile.clone()),
    }
    Ok(profile)
}

/// Reads the profiles file. A missing or unreadable file means no custom profiles;
/// entries that are not well formed are dropped.
pub fn load_from(path: &Path) -> Vec<CustomProfile> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    let mut text = String::new();
    if file.take(MAX_FILE_BYTES).read_to_string(&mut text).is_err() {
        return Vec::new();
    }
    let Ok(stored) = serde_json::from_str::<StoreFile>(&text) else {
        return Vec::new();
    };
    let mut profiles: Vec<CustomProfile> = Vec::new();
    for profile in stored.profiles {
        if is_well_formed(&profile) && !profiles.iter().any(|known| known.id == profile.id) {
            profiles.push(profile);
        }
    }
    profiles.truncate(MAX_PROFILES);
    profiles
}

/// Writes the file through a temporary file so a crash never leaves half a file behind.
fn write_to(path: &Path, profiles: &[CustomProfile]) -> Result<(), String> {
    let failed = |error: std::io::Error| format!("Could not save profiles: {error}");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(failed)?;
    }
    let text = serde_json::to_string_pretty(&StoreFile {
        version: FILE_VERSION,
        profiles: profiles.to_vec(),
    })
    .map_err(|error| format!("Could not save profiles: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, text).map_err(failed)?;
    fs::rename(&temporary, path).map_err(failed)
}

/// Where custom profiles are kept. The lock serializes edits; reads need none because
/// the file is replaced in one step.
pub struct CustomProfiles {
    path: Option<PathBuf>,
    lock: Mutex<()>,
}

impl CustomProfiles {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn load(&self) -> Vec<CustomProfile> {
        self.path().map(load_from).unwrap_or_default()
    }

    fn edit<T, E: From<String>>(
        &self,
        change: impl FnOnce(&mut Vec<CustomProfile>) -> Result<T, E>,
    ) -> Result<T, E> {
        let path = self
            .path()
            .ok_or_else(|| "The settings folder is not available.".to_owned())?;
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Internal profile lock is poisoned.".to_owned())?;
        let mut profiles = load_from(path);
        let result = change(&mut profiles)?;
        write_to(path, &profiles)?;
        Ok(result)
    }

    /// The stored profile with this id, with its program located now.
    pub fn resolve(&self, id: &str) -> Result<profiles::ResolvedProfile, String> {
        let profile = self
            .load()
            .into_iter()
            .find(|profile| profile.id == id)
            .ok_or_else(|| format!("Unknown profile '{id}'."))?;
        let executable = resolve_executable(&profile.executable).ok_or_else(|| {
            format!(
                "The program of '{}' was not found: {}",
                profile.name, profile.executable
            )
        })?;
        Ok(profiles::ResolvedProfile {
            executable,
            args: profile.arguments(),
            cwd: profile.cwd,
        })
    }
}

impl From<String> for FieldError {
    fn from(message: String) -> Self {
        FieldError::general(message)
    }
}

#[tauri::command]
pub async fn list_custom_profiles(
    store: State<'_, CustomProfiles>,
) -> Result<Vec<CustomProfileView>, String> {
    Ok(store.load().into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn save_custom_profile(
    profile: CustomProfileInput,
    store: State<'_, CustomProfiles>,
) -> Result<CustomProfileView, FieldError> {
    store
        .edit(|profiles| upsert(profiles, &profile, resolve_executable))
        .map(Into::into)
}

#[tauri::command]
pub async fn delete_custom_profile(
    id: String,
    store: State<'_, CustomProfiles>,
) -> Result<(), String> {
    store.edit(|profiles| {
        profiles.retain(|profile| profile.id != id);
        Ok::<(), String>(())
    })
}

/// Lets the editor preview how its arguments line will be split.
#[tauri::command]
pub fn split_arguments(line: String) -> Result<Vec<String>, String> {
    split_arguments_line(&line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn scratch(name: &str) -> PathBuf {
        let directory =
            env::temp_dir().join(format!("nebula-custom-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn input(name: &str, executable: &str) -> CustomProfileInput {
        CustomProfileInput {
            id: None,
            name: name.into(),
            executable: executable.into(),
            arguments: String::new(),
            cwd: String::new(),
            accent: "#5B8DEF".into(),
            ssh: None,
        }
    }

    fn ssh_input(host: &str) -> CustomProfileInput {
        CustomProfileInput {
            ssh: Some(SshInput {
                host: host.into(),
                ..SshInput::default()
            }),
            ..input("Server", "")
        }
    }

    fn found(_: &str) -> Option<PathBuf> {
        Some(PathBuf::from("/bin/tool"))
    }

    fn missing(_: &str) -> Option<PathBuf> {
        None
    }

    fn id() -> String {
        format!("{ID_PREFIX}{}", uuid::Uuid::new_v4())
    }

    #[test]
    fn validates_and_normalizes_a_profile() {
        let mut request = input("  Python  ", "\"python.exe\"");
        request.arguments = r#"-m "http.server" 8000"#.into();
        let profile = validate(&request, id(), found).unwrap();
        assert_eq!(profile.name, "Python");
        assert_eq!(profile.executable, "python.exe");
        assert_eq!(profile.args, vec!["-m", "http.server", "8000"]);
        assert_eq!(profile.cwd, None);
        assert_eq!(profile.accent, "#5b8def");
    }

    #[test]
    fn reports_the_field_that_is_wrong() {
        let field = |request: CustomProfileInput, find: fn(&str) -> Option<PathBuf>| {
            validate(&request, id(), find).unwrap_err().field
        };
        assert_eq!(field(input(" ", "tool"), found), Some("name"));
        assert_eq!(field(input(&"x".repeat(61), "tool"), found), Some("name"));
        assert_eq!(field(input("Line\nbreak", "tool"), found), Some("name"));
        assert_eq!(field(input("Tool", ""), found), Some("executable"));
        assert_eq!(
            field(input("Tool", "bin/tool.exe"), found),
            Some("executable")
        );
        assert_eq!(
            field(input("Tool", "..\\tool.exe"), found),
            Some("executable")
        );
        assert_eq!(
            field(input("Tool", "C:tool.exe"), found),
            Some("executable")
        );
        assert_eq!(field(input("Tool", "tool"), missing), Some("executable"));

        let mut request = input("Tool", "tool");
        request.accent = "blue".into();
        assert_eq!(field(request, found), Some("accent"));

        let mut request = input("Tool", "tool");
        request.arguments = "a ".repeat(MAX_ARGUMENTS + 1);
        assert_eq!(field(request, found), Some("arguments"));

        let mut request = input("Tool", "tool");
        request.cwd = "/surely/not/a/real/folder".into();
        assert_eq!(field(request, found), Some("cwd"));
    }

    #[test]
    fn accepts_an_existing_starting_folder() {
        let folder = scratch("cwd");
        let mut request = input("Tool", "tool");
        request.cwd = folder.to_string_lossy().into_owned();
        assert_eq!(
            validate(&request, id(), found).unwrap().cwd,
            Some(request.cwd)
        );
    }

    #[test]
    fn finds_programs_by_absolute_path_only_when_they_exist() {
        let folder = scratch("exe");
        let program = folder.join("tool.exe");
        fs::write(&program, b"").unwrap();
        assert_eq!(
            resolve_executable(&program.to_string_lossy()),
            Some(program.clone())
        );
        assert_eq!(
            resolve_executable(&folder.join("other.exe").to_string_lossy()),
            None
        );
        assert_eq!(resolve_executable(&folder.to_string_lossy()), None);
        assert_eq!(resolve_executable("sub/tool.exe"), None);
        assert_eq!(resolve_executable(""), None);
    }

    #[test]
    fn checks_ids() {
        assert!(is_valid_id("custom:0f8fad5b-d9cb-469f-a165-70867728950e"));
        assert!(!is_valid_id("custom:0F8FAD5B-D9CB-469F-A165-70867728950E"));
        assert!(!is_valid_id("custom:0f8fad5bd9cb469fa16570867728950e"));
        assert!(!is_valid_id("custom:../../etc"));
        assert!(!is_valid_id("pwsh"));
    }

    #[test]
    fn adds_updates_and_limits_profiles() {
        let mut profiles = Vec::new();
        let added = upsert(&mut profiles, &input("One", "tool"), found).unwrap();
        assert!(is_valid_id(&added.id));

        let mut rename = input("Uno", "tool");
        rename.id = Some(added.id.clone());
        let updated = upsert(&mut profiles, &rename, found).unwrap();
        assert_eq!(updated.id, added.id);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "Uno");

        let mut gone = input("Gone", "tool");
        gone.id = Some(id());
        assert_eq!(upsert(&mut profiles, &gone, found).unwrap_err().field, None);

        while profiles.len() < MAX_PROFILES {
            upsert(&mut profiles, &input("More", "tool"), found).unwrap();
        }
        assert!(upsert(&mut profiles, &input("Too many", "tool"), found).is_err());
        // Editing still works at the limit.
        assert!(upsert(&mut profiles, &rename, found).is_ok());
    }

    #[test]
    fn a_failed_validation_changes_nothing() {
        let mut profiles = Vec::new();
        let added = upsert(&mut profiles, &input("One", "tool"), found).unwrap();
        let mut broken = input("", "tool");
        broken.id = Some(added.id.clone());
        assert!(upsert(&mut profiles, &broken, found).is_err());
        assert_eq!(profiles, vec![added]);
    }

    #[test]
    fn saves_and_loads_the_file() {
        let folder = scratch("store");
        let path = folder.join("nested").join("custom-profiles.json");
        assert!(load_from(&path).is_empty());

        let mut profiles = Vec::new();
        let mut request = input("Python", "python");
        request.arguments = r#"-X "dev mode""#.into();
        upsert(&mut profiles, &request, found).unwrap();
        write_to(&path, &profiles).unwrap();
        assert_eq!(load_from(&path), profiles);
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn drops_malformed_entries_when_loading() {
        let folder = scratch("malformed");
        let path = folder.join("custom-profiles.json");
        let good = id();
        let text = format!(
            r##"{{"version":1,"profiles":[
                {{"id":"{good}","name":"Good","executable":"tool","args":["-x"],"accent":"#112233"}},
                {{"id":"{good}","name":"Duplicate","executable":"tool","accent":"#112233"}},
                {{"id":"custom:nope","name":"Bad id","executable":"tool","accent":"#112233"}},
                {{"id":"{}","name":"Bad color","executable":"tool","accent":"red"}},
                {{"id":"{}","name":"","executable":"tool","accent":"#112233"}}
            ]}}"##,
            id(),
            id()
        );
        fs::write(&path, text).unwrap();
        let loaded = load_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Good");

        fs::write(&path, "not json").unwrap();
        assert!(load_from(&path).is_empty());
    }

    #[test]
    fn editor_view_round_trips_arguments() {
        let profile = CustomProfile {
            id: id(),
            name: "Tool".into(),
            executable: "tool".into(),
            args: vec!["--path".into(), r"C:\My Files\".into(), String::new()],
            cwd: None,
            accent: "#112233".into(),
            ssh: None,
        };
        let view = CustomProfileView::from(profile.clone());
        assert_eq!(split_arguments_line(&view.arguments).unwrap(), profile.args);
    }

    #[test]
    fn the_store_resolves_saved_profiles_only() {
        let folder = scratch("resolve");
        let program = folder.join("tool.exe");
        fs::write(&program, b"").unwrap();
        let store = CustomProfiles::new(Some(folder.join("custom-profiles.json")));
        let mut request = input("Tool", &program.to_string_lossy());
        request.arguments = "-a b".into();
        request.cwd = folder.to_string_lossy().into_owned();
        let saved = store
            .edit(|profiles| upsert(profiles, &request, resolve_executable))
            .unwrap();

        let resolved = store.resolve(&saved.id).unwrap();
        assert_eq!(resolved.executable, program);
        assert_eq!(resolved.args, vec!["-a", "b"]);
        assert_eq!(resolved.cwd.as_deref(), Some(request.cwd.as_str()));
        assert!(store.resolve(&id()).is_err());

        fs::remove_file(&program).unwrap();
        assert!(store.resolve(&saved.id).unwrap_err().contains("not found"));
    }

    #[test]
    fn ssh_connection_builds_the_command() {
        let key = scratch("ssh-key").join("id_ed25519");
        fs::write(&key, "key").unwrap();
        let mut value = ssh_input("  build.example.com ");
        let ssh = value.ssh.as_mut().unwrap();
        ssh.user = "deploy".into();
        ssh.port = "2222".into();
        ssh.identity_file = format!("\"{}\"", key.display());
        ssh.local_forwards = "8080:localhost:80\n\n 127.0.0.1:5433:db:5432 ".into();
        ssh.remote_forwards = "9000:[::1]:9000".into();

        let profile = validate(&value, id(), found).unwrap();
        assert_eq!(profile.executable, "ssh");
        assert_eq!(profile.cwd, None);
        assert_eq!(
            profile.args,
            [
                "-p",
                "2222",
                "-i",
                &key.display().to_string(),
                "-L",
                "8080:localhost:80",
                "-L",
                "127.0.0.1:5433:db:5432",
                "-R",
                "9000:[::1]:9000",
                "deploy@build.example.com",
            ]
        );
        assert!(is_well_formed(&profile));

        // Stored arguments are never trusted: they come from the connection.
        let tampered = CustomProfile {
            args: vec!["-oProxyCommand=calc".into()],
            ..profile.clone()
        };
        assert_eq!(tampered.arguments(), profile.args);
    }

    #[test]
    fn ssh_connection_rejects_anything_that_reads_as_an_option() {
        let error = |value: CustomProfileInput| validate(&value, id(), found).unwrap_err().field;
        assert_eq!(error(ssh_input("")), Some("host"));
        assert_eq!(error(ssh_input("-oProxyCommand=calc")), Some("host"));
        assert_eq!(error(ssh_input("server name")), Some("host"));

        let with = |change: fn(&mut SshInput)| {
            let mut value = ssh_input("server");
            change(value.ssh.as_mut().unwrap());
            value
        };
        assert_eq!(error(with(|ssh| ssh.user = "-l root".into())), Some("user"));
        assert_eq!(error(with(|ssh| ssh.port = "0".into())), Some("port"));
        assert_eq!(error(with(|ssh| ssh.port = "70000".into())), Some("port"));
        assert_eq!(
            error(with(|ssh| ssh.identity_file = "C:\\nowhere\\id_rsa".into())),
            Some("identityFile")
        );
        assert_eq!(
            error(with(|ssh| ssh.local_forwards = "8080".into())),
            Some("localForwards")
        );
        assert_eq!(
            error(with(|ssh| ssh.local_forwards = "-oX:a:1".into())),
            Some("localForwards")
        );
        assert_eq!(
            error(with(|ssh| ssh.remote_forwards = "1:host:port".into())),
            Some("remoteForwards")
        );

        let missing_ssh = validate(&ssh_input("server"), id(), missing).unwrap_err();
        assert_eq!(missing_ssh.field, None);

        let user = with(|ssh| ssh.user = r"CORP\jane".into());
        assert_eq!(
            validate(&user, id(), found).unwrap().args,
            [r"CORP\jane@server"]
        );
    }

    #[test]
    fn stored_ssh_connections_are_checked_on_load() {
        let profile = validate(&ssh_input("server"), id(), found).unwrap();
        let mut bad = profile.clone();
        bad.ssh.as_mut().unwrap().host = "-oProxyCommand=calc".into();
        assert!(is_well_formed(&profile));
        assert!(!is_well_formed(&bad));
    }
}

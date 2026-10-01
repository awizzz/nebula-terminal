//! File icons (Nerd Font glyphs) and colors for `ls` and `tree`.
//! Nebula Terminal bundles the "Symbols Nerd Font", so these render without setup.

use nu_ansi_term::{Color, Style};
use std::fs::Metadata;
use std::path::Path;

pub const FOLDER: char = '\u{f07b}';
pub const FOLDER_GIT: char = '\u{e5fb}';
pub const FOLDER_NODE: char = '\u{e718}';
pub const FILE: char = '\u{f15b}';
pub const SYMLINK: char = '\u{f481}';

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Directory,
    Symlink,
    Executable,
    Archive,
    Image,
    Media,
    Hidden,
    Plain,
}

fn extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

pub fn icon(name: &str, kind: Kind) -> char {
    let lower = name.to_ascii_lowercase();
    match kind {
        Kind::Directory => {
            return match lower.as_str() {
                ".git" => FOLDER_GIT,
                "node_modules" => FOLDER_NODE,
                _ => FOLDER,
            };
        }
        Kind::Symlink => return SYMLINK,
        _ => {}
    }
    match lower.as_str() {
        ".gitignore" | ".gitattributes" | ".gitmodules" => return '\u{f1d3}',
        "dockerfile" | "docker-compose.yml" | "compose.yaml" => return '\u{f308}',
        "license" | "license.md" | "license.txt" => return '\u{f02d}',
        "cargo.toml" | "cargo.lock" => return '\u{e7a8}',
        "package.json" | "package-lock.json" => return '\u{e718}',
        "makefile" => return '\u{f489}',
        _ => {}
    }
    match extension(name).as_str() {
        "rs" => '\u{e7a8}',
        "js" | "mjs" | "cjs" => '\u{e74e}',
        "ts" | "mts" | "cts" => '\u{e628}',
        "jsx" | "tsx" => '\u{e7ba}',
        "json" | "jsonc" => '\u{e60b}',
        "md" | "markdown" => '\u{f48a}',
        "py" | "pyw" => '\u{e606}',
        "html" | "htm" => '\u{e736}',
        "css" | "scss" | "sass" | "less" => '\u{e749}',
        "c" | "h" => '\u{e61e}',
        "cpp" | "cc" | "cxx" | "hpp" => '\u{e61d}',
        "cs" => '\u{e648}',
        "go" => '\u{e626}',
        "java" | "jar" => '\u{e738}',
        "php" => '\u{e73d}',
        "rb" => '\u{e739}',
        "lua" => '\u{e620}',
        "swift" => '\u{e755}',
        "kt" | "kts" => '\u{e634}',
        "vue" => '\u{e6a0}',
        "svelte" => '\u{e697}',
        "toml" | "yml" | "yaml" | "ini" | "cfg" | "conf" | "env" => '\u{e615}',
        "lock" => '\u{f023}',
        "sh" | "bash" | "zsh" | "fish" | "ps1" | "psm1" | "bat" | "cmd" => '\u{f489}',
        "exe" | "msi" | "dll" | "com" => '\u{f17a}',
        "txt" | "log" | "rtf" => '\u{f15c}',
        "pdf" => '\u{f1c1}',
        "doc" | "docx" | "odt" => '\u{f1c2}',
        "xls" | "xlsx" | "csv" | "ods" => '\u{f1c3}',
        "ppt" | "pptx" | "odp" => '\u{f1c4}',
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "ico" | "svg" | "tiff" | "avif"
        | "heic" => '\u{f1c5}',
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" => '\u{f1c7}',
        "mp4" | "mkv" | "mov" | "avi" | "webm" | "wmv" => '\u{f1c8}',
        "zip" | "tar" | "gz" | "tgz" | "xz" | "bz2" | "7z" | "rar" | "zst" => '\u{f410}',
        "sql" | "db" | "sqlite" => '\u{f1c0}',
        "ttf" | "otf" | "woff" | "woff2" => '\u{f031}',
        "xml" => '\u{f05c0}',
        _ => FILE,
    }
}

pub fn kind_of(name: &str, metadata: Option<&Metadata>, symlink: bool, hidden: bool) -> Kind {
    if symlink {
        return Kind::Symlink;
    }
    if metadata.is_some_and(Metadata::is_dir) {
        return Kind::Directory;
    }
    let ext = extension(name);
    if matches!(
        ext.as_str(),
        "exe" | "bat" | "cmd" | "com" | "ps1" | "msi" | "sh"
    ) || is_unix_executable(metadata)
    {
        return Kind::Executable;
    }
    if matches!(
        ext.as_str(),
        "zip" | "tar" | "gz" | "tgz" | "xz" | "bz2" | "7z" | "rar" | "zst"
    ) {
        return Kind::Archive;
    }
    if matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "ico" | "svg" | "tiff" | "avif" | "heic"
    ) {
        return Kind::Image;
    }
    if matches!(
        ext.as_str(),
        "mp3"
            | "wav"
            | "flac"
            | "ogg"
            | "m4a"
            | "aac"
            | "mp4"
            | "mkv"
            | "mov"
            | "avi"
            | "webm"
            | "wmv"
    ) {
        return Kind::Media;
    }
    if hidden {
        return Kind::Hidden;
    }
    Kind::Plain
}

#[cfg(unix)]
fn is_unix_executable(metadata: Option<&Metadata>) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.is_some_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_unix_executable(_: Option<&Metadata>) -> bool {
    false
}

/// GNU `ls --color` style for each kind of entry.
pub fn style(kind: Kind) -> Style {
    match kind {
        Kind::Directory => Style::new().bold().fg(Color::Blue),
        Kind::Symlink => Style::new().bold().fg(Color::Cyan),
        Kind::Executable => Style::new().bold().fg(Color::Green),
        Kind::Archive => Style::new().bold().fg(Color::Red),
        Kind::Image | Kind::Media => Style::new().bold().fg(Color::Magenta),
        Kind::Hidden => Style::new().dimmed(),
        Kind::Plain => Style::new(),
    }
}

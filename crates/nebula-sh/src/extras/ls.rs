//! `ls` with icons and colors on screen. Anything this view doesn't handle (other
//! options, output to a file or pipe) goes to the GNU-compatible uutils `ls`.

use super::{expand_short_flags, stdout_is_terminal};
use crate::icons::{self, Kind};
use crate::style;
use crate::sys;
use std::cmp::Ordering;
use std::ffi::OsString;
use std::fs::{self, Metadata};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use unicode_width::UnicodeWidthStr;

#[derive(Default)]
struct Options {
    all: bool,
    almost_all: bool,
    long: bool,
    human: bool,
    one_per_line: bool,
    by_time: bool,
    by_size: bool,
    reverse: bool,
    directory: bool,
    color: bool,
}

struct Entry {
    name: String,
    metadata: Option<Metadata>,
    link_target: Option<PathBuf>,
    kind: Kind,
}

fn delegate(args: &[String]) -> i32 {
    let mut forwarded: Vec<OsString> = args.iter().map(OsString::from).collect();
    if stdout_is_terminal() && !args.iter().any(|arg| arg.starts_with("--color")) {
        forwarded.push("--color=auto".into());
    }
    crate::coreutils::run("ls", forwarded).unwrap_or(2)
}

pub fn run(args: &[String]) -> i32 {
    if !stdout_is_terminal() {
        return delegate(args);
    }
    let mut options = Options {
        color: style::enabled(),
        ..Options::default()
    };
    let mut operands = Vec::new();
    for arg in expand_short_flags(&args[1..], &[]) {
        match arg.as_str() {
            "-a" | "--all" => options.all = true,
            "-A" | "--almost-all" => options.almost_all = true,
            "-l" => options.long = true,
            "-h" | "--human-readable" => options.human = true,
            "-1" => options.one_per_line = true,
            "-t" => options.by_time = true,
            "-S" => options.by_size = true,
            "-r" | "--reverse" => options.reverse = true,
            "-d" | "--directory" => options.directory = true,
            "--color" | "--color=auto" | "--color=always" => {}
            "--color=never" => options.color = false,
            "--" => {}
            other if other.starts_with('-') && other.len() > 1 => return delegate(args),
            other => operands.push(sys::translate_path(other)),
        }
    }
    if operands.is_empty() {
        operands.push(".".to_owned());
    }

    let mut status = 0;
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for operand in &operands {
        let path = PathBuf::from(operand);
        match fs::symlink_metadata(&path) {
            Ok(meta) => {
                let target_is_dir = fs::metadata(&path).is_ok_and(|m| m.is_dir());
                if (meta.is_dir() || target_is_dir) && !options.directory {
                    dirs.push(path);
                } else {
                    files.push(entry_for(operand.clone(), path));
                }
            }
            Err(error) => {
                eprintln!(
                    "ls: cannot access '{operand}': {}",
                    crate::exec::describe_io_error(&error)
                );
                status = 2;
            }
        }
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let width = sys::terminal_width();
    let icons = sys::icons_enabled();
    if !files.is_empty() {
        sort(&mut files, &options);
        render(&mut out, &files, &options, width, icons);
    }
    let show_headers = dirs.len() > 1 || !files.is_empty();
    for (index, dir) in dirs.iter().enumerate() {
        if show_headers {
            if index > 0 || !files.is_empty() {
                let _ = writeln!(out);
            }
            let _ = writeln!(out, "{}:", dir.display());
        }
        match read_dir(dir, &options) {
            Ok(mut entries) => {
                sort(&mut entries, &options);
                render(&mut out, &entries, &options, width, icons);
            }
            Err(error) => {
                let _ = out.flush();
                eprintln!(
                    "ls: cannot open directory '{}': {}",
                    dir.display(),
                    crate::exec::describe_io_error(&error)
                );
                status = 2;
            }
        }
    }
    let _ = out.flush();
    status
}

fn is_hidden(name: &str, metadata: Option<&Metadata>) -> bool {
    if name.starts_with('.') {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        if metadata.is_some_and(|meta| meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0) {
            return true;
        }
    }
    let _ = metadata;
    false
}

fn entry_for(name: String, path: PathBuf) -> Entry {
    let link = fs::symlink_metadata(&path).ok();
    let symlink = link.as_ref().is_some_and(Metadata::is_symlink);
    let metadata = if symlink {
        fs::metadata(&path).ok().or(link)
    } else {
        link
    };
    let hidden = is_hidden(&name, metadata.as_ref());
    let kind = icons::kind_of(&name, metadata.as_ref(), symlink, hidden);
    let link_target = if symlink {
        fs::read_link(&path).ok()
    } else {
        None
    };
    Entry {
        name,
        metadata,
        link_target,
        kind,
    }
}

fn read_dir(dir: &Path, options: &Options) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    if options.all {
        entries.push(entry_for(".".into(), dir.to_path_buf()));
        entries.push(entry_for("..".into(), dir.join("..")));
    }
    for item in fs::read_dir(dir)? {
        let item = item?;
        let name = item.file_name().to_string_lossy().into_owned();
        let entry = entry_for(name, item.path());
        if !(options.all || options.almost_all) && is_hidden(&entry.name, entry.metadata.as_ref()) {
            continue;
        }
        entries.push(entry);
    }
    Ok(entries)
}

fn modified(entry: &Entry) -> SystemTime {
    entry
        .metadata
        .as_ref()
        .and_then(|meta| meta.modified().ok())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn size(entry: &Entry) -> u64 {
    entry.metadata.as_ref().map_or(0, Metadata::len)
}

fn sort(entries: &mut [Entry], options: &Options) {
    entries.sort_by(|a, b| {
        let by_name = || {
            let key = |e: &Entry| e.name.trim_start_matches('.').to_lowercase();
            key(a).cmp(&key(b)).then_with(|| a.name.cmp(&b.name))
        };
        let order = if options.by_time {
            modified(b).cmp(&modified(a)).then_with(by_name)
        } else if options.by_size {
            size(b).cmp(&size(a)).then_with(by_name)
        } else {
            by_name()
        };
        if options.reverse {
            order.reverse()
        } else {
            order
        }
    });
    if options.all {
        // Keep `.` and `..` first, as GNU ls does.
        entries.sort_by(|a, b| match (a.name.as_str(), b.name.as_str()) {
            (".", _) => Ordering::Less,
            (_, ".") => Ordering::Greater,
            ("..", _) => Ordering::Less,
            (_, "..") => Ordering::Greater,
            _ => Ordering::Equal,
        });
    }
}

fn label(entry: &Entry, icons: bool, color: bool) -> (String, usize) {
    let text = if icons {
        format!("{}  {}", icons::icon(&entry.name, entry.kind), entry.name)
    } else {
        entry.name.clone()
    };
    let width = UnicodeWidthStr::width(entry.name.as_str()) + if icons { 3 } else { 0 };
    let painted = if color {
        icons::style(entry.kind).paint(&text).to_string()
    } else {
        text
    };
    (painted, width)
}

fn render(out: &mut impl Write, entries: &[Entry], options: &Options, width: usize, icons: bool) {
    if options.long {
        return render_long(out, entries, options, icons);
    }
    if entries.is_empty() {
        return;
    }
    let labels: Vec<(String, usize)> = entries
        .iter()
        .map(|entry| label(entry, icons, options.color))
        .collect();
    if options.one_per_line {
        for (text, _) in &labels {
            let _ = writeln!(out, "{text}");
        }
        return;
    }
    // Column-major grid like GNU ls: as many columns as fit, two spaces apart.
    let count = labels.len();
    let mut layout = (1, vec![labels.iter().map(|(_, w)| *w).max().unwrap_or(0)]);
    for columns in (2..=count.min(width / 3).max(1)).rev() {
        let rows = count.div_ceil(columns);
        let widths: Vec<usize> = (0..columns)
            .map(|c| {
                (0..rows)
                    .filter_map(|r| labels.get(c * rows + r))
                    .map(|(_, w)| *w)
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let total: usize = widths.iter().sum::<usize>() + 2 * widths.len().saturating_sub(1);
        if total <= width && widths.iter().all(|w| *w > 0) {
            layout = (columns, widths);
            break;
        }
    }
    let (columns, widths) = layout;
    let rows = count.div_ceil(columns);
    for row in 0..rows {
        let mut line = String::new();
        for (column, column_width) in widths.iter().enumerate() {
            let Some((text, label_width)) = labels.get(column * rows + row) else {
                continue;
            };
            line.push_str(text);
            let is_last = column + 1 == columns || labels.get((column + 1) * rows + row).is_none();
            if !is_last {
                line.push_str(&" ".repeat(column_width - label_width + 2));
            }
        }
        let _ = writeln!(out, "{line}");
    }
}

fn permissions(entry: &Entry) -> String {
    let Some(meta) = &entry.metadata else {
        return "?---------".into();
    };
    let kind = match entry.kind {
        Kind::Symlink => 'l',
        _ if meta.is_dir() => 'd',
        _ => '-',
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        let bits: String = (0..9)
            .map(|i| {
                let set = mode & (1 << (8 - i)) != 0;
                if !set {
                    '-'
                } else {
                    ['r', 'w', 'x'][i % 3]
                }
            })
            .collect();
        format!("{kind}{bits}")
    }
    #[cfg(not(unix))]
    {
        let write = if meta.permissions().readonly() {
            '-'
        } else {
            'w'
        };
        let exec = if meta.is_dir() || entry.kind == Kind::Executable {
            'x'
        } else {
            '-'
        };
        format!("{kind}r{write}{exec}r-{exec}r-{exec}")
    }
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["", "K", "M", "G", "T", "P"];
    if bytes < 1024 {
        return bytes.to_string();
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value < 10.0 {
        format!("{:.1}{}", (value * 10.0).ceil() / 10.0, UNITS[unit])
    } else {
        format!("{}{}", value.ceil() as u64, UNITS[unit])
    }
}

fn date(time: SystemTime) -> String {
    let Ok(zoned) = jiff::Zoned::try_from(time) else {
        return "?".into();
    };
    let now = jiff::Zoned::now();
    let recent = now.timestamp().duration_since(zoned.timestamp()).as_secs() < 180 * 24 * 3600
        && zoned.timestamp() <= now.timestamp();
    if recent {
        zoned.strftime("%b %e %H:%M").to_string()
    } else {
        zoned.strftime("%b %e  %Y").to_string()
    }
}

fn render_long(out: &mut impl Write, entries: &[Entry], options: &Options, icons: bool) {
    let rows: Vec<(String, String, String, String)> = entries
        .iter()
        .map(|entry| {
            let is_dir = entry.metadata.as_ref().is_some_and(Metadata::is_dir);
            let size = if is_dir && cfg!(windows) {
                "-".to_owned()
            } else if options.human {
                human_size(size(entry))
            } else {
                size(entry).to_string()
            };
            let (mut name, _) = label(entry, icons, options.color);
            if let Some(target) = &entry.link_target {
                name.push_str(&format!(" -> {}", target.display()));
            }
            (permissions(entry), size, date(modified(entry)), name)
        })
        .collect();
    if options.human || !rows.is_empty() {
        let total: u64 = entries.iter().map(size).sum::<u64>().div_ceil(1024);
        let _ = writeln!(
            out,
            "total {}",
            if options.human {
                human_size(total * 1024)
            } else {
                total.to_string()
            }
        );
    }
    let size_width = rows.iter().map(|row| row.1.len()).max().unwrap_or(0);
    let date_width = rows
        .iter()
        .map(|row| row.2.chars().count())
        .max()
        .unwrap_or(0);
    for (perms, size, date, name) in rows {
        let perms = if options.color {
            nu_ansi_term::Style::new().dimmed().paint(perms).to_string()
        } else {
            perms
        };
        let size = if options.color {
            nu_ansi_term::Color::Green
                .paint(format!("{size:>size_width$}"))
                .to_string()
        } else {
            format!("{size:>size_width$}")
        };
        let date = if options.color {
            nu_ansi_term::Color::Blue
                .paint(format!("{date:<date_width$}"))
                .to_string()
        } else {
            format!("{date:<date_width$}")
        };
        let _ = writeln!(out, "{perms} {size} {date} {name}");
    }
}

#[cfg(test)]
mod tests {
    use super::human_size;

    #[test]
    fn formats_sizes_like_gnu() {
        assert_eq!(human_size(512), "512");
        assert_eq!(human_size(1024), "1.0K");
        assert_eq!(human_size(1536), "1.5K");
        assert_eq!(human_size(10 * 1024 * 1024), "10M");
    }
}

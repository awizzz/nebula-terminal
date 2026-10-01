//! `tree`: a directory listing drawn as a tree, with icons on screen.

use super::{expand_short_flags, stdout_is_terminal};
use crate::icons;
use crate::sys;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

struct Options {
    all: bool,
    dirs_only: bool,
    max_depth: Option<usize>,
    ignore: Option<glob::Pattern>,
    report: bool,
    pretty: bool,
    icons: bool,
}

#[derive(Default)]
struct Counts {
    dirs: usize,
    files: usize,
}

pub fn run(args: &[String]) -> i32 {
    let pretty = stdout_is_terminal() && crate::style::enabled();
    let mut options = Options {
        all: false,
        dirs_only: false,
        max_depth: None,
        ignore: None,
        report: true,
        pretty,
        icons: pretty && sys::icons_enabled(),
    };
    let mut roots = Vec::new();
    let mut iter = expand_short_flags(args, &['L', 'I']).into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-a" => options.all = true,
            "-d" => options.dirs_only = true,
            "-L" => match iter
                .next()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|depth| *depth > 0)
            {
                Some(depth) => options.max_depth = Some(depth),
                None => {
                    eprintln!("tree: invalid level, must be greater than 0");
                    return 1;
                }
            },
            "-I" => {
                let pattern = iter.next().unwrap_or_default();
                options.ignore = glob::Pattern::new(&pattern).ok();
            }
            "--noreport" => options.report = false,
            "-n" => {
                options.pretty = false;
                options.icons = false;
            }
            "--help" => {
                println!(
                    "Usage: tree [-a] [-d] [-L level] [-I pattern] [-n] [--noreport] [directory…]"
                );
                return 0;
            }
            other if other.starts_with('-') && other != "-" => {
                eprintln!(
                    "tree: unsupported option '{other}' (supported: -a -d -L -I -n --noreport)"
                );
                return 1;
            }
            other => roots.push(sys::translate_path(other)),
        }
    }
    if roots.is_empty() {
        roots.push(".".into());
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut counts = Counts::default();
    let mut status = 0;
    for root in &roots {
        let path = Path::new(root);
        if !path.is_dir() {
            let _ = writeln!(out, "{root}  [error opening dir]");
            status = 2;
            continue;
        }
        let name = if options.pretty {
            icons::style(icons::Kind::Directory)
                .paint(root.as_str())
                .to_string()
        } else {
            root.clone()
        };
        let _ = writeln!(out, "{name}");
        walk(&mut out, path, "", 1, &options, &mut counts);
    }
    if options.report {
        let dirs = if counts.dirs == 1 {
            "directory"
        } else {
            "directories"
        };
        if options.dirs_only {
            let _ = writeln!(out, "\n{} {dirs}", counts.dirs);
        } else {
            let files = if counts.files == 1 { "file" } else { "files" };
            let _ = writeln!(out, "\n{} {dirs}, {} {files}", counts.dirs, counts.files);
        }
    }
    status
}

fn walk(
    out: &mut impl Write,
    dir: &Path,
    prefix: &str,
    depth: usize,
    options: &Options,
    counts: &mut Counts,
) {
    if options.max_depth.is_some_and(|max| depth > max) {
        return;
    }
    let Ok(read) = fs::read_dir(dir) else { return };
    let mut entries: Vec<(String, fs::Metadata, bool)> = read
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let symlink = entry.file_type().is_ok_and(|kind| kind.is_symlink());
            let meta = fs::metadata(entry.path())
                .or_else(|_| entry.metadata())
                .ok()?;
            Some((name, meta, symlink))
        })
        .filter(|(name, meta, _)| {
            (options.all || !name.starts_with('.'))
                && (!options.dirs_only || meta.is_dir())
                && !options
                    .ignore
                    .as_ref()
                    .is_some_and(|pattern| pattern.matches(name))
        })
        .collect();
    entries.sort_by_key(|(name, _, _)| name.to_lowercase());

    let count = entries.len();
    for (index, (name, meta, symlink)) in entries.into_iter().enumerate() {
        let last = index + 1 == count;
        let branch = if last { "└── " } else { "├── " };
        let kind = icons::kind_of(&name, Some(&meta), symlink, name.starts_with('.'));
        let text = if options.icons {
            format!("{}  {name}", icons::icon(&name, kind))
        } else {
            name.clone()
        };
        let text = if options.pretty {
            icons::style(kind).paint(text).to_string()
        } else {
            text
        };
        let connector = if options.pretty {
            crate::style::apply(
                nu_ansi_term::Style::new().dimmed(),
                &format!("{prefix}{branch}"),
            )
        } else {
            format!("{prefix}{branch}")
        };
        let _ = writeln!(out, "{connector}{text}");
        if meta.is_dir() {
            counts.dirs += 1;
            if !symlink {
                let child_prefix = format!("{prefix}{}", if last { "    " } else { "│   " });
                walk(
                    out,
                    &dir.join(&name),
                    &child_prefix,
                    depth + 1,
                    options,
                    counts,
                );
            }
        } else {
            counts.files += 1;
        }
    }
}

//! `diff` and `cmp`, on top of the uutils diffutils library. Exit codes follow GNU:
//! 0 when the files are the same, 1 when they differ, 2 on trouble.

use diffutilslib::params::Format;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

fn read_input(path: &Path) -> io::Result<Vec<u8>> {
    if path == Path::new("-") {
        let mut content = Vec::new();
        io::stdin().read_to_end(&mut content)?;
        Ok(content)
    } else {
        fs::read(path)
    }
}

/// `diff dir file` compares `dir/file`, as GNU diff does.
fn resolve(path: &Path, other: &Path) -> PathBuf {
    match other.file_name() {
        Some(name) if path.is_dir() && !other.is_dir() => path.join(name),
        _ => path.to_path_buf(),
    }
}

pub fn diff(args: Vec<OsString>) -> i32 {
    let params = match diffutilslib::params::parse_params(args.into_iter().peekable()) {
        Ok(params) => params,
        Err(error) => {
            eprintln!("diff: {error}");
            return 2;
        }
    };
    let from = resolve(Path::new(&params.from), Path::new(&params.to));
    let to = resolve(Path::new(&params.to), Path::new(&params.from));
    if from.is_dir() && to.is_dir() {
        eprintln!("diff: comparing folders isn't supported yet; compare the files inside them");
        return 2;
    }

    let mut contents = Vec::with_capacity(2);
    for path in [&from, &to] {
        match read_input(path) {
            Ok(content) => contents.push(content),
            Err(error) => {
                eprintln!("diff: {}: {error}", path.display());
                return 2;
            }
        }
    }
    let (old, new) = (&contents[0], &contents[1]);

    let mut stdout = io::stdout().lock();
    let output = match params.format {
        Format::Normal => diffutilslib::normal_diff(old, new, &params),
        Format::Unified => diffutilslib::unified_diff(old, new, &params),
        Format::Context => diffutilslib::context_diff(old, new, &params),
        Format::Ed => match diffutilslib::ed_diff(old, new, &params) {
            Ok(output) => output,
            Err(error) => {
                eprintln!("diff: {error}");
                return 2;
            }
        },
        Format::SideBySide => {
            let mut output = Vec::new();
            diffutilslib::side_by_side_diff(old, new, &mut output, &params);
            output
        }
    };
    // Side by side shows every line, so only the contents tell whether they differ.
    let differ = match params.format {
        Format::SideBySide => old != new,
        _ => !output.is_empty(),
    };
    if !differ {
        if params.report_identical_files {
            let _ = writeln!(
                stdout,
                "Files {} and {} are identical",
                from.display(),
                to.display()
            );
        }
        return 0;
    }
    let written = if params.brief {
        writeln!(
            stdout,
            "Files {} and {} differ",
            from.display(),
            to.display()
        )
    } else {
        stdout.write_all(&output)
    };
    match written.and_then(|_| stdout.flush()) {
        Ok(()) => 1,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => 1,
        Err(error) => {
            eprintln!("diff: {error}");
            2
        }
    }
}

pub fn cmp(args: Vec<OsString>) -> i32 {
    let quiet = args
        .iter()
        .skip(1)
        .any(|arg| arg == "-s" || arg == "--quiet" || arg == "--silent");
    let params = match diffutilslib::cmp::parse_params(args.into_iter().peekable()) {
        Ok(params) => params,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    match diffutilslib::cmp::cmp(&params) {
        Ok(diffutilslib::cmp::Cmp::Equal) => 0,
        Ok(diffutilslib::cmp::Cmp::Different) => 1,
        Err(error) => {
            if !quiet {
                eprintln!("{error}");
            }
            2
        }
    }
}

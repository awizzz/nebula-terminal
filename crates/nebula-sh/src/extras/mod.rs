//! Commands Nebula implements itself: richer `ls` and `tree`, plus `grep`, `find`,
//! `awk`, `ps`, `kill`, `open` and `xargs`, which uutils does not provide on Windows.
//! `sed`, `diff` and `cmp` come from the uutils sed and diffutils libraries.

mod awk;
mod compare;
mod find;
mod grep;
mod ls;
mod misc;
mod procs;
mod tree;

use std::ffi::OsString;

/// Runs `name` when it is one of Nebula's own commands. `args[0]` is the command name.
pub fn run(name: &str, args: Vec<OsString>) -> Option<i32> {
    // These take their arguments as they are: file names may not be valid Unicode.
    match name {
        "sed" => {
            uucore_sed::set_utility_is_second_arg();
            return Some(sed::sed::uumain(sed_args(args).into_iter()));
        }
        "diff" => return Some(compare::diff(args)),
        "cmp" => return Some(compare::cmp(args)),
        _ => {}
    }
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    Some(match name {
        "ls" => ls::run(&args),
        "tree" => tree::run(&args[1..]),
        "grep" => grep::run(&args[1..]),
        "find" => find::run(&args[1..]),
        "ps" => procs::ps(&args[1..]),
        "kill" => procs::kill(&args[1..]),
        "killall" | "pkill" => procs::killall(name, &args[1..]),
        "open" | "xdg-open" => misc::open(name, &args[1..]),
        "less" => misc::less(&args),
        "xargs" => misc::xargs(&args[1..]),
        "awk" => awk::run(&args[1..]),
        _ => return None,
    })
}

/// GNU sed only takes the backup suffix of `-i` when it is attached (`-i.bak`); uutils
/// sed would also take the next argument, the script. Spell the option out so it can't.
fn sed_args(args: Vec<OsString>) -> Vec<OsString> {
    let mut out = Vec::with_capacity(args.len());
    let mut options = true;
    for (index, arg) in args.into_iter().enumerate() {
        let text = arg.to_str().unwrap_or("");
        if index == 0 || !options || !text.starts_with('-') || text == "-" {
            out.push(arg);
            continue;
        }
        if text == "--" {
            options = false;
            out.push(arg);
            continue;
        }
        if text == "--in-place" {
            out.push("--in-place=".into());
            continue;
        }
        if !text.starts_with("--") {
            // Only flags without values may come before the `i` of a cluster like `-Ei`.
            if let Some(position) = text[1..].find('i') {
                let before = &text[1..1 + position];
                if before.chars().all(|flag| "nErsuz".contains(flag)) {
                    if !before.is_empty() {
                        out.push(format!("-{before}").into());
                    }
                    out.push(format!("--in-place={}", &text[2 + position..]).into());
                    continue;
                }
            }
        }
        out.push(arg);
    }
    out
}

/// Splits `-abc` into `-a -b -c`, leaving long options, values and operands alone.
pub(crate) fn expand_short_flags(args: &[String], takes_value: &[char]) -> Vec<String> {
    let mut out = Vec::new();
    let mut operands_only = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if operands_only || arg == "-" || !arg.starts_with('-') || arg.starts_with("--") {
            if arg == "--" {
                operands_only = true;
            }
            out.push(arg.clone());
            continue;
        }
        let chars: Vec<char> = arg[1..].chars().collect();
        let mut index = 0;
        while index < chars.len() {
            let flag = chars[index];
            out.push(format!("-{flag}"));
            if takes_value.contains(&flag) {
                let rest: String = chars[index + 1..].iter().collect();
                if !rest.is_empty() {
                    out.push(rest);
                } else if let Some(value) = iter.next() {
                    out.push(value.clone());
                }
                break;
            }
            index += 1;
        }
    }
    out
}

pub(crate) fn stdout_is_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

#[cfg(test)]
mod tests {
    use super::expand_short_flags;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn spells_out_sed_in_place() {
        let args = |items: &[&str]| -> Vec<std::ffi::OsString> {
            items.iter().map(|item| (*item).into()).collect()
        };
        assert_eq!(
            super::sed_args(args(&["sed", "-i", "s/a/b/", "f"])),
            args(&["sed", "--in-place=", "s/a/b/", "f"])
        );
        assert_eq!(
            super::sed_args(args(&["sed", "-Ei.bak", "-e", "s/i/j/", "--", "-i"])),
            args(&["sed", "-E", "--in-place=.bak", "-e", "s/i/j/", "--", "-i"])
        );
        assert_eq!(
            super::sed_args(args(&["sed", "-n", "/x/p", "--in-place"])),
            args(&["sed", "-n", "/x/p", "--in-place="])
        );
    }

    #[test]
    fn expands_clusters_and_values() {
        assert_eq!(
            expand_short_flags(
                &strings(&["-rin", "x", "-A3", "--color=never", "file"]),
                &['A']
            ),
            strings(&["-r", "-i", "-n", "x", "-A", "3", "--color=never", "file"])
        );
        assert_eq!(
            expand_short_flags(&strings(&["-e", "-x", "--", "-a"]), &['e']),
            strings(&["-e", "-x", "--", "-a"])
        );
    }
}

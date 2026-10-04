//! Pathname expansion: `*`, `?` and `[…]` matched against the files on disk, and `**`
//! for any depth of folders, as bash does with `globstar` on. A name that starts with a
//! dot needs a pattern that starts with one, `.` and `..` never match, and a pattern
//! that ends with a slash only matches folders.

use glob::{MatchOptions, Pattern};
use std::fs;
use std::path::{is_separator, Path, PathBuf};

fn has_wildcard(text: &str) -> bool {
    text.contains(['*', '?', '['])
}

/// Splits a pattern into components, each with the separator typed after it.
fn split(pattern: &str) -> Vec<(&str, &str)> {
    let mut pieces = Vec::new();
    let mut start = 0;
    for (index, c) in pattern.char_indices() {
        if is_separator(c) {
            let end = index + c.len_utf8();
            pieces.push((&pattern[start..index], &pattern[index..end]));
            start = end;
        }
    }
    pieces.push((&pattern[start..], ""));
    pieces
}

/// Only a whole `**` component reaches into folders: `a**b` and `***` mean `*`.
fn component_pattern(component: &str) -> Option<Pattern> {
    let mut text = String::with_capacity(component.len());
    for c in component.chars() {
        if c != '*' || !text.ends_with('*') {
            text.push(c);
        }
    }
    // An unmatched `[` is an ordinary character, as in bash.
    Pattern::new(&text)
        .or_else(|_| Pattern::new(&text.replace('[', "[[]")))
        .ok()
}

/// The folder a prefix names, in a form Windows understands (`/c/Users` is `C:/Users`).
fn directory(prefix: &str) -> PathBuf {
    if prefix.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(crate::sys::translate_path(prefix))
    }
}

/// Everything under `dir` that `**` reaches, without hidden names or links to folders.
fn walk(dir: &Path, prefix: &str, separator: &str, folders_only: bool, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = format!("{prefix}{name}");
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            let inside = format!("{path}{separator}");
            out.push(if folders_only { inside.clone() } else { path });
            walk(&entry.path(), &inside, separator, folders_only, out);
        } else if !folders_only {
            out.push(path);
        }
    }
}

/// The paths a pattern matches, unsorted. Nothing matches when the pattern has no
/// wildcard.
pub fn expand(pattern: &str, case_sensitive: bool) -> Vec<String> {
    let mut pieces = split(pattern);
    // `**/**` reaches no further than `**`, and would list everything twice.
    pieces.dedup_by(|later, earlier| later.0 == "**" && earlier.0 == "**");
    let Some(first) = pieces
        .iter()
        .position(|(component, _)| has_wildcard(component))
    else {
        return Vec::new();
    };
    let options = MatchOptions {
        case_sensitive,
        require_literal_separator: false,
        require_literal_leading_dot: false,
    };
    // Every path so far ends with a separator, or is empty for the current folder.
    // What comes before the first wildcard stays as typed.
    let mut paths: Vec<String> = vec![pieces[..first]
        .iter()
        .map(|(component, separator)| format!("{component}{separator}"))
        .collect()];
    for (index, &(component, separator)) in pieces.iter().enumerate().skip(first) {
        let last = index + 1 == pieces.len();
        let pattern = (component != "**" && has_wildcard(component))
            .then(|| component_pattern(component))
            .flatten();
        let mut next = Vec::new();
        for prefix in &paths {
            if component == "**" {
                let dir = directory(prefix);
                if !dir.is_dir() {
                    continue;
                }
                // Zero folders deep, then every folder (or, last, everything) below.
                let joiner = match (separator, prefix.chars().last()) {
                    ("", Some(c)) if is_separator(c) => &prefix[prefix.len() - c.len_utf8()..],
                    ("", _) => "/",
                    _ => separator,
                };
                next.push(prefix.clone());
                walk(&dir, prefix, joiner, !last, &mut next);
            } else if component.is_empty() {
                // A trailing slash keeps the folders found so far; a doubled one stays.
                next.push(if last {
                    prefix.clone()
                } else {
                    format!("{prefix}{separator}")
                });
            } else if !has_wildcard(component) {
                let path = format!("{prefix}{component}");
                let on_disk = directory(&path);
                if last && fs::symlink_metadata(&on_disk).is_ok() {
                    next.push(path);
                } else if !last && on_disk.is_dir() {
                    next.push(format!("{path}{separator}"));
                }
            } else {
                let Some(pattern) = &pattern else {
                    continue;
                };
                let Ok(entries) = fs::read_dir(directory(prefix)) else {
                    continue;
                };
                let hidden = component.starts_with('.');
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if (name.starts_with('.') && !hidden) || !pattern.matches_with(&name, options) {
                        continue;
                    }
                    if last {
                        next.push(format!("{prefix}{name}"));
                    } else if entry.path().is_dir() {
                        next.push(format!("{prefix}{name}{separator}"));
                    }
                }
            }
        }
        paths = next;
    }
    paths.retain(|path| !path.is_empty());
    paths
}

#[cfg(test)]
mod tests {
    use super::expand;
    use std::fs;

    #[test]
    fn matches_like_bash_with_globstar() {
        let root = std::env::temp_dir().join(format!("nebula-pathnames-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for dir in ["src/a/b", "src/.hidden", "docs", ".cache"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        for file in [
            "src/main.rs",
            "src/a/lib.rs",
            "src/a/b/deep.rs",
            "src/.hidden/x.rs",
            "docs/readme.md",
            "top.rs",
            ".env",
        ] {
            fs::write(root.join(file), "").unwrap();
        }
        let base = format!("{}/", root.to_string_lossy());
        let run = |pattern: &str| {
            let mut found: Vec<String> = expand(&format!("{base}{pattern}"), true)
                .into_iter()
                .map(|path| path.strip_prefix(&base).unwrap().to_owned())
                .collect();
            found.sort();
            found.join(" ")
        };
        assert_eq!(
            run("**/*.rs"),
            "src/a/b/deep.rs src/a/lib.rs src/main.rs top.rs"
        );
        assert_eq!(
            run("src/**"),
            "src/ src/a src/a/b src/a/b/deep.rs src/a/lib.rs src/main.rs"
        );
        assert_eq!(run("src/**/"), "src/ src/a/ src/a/b/");
        assert_eq!(run("*/"), "docs/ src/");
        assert_eq!(run("s**"), "src");
        assert_eq!(run("src/***"), "src/a src/main.rs");
        assert_eq!(run("**.rs"), "top.rs");
        assert_eq!(run(".*"), ".cache .env");
        assert_eq!(run("src/.*/"), "src/.hidden/");
        assert_eq!(run("src/.h*/*.rs"), "src/.hidden/x.rs");
        assert_eq!(run("*/readme.md"), "docs/readme.md");
        assert_eq!(run("src/a/[bl]*"), "src/a/b src/a/lib.rs");
        assert_eq!(run("**/**/deep.rs"), "src/a/b/deep.rs");
        assert_eq!(run("nothing/**"), "");
        assert_eq!(run("nothing*"), "");
        assert_eq!(run("top.rs"), "");
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn reads_git_bash_drive_paths() {
        assert_eq!(expand("/c/Windows/sys*32", false), ["/c/Windows/System32"]);
    }
}

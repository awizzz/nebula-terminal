//! Hosts from the user's OpenSSH configuration, so each one can have its own profile.
//!
//! Only `Host` and `Include` lines are read. Patterns (`*`, `?`, `!`) are not hosts you
//! can connect to and are skipped. Included files are read one level deep.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const MAX_CONFIG_BYTES: u64 = 512 * 1024;
const MAX_INCLUDED_FILES: usize = 32;
const MAX_HOSTS: usize = 64;

#[derive(Debug, PartialEq, Eq)]
pub enum Directive {
    Host(String),
    Include(String),
}

/// Splits the arguments of a configuration line: whitespace separated, double quotes
/// group words, and an unquoted word starting with `#` begins a comment.
fn words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut chars = text.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        match chars.peek() {
            None | Some('#') => return words,
            _ => {}
        }
        let mut word = String::new();
        let mut quoted = false;
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() && !quoted {
                break;
            }
            chars.next();
            if c == '"' {
                quoted = !quoted;
            } else {
                word.push(c);
            }
        }
        words.push(word);
    }
}

/// Reads `Host` names and `Include` paths from the text of an ssh config file, in order.
/// Keywords are case-insensitive and may be separated from their value by `=`.
pub fn parse_config(text: &str) -> Vec<Directive> {
    let mut directives = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let split = line
            .find(|c: char| c.is_whitespace() || c == '=')
            .unwrap_or(line.len());
        let (keyword, rest) = line.split_at(split);
        let rest = rest.trim_start();
        let rest = rest.strip_prefix('=').unwrap_or(rest);

        if keyword.eq_ignore_ascii_case("host") {
            directives.extend(
                words(rest)
                    .into_iter()
                    .filter(|name| is_valid_alias(name))
                    .map(Directive::Host),
            );
        } else if keyword.eq_ignore_ascii_case("include") {
            directives.extend(words(rest).into_iter().map(Directive::Include));
        }
    }
    directives
}

/// A host name that can be passed to `ssh.exe` as is. Patterns are rejected because they
/// contain `*`, `?` or `!`, and a leading letter or digit means it can never be read as
/// an option.
pub fn is_valid_alias(name: &str) -> bool {
    name.len() <= 253
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// `?` and `*` in a file name, as `Include` allows.
fn wildcard_match(pattern: &[u8], name: &[u8]) -> bool {
    match (pattern.split_first(), name.split_first()) {
        (None, None) => true,
        (Some((b'*', rest)), _) => {
            wildcard_match(rest, name) || (!name.is_empty() && wildcard_match(pattern, &name[1..]))
        }
        (Some((b'?', rest)), Some((_, name_rest))) => wildcard_match(rest, name_rest),
        (Some((expected, rest)), Some((actual, name_rest))) => {
            expected.eq_ignore_ascii_case(actual) && wildcard_match(rest, name_rest)
        }
        _ => false,
    }
}

/// The files an `Include` argument refers to. Relative paths start in `ssh_dir`, `~/`
/// in `home`. Wildcards are supported in the file name, not in folder names.
pub fn include_paths(argument: &str, ssh_dir: &Path, home: &Path) -> Vec<PathBuf> {
    let path = if let Some(rest) = argument
        .strip_prefix("~/")
        .or_else(|| argument.strip_prefix("~\\"))
    {
        home.join(rest)
    } else if Path::new(argument).is_absolute() {
        PathBuf::from(argument)
    } else {
        ssh_dir.join(argument)
    };

    let Some(pattern) = path.file_name().and_then(|name| name.to_str()) else {
        return Vec::new();
    };
    if !pattern.contains(['*', '?']) {
        return vec![path];
    }
    let Some(parent) = path.parent() else {
        return Vec::new();
    };
    if parent.to_string_lossy().contains(['*', '?']) {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut matches: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| wildcard_match(pattern.as_bytes(), name.as_bytes()))
        })
        .map(|entry| entry.path())
        .collect();
    matches.sort();
    matches
}

fn read_config(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    file.take(MAX_CONFIG_BYTES).read_to_string(&mut text).ok()?;
    Some(text)
}

fn push_host(hosts: &mut Vec<String>, name: String) {
    if hosts.len() < MAX_HOSTS && !hosts.iter().any(|known| known.eq_ignore_ascii_case(&name)) {
        hosts.push(name);
    }
}

/// Hosts defined in `config` and in the files it includes (one level deep), in the order
/// they appear, without duplicates.
pub fn hosts_in(config: &Path, ssh_dir: &Path, home: &Path) -> Vec<String> {
    let Some(text) = read_config(config) else {
        return Vec::new();
    };
    let mut hosts = Vec::new();
    let mut included = 0;
    for directive in parse_config(&text) {
        match directive {
            Directive::Host(name) => push_host(&mut hosts, name),
            Directive::Include(argument) => {
                for path in include_paths(&argument, ssh_dir, home) {
                    if included == MAX_INCLUDED_FILES {
                        break;
                    }
                    included += 1;
                    let Some(text) = read_config(&path) else {
                        continue;
                    };
                    for directive in parse_config(&text) {
                        if let Directive::Host(name) = directive {
                            push_host(&mut hosts, name);
                        }
                    }
                }
            }
        }
    }
    hosts
}

/// Hosts from `~/.ssh/config`.
pub fn user_hosts(home: &Path) -> Vec<String> {
    let ssh_dir = home.join(".ssh");
    hosts_in(&ssh_dir.join("config"), &ssh_dir, home)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosts(text: &str) -> Vec<String> {
        parse_config(text)
            .into_iter()
            .filter_map(|directive| match directive {
                Directive::Host(name) => Some(name),
                Directive::Include(_) => None,
            })
            .collect()
    }

    #[test]
    fn reads_host_names() {
        let config = "\
# Personal machines
Host pi
    HostName 192.168.1.20
    User pi

host build-box.local  nas
  HostName nas.lan
";
        assert_eq!(hosts(config), vec!["pi", "build-box.local", "nas"]);
    }

    #[test]
    fn skips_patterns_and_negations() {
        assert_eq!(
            hosts("Host *\nHost *.example.com web?\nHost !bastion jump\n"),
            vec!["jump"]
        );
    }

    #[test]
    fn accepts_equals_signs_quotes_and_any_case() {
        assert_eq!(
            hosts("HOST=alpha\nhost = beta\n  hOsT \"gamma\" delta\n"),
            vec!["alpha", "beta", "gamma", "delta"]
        );
    }

    #[test]
    fn ignores_comments_and_other_keywords() {
        let config = "#Host hidden\n  # Host also-hidden\nHostName not-a-host\nHost real # trailing comment\nMatch host other\nHostkeyAlias nope\n";
        assert_eq!(hosts(config), vec!["real"]);
    }

    #[test]
    fn rejects_names_that_could_be_options() {
        assert_eq!(
            hosts("Host -oProxyCommand=calc ok\nHost \"two words\" a;b\n"),
            vec!["ok"]
        );
    }

    #[test]
    fn reads_includes() {
        assert_eq!(
            parse_config("Include config.d/*  ~/.ssh/work\ninclude=/etc/ssh/extra\n"),
            vec![
                Directive::Include("config.d/*".into()),
                Directive::Include("~/.ssh/work".into()),
                Directive::Include("/etc/ssh/extra".into()),
            ]
        );
    }

    #[test]
    fn matches_wildcards() {
        assert!(wildcard_match(b"*.conf", b"work.conf"));
        assert!(wildcard_match(b"*", b"anything"));
        assert!(wildcard_match(b"host-?", b"host-1"));
        assert!(!wildcard_match(b"*.conf", b"work.txt"));
        assert!(!wildcard_match(b"host-?", b"host-10"));
    }

    #[test]
    fn follows_includes_one_level_deep() {
        let home = std::env::temp_dir().join(format!("nebula-ssh-{}", std::process::id()));
        let ssh = home.join(".ssh");
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(ssh.join("config.d")).unwrap();
        fs::write(
            ssh.join("config"),
            "Include config.d/*.conf\nHost main\nInclude ~/.ssh/missing\nInclude ~/extra\n",
        )
        .unwrap();
        fs::write(
            ssh.join("config.d").join("b.conf"),
            "Host beta\nHost main\n",
        )
        .unwrap();
        fs::write(
            ssh.join("config.d").join("a.conf"),
            "Host alpha\nInclude ../deeper\n",
        )
        .unwrap();
        fs::write(ssh.join("config.d").join("notes.txt"), "Host ignored\n").unwrap();
        fs::write(ssh.join("deeper"), "Host too-deep\n").unwrap();
        fs::write(home.join("extra"), "Host extra\n").unwrap();

        assert_eq!(user_hosts(&home), vec!["alpha", "beta", "main", "extra"]);
        assert!(user_hosts(&home.join("nowhere")).is_empty());
        let _ = fs::remove_dir_all(&home);
    }
}

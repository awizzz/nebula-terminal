//! Tab completion: commands, paths, options, variables, Git branches and SSH hosts.

use crate::commands;
use crate::exec::Shell;
use crate::parse;
use crate::sys;
use nu_ansi_term::{Color, Style};
use reedline::{Completer, CompletionResult, Span, Suggestion};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::Duration;

/// How long completion waits for `git` or a `--help` page.
const TIMEOUT: Duration = Duration::from_millis(400);

/// Command names the highlighter and completer know about, refreshed after each command.
#[derive(Clone, Default)]
pub struct Known {
    aliases: Arc<Mutex<BTreeSet<String>>>,
    functions: Arc<Mutex<BTreeSet<String>>>,
}

fn path_commands() -> &'static BTreeSet<String> {
    static COMMANDS: OnceLock<BTreeSet<String>> = OnceLock::new();
    COMMANDS.get_or_init(sys::path_commands)
}

impl Known {
    pub fn update(&self, shell: &Shell) {
        if let Ok(mut aliases) = self.aliases.lock() {
            *aliases = shell.aliases.keys().cloned().collect();
        }
        if let Ok(mut functions) = self.functions.lock() {
            *functions = shell.functions.keys().cloned().collect();
        }
    }

    pub fn is_command(&self, name: &str) -> bool {
        if commands::is_builtin(name)
            || commands::is_util(name)
            || self.aliases.lock().is_ok_and(|a| a.contains(name))
            || self.functions.lock().is_ok_and(|f| f.contains(name))
        {
            return true;
        }
        if name.contains('/') || name.contains('\\') {
            return sys::find_executable(name).is_some();
        }
        let lower = name.to_lowercase();
        let bare = lower.strip_suffix(".exe").unwrap_or(&lower);
        path_commands().iter().any(|candidate| {
            if cfg!(windows) {
                candidate.eq_ignore_ascii_case(bare)
            } else {
                candidate == name
            }
        })
    }

    fn command_names(&self) -> BTreeSet<String> {
        let mut names: BTreeSet<String> = commands::all_names().map(str::to_owned).collect();
        names.extend(path_commands().iter().cloned());
        if let Ok(aliases) = self.aliases.lock() {
            names.extend(aliases.iter().cloned());
        }
        if let Ok(functions) = self.functions.lock() {
            names.extend(functions.iter().cloned());
        }
        names
    }

    fn is_function(&self, name: &str) -> bool {
        self.functions.lock().is_ok_and(|f| f.contains(name))
    }
}

pub struct NebulaCompleter {
    pub known: Known,
}

/// Characters a completed word must escape to stay one word.
fn needs_escape(c: char) -> bool {
    matches!(
        c,
        ' ' | '\'' | '"' | '$' | '&' | '|' | ';' | '(' | ')' | '<' | '>' | '#' | '`'
    )
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if needs_escape(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Undoes [`escape`] (and plain quotes), so the typed text can be looked up on disk.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '\'' | '"') => quote = Some(c),
            (None, '\\') if chars.peek().is_some_and(|next| needs_escape(*next)) => {
                out.extend(chars.next());
            }
            _ => out.push(c),
        }
    }
    out
}

fn starts_with(candidate: &str, prefix: &str) -> bool {
    if cfg!(windows) {
        candidate.to_lowercase().starts_with(&prefix.to_lowercase())
    } else {
        candidate.starts_with(prefix)
    }
}

/// Where the word under the cursor starts: after the last unescaped, unquoted space
/// or operator.
fn token_start(before: &str) -> usize {
    let mut start = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, c) in before.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '\\') => escaped = true,
            (None, '\'' | '"') => quote = Some(c),
            (None, c)
                if c.is_whitespace() || matches!(c, '|' | ';' | '&' | '>' | '<' | '(' | ')') =>
            {
                start = index + c.len_utf8();
            }
            _ => {}
        }
    }
    start
}

/// The words of the command being typed, before the cursor's word. Empty when the
/// cursor is where a command name goes.
fn command_words(text: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = text.chars().peekable();
    let finish = |words: &mut Vec<String>, current: &mut String, in_word: &mut bool| {
        if *in_word {
            let word = std::mem::take(current);
            // A keyword in command position introduces the next command.
            if words.is_empty()
                && matches!(
                    word.as_str(),
                    "if" | "then" | "else" | "elif" | "do" | "while" | "until" | "{" | "!" | "time"
                )
            {
                words.clear();
            } else {
                words.push(word);
            }
            *in_word = false;
        }
    };
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                in_word = true;
            }
            (None, '\\') => {
                current.extend(chars.next());
                in_word = true;
            }
            (None, '|' | ';' | '&' | '(' | ')' | '\n') => {
                finish(&mut words, &mut current, &mut in_word);
                words.clear();
            }
            (None, c) if c.is_whitespace() => finish(&mut words, &mut current, &mut in_word),
            (None, c) => {
                current.push(c);
                in_word = true;
            }
        }
    }
    finish(&mut words, &mut current, &mut in_word);
    // Leading assignments (`FOO=1 cmd`) are not the command.
    while words.first().is_some_and(|word| {
        word.split_once('=')
            .is_some_and(|(name, _)| parse::is_name(name))
    }) {
        words.remove(0);
    }
    words
}

/// Runs a program with a deadline and returns its standard output.
fn output_within(mut command: Command, timeout: Duration) -> Option<String> {
    command
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .env("NO_COLOR", "1");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(command.output());
    });
    match receiver.recv_timeout(timeout) {
        Ok(Ok(output)) => Some(String::from_utf8_lossy(&output.stdout).into_owned()),
        _ => None,
    }
}

/// Options found in a `--help` page: GNU style (`-a, --all   text`) and clap style
/// (option line, description on the next line).
pub fn parse_help(text: &str) -> Options {
    let lines: Vec<&str> = text.lines().collect();
    let mut options: Options = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('-') || line.len() == trimmed.len() {
            continue;
        }
        let (spec, description) = match trimmed.find("  ") {
            Some(split) => (&trimmed[..split], trimmed[split..].trim()),
            None => (trimmed, ""),
        };
        let description = if description.is_empty() {
            lines
                .get(index + 1)
                .map(|next| next.trim())
                .filter(|next| !next.starts_with('-'))
                .unwrap_or("")
        } else {
            description
        };
        for part in spec.split(',') {
            let name: String = part
                .trim()
                .chars()
                .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
                .collect();
            if name.len() > 1 && name.starts_with('-') && name != "--" {
                let takes_value = part.contains('=') || part.trim().contains([' ', '<']);
                let name = if takes_value && name.starts_with("--") {
                    format!("{name}=")
                } else {
                    name
                };
                if !options.iter().any(|(existing, _)| *existing == name) {
                    options.push((name, description.to_owned()));
                }
            }
        }
    }
    if options.is_empty() {
        // Compact usage lines: `tree [-a] [-d] …`, `Tests: -name -iname …`.
        for word in text.split(|c: char| c.is_whitespace() || matches!(c, '[' | ']' | '|')) {
            let word = word.trim_end_matches([',', '.']);
            if word.len() > 1
                && word.starts_with('-')
                && word[1..].chars().all(|c| c.is_alphanumeric() || c == '-')
                && word
                    .chars()
                    .nth(1)
                    .is_some_and(|c| c.is_alphabetic() || c == '-')
                && !options.iter().any(|(existing, _)| existing == word)
            {
                options.push((word.to_owned(), String::new()));
            }
        }
    }
    options
}

/// Option names with their descriptions.
type Options = Vec<(String, String)>;

fn builtin_options(name: &str) -> Option<&'static [(&'static str, &'static str)]> {
    Some(match name {
        "echo" => &[
            ("-n", "no trailing newline"),
            ("-e", "interpret backslash escapes"),
            ("-E", "print backslashes as typed"),
        ],
        "read" => &[
            ("-r", "keep backslashes"),
            ("-p", "show a prompt"),
            ("-s", "do not echo the input"),
            ("-d", "stop at this character"),
            ("-n", "read this many characters"),
        ],
        "set" => &[
            ("-e", "stop at the first failing command"),
            ("-u", "unset variables are errors"),
            ("-x", "print commands before running them"),
            ("-o", "set an option by name"),
        ],
        "type" => &[("-t", "print only the kind")],
        "command" => &[("-v", "print what would run"), ("-V", "describe it")],
        "history" => &[("-c", "clear the history")],
        "unalias" => &[("-a", "remove every alias")],
        "unset" => &[("-f", "remove a function"), ("-v", "remove a variable")],
        "declare" | "typeset" => &[
            ("-f", "print functions"),
            ("-F", "list function names"),
            ("-g", "global variable"),
        ],
        "export" => &[("-p", "list exported variables")],
        _ => return None,
    })
}

/// The options of a command, from its `--help` page (Nebula commands only; other
/// programs are never run just to complete).
fn options_for(name: &str) -> Options {
    static CACHE: OnceLock<Mutex<HashMap<String, Options>>> = OnceLock::new();
    if let Some(options) = builtin_options(name) {
        return options
            .iter()
            .map(|(option, help)| ((*option).to_owned(), (*help).to_owned()))
            .collect();
    }
    if !commands::is_util(name) {
        return Vec::new();
    }
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(options) = cache.lock().ok().and_then(|c| c.get(name).cloned()) {
        return options;
    }
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("nebula-sh"));
    let mut command = Command::new(exe);
    command.arg(name).arg("--help");
    let options = output_within(command, Duration::from_secs(1))
        .map(|text| parse_help(&text))
        .unwrap_or_default();
    if let Ok(mut cache) = cache.lock() {
        cache.insert(name.to_owned(), options.clone());
    }
    options
}

const GIT_COMMANDS: &[(&str, &str)] = &[
    ("add", "stage changes"),
    ("bisect", "find the commit that broke something"),
    ("blame", "show who changed each line"),
    ("branch", "list, create or delete branches"),
    ("checkout", "switch branches or restore files"),
    ("cherry-pick", "apply commits from elsewhere"),
    ("clean", "remove untracked files"),
    ("clone", "copy a repository"),
    ("commit", "record changes"),
    ("config", "settings"),
    ("diff", "show changes"),
    ("fetch", "download from a remote"),
    ("grep", "search tracked files"),
    ("init", "create a repository"),
    ("log", "show history"),
    ("merge", "join histories"),
    ("mv", "move or rename tracked files"),
    ("pull", "fetch and integrate"),
    ("push", "upload to a remote"),
    ("rebase", "replay commits on another base"),
    ("reflog", "where HEAD has been"),
    ("remote", "manage remotes"),
    ("reset", "move HEAD, unstage"),
    ("restore", "restore files"),
    ("revert", "undo a commit with a new one"),
    ("rm", "remove tracked files"),
    ("show", "show an object"),
    ("stash", "set changes aside"),
    ("status", "show the working tree state"),
    ("switch", "switch branches"),
    ("tag", "list or create tags"),
    ("worktree", "manage worktrees"),
];

/// Git subcommands whose arguments are usually branches, tags or commits.
fn takes_refs(subcommand: &str) -> bool {
    matches!(
        subcommand,
        "checkout"
            | "switch"
            | "merge"
            | "rebase"
            | "branch"
            | "log"
            | "diff"
            | "reset"
            | "cherry-pick"
            | "show"
            | "revert"
            | "tag"
            | "worktree"
    )
}

fn git_lines(args: &[&str]) -> Vec<String> {
    let mut command = Command::new("git");
    command.args(args);
    output_within(command, TIMEOUT)
        .map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.ends_with("/HEAD"))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn git_refs() -> Vec<String> {
    let mut refs = git_lines(&[
        "for-each-ref",
        "--format=%(refname:short)",
        "refs/heads",
        "refs/remotes",
        "refs/tags",
    ]);
    let mut seen = BTreeSet::new();
    refs.retain(|name| seen.insert(name.clone()));
    refs
}

/// `Host` names from `~/.ssh/config` (patterns skipped).
fn ssh_hosts() -> Vec<String> {
    let Some(path) = sys::home().map(|home| home.join(".ssh").join("config")) else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut hosts = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some((keyword, rest)) = line.split_once(|c: char| c.is_whitespace() || c == '=') else {
            continue;
        };
        if !keyword.eq_ignore_ascii_case("host") {
            continue;
        }
        for name in rest.split_whitespace() {
            if !name.contains(['*', '?', '!']) && !hosts.iter().any(|h| h == name) {
                hosts.push(name.to_owned());
            }
        }
    }
    hosts
}

fn word_suggestions<I>(candidates: I, prefix: &str, span: Span) -> Vec<Suggestion>
where
    I: IntoIterator<Item = (String, String)>,
{
    candidates
        .into_iter()
        .filter(|(value, _)| starts_with(value, prefix))
        .map(|(value, description)| Suggestion {
            append_whitespace: !value.ends_with('='),
            value: escape(&value),
            description: (!description.is_empty()).then_some(description),
            span,
            ..Suggestion::default()
        })
        .collect()
}

impl NebulaCompleter {
    fn paths(&self, token: &str, span: Span, dirs_only: bool) -> Vec<Suggestion> {
        let unescaped = unescape(token);
        let (dir_typed, prefix) = match unescaped.rfind(['/', '\\']) {
            Some(index) => (&unescaped[..=index], &unescaped[index + 1..]),
            None => ("", unescaped.as_str()),
        };
        let dir_path: PathBuf = if dir_typed.is_empty() {
            PathBuf::from(".")
        } else if let Some(rest) = dir_typed.strip_prefix("~") {
            sys::home()
                .unwrap_or_default()
                .join(rest.trim_start_matches(['/', '\\']))
        } else {
            PathBuf::from(sys::translate_path(dir_typed))
        };
        let Ok(entries) = std::fs::read_dir(&dir_path) else {
            return Vec::new();
        };
        let mut suggestions: Vec<Suggestion> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !starts_with(&name, prefix)
                    || (name.starts_with('.') && !prefix.starts_with('.'))
                {
                    return None;
                }
                let is_dir = std::fs::metadata(entry.path()).is_ok_and(|m| m.is_dir());
                if dirs_only && !is_dir {
                    return None;
                }
                let value = format!(
                    "{}{}{}",
                    escape(dir_typed),
                    escape(&name),
                    if is_dir { "/" } else { "" }
                );
                Some(Suggestion {
                    value,
                    display_override: Some(format!("{name}{}", if is_dir { "/" } else { "" })),
                    style: Some(if is_dir {
                        Style::new().fg(Color::Blue).bold()
                    } else {
                        Style::new()
                    }),
                    span,
                    append_whitespace: !is_dir,
                    ..Suggestion::default()
                })
            })
            .collect();
        suggestions.sort_by_key(|s| s.value.to_lowercase());
        suggestions
    }

    fn commands(&self, token: &str, span: Span) -> Vec<Suggestion> {
        let mut names = self.known.command_names();
        names.extend(
            ["if", "for", "while", "until", "case", "function"]
                .iter()
                .map(|k| (*k).to_owned()),
        );
        names
            .into_iter()
            .filter(|name| starts_with(name, token))
            .take(200)
            .map(|name| Suggestion {
                description: if self.known.is_function(&name) {
                    Some("function".to_owned())
                } else {
                    commands::describe(&name).map(str::to_owned)
                },
                value: name,
                span,
                append_whitespace: true,
                ..Suggestion::default()
            })
            .collect()
    }

    /// `$NA` → `$NAME`, `${NA` → `${NAME}`.
    fn variables(&self, token: &str, start: usize, pos: usize) -> Option<Vec<Suggestion>> {
        let dollar = token.rfind('$')?;
        let after = &token[dollar + 1..];
        let (braced, prefix) = match after.strip_prefix('{') {
            Some(rest) => (true, rest),
            None => (false, after),
        };
        if !prefix.chars().all(parse::is_name_char) {
            return None;
        }
        let span = Span::new(start + dollar, pos);
        let mut names: Vec<String> = std::env::vars_os()
            .filter_map(|(name, _)| name.into_string().ok())
            .filter(|name| parse::is_name(name) && starts_with(name, prefix))
            .collect();
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup();
        Some(
            names
                .into_iter()
                .map(|name| Suggestion {
                    value: if braced {
                        format!("${{{name}}}")
                    } else {
                        format!("${name}")
                    },
                    description: std::env::var(&name)
                        .ok()
                        .map(|value| value.chars().take(40).collect()),
                    span,
                    append_whitespace: false,
                    ..Suggestion::default()
                })
                .collect(),
        )
    }

    fn git(&self, words: &[String], token: &str, span: Span) -> Option<Vec<Suggestion>> {
        let args: Vec<&str> = words[1..]
            .iter()
            .map(String::as_str)
            .filter(|arg| !arg.starts_with('-'))
            .collect();
        match args.as_slice() {
            [] => Some(word_suggestions(
                GIT_COMMANDS
                    .iter()
                    .map(|(name, help)| ((*name).to_owned(), (*help).to_owned())),
                token,
                span,
            )),
            ["push" | "pull" | "fetch"] => Some(word_suggestions(
                git_lines(&["remote"])
                    .into_iter()
                    .map(|r| (r, String::new())),
                token,
                span,
            )),
            ["push" | "pull" | "fetch", _, ..] => Some(word_suggestions(
                git_refs().into_iter().map(|r| (r, String::new())),
                token,
                span,
            )),
            [subcommand, ..] if takes_refs(subcommand) => {
                let mut suggestions = word_suggestions(
                    git_refs().into_iter().map(|r| (r, String::new())),
                    token,
                    span,
                );
                // `git checkout -- file` and `git diff path` still want paths.
                suggestions.extend(self.paths(token, span, false));
                Some(suggestions)
            }
            _ => None,
        }
    }
}

impl Completer for NebulaCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> CompletionResult {
        let before = &line[..pos];
        let start = token_start(before);
        let token = &before[start..];
        let span = Span::new(start, pos);
        let words = command_words(&before[..start]);

        let suggestions = if let Some(variables) = self.variables(token, start, pos) {
            variables
        } else if words.is_empty()
            && !token.contains(['/', '\\'])
            && !token.starts_with('.')
            && !token.starts_with('~')
        {
            self.commands(token, span)
        } else if token.starts_with('-') && !words.is_empty() {
            word_suggestions(options_for(&words[0]), token, span)
        } else {
            let command = words.first().map_or("", String::as_str);
            match command {
                "git" => self
                    .git(&words, token, span)
                    .unwrap_or_else(|| self.paths(token, span, false)),
                "ssh" | "scp" | "sftp" => {
                    let mut suggestions = word_suggestions(
                        ssh_hosts().into_iter().map(|h| (h, "SSH host".to_owned())),
                        token,
                        span,
                    );
                    if command != "ssh" {
                        suggestions.extend(self.paths(token, span, false));
                    }
                    suggestions
                }
                "cd" | "pushd" | "rmdir" => self.paths(token, span, true),
                "unset" | "export" | "local" | "declare" | "typeset" if !token.contains('=') => {
                    let mut names: Vec<String> = std::env::vars_os()
                        .filter_map(|(name, _)| name.into_string().ok())
                        .filter(|name| parse::is_name(name))
                        .collect();
                    names.sort_by_key(|name| name.to_lowercase());
                    names.dedup();
                    word_suggestions(names.into_iter().map(|n| (n, String::new())), token, span)
                }
                "type" | "which" | "command" | "help" => self.commands(token, span),
                _ => self.paths(token, span, false),
            }
        };
        CompletionResult::fresh(suggestions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_word_under_the_cursor() {
        let line = r"ls Program\ Files\ \(x86\)/Mi";
        assert_eq!(&line[token_start(line)..], r"Program\ Files\ \(x86\)/Mi");
        assert_eq!(token_start("echo a|gr"), 7);
        assert_eq!(&"cat 'my fi"[token_start("cat 'my fi")..], "'my fi");
    }

    #[test]
    fn unescapes_completed_paths() {
        assert_eq!(
            unescape(r"Program\ Files\ \(x86\)/"),
            "Program Files (x86)/"
        );
        assert_eq!(unescape("'my file"), "my file");
        assert_eq!(unescape(r"C:\Users\me"), r"C:\Users\me");
    }

    #[test]
    fn splits_the_current_command() {
        assert_eq!(command_words("git checkout "), vec!["git", "checkout"]);
        assert_eq!(command_words("ls | grep -i "), vec!["grep", "-i"]);
        assert!(command_words("ls && ").is_empty());
        assert_eq!(command_words("FOO=1 cargo "), vec!["cargo"]);
        assert_eq!(command_words("if true; then echo "), vec!["echo"]);
        assert!(command_words("for x in a; do ").is_empty());
    }

    #[test]
    fn reads_options_from_help_pages() {
        let gnu = "Usage: grep\n  -E, --extended-regexp     extended\n  -e, --regexp=PATTERNS     use PATTERNS\n";
        let options = parse_help(gnu);
        assert!(options.contains(&("-E".into(), "extended".into())));
        assert!(options.contains(&("--regexp=".into(), "use PATTERNS".into())));
        let clap =
            "Options:\n      --help\n          Print help.\n  -a, --all\n          Show all.\n";
        let options = parse_help(clap);
        assert!(options.contains(&("--all".into(), "Show all.".into())));
        assert!(options.contains(&("-a".into(), "Show all.".into())));
        let compact = parse_help("Usage: tree [-a] [-d] [-L level] [--noreport]");
        let names: Vec<&str> = compact.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["-a", "-d", "-L", "--noreport"]);
    }

    #[test]
    fn completes_variables() {
        std::env::set_var("NEBULA_COMPLETION_TEST", "1");
        let mut completer = NebulaCompleter {
            known: Known::default(),
        };
        let line = "echo $NEBULA_COMPLETION_T";
        let result = completer.complete(line, line.len());
        let values: Vec<String> = result
            .suggestions()
            .iter()
            .map(|suggestion| suggestion.value.clone())
            .collect();
        assert_eq!(values, vec!["$NEBULA_COMPLETION_TEST"]);
    }
}

//! Interactive mode: line editing with syntax colors, history suggestions,
//! Tab completion, history expansion and the window title.

use crate::commands;
use crate::exec::Shell;
use crate::prompt::NebulaPrompt;
use crate::sys;
use nu_ansi_term::{Color, Style};
use reedline::{
    default_emacs_keybindings, ColumnarMenu, Completer, CompletionResult, DefaultHinter, Emacs,
    FileBackedHistory, Highlighter, KeyCode, KeyModifiers, MenuBuilder, Reedline, ReedlineEvent,
    ReedlineMenu, Signal, Span, StyledText, Suggestion, ValidationResult, Validator,
};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

/// Command names the highlighter and completer know about, refreshed after each command.
#[derive(Clone, Default)]
struct Known {
    aliases: Arc<Mutex<BTreeSet<String>>>,
}

fn path_commands() -> &'static BTreeSet<String> {
    static COMMANDS: OnceLock<BTreeSet<String>> = OnceLock::new();
    COMMANDS.get_or_init(sys::path_commands)
}

impl Known {
    fn update(&self, shell: &Shell) {
        if let Ok(mut aliases) = self.aliases.lock() {
            *aliases = shell.aliases.keys().cloned().collect();
        }
    }

    fn is_command(&self, name: &str) -> bool {
        if commands::is_builtin(name)
            || commands::is_util(name)
            || self.aliases.lock().is_ok_and(|a| a.contains(name))
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
        names
    }
}

// ---------------------------------------------------------------------------
// Highlighting

struct NebulaHighlighter {
    known: Known,
}

#[derive(PartialEq)]
enum Tok {
    Space,
    Word { command: bool },
    Quote,
    Var,
    Op,
    Comment,
}

/// Splits a (possibly incomplete) line into coloured pieces. Never fails.
fn lex(line: &str) -> Vec<(Tok, String)> {
    let chars: Vec<char> = line.chars().collect();
    let mut out: Vec<(Tok, String)> = Vec::new();
    let mut expect_command = true;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            let start = i;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            out.push((Tok::Space, chars[start..i].iter().collect()));
            continue;
        }
        if c == '#' && (i == 0 || chars[i - 1].is_whitespace()) {
            out.push((Tok::Comment, chars[i..].iter().collect()));
            break;
        }
        let two: String = chars[i..chars.len().min(i + 2)].iter().collect();
        if matches!(two.as_str(), "&&" | "||" | ">>" | "2>" | "&>") {
            out.push((Tok::Op, two.clone()));
            i += 2;
            expect_command = matches!(two.as_str(), "&&" | "||");
            continue;
        }
        if matches!(c, '|' | ';' | '>' | '<' | '&') {
            out.push((Tok::Op, c.to_string()));
            i += 1;
            expect_command = matches!(c, '|' | ';' | '&');
            continue;
        }
        // A word, possibly with quotes and expansions inside.
        let mut pieces: Vec<(Tok, String)> = Vec::new();
        let mut plain = String::new();
        let mut word_text = String::new();
        while i < chars.len() {
            let c = chars[i];
            if c.is_whitespace() || matches!(c, '|' | ';' | '>' | '<' | '&') {
                break;
            }
            if c == '\'' || c == '"' {
                if !plain.is_empty() {
                    pieces.push((Tok::Word { command: false }, std::mem::take(&mut plain)));
                }
                let start = i;
                i += 1;
                while i < chars.len() && chars[i] != c {
                    if chars[i] == '\\' && c == '"' {
                        i += 1;
                    }
                    i += 1;
                }
                i = (i + 1).min(chars.len());
                let text: String = chars[start..i].iter().collect();
                word_text.push_str(text.trim_matches(c));
                pieces.push((Tok::Quote, text));
                continue;
            }
            if c == '$' {
                if !plain.is_empty() {
                    pieces.push((Tok::Word { command: false }, std::mem::take(&mut plain)));
                }
                let start = i;
                i += 1;
                if i < chars.len() && chars[i] == '(' {
                    let mut depth = 0;
                    while i < chars.len() {
                        if chars[i] == '(' {
                            depth += 1;
                        } else if chars[i] == ')' {
                            depth -= 1;
                            if depth == 0 {
                                i += 1;
                                break;
                            }
                        }
                        i += 1;
                    }
                } else if i < chars.len() && chars[i] == '{' {
                    while i < chars.len() && chars[i] != '}' {
                        i += 1;
                    }
                    i = (i + 1).min(chars.len());
                } else {
                    while i < chars.len()
                        && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '?' | '$'))
                    {
                        i += 1;
                    }
                }
                pieces.push((Tok::Var, chars[start..i].iter().collect()));
                continue;
            }
            if c == '\\' && i + 1 < chars.len() {
                plain.push(c);
                plain.push(chars[i + 1]);
                word_text.push(chars[i + 1]);
                i += 2;
                continue;
            }
            plain.push(c);
            word_text.push(c);
            i += 1;
        }
        if !plain.is_empty() {
            pieces.push((Tok::Word { command: false }, plain));
        }
        let is_assignment = expect_command
            && word_text.split_once('=').is_some_and(|(name, _)| {
                !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            });
        if expect_command && !is_assignment {
            for piece in &mut pieces {
                if let Tok::Word { command } = &mut piece.0 {
                    *command = true;
                }
            }
            if word_text != "!" {
                expect_command = false;
            }
        }
        out.extend(pieces);
    }
    out
}

impl Highlighter for NebulaHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        let mut styled = StyledText::new();
        let tokens = lex(line);
        let mut index = 0;
        while index < tokens.len() {
            let (kind, text) = &tokens[index];
            let style = match kind {
                Tok::Space => Style::new(),
                Tok::Comment => Style::new().dimmed().italic(),
                Tok::Quote => Style::new().fg(Color::Yellow),
                Tok::Var => Style::new().fg(Color::Magenta),
                Tok::Op => Style::new().fg(Color::Cyan).bold(),
                Tok::Word { command: true } => {
                    // A command may be split by quotes; judge the whole word.
                    let mut word = text.clone();
                    let mut end = index + 1;
                    while let Some((Tok::Word { command: true } | Tok::Quote, more)) =
                        tokens.get(end)
                    {
                        word.push_str(more);
                        end += 1;
                    }
                    let name = word.replace(['\'', '"'], "");
                    let style = if self.known.is_command(&name) {
                        Style::new().fg(Color::Green).bold()
                    } else {
                        Style::new().fg(Color::Red)
                    };
                    for (_, piece) in &tokens[index..end] {
                        styled.push((style, piece.clone()));
                    }
                    index = end;
                    continue;
                }
                Tok::Word { command: false } if text.starts_with('-') => {
                    Style::new().fg(Color::Cyan)
                }
                Tok::Word { command: false } => {
                    let path = text.replace('\\', "");
                    if path.len() > 1 && Path::new(&sys::translate_path(&path)).exists() {
                        Style::new().underline()
                    } else {
                        Style::new()
                    }
                }
            };
            styled.push((style, text.clone()));
            index += 1;
        }
        styled
    }
}

// ---------------------------------------------------------------------------
// Completion

struct NebulaCompleter {
    known: Known,
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(
            c,
            ' ' | '\'' | '"' | '$' | '&' | '|' | ';' | '(' | ')' | '<' | '>' | '#' | '`'
        ) {
            out.push('\\');
        }
        out.push(c);
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

impl NebulaCompleter {
    fn paths(&self, token: &str, span: Span, dirs_only: bool) -> Vec<Suggestion> {
        let unescaped = token.replace("\\ ", " ");
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
}

impl Completer for NebulaCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> CompletionResult {
        let before = &line[..pos];
        // Find the start of the token under the cursor (escaped spaces stay in the token).
        let mut start = 0;
        let bytes = before.as_bytes();
        for (index, byte) in bytes.iter().enumerate() {
            if (byte.is_ascii_whitespace() && (index == 0 || bytes[index - 1] != b'\\'))
                || matches!(byte, b'|' | b';' | b'&' | b'>' | b'<')
            {
                start = index + 1;
            }
        }
        let token = &before[start..];
        let previous = before[..start].trim_end();
        let command_position = previous.is_empty() || previous.ends_with(['|', ';', '&']);
        let span = Span::new(start, pos);

        let suggestions = if command_position
            && !token.contains(['/', '\\'])
            && !token.starts_with('.')
            && !token.starts_with('~')
        {
            self.known
                .command_names()
                .into_iter()
                .filter(|name| starts_with(name, token))
                .take(200)
                .map(|name| Suggestion {
                    description: commands::describe(&name).map(str::to_owned),
                    value: name,
                    span,
                    append_whitespace: true,
                    ..Suggestion::default()
                })
                .collect()
        } else {
            let first_word = before.split_whitespace().next().unwrap_or("");
            self.paths(token, span, matches!(first_word, "cd" | "pushd" | "rmdir"))
        };
        CompletionResult::fresh(suggestions)
    }
}

// ---------------------------------------------------------------------------
// Validation: keep reading lines while quotes or operators are left open.

struct NebulaValidator;

impl Validator for NebulaValidator {
    fn validate(&self, line: &str) -> ValidationResult {
        match crate::parse::parse(line) {
            Err(error) if error.incomplete => ValidationResult::Incomplete,
            _ => ValidationResult::Complete,
        }
    }
}

// ---------------------------------------------------------------------------

fn data_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| sys::home().unwrap_or_default())
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| sys::home().unwrap_or_default().join(".local/share"))
    };
    base.join(if cfg!(windows) { "Nebula" } else { "nebula" })
}

/// `!!` is the previous line, `!$` its last argument (outside single quotes).
pub fn expand_history(line: &str, previous: Option<&str>) -> Option<String> {
    let previous = previous?;
    if !line.contains('!') {
        return None;
    }
    let last_arg = previous.split_whitespace().last().unwrap_or("");
    let mut out = String::new();
    let mut in_single = false;
    let mut changed = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' {
            in_single = !in_single;
        }
        if c == '!' && !in_single {
            match chars.peek() {
                Some('!') => {
                    chars.next();
                    out.push_str(previous);
                    changed = true;
                    continue;
                }
                Some('$') => {
                    chars.next();
                    out.push_str(last_arg);
                    changed = true;
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
    }
    changed.then_some(out)
}

fn set_title(title: &str) {
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]0;{title}\x07");
    let _ = out.flush();
}

pub fn interactive(shell: &mut Shell) -> i32 {
    let data = data_dir();
    let _ = std::fs::create_dir_all(&data);
    let history_path = data.join("history.txt");
    if let Ok(text) = std::fs::read_to_string(&history_path) {
        shell.history = text
            .lines()
            .map(|line| line.replace("<\\n>", "\n"))
            .collect();
    }
    if let Some(rc) = sys::home()
        .map(|home| home.join(".nebularc"))
        .filter(|path| path.is_file())
    {
        if let Ok(text) = std::fs::read_to_string(&rc) {
            shell.run_line(&text);
        }
    }

    let known = Known::default();
    known.update(shell);

    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".into()),
            ReedlineEvent::MenuNext,
        ]),
    );
    keybindings.add_binding(
        KeyModifiers::SHIFT,
        KeyCode::BackTab,
        ReedlineEvent::MenuPrevious,
    );

    let menu = ColumnarMenu::default()
        .with_name("completion_menu")
        .with_text_style(Style::new())
        .with_selected_text_style(Style::new().fg(Color::Black).on(Color::Blue))
        .with_description_text_style(Style::new().dimmed())
        .with_marker("");

    let mut editor = Reedline::create()
        .with_edit_mode(Box::new(Emacs::new(keybindings)))
        .with_highlighter(Box::new(NebulaHighlighter {
            known: known.clone(),
        }))
        .with_hinter(Box::new(
            DefaultHinter::default().with_style(Style::new().dimmed()),
        ))
        .with_completer(Box::new(NebulaCompleter {
            known: known.clone(),
        }))
        .with_menu(ReedlineMenu::EngineCompleter(Box::new(menu)))
        .with_validator(Box::new(NebulaValidator))
        .with_quick_completions(true)
        .with_partial_completions(true)
        .use_bracketed_paste(true);
    if let Ok(history) = FileBackedHistory::with_file(10_000, history_path) {
        editor = editor.with_history(Box::new(history));
    }

    let mut last_duration = None;
    let mut first = true;
    loop {
        let cwd = std::env::current_dir().unwrap_or_default();
        set_title(&sys::display_path(&cwd));
        if !first {
            println!();
        }
        first = false;
        let prompt = NebulaPrompt::new(shell.last_status, last_duration);
        match editor.read_line(&prompt) {
            Ok(Signal::Success(line)) => {
                if line.trim().is_empty() {
                    first = true;
                    continue;
                }
                let line = match expand_history(&line, shell.history.last().map(String::as_str)) {
                    Some(expanded) => {
                        println!("{expanded}");
                        expanded
                    }
                    None => line,
                };
                shell.history.push(line.clone());
                let name = line.split_whitespace().next().unwrap_or("").to_owned();
                set_title(&name);
                let started = Instant::now();
                shell.run_line(&line);
                last_duration = Some(started.elapsed());
                known.update(shell);
                if name == "clear" {
                    first = true;
                }
                if shell.exit_code.is_some() {
                    break;
                }
            }
            Ok(Signal::CtrlC) => {
                first = true;
            }
            Ok(Signal::CtrlD) => break,
            Ok(_) => {}
            Err(error) => {
                eprintln!("nebula: {error}");
                break;
            }
        }
    }
    shell.exit_code.unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_bang_bang() {
        assert_eq!(
            expand_history("sudo !!", Some("apt update")).as_deref(),
            Some("sudo apt update")
        );
        assert_eq!(
            expand_history("cd !$", Some("mkdir -p a/b")).as_deref(),
            Some("cd a/b")
        );
        assert_eq!(expand_history("echo '!!'", Some("x")), None);
        assert_eq!(expand_history("echo hi!", Some("x")), None);
    }

    #[test]
    fn lexes_commands_after_operators() {
        let tokens = lex("ls -la | grep \"x y\" && echo $HOME");
        let commands: Vec<&str> = tokens
            .iter()
            .filter(|(t, _)| *t == Tok::Word { command: true })
            .map(|(_, s)| s.as_str())
            .collect();
        assert_eq!(commands, vec!["ls", "grep", "echo"]);
        assert!(tokens.iter().any(|(t, s)| *t == Tok::Var && s == "$HOME"));
        assert!(tokens
            .iter()
            .any(|(t, s)| *t == Tok::Quote && s == "\"x y\""));
    }

    #[test]
    fn treats_assignments_as_prefix() {
        let tokens = lex("FOO=1 cargo build");
        let commands: Vec<&str> = tokens
            .iter()
            .filter(|(t, _)| *t == Tok::Word { command: true })
            .map(|(_, s)| s.as_str())
            .collect();
        assert_eq!(commands, vec!["cargo"]);
    }
}

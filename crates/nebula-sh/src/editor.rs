//! Interactive mode: line editing with syntax colors, history suggestions,
//! Tab completion, history expansion and the window title.

use crate::complete::{Known, NebulaCompleter};
use crate::exec::Shell;
use crate::prompt::NebulaPrompt;
use crate::sys;
use nu_ansi_term::{Color, Style};
use reedline::{
    default_emacs_keybindings, ColumnarMenu, DefaultHinter, Emacs, FileBackedHistory, Highlighter,
    KeyCode, KeyModifiers, MenuBuilder, Reedline, ReedlineEvent, ReedlineMenu, Signal, StyledText,
    ValidationResult, Validator,
};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Highlighting

struct NebulaHighlighter {
    known: Known,
}

#[derive(PartialEq)]
enum Tok {
    Space,
    Word { command: bool },
    Keyword,
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
        if matches!(c, '|' | ';' | '>' | '<' | '&' | '(' | ')') {
            out.push((Tok::Op, c.to_string()));
            i += 1;
            expect_command = matches!(c, '|' | ';' | '&' | '(');
            continue;
        }
        // A word, possibly with quotes and expansions inside.
        let mut pieces: Vec<(Tok, String)> = Vec::new();
        let mut plain = String::new();
        let mut word_text = String::new();
        while i < chars.len() {
            let c = chars[i];
            if c.is_whitespace() || matches!(c, '|' | ';' | '>' | '<' | '&' | '(' | ')') {
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
        let keyword = expect_command
            && pieces.len() == 1
            && matches!(pieces[0].0, Tok::Word { .. })
            && crate::parse::KEYWORDS.contains(&word_text.as_str());
        if keyword {
            out.push((Tok::Keyword, word_text.clone()));
            // After these, the next word is a command again (`then ls`, `do echo`).
            expect_command = matches!(
                word_text.as_str(),
                "if" | "then" | "else" | "elif" | "do" | "while" | "until" | "{" | "!" | "time"
            );
            continue;
        }
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
                Tok::Keyword => Style::new().fg(Color::Purple).bold(),
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

/// The last word of a command line as typed, quotes included (`"My Docs"`).
fn last_word(line: &str) -> &str {
    let mut start = 0;
    let mut end = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut in_word = false;
    for (index, c) in line.char_indices() {
        let next = index + c.len_utf8();
        if escaped {
            escaped = false;
            end = next;
            continue;
        }
        match (quote, c) {
            (Some(q), c) if c == q => {
                quote = None;
                end = next;
            }
            (Some(_), _) => end = next,
            (None, c) if c.is_whitespace() || matches!(c, '|' | ';' | '&' | '<' | '>') => {
                in_word = false;
            }
            (None, c) => {
                if !in_word {
                    start = index;
                    in_word = true;
                }
                match c {
                    '\\' => escaped = true,
                    '\'' | '"' => quote = Some(c),
                    _ => {}
                }
                end = next;
            }
        }
    }
    &line[start..end]
}

/// `!!` is the previous line, `!$` its last argument (not inside single quotes,
/// and not after a backslash).
pub fn expand_history(line: &str, previous: Option<&str>) -> Option<String> {
    let previous = previous?;
    if !line.contains('!') {
        return None;
    }
    let last_arg = last_word(previous);
    let mut out = String::new();
    let mut quote: Option<char> = None;
    let mut changed = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && quote != Some('\'') {
            out.push(c);
            out.extend(chars.next());
            continue;
        }
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '\'' | '"') => quote = Some(c),
            _ => {}
        }
        if c == '!' && quote != Some('\'') {
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

/// Terminals that understand the FinalTerm shell-integration marks (OSC 133): Nebula
/// Terminal uses them to notice long commands finishing in the background.
fn shell_integration() -> bool {
    std::env::var_os("NEBULA_TERMINAL").is_some()
        || std::env::var_os("WT_SESSION").is_some()
        || std::env::var("TERM_PROGRAM").is_ok_and(|program| program == "vscode")
}

fn mark(sequence: &str) {
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]133;{sequence}\x07");
    let _ = out.flush();
}

pub fn interactive(shell: &mut Shell) -> i32 {
    shell.interactive = true;
    let integration = shell_integration();
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
                if integration {
                    mark("C");
                }
                let started = Instant::now();
                let status = shell.run_line(&line);
                last_duration = Some(started.elapsed());
                if integration {
                    mark(&format!("D;{status}"));
                }
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
        assert_eq!(
            expand_history("cd !$", Some("mkdir \"My Docs\"")).as_deref(),
            Some("cd \"My Docs\"")
        );
        assert_eq!(
            expand_history("echo \"don't\" !!", Some("ls")).as_deref(),
            Some("echo \"don't\" ls")
        );
        assert_eq!(expand_history(r"echo \!!", Some("ls")), None);
    }

    #[test]
    fn highlights_keywords() {
        let tokens = lex("if true; then echo if; fi");
        let keywords: Vec<&str> = tokens
            .iter()
            .filter(|(t, _)| *t == Tok::Keyword)
            .map(|(_, s)| s.as_str())
            .collect();
        assert_eq!(keywords, vec!["if", "then", "fi"]);
        let commands: Vec<&str> = tokens
            .iter()
            .filter(|(t, _)| *t == Tok::Word { command: true })
            .map(|(_, s)| s.as_str())
            .collect();
        assert_eq!(commands, vec!["true", "echo"]);
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

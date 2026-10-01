//! Parser for the interactive subset of POSIX shell syntax that Nebula supports:
//! words with quoting and `$` expansions, pipelines, `&&` / `||` / `;` lists and
//! redirections. Control flow (`if`, `for`, functions) is intentionally out of scope.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// Unquoted text; may contain glob characters.
    Lit(String),
    /// Quoted text; never globbed or split.
    Quoted(String),
    /// `$NAME`, `${NAME}`, `$?`, `$$`.
    Var { name: String, quoted: bool },
    /// `$(command)`.
    Subst { source: String, quoted: bool },
    /// A leading unquoted `~`.
    Tilde,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Word(pub Vec<Part>);

impl Word {
    /// The word as plain text when it contains no expansions.
    pub fn as_plain(&self) -> Option<String> {
        let mut out = String::new();
        for part in &self.0 {
            match part {
                Part::Lit(text) | Part::Quoted(text) => out.push_str(text),
                _ => return None,
            }
        }
        Some(out)
    }

    fn is_quoted_anywhere(&self) -> bool {
        self.0.iter().any(|part| !matches!(part, Part::Lit(_)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectKind {
    Read,
    Write,
    Append,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedirectTarget {
    File(Word),
    /// `>&N`: duplicate another descriptor.
    Fd(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirect {
    pub fd: u32,
    pub kind: RedirectKind,
    pub target: RedirectTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Command {
    pub assignments: Vec<(String, Word)>,
    pub words: Vec<Word>,
    pub redirects: Vec<Redirect>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pipeline {
    pub negate: bool,
    pub commands: Vec<Command>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connector {
    And,
    Or,
    Seq,
}

/// A sequence of pipelines joined by `&&`, `||` and `;`. The connector of the
/// first entry is ignored.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct List(pub Vec<(Connector, Pipeline)>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    /// More input could complete the line (open quote, trailing `|`…).
    pub incomplete: bool,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

fn error(message: impl Into<String>) -> ParseError {
    ParseError {
        message: message.into(),
        incomplete: false,
    }
}

fn incomplete(message: impl Into<String>) -> ParseError {
    ParseError {
        message: message.into(),
        incomplete: true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(Word),
    Pipe,
    And,
    Or,
    Seq,
    Background,
    Redirect {
        fd: u32,
        kind: RedirectKind,
        dup: Option<u32>,
        both: bool,
    },
}

/// Characters that a backslash escapes outside quotes. Any other backslash is kept,
/// so Windows paths such as `C:\Users\me` work unquoted.
fn escapable(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t'
            | '\''
            | '"'
            | '\\'
            | '$'
            | '`'
            | '|'
            | '&'
            | ';'
            | '<'
            | '>'
            | '('
            | ')'
            | '*'
            | '?'
            | '['
            | ']'
            | '#'
            | '~'
            | '{'
            | '}'
            | '!'
            | '\n'
    )
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

struct Lexer<'a> {
    chars: Vec<char>,
    pos: usize,
    _source: &'a str,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            chars: source.chars().collect(),
            pos: 0,
            _source: source,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn tokens(mut self) -> Result<Vec<Token>, ParseError> {
        let mut tokens = Vec::new();
        loop {
            while matches!(self.peek(), Some(' ' | '\t' | '\r')) {
                self.pos += 1;
            }
            let Some(c) = self.peek() else { break };
            match c {
                '#' => {
                    while !matches!(self.peek(), None | Some('\n')) {
                        self.pos += 1;
                    }
                }
                '\n' | ';' => {
                    self.pos += 1;
                    tokens.push(Token::Seq);
                }
                '|' => {
                    self.pos += 1;
                    if self.peek() == Some('|') {
                        self.pos += 1;
                        tokens.push(Token::Or);
                    } else {
                        tokens.push(Token::Pipe);
                    }
                }
                '&' => {
                    self.pos += 1;
                    if self.peek() == Some('&') {
                        self.pos += 1;
                        tokens.push(Token::And);
                    } else if self.peek() == Some('>') {
                        self.pos += 1;
                        let kind = if self.peek() == Some('>') {
                            self.pos += 1;
                            RedirectKind::Append
                        } else {
                            RedirectKind::Write
                        };
                        tokens.push(Token::Redirect {
                            fd: 1,
                            kind,
                            dup: None,
                            both: true,
                        });
                    } else {
                        tokens.push(Token::Background);
                    }
                }
                '(' | ')' => return Err(error("subshells `( … )` are not supported")),
                '<' | '>' => tokens.push(self.redirect(None)?),
                '0'..='2' if matches!(self.peek_at(1), Some('<' | '>')) => {
                    let fd = c.to_digit(10).unwrap_or(1);
                    self.pos += 1;
                    tokens.push(self.redirect(Some(fd))?);
                }
                _ => tokens.push(Token::Word(self.word()?)),
            }
        }
        Ok(tokens)
    }

    fn redirect(&mut self, fd: Option<u32>) -> Result<Token, ParseError> {
        let c = self.peek().unwrap_or('>');
        self.pos += 1;
        let (fd, kind) = if c == '<' {
            (fd.unwrap_or(0), RedirectKind::Read)
        } else if self.peek() == Some('>') {
            self.pos += 1;
            (fd.unwrap_or(1), RedirectKind::Append)
        } else {
            (fd.unwrap_or(1), RedirectKind::Write)
        };
        if self.peek() == Some('&') && kind != RedirectKind::Read {
            if let Some(target) = self.peek_at(1).and_then(|d| d.to_digit(10)) {
                self.pos += 2;
                return Ok(Token::Redirect {
                    fd,
                    kind,
                    dup: Some(target),
                    both: false,
                });
            }
        }
        Ok(Token::Redirect {
            fd,
            kind,
            dup: None,
            both: false,
        })
    }

    fn word(&mut self) -> Result<Word, ParseError> {
        let mut parts: Vec<Part> = Vec::new();
        let mut literal = String::new();
        let start = self.pos;

        let flush = |literal: &mut String, parts: &mut Vec<Part>| {
            if !literal.is_empty() {
                parts.push(Part::Lit(std::mem::take(literal)));
            }
        };

        while let Some(c) = self.peek() {
            match c {
                ' ' | '\t' | '\r' | '\n' | '|' | '&' | ';' | '<' | '>' | '(' | ')' => break,
                '~' if self.pos == start
                    && matches!(
                        self.peek_at(1),
                        None | Some('/' | '\\' | ' ' | '\t' | ';' | '|' | '&' | ')')
                    ) =>
                {
                    self.pos += 1;
                    parts.push(Part::Tilde);
                }
                '\\' => match self.peek_at(1) {
                    Some('\n') => self.pos += 2,
                    Some(next) if escapable(next) => {
                        flush(&mut literal, &mut parts);
                        parts.push(Part::Quoted(next.to_string()));
                        self.pos += 2;
                    }
                    None => return Err(incomplete("line continues after `\\`")),
                    Some(_) => {
                        literal.push('\\');
                        self.pos += 1;
                    }
                },
                '\'' => {
                    flush(&mut literal, &mut parts);
                    self.pos += 1;
                    let mut text = String::new();
                    loop {
                        match self.peek() {
                            None => return Err(incomplete("unclosed single quote")),
                            Some('\'') => {
                                self.pos += 1;
                                break;
                            }
                            Some(ch) => {
                                text.push(ch);
                                self.pos += 1;
                            }
                        }
                    }
                    parts.push(Part::Quoted(text));
                }
                '"' => {
                    flush(&mut literal, &mut parts);
                    self.pos += 1;
                    self.double_quoted(&mut parts)?;
                }
                '$' => {
                    flush(&mut literal, &mut parts);
                    if let Some(part) = self.dollar(false)? {
                        parts.push(part);
                    } else {
                        literal.push('$');
                    }
                }
                '`' => return Err(error("use $(command) instead of backticks")),
                _ => {
                    literal.push(c);
                    self.pos += 1;
                }
            }
        }
        flush(&mut literal, &mut parts);
        if parts.is_empty() {
            parts.push(Part::Quoted(String::new()));
        }
        Ok(Word(parts))
    }

    fn double_quoted(&mut self, parts: &mut Vec<Part>) -> Result<(), ParseError> {
        let mut text = String::new();
        let mut any = false;
        loop {
            match self.peek() {
                None => return Err(incomplete("unclosed double quote")),
                Some('"') => {
                    self.pos += 1;
                    break;
                }
                Some('\\') if matches!(self.peek_at(1), Some('"' | '\\' | '$' | '`' | '\n')) => {
                    let next = self.peek_at(1).unwrap_or('\\');
                    if next != '\n' {
                        text.push(next);
                    }
                    self.pos += 2;
                }
                Some('$') => {
                    if let Some(part) = self.dollar(true)? {
                        if !text.is_empty() {
                            parts.push(Part::Quoted(std::mem::take(&mut text)));
                        }
                        parts.push(part);
                        any = true;
                    } else {
                        text.push('$');
                    }
                }
                Some(ch) => {
                    text.push(ch);
                    self.pos += 1;
                }
            }
        }
        if !text.is_empty() || !any {
            parts.push(Part::Quoted(text));
        }
        Ok(())
    }

    /// Parses an expansion starting at `$`. Returns `None` (consuming the `$`) for a lone dollar sign.
    fn dollar(&mut self, quoted: bool) -> Result<Option<Part>, ParseError> {
        self.pos += 1;
        match self.peek() {
            Some('(') => {
                self.pos += 1;
                let source = self.balanced_parens()?;
                Ok(Some(Part::Subst { source, quoted }))
            }
            Some('{') => {
                self.pos += 1;
                let mut name = String::new();
                loop {
                    match self.peek() {
                        None => return Err(incomplete("unclosed ${")),
                        Some('}') => {
                            self.pos += 1;
                            break;
                        }
                        Some(ch) => {
                            name.push(ch);
                            self.pos += 1;
                        }
                    }
                }
                if name.is_empty()
                    || !(name.chars().all(is_name_char) || name == "?" || name == "$")
                {
                    return Err(error(format!("bad substitution: ${{{name}}}")));
                }
                Ok(Some(Part::Var { name, quoted }))
            }
            Some(c @ ('?' | '$' | '0'..='9')) => {
                self.pos += 1;
                Ok(Some(Part::Var {
                    name: c.to_string(),
                    quoted,
                }))
            }
            Some(c) if is_name_start(c) => {
                let mut name = String::new();
                while let Some(ch) = self.peek().filter(|ch| is_name_char(*ch)) {
                    name.push(ch);
                    self.pos += 1;
                }
                Ok(Some(Part::Var { name, quoted }))
            }
            _ => Ok(None),
        }
    }

    fn balanced_parens(&mut self) -> Result<String, ParseError> {
        let mut depth = 1;
        let mut source = String::new();
        let mut quote: Option<char> = None;
        while let Some(c) = self.peek() {
            self.pos += 1;
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => {}
                None => match c {
                    '\'' | '"' => quote = Some(c),
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(source);
                        }
                    }
                    _ => {}
                },
            }
            source.push(c);
        }
        Err(incomplete("unclosed $("))
    }
}

fn assignment(word: &Word) -> Option<(String, Word)> {
    let Some(Part::Lit(first)) = word.0.first() else {
        return None;
    };
    let eq = first.find('=')?;
    let name = &first[..eq];
    if name.is_empty()
        || !name.chars().next().is_some_and(is_name_start)
        || !name.chars().all(is_name_char)
    {
        return None;
    }
    let mut value = Vec::new();
    let rest = &first[eq + 1..];
    if !rest.is_empty() {
        value.push(Part::Lit(rest.to_owned()));
    }
    value.extend(word.0[1..].iter().cloned());
    if value.is_empty() {
        value.push(Part::Quoted(String::new()));
    }
    Some((name.to_owned(), Word(value)))
}

pub fn parse(source: &str) -> Result<List, ParseError> {
    let tokens = Lexer::new(source).tokens()?;
    let mut list = List::default();
    let mut connector = Connector::Seq;
    let mut pipeline = Pipeline::default();
    let mut command = Command::default();
    let mut iter = tokens.into_iter().peekable();
    // Whether the token stream just ended an operator that needs a right-hand side.
    let mut dangling: Option<&'static str> = None;

    let finish_command =
        |command: &mut Command, pipeline: &mut Pipeline| -> Result<(), ParseError> {
            if command.words.is_empty()
                && command.assignments.is_empty()
                && command.redirects.is_empty()
            {
                return Err(error("missing command"));
            }
            pipeline.commands.push(std::mem::take(command));
            Ok(())
        };

    while let Some(token) = iter.next() {
        match token {
            Token::Word(word) => {
                dangling = None;
                if command.words.is_empty()
                    && pipeline.commands.is_empty()
                    && command.assignments.is_empty()
                    && word.as_plain().as_deref() == Some("!")
                    && !word.is_quoted_anywhere()
                {
                    pipeline.negate = !pipeline.negate;
                    continue;
                }
                if command.words.is_empty() {
                    if let Some(assign) = assignment(&word) {
                        command.assignments.push(assign);
                        continue;
                    }
                }
                command.words.push(word);
            }
            Token::Redirect {
                fd,
                kind,
                dup,
                both,
            } => {
                let target = match dup {
                    Some(target) => RedirectTarget::Fd(target),
                    None => match iter.next() {
                        Some(Token::Word(word)) => RedirectTarget::File(word),
                        None => return Err(incomplete("missing file after redirection")),
                        Some(_) => return Err(error("missing file after redirection")),
                    },
                };
                if both {
                    command.redirects.push(Redirect {
                        fd: 1,
                        kind,
                        target: target.clone(),
                    });
                    command.redirects.push(Redirect {
                        fd: 2,
                        kind: RedirectKind::Write,
                        target: RedirectTarget::Fd(1),
                    });
                } else {
                    command.redirects.push(Redirect { fd, kind, target });
                }
                dangling = None;
            }
            Token::Pipe => {
                finish_command(&mut command, &mut pipeline)?;
                dangling = Some("|");
            }
            Token::And | Token::Or | Token::Seq => {
                if token == Token::Seq
                    && command.words.is_empty()
                    && command.assignments.is_empty()
                    && command.redirects.is_empty()
                    && pipeline.commands.is_empty()
                {
                    if dangling.is_some() {
                        continue;
                    }
                    continue;
                }
                finish_command(&mut command, &mut pipeline)?;
                list.0.push((connector, std::mem::take(&mut pipeline)));
                connector = match token {
                    Token::And => Connector::And,
                    Token::Or => Connector::Or,
                    _ => Connector::Seq,
                };
                dangling = match token {
                    Token::And => Some("&&"),
                    Token::Or => Some("||"),
                    _ => None,
                };
            }
            Token::Background => {
                return Err(error(
                    "background jobs `&` are not supported; open a new tab or pane instead",
                ))
            }
        }
    }

    if let Some(op) = dangling {
        return Err(incomplete(format!("command expected after `{op}`")));
    }
    if !(command.words.is_empty() && command.assignments.is_empty() && command.redirects.is_empty())
        || !pipeline.commands.is_empty()
    {
        finish_command(&mut command, &mut pipeline)?;
        list.0.push((connector, pipeline));
    }
    Ok(list)
}

/// Quotes a string so the parser reads it back as a single literal word.
pub fn quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || "-_./:=+,@%\\".contains(c))
    {
        return text.to_owned();
    }
    format!("'{}'", text.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(source: &str) -> Vec<Vec<String>> {
        parse(source)
            .unwrap()
            .0
            .into_iter()
            .flat_map(|(_, p)| p.commands)
            .map(|c| {
                c.words
                    .iter()
                    .map(|w| w.as_plain().unwrap_or_else(|| format!("{w:?}")))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn splits_words_and_quotes() {
        assert_eq!(
            words(r#"echo 'a b' "c d" e\ f"#),
            vec![vec!["echo", "a b", "c d", "e f"]]
        );
    }

    #[test]
    fn keeps_windows_backslashes() {
        assert_eq!(words(r"cd C:\Users\me"), vec![vec!["cd", r"C:\Users\me"]]);
    }

    #[test]
    fn parses_operators() {
        let list = parse("a | b && c || d; e").unwrap();
        let connectors: Vec<_> = list.0.iter().map(|(c, _)| *c).collect();
        assert_eq!(
            connectors,
            vec![
                Connector::Seq,
                Connector::And,
                Connector::Or,
                Connector::Seq
            ]
        );
        assert_eq!(list.0[0].1.commands.len(), 2);
    }

    #[test]
    fn parses_redirections() {
        let list = parse("cmd > out.txt 2>&1 < in.txt >> log").unwrap();
        let redirects = &list.0[0].1.commands[0].redirects;
        assert_eq!(redirects.len(), 4);
        assert_eq!(
            redirects[1],
            Redirect {
                fd: 2,
                kind: RedirectKind::Write,
                target: RedirectTarget::Fd(1)
            }
        );
        assert_eq!(redirects[3].kind, RedirectKind::Append);
    }

    #[test]
    fn parses_ampersand_redirect() {
        let list = parse("cmd &> all.log").unwrap();
        assert_eq!(list.0[0].1.commands[0].redirects.len(), 2);
    }

    #[test]
    fn parses_expansions() {
        let list = parse(r#"echo $HOME "${USER}x" $(pwd) ~/a $?"#).unwrap();
        let words = &list.0[0].1.commands[0].words;
        assert_eq!(
            words[1].0,
            vec![Part::Var {
                name: "HOME".into(),
                quoted: false
            }]
        );
        assert_eq!(
            words[2].0,
            vec![
                Part::Var {
                    name: "USER".into(),
                    quoted: true
                },
                Part::Quoted("x".into())
            ]
        );
        assert_eq!(
            words[3].0,
            vec![Part::Subst {
                source: "pwd".into(),
                quoted: false
            }]
        );
        assert_eq!(words[4].0, vec![Part::Tilde, Part::Lit("/a".into())]);
        assert_eq!(
            words[5].0,
            vec![Part::Var {
                name: "?".into(),
                quoted: false
            }]
        );
    }

    #[test]
    fn parses_assignments() {
        let list = parse("FOO=bar BAZ= cmd x=y").unwrap();
        let command = &list.0[0].1.commands[0];
        assert_eq!(command.assignments.len(), 2);
        assert_eq!(command.words.len(), 2);
    }

    #[test]
    fn reports_incomplete_input() {
        for source in ["echo 'abc", "echo \"abc", "ls |", "a &&", "echo $(pwd"] {
            assert!(parse(source).unwrap_err().incomplete, "{source}");
        }
        assert!(!parse("| ls").unwrap_err().incomplete);
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        assert_eq!(words("# hi\n\nls # list\n"), vec![vec!["ls"]]);
    }

    #[test]
    fn quote_round_trips() {
        for text in ["plain", "with space", "it's", "", "$HOME"] {
            assert_eq!(words(&format!("echo {}", quote(text)))[0][1], text);
        }
    }
}

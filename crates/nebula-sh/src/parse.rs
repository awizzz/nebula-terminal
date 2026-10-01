//! Parser for the shell language Nebula supports: words with quoting and `$`
//! expansions, pipelines, `&&` / `||` / `;` lists, redirections and here-documents,
//! and the usual control flow: `if`, `for`, `while`, `until`, `case`, `{ … }`,
//! `( … )`, `(( … ))`, `[[ … ]]` and functions.

use std::fmt;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// Unquoted text; may contain glob characters.
    Lit(String),
    /// Quoted text; never globbed or split.
    Quoted(String),
    /// `$NAME`, `${NAME}`, `$?`, `$1`, `$@`…
    Var { name: String, quoted: bool },
    /// `${NAME<op>…}`.
    Param {
        name: String,
        op: Box<ParamOp>,
        quoted: bool,
    },
    /// `$(command)`.
    Subst { source: String, quoted: bool },
    /// `$((expression))`.
    Arith { source: String, quoted: bool },
    /// A leading unquoted `~`.
    Tilde,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamOp {
    /// `${#name}`
    Length,
    /// `${name:-word}` / `${name-word}`
    Default { colon: bool, word: Word },
    /// `${name:=word}` / `${name=word}`
    Assign { colon: bool, word: Word },
    /// `${name:+word}` / `${name+word}`
    Alternative { colon: bool, word: Word },
    /// `${name:?word}` / `${name?word}`
    Error { colon: bool, word: Word },
    /// `${name#pattern}` / `${name##pattern}`
    RemovePrefix { longest: bool, pattern: Word },
    /// `${name%pattern}` / `${name%%pattern}`
    RemoveSuffix { longest: bool, pattern: Word },
    /// `${name/pattern/replacement}` and the `//`, `/#`, `/%` forms.
    Replace {
        all: bool,
        anchor: Anchor,
        pattern: Word,
        replacement: Word,
    },
    /// `${name:offset}` / `${name:offset:length}` (arithmetic expressions).
    Substring {
        offset: String,
        length: Option<String>,
    },
    /// `${name^}`, `${name^^}`, `${name,}`, `${name,,}`
    Case { upper: bool, all: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    None,
    Start,
    End,
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

    /// The text of a word made of one unquoted literal: how reserved words are recognised.
    pub fn keyword(&self) -> Option<&str> {
        match self.0.as_slice() {
            [Part::Lit(text)] => Some(text),
            _ => None,
        }
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
    /// A here-document or here-string: the text becomes standard input.
    Text(Word),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirect {
    pub fd: u32,
    pub kind: RedirectKind,
    pub target: RedirectTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Simple {
    pub assignments: Vec<(String, Word)>,
    pub words: Vec<Word>,
    pub redirects: Vec<Redirect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Simple(Simple),
    Compound {
        body: Compound,
        redirects: Vec<Redirect>,
    },
    Function(Rc<Function>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub body: Command,
    /// The definition as written, used to pass functions to sub-shells and by `type`.
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Compound {
    Group(List),
    /// `( … )`; `source` is the text between the parentheses.
    Subshell {
        list: List,
        source: String,
    },
    If {
        branches: Vec<(List, List)>,
        otherwise: Option<List>,
    },
    For {
        var: String,
        items: Option<Vec<Word>>,
        body: List,
    },
    ArithFor {
        init: String,
        test: String,
        step: String,
        body: List,
    },
    Loop {
        until: bool,
        test: List,
        body: List,
    },
    Case {
        word: Word,
        arms: Vec<CaseArm>,
    },
    Arith(String),
    Test(Vec<TestToken>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseArm {
    pub patterns: Vec<Word>,
    pub body: List,
    pub end: CaseEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseEnd {
    /// `;;`
    Break,
    /// `;&`: run the next arm's body too.
    FallThrough,
    /// `;;&`: keep testing the following patterns.
    Continue,
}

/// One token of a `[[ … ]]` expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestToken {
    Word(Word),
    And,
    Or,
    Not,
    Open,
    Close,
    Less,
    Greater,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pipeline {
    pub negate: bool,
    pub commands: Vec<Command>,
    /// The text of each command, for stages that run in a sub-shell.
    pub sources: Vec<String>,
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
    /// More input could complete the line (open quote, trailing `|`, missing `fi`…).
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

/// Words that end a list when they appear where a command is expected.
const CLOSERS: &[&str] = &["then", "elif", "else", "fi", "do", "done", "esac", "}"];

pub const KEYWORDS: &[&str] = &[
    "if", "then", "elif", "else", "fi", "for", "in", "while", "until", "do", "done", "case",
    "esac", "function", "{", "}", "[[", "]]", "!",
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(Word),
    Newline,
    Semi,
    DoubleSemi,
    SemiAmp,
    DoubleSemiAmp,
    Pipe,
    And,
    Or,
    Background,
    Open,
    Close,
    /// `(( … ))` where a command is expected.
    Arith(String),
    Redirect {
        fd: u32,
        kind: RedirectKind,
        dup: Option<u32>,
        both: bool,
    },
    /// `<<<word`
    HereString {
        fd: u32,
    },
    /// `<<DELIM`; the body is filled in once the end of the line is reached.
    HereDoc {
        fd: u32,
        body: Option<Word>,
    },
}

#[derive(Debug, Clone)]
struct Spanned {
    token: Token,
    start: usize,
    end: usize,
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
            | '='
            | '\n'
    )
}

pub fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

pub fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

pub fn is_name(text: &str) -> bool {
    text.chars().next().is_some_and(is_name_start) && text.chars().all(is_name_char)
}

fn is_special_param(c: char) -> bool {
    matches!(c, '?' | '$' | '#' | '@' | '*' | '!' | '-' | '0'..='9')
}

struct PendingHereDoc {
    index: usize,
    delimiter: String,
    strip_tabs: bool,
    expand: bool,
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    pending: Vec<PendingHereDoc>,
}

impl Lexer {
    fn new(source: &str) -> Self {
        Self {
            chars: source.replace("\r\n", "\n").chars().collect(),
            pos: 0,
            pending: Vec::new(),
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn tokens(mut self) -> Result<(Vec<Spanned>, Vec<char>), ParseError> {
        let mut tokens: Vec<Spanned> = Vec::new();
        loop {
            while matches!(self.peek(), Some(' ' | '\t' | '\r')) {
                self.pos += 1;
            }
            let Some(c) = self.peek() else { break };
            let start = self.pos;
            let token = match c {
                '#' => {
                    while !matches!(self.peek(), None | Some('\n')) {
                        self.pos += 1;
                    }
                    continue;
                }
                '\n' => {
                    self.pos += 1;
                    self.read_here_docs(&mut tokens)?;
                    Token::Newline
                }
                ';' => {
                    self.pos += 1;
                    if self.peek() == Some(';') {
                        self.pos += 1;
                        if self.peek() == Some('&') {
                            self.pos += 1;
                            Token::DoubleSemiAmp
                        } else {
                            Token::DoubleSemi
                        }
                    } else if self.peek() == Some('&') {
                        self.pos += 1;
                        Token::SemiAmp
                    } else {
                        Token::Semi
                    }
                }
                '|' => {
                    self.pos += 1;
                    if self.peek() == Some('|') {
                        self.pos += 1;
                        Token::Or
                    } else {
                        Token::Pipe
                    }
                }
                '&' => {
                    self.pos += 1;
                    if self.peek() == Some('&') {
                        self.pos += 1;
                        Token::And
                    } else if self.peek() == Some('>') {
                        self.pos += 1;
                        let kind = if self.peek() == Some('>') {
                            self.pos += 1;
                            RedirectKind::Append
                        } else {
                            RedirectKind::Write
                        };
                        Token::Redirect {
                            fd: 1,
                            kind,
                            dup: None,
                            both: true,
                        }
                    } else {
                        Token::Background
                    }
                }
                '(' if self.peek_at(1) == Some('(') => {
                    self.pos += 2;
                    Token::Arith(self.arithmetic()?)
                }
                '(' => {
                    self.pos += 1;
                    Token::Open
                }
                ')' => {
                    self.pos += 1;
                    Token::Close
                }
                '<' | '>' => self.redirect(None, tokens.len())?,
                '0'..='9' if matches!(self.peek_at(1), Some('<' | '>')) => {
                    let fd = c.to_digit(10).unwrap_or(1);
                    self.pos += 1;
                    self.redirect(Some(fd), tokens.len())?
                }
                _ => Token::Word(self.word()?),
            };
            tokens.push(Spanned {
                token,
                start,
                end: self.pos,
            });
        }
        if let Some(pending) = self.pending.first() {
            return Err(incomplete(format!(
                "here-document is not closed (expected `{}`)",
                pending.delimiter
            )));
        }
        Ok((tokens, self.chars))
    }

    fn redirect(&mut self, fd: Option<u32>, index: usize) -> Result<Token, ParseError> {
        let c = self.peek().unwrap_or('>');
        self.pos += 1;
        if c == '<' && self.peek() == Some('<') {
            self.pos += 1;
            if self.peek() == Some('<') {
                self.pos += 1;
                return Ok(Token::HereString {
                    fd: fd.unwrap_or(0),
                });
            }
            let strip_tabs = self.peek() == Some('-');
            if strip_tabs {
                self.pos += 1;
            }
            while matches!(self.peek(), Some(' ' | '\t')) {
                self.pos += 1;
            }
            let (delimiter, quoted) = self.here_doc_delimiter()?;
            self.pending.push(PendingHereDoc {
                index,
                delimiter,
                strip_tabs,
                expand: !quoted,
            });
            return Ok(Token::HereDoc {
                fd: fd.unwrap_or(0),
                body: None,
            });
        }
        let (fd, kind) = if c == '<' {
            (fd.unwrap_or(0), RedirectKind::Read)
        } else if self.peek() == Some('>') {
            self.pos += 1;
            (fd.unwrap_or(1), RedirectKind::Append)
        } else {
            if self.peek() == Some('|') {
                self.pos += 1;
            }
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

    /// Reads the delimiter after `<<`. Quoting any part of it turns off expansion.
    fn here_doc_delimiter(&mut self) -> Result<(String, bool), ParseError> {
        let mut text = String::new();
        let mut quoted = false;
        while let Some(c) = self.peek() {
            match c {
                ' ' | '\t' | '\n' | ';' | '|' | '&' | '<' | '>' | '(' | ')' => break,
                '\'' | '"' => {
                    quoted = true;
                    self.pos += 1;
                    loop {
                        match self.peek() {
                            None => return Err(incomplete("unclosed quote")),
                            Some(ch) if ch == c => {
                                self.pos += 1;
                                break;
                            }
                            Some(ch) => {
                                text.push(ch);
                                self.pos += 1;
                            }
                        }
                    }
                }
                '\\' => {
                    quoted = true;
                    self.pos += 1;
                    if let Some(ch) = self.peek() {
                        text.push(ch);
                        self.pos += 1;
                    }
                }
                _ => {
                    text.push(c);
                    self.pos += 1;
                }
            }
        }
        if text.is_empty() {
            return Err(error("missing here-document delimiter after `<<`"));
        }
        Ok((text, quoted))
    }

    /// Called after a newline: reads the bodies of the here-documents opened on that line.
    fn read_here_docs(&mut self, tokens: &mut [Spanned]) -> Result<(), ParseError> {
        for pending in std::mem::take(&mut self.pending) {
            let mut body = String::new();
            let mut closed = false;
            while self.pos < self.chars.len() {
                let mut line = String::new();
                while let Some(c) = self.peek() {
                    self.pos += 1;
                    if c == '\n' {
                        break;
                    }
                    line.push(c);
                }
                let line = if pending.strip_tabs {
                    line.trim_start_matches('\t').to_owned()
                } else {
                    line
                };
                if line.trim_end_matches('\r') == pending.delimiter {
                    closed = true;
                    break;
                }
                body.push_str(&line);
                body.push('\n');
            }
            if !closed {
                return Err(incomplete(format!(
                    "here-document is not closed (expected `{}`)",
                    pending.delimiter
                )));
            }
            let word = if pending.expand {
                lex_text(&body, TextMode::HereDoc)?
            } else {
                Word(vec![Part::Quoted(body)])
            };
            if let Some(Spanned {
                token: Token::HereDoc { body, .. },
                ..
            }) = tokens.get_mut(pending.index)
            {
                *body = Some(word);
            }
        }
        Ok(())
    }

    /// Reads the inside of `(( … ))` after the opening parentheses.
    fn arithmetic(&mut self) -> Result<String, ParseError> {
        let mut depth = 0usize;
        let mut source = String::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                '(' => depth += 1,
                ')' if depth == 0 => {
                    if self.peek() == Some(')') {
                        self.pos += 1;
                        return Ok(source);
                    }
                    return Err(error("expected `))`"));
                }
                ')' => depth -= 1,
                _ => {}
            }
            source.push(c);
        }
        Err(incomplete("unclosed (("))
    }

    fn word(&mut self) -> Result<Word, ParseError> {
        let mut parts: Vec<Part> = Vec::new();
        let mut literal = String::new();
        let start = self.pos;

        while let Some(c) = self.peek() {
            match c {
                ' ' | '\t' | '\r' | '\n' | '|' | '&' | ';' | '<' | '>' | '(' | ')' => break,
                '~' if self.pos == start
                    && matches!(
                        self.peek_at(1),
                        None | Some('/' | '\\' | ' ' | '\t' | '\n' | ';' | '|' | '&' | ')')
                    ) =>
                {
                    self.pos += 1;
                    parts.push(Part::Tilde);
                }
                // `\\server\share`: a UNC path keeps its leading backslashes.
                '\\' if self.pos == start
                    && self.peek_at(1) == Some('\\')
                    && self.peek_at(2).is_some_and(|c| c.is_alphanumeric()) =>
                {
                    literal.push_str("\\\\");
                    self.pos += 2;
                }
                '\\' => match self.peek_at(1) {
                    Some('\n') => self.pos += 2,
                    Some(next) if escapable(next) => {
                        flush(&mut literal, &mut parts);
                        parts.push(Part::Quoted(next.to_string()));
                        self.pos += 2;
                    }
                    // A trailing backslash ends a Windows path (`cd C:\`); on its own
                    // it continues the line.
                    None if self.pos > start => {
                        literal.push('\\');
                        self.pos += 1;
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
                    parts.push(Part::Quoted(self.single_quoted()?));
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

    fn single_quoted(&mut self) -> Result<String, ParseError> {
        let mut text = String::new();
        loop {
            match self.peek() {
                None => return Err(incomplete("unclosed single quote")),
                Some('\'') => {
                    self.pos += 1;
                    return Ok(text);
                }
                Some(ch) => {
                    text.push(ch);
                    self.pos += 1;
                }
            }
        }
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

    /// `$'…'`: ANSI-C quoting with backslash escapes.
    fn ansi_quoted(&mut self) -> Result<String, ParseError> {
        let mut text = String::new();
        loop {
            match self.peek() {
                None => return Err(incomplete("unclosed $'")),
                Some('\'') => {
                    self.pos += 1;
                    return Ok(text);
                }
                Some('\\') => {
                    self.pos += 1;
                    let Some(c) = self.peek() else {
                        return Err(incomplete("unclosed $'"));
                    };
                    self.pos += 1;
                    match c {
                        'n' => text.push('\n'),
                        't' => text.push('\t'),
                        'r' => text.push('\r'),
                        'a' => text.push('\x07'),
                        'b' => text.push('\x08'),
                        'e' | 'E' => text.push('\x1b'),
                        'f' => text.push('\x0c'),
                        'v' => text.push('\x0b'),
                        '0'..='7' => {
                            let mut value = c.to_digit(8).unwrap_or(0);
                            for _ in 0..2 {
                                match self.peek().and_then(|d| d.to_digit(8)) {
                                    Some(d) => {
                                        value = value * 8 + d;
                                        self.pos += 1;
                                    }
                                    None => break,
                                }
                            }
                            text.extend(char::from_u32(value));
                        }
                        'x' | 'u' | 'U' => {
                            let max = match c {
                                'x' => 2,
                                'u' => 4,
                                _ => 8,
                            };
                            let mut value = 0u32;
                            let mut digits = 0;
                            while digits < max {
                                match self.peek().and_then(|d| d.to_digit(16)) {
                                    Some(d) => {
                                        value = value * 16 + d;
                                        self.pos += 1;
                                        digits += 1;
                                    }
                                    None => break,
                                }
                            }
                            if digits == 0 {
                                text.push('\\');
                                text.push(c);
                            } else {
                                text.extend(char::from_u32(value));
                            }
                        }
                        other => {
                            if !matches!(other, '\\' | '\'' | '"' | '?') {
                                text.push('\\');
                            }
                            text.push(other);
                        }
                    }
                }
                Some(ch) => {
                    text.push(ch);
                    self.pos += 1;
                }
            }
        }
    }

    /// Parses an expansion starting at `$`. Returns `None` (consuming the `$`) for a lone dollar sign.
    fn dollar(&mut self, quoted: bool) -> Result<Option<Part>, ParseError> {
        self.pos += 1;
        match self.peek() {
            Some('(') if self.peek_at(1) == Some('(') => {
                self.pos += 2;
                let source = self.arithmetic()?;
                Ok(Some(Part::Arith { source, quoted }))
            }
            Some('(') => {
                self.pos += 1;
                let source = self.balanced_parens()?;
                Ok(Some(Part::Subst { source, quoted }))
            }
            Some('{') => {
                self.pos += 1;
                self.braced(quoted).map(Some)
            }
            Some('\'') if !quoted => {
                self.pos += 1;
                Ok(Some(Part::Quoted(self.ansi_quoted()?)))
            }
            Some(c) if is_special_param(c) => {
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

    /// Parses `${…}` after the opening brace.
    fn braced(&mut self, quoted: bool) -> Result<Part, ParseError> {
        let raw = self.until_closing_brace()?;
        let chars: Vec<char> = raw.chars().collect();
        let bad = || error(format!("bad substitution: ${{{raw}}}"));

        // `${#name}` is the length; `${#}` is the number of arguments.
        if chars.len() > 1 && chars[0] == '#' {
            let name: String = chars[1..].iter().collect();
            if is_name(&name)
                || name.chars().all(|c| c.is_ascii_digit())
                || (name.len() == 1 && name.chars().all(is_special_param))
            {
                return Ok(Part::Param {
                    name,
                    op: Box::new(ParamOp::Length),
                    quoted,
                });
            }
        }

        let mut index = 0;
        let name: String = if chars.first().is_some_and(|c| is_name_start(*c)) {
            while chars.get(index).is_some_and(|c| is_name_char(*c)) {
                index += 1;
            }
            chars[..index].iter().collect()
        } else if chars.first().is_some_and(char::is_ascii_digit) {
            while chars.get(index).is_some_and(char::is_ascii_digit) {
                index += 1;
            }
            chars[..index].iter().collect()
        } else if chars.first().is_some_and(|c| is_special_param(*c)) {
            index = 1;
            chars[0].to_string()
        } else {
            return Err(bad());
        };
        if index == chars.len() {
            return Ok(Part::Var { name, quoted });
        }

        let rest: String = chars[index..].iter().collect();
        let operand = |text: &str| lex_text(text, TextMode::Operand { quoted });
        let op = if let Some(word) = rest.strip_prefix(":-") {
            ParamOp::Default {
                colon: true,
                word: operand(word)?,
            }
        } else if let Some(word) = rest.strip_prefix(":=") {
            ParamOp::Assign {
                colon: true,
                word: operand(word)?,
            }
        } else if let Some(word) = rest.strip_prefix(":+") {
            ParamOp::Alternative {
                colon: true,
                word: operand(word)?,
            }
        } else if let Some(word) = rest.strip_prefix(":?") {
            ParamOp::Error {
                colon: true,
                word: operand(word)?,
            }
        } else if let Some(spec) = rest.strip_prefix(':') {
            let (offset, length) = match split_unquoted(spec, ':') {
                Some((offset, length)) => (offset, Some(length.to_owned())),
                None => (spec, None),
            };
            ParamOp::Substring {
                offset: offset.to_owned(),
                length,
            }
        } else if let Some(word) = rest.strip_prefix('-') {
            ParamOp::Default {
                colon: false,
                word: operand(word)?,
            }
        } else if let Some(word) = rest.strip_prefix('=') {
            ParamOp::Assign {
                colon: false,
                word: operand(word)?,
            }
        } else if let Some(word) = rest.strip_prefix('+') {
            ParamOp::Alternative {
                colon: false,
                word: operand(word)?,
            }
        } else if let Some(word) = rest.strip_prefix('?') {
            ParamOp::Error {
                colon: false,
                word: operand(word)?,
            }
        } else if let Some(pattern) = rest.strip_prefix("##") {
            ParamOp::RemovePrefix {
                longest: true,
                pattern: operand(pattern)?,
            }
        } else if let Some(pattern) = rest.strip_prefix('#') {
            ParamOp::RemovePrefix {
                longest: false,
                pattern: operand(pattern)?,
            }
        } else if let Some(pattern) = rest.strip_prefix("%%") {
            ParamOp::RemoveSuffix {
                longest: true,
                pattern: operand(pattern)?,
            }
        } else if let Some(pattern) = rest.strip_prefix('%') {
            ParamOp::RemoveSuffix {
                longest: false,
                pattern: operand(pattern)?,
            }
        } else if let Some(spec) = rest.strip_prefix('/') {
            let (all, anchor, spec) = if let Some(spec) = spec.strip_prefix('/') {
                (true, Anchor::None, spec)
            } else if let Some(spec) = spec.strip_prefix('#') {
                (false, Anchor::Start, spec)
            } else if let Some(spec) = spec.strip_prefix('%') {
                (false, Anchor::End, spec)
            } else {
                (false, Anchor::None, spec)
            };
            let (pattern, replacement) = split_unquoted(spec, '/').unwrap_or((spec, ""));
            ParamOp::Replace {
                all,
                anchor,
                pattern: operand(pattern)?,
                replacement: operand(replacement)?,
            }
        } else if rest == "^^" || rest == "^" || rest == ",," || rest == "," {
            ParamOp::Case {
                upper: rest.starts_with('^'),
                all: rest.len() == 2,
            }
        } else {
            return Err(bad());
        };
        Ok(Part::Param {
            name,
            op: Box::new(op),
            quoted,
        })
    }

    /// Reads up to the `}` that closes `${`, skipping nested braces and quotes.
    fn until_closing_brace(&mut self) -> Result<String, ParseError> {
        let mut depth = 0;
        let mut text = String::new();
        let mut quote: Option<char> = None;
        while let Some(c) = self.peek() {
            self.pos += 1;
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => {
                    if c == '\\' && quote == Some('"') {
                        text.push(c);
                        if let Some(next) = self.peek() {
                            self.pos += 1;
                            text.push(next);
                        }
                        continue;
                    }
                }
                None => match c {
                    '\'' | '"' => quote = Some(c),
                    '\\' => {
                        text.push(c);
                        if let Some(next) = self.peek() {
                            self.pos += 1;
                            text.push(next);
                        }
                        continue;
                    }
                    '{' => depth += 1,
                    '}' if depth == 0 => return Ok(text),
                    '}' => depth -= 1,
                    _ => {}
                },
            }
            text.push(c);
        }
        Err(incomplete("unclosed ${"))
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
                    '\\' => {
                        source.push(c);
                        if let Some(next) = self.peek() {
                            self.pos += 1;
                            source.push(next);
                        }
                        continue;
                    }
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

fn flush(literal: &mut String, parts: &mut Vec<Part>) {
    if !literal.is_empty() {
        parts.push(Part::Lit(std::mem::take(literal)));
    }
}

/// Splits `text` at the first `separator` that is not quoted or escaped.
fn split_unquoted(text: &str, separator: char) -> Option<(&str, &str)> {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut depth = 0;
    for (index, c) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '\\' => escaped = true,
                '\'' | '"' => quote = Some(c),
                '{' => depth += 1,
                '}' => depth -= 1,
                _ if c == separator && depth == 0 => {
                    return Some((&text[..index], &text[index + c.len_utf8()..]));
                }
                _ => {}
            },
        }
    }
    None
}

#[derive(Clone, Copy)]
enum TextMode {
    /// The body of a here-document: `$` expansions, no quote removal.
    HereDoc,
    /// The word inside `${name:-word}`; `quoted` when the whole expansion is in double quotes.
    Operand { quoted: bool },
}

/// Parses text that is not split into words: here-document bodies and `${…}` operands.
fn lex_text(text: &str, mode: TextMode) -> Result<Word, ParseError> {
    let mut lexer = Lexer::new(text);
    lexer.chars = text.chars().collect();
    let mut parts = Vec::new();
    let mut literal = String::new();
    let outer_quoted = !matches!(mode, TextMode::Operand { quoted: false });
    let push_text = |parts: &mut Vec<Part>, literal: &mut String| {
        if literal.is_empty() {
            return;
        }
        let text = std::mem::take(literal);
        parts.push(if outer_quoted {
            Part::Quoted(text)
        } else {
            Part::Lit(text)
        });
    };
    while let Some(c) = lexer.peek() {
        match c {
            '\\' => {
                let next = lexer.peek_at(1);
                let escapes = match mode {
                    TextMode::HereDoc => matches!(next, Some('$' | '`' | '\\' | '\n')),
                    TextMode::Operand { quoted: true } => {
                        matches!(next, Some('$' | '`' | '\\' | '"' | '}' | '\n'))
                    }
                    TextMode::Operand { quoted: false } => next.is_some(),
                };
                if escapes {
                    let next = next.unwrap_or('\\');
                    lexer.pos += 2;
                    if next != '\n' {
                        push_text(&mut parts, &mut literal);
                        parts.push(Part::Quoted(next.to_string()));
                    }
                } else {
                    literal.push('\\');
                    lexer.pos += 1;
                }
            }
            '$' => {
                push_text(&mut parts, &mut literal);
                match lexer.dollar(outer_quoted)? {
                    Some(part) => parts.push(part),
                    None => literal.push('$'),
                }
            }
            '\'' if matches!(mode, TextMode::Operand { quoted: false }) => {
                push_text(&mut parts, &mut literal);
                lexer.pos += 1;
                parts.push(Part::Quoted(lexer.single_quoted()?));
            }
            '"' if matches!(mode, TextMode::Operand { .. }) => {
                push_text(&mut parts, &mut literal);
                lexer.pos += 1;
                lexer.double_quoted(&mut parts)?;
            }
            _ => {
                literal.push(c);
                lexer.pos += 1;
            }
        }
    }
    push_text(&mut parts, &mut literal);
    if parts.is_empty() {
        parts.push(Part::Quoted(String::new()));
    }
    Ok(Word(parts))
}

/// Parses the text inside `$(( … ))` or `(( … ))` so its `$` expansions can run first.
pub fn parse_arith_text(text: &str) -> Result<Word, ParseError> {
    lex_text(text, TextMode::HereDoc)
}

fn assignment(word: &Word) -> Option<(String, Word)> {
    let Some(Part::Lit(first)) = word.0.first() else {
        return None;
    };
    let eq = first.find('=')?;
    let name = &first[..eq];
    if !is_name(name) {
        return None;
    }
    let mut value = tilde_parts(&first[eq + 1..]);
    value.extend(word.0[1..].iter().cloned());
    if value.is_empty() {
        value.push(Part::Quoted(String::new()));
    }
    Some((name.to_owned(), Word(value)))
}

/// Splits the start of an assignment value so `~` is expanded after `=` and after
/// each `:` (`PATH=~/bin:~/tools`), as in bash.
fn tilde_parts(text: &str) -> Vec<Part> {
    fn push_text(parts: &mut Vec<Part>, text: &str) {
        match parts.last_mut() {
            Some(Part::Lit(previous)) => previous.push_str(text),
            _ if !text.is_empty() => parts.push(Part::Lit(text.to_owned())),
            _ => {}
        }
    }
    let mut parts = Vec::new();
    for (index, piece) in text.split(':').enumerate() {
        if index > 0 {
            push_text(&mut parts, ":");
        }
        if piece == "~" || piece.starts_with("~/") {
            parts.push(Part::Tilde);
            push_text(&mut parts, &piece[1..]);
        } else {
            push_text(&mut parts, piece);
        }
    }
    parts
}

/// `export NAME=~/x`, `local NAME=~/x`: declaration arguments get assignment tildes.
fn declaration_word(word: Word) -> Word {
    let Some(Part::Lit(first)) = word.0.first() else {
        return word;
    };
    let Some((name, value)) = first.split_once('=') else {
        return word;
    };
    if !is_name(name) || !value.contains('~') {
        return word;
    }
    let mut parts = vec![Part::Lit(format!("{name}="))];
    parts.extend(tilde_parts(value));
    parts.extend(word.0[1..].iter().cloned());
    Word(parts)
}

struct Parser {
    tokens: Vec<Spanned>,
    pos: usize,
    chars: Vec<char>,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|spanned| &spanned.token)
    }

    fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens
            .get(self.pos + offset)
            .map(|spanned| &spanned.token)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self
            .tokens
            .get(self.pos)
            .map(|spanned| spanned.token.clone());
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    fn peek_keyword(&self) -> Option<&str> {
        match self.peek() {
            Some(Token::Word(word)) => word.keyword(),
            _ => None,
        }
    }

    fn text(&self, start: usize, end: usize) -> String {
        self.chars[start.min(self.chars.len())..end.min(self.chars.len())]
            .iter()
            .collect()
    }

    /// The source offset where the current token starts (or the end of input).
    fn offset(&self) -> usize {
        self.tokens
            .get(self.pos)
            .map_or(self.chars.len(), |spanned| spanned.start)
    }

    /// The source offset where the previous token ended.
    fn last_end(&self) -> usize {
        self.pos
            .checked_sub(1)
            .and_then(|index| self.tokens.get(index))
            .map_or(0, |spanned| spanned.end)
    }

    fn skip_newlines(&mut self) {
        while self.peek() == Some(&Token::Newline) {
            self.pos += 1;
        }
    }

    fn skip_separators(&mut self) {
        while matches!(self.peek(), Some(Token::Newline | Token::Semi)) {
            self.pos += 1;
        }
    }

    fn unexpected(&self) -> ParseError {
        match self.peek() {
            None => incomplete("unexpected end of input"),
            Some(token) => error(format!("syntax error near {}", describe(token))),
        }
    }

    fn expect_keyword(&mut self, keyword: &str) -> Result<(), ParseError> {
        if self.peek_keyword() == Some(keyword) {
            self.pos += 1;
            return Ok(());
        }
        match self.peek() {
            None => Err(incomplete(format!("`{keyword}` expected"))),
            Some(token) => Err(error(format!(
                "`{keyword}` expected, found {}",
                describe(token)
            ))),
        }
    }

    fn at_list_end(&self) -> bool {
        match self.peek() {
            None
            | Some(Token::Close | Token::DoubleSemi | Token::SemiAmp | Token::DoubleSemiAmp) => {
                true
            }
            Some(Token::Word(word)) => word.keyword().is_some_and(|k| CLOSERS.contains(&k)),
            _ => false,
        }
    }

    /// A list of and-or chains separated by `;` or newlines, up to a closing keyword.
    fn list(&mut self) -> Result<List, ParseError> {
        let mut list = List::default();
        self.skip_separators();
        while !self.at_list_end() {
            self.and_or(&mut list)?;
            match self.peek() {
                Some(Token::Semi | Token::Newline) => self.skip_separators(),
                Some(Token::Background) => {
                    return Err(error(
                        "background jobs `&` are not supported; open a new tab or pane instead",
                    ))
                }
                _ if self.at_list_end() => break,
                _ => return Err(self.unexpected()),
            }
        }
        Ok(list)
    }

    /// A list that must hold at least one command (`if`, loops, `{ … }`).
    fn body(&mut self) -> Result<List, ParseError> {
        let list = self.list()?;
        if list.0.is_empty() {
            return Err(self.unexpected());
        }
        Ok(list)
    }

    fn and_or(&mut self, list: &mut List) -> Result<(), ParseError> {
        let mut connector = Connector::Seq;
        loop {
            let pipeline = self.pipeline()?;
            list.0.push((connector, pipeline));
            connector = match self.peek() {
                Some(Token::And) => Connector::And,
                Some(Token::Or) => Connector::Or,
                _ => return Ok(()),
            };
            self.pos += 1;
            self.skip_newlines();
            if self.peek().is_none() {
                let op = if connector == Connector::And {
                    "&&"
                } else {
                    "||"
                };
                return Err(incomplete(format!("command expected after `{op}`")));
            }
        }
    }

    fn pipeline(&mut self) -> Result<Pipeline, ParseError> {
        let mut pipeline = Pipeline::default();
        while self.peek_keyword() == Some("!") {
            self.pos += 1;
            pipeline.negate = !pipeline.negate;
        }
        loop {
            let start = self.offset();
            let command = self.command()?;
            pipeline.sources.push(self.text(start, self.last_end()));
            pipeline.commands.push(command);
            if self.peek() != Some(&Token::Pipe) {
                return Ok(pipeline);
            }
            self.pos += 1;
            self.skip_newlines();
            if self.peek().is_none() {
                return Err(incomplete("command expected after `|`"));
            }
        }
    }

    fn command(&mut self) -> Result<Command, ParseError> {
        let start = self.offset();
        match self.peek() {
            None => return Err(incomplete("command expected")),
            Some(Token::Open) => {
                self.pos += 1;
                let inner_start = self.offset();
                let list = self.body()?;
                let inner_end = self.offset();
                if self.next() != Some(Token::Close) {
                    return Err(self.missing("`)`"));
                }
                let source = self.text(inner_start, inner_end);
                return self.compound(Compound::Subshell { list, source });
            }
            Some(Token::Arith(source)) => {
                let source = source.clone();
                self.pos += 1;
                return self.compound(Compound::Arith(source));
            }
            Some(Token::Word(word)) => {
                if let Some(keyword) = word.keyword() {
                    match keyword {
                        "if" => return self.if_clause(),
                        "for" => return self.for_clause(),
                        "while" | "until" => return self.loop_clause(),
                        "case" => return self.case_clause(),
                        "{" => {
                            self.pos += 1;
                            let list = self.body()?;
                            self.expect_keyword("}")?;
                            return self.compound(Compound::Group(list));
                        }
                        "[[" => return self.test_clause(),
                        "function" => return self.function(start, true),
                        _ if CLOSERS.contains(&keyword) => return Err(self.unexpected()),
                        _ => {}
                    }
                    if self.peek_at(1) == Some(&Token::Open)
                        && self.peek_at(2) == Some(&Token::Close)
                    {
                        return self.function(start, false);
                    }
                }
            }
            _ => {}
        }
        self.simple()
    }

    fn missing(&self, what: &str) -> ParseError {
        if self.pos >= self.tokens.len() {
            incomplete(format!("{what} expected"))
        } else {
            error(format!("{what} expected"))
        }
    }

    /// Wraps a compound command with any redirections that follow it.
    fn compound(&mut self, body: Compound) -> Result<Command, ParseError> {
        let mut redirects = Vec::new();
        while matches!(
            self.peek(),
            Some(Token::Redirect { .. } | Token::HereDoc { .. } | Token::HereString { .. })
        ) {
            let token = self.next().unwrap_or(Token::Semi);
            self.redirect(token, &mut redirects)?;
        }
        Ok(Command::Compound { body, redirects })
    }

    fn simple(&mut self) -> Result<Command, ParseError> {
        let mut command = Simple::default();
        loop {
            match self.peek() {
                Some(Token::Word(word)) => {
                    let word = word.clone();
                    self.pos += 1;
                    if command.words.is_empty() {
                        if let Some(assign) = assignment(&word) {
                            command.assignments.push(assign);
                            continue;
                        }
                    }
                    let declaration =
                        command
                            .words
                            .first()
                            .and_then(Word::keyword)
                            .is_some_and(|name| {
                                matches!(
                                    name,
                                    "export" | "local" | "declare" | "typeset" | "readonly"
                                )
                            });
                    command.words.push(if declaration {
                        declaration_word(word)
                    } else {
                        word
                    });
                }
                Some(Token::Redirect { .. } | Token::HereDoc { .. } | Token::HereString { .. }) => {
                    let token = self.next().unwrap_or(Token::Semi);
                    self.redirect(token, &mut command.redirects)?;
                }
                _ => break,
            }
        }
        if command.words.is_empty()
            && command.assignments.is_empty()
            && command.redirects.is_empty()
        {
            return Err(match self.peek() {
                Some(Token::Open) => error("syntax error near `(`"),
                _ => self.unexpected(),
            });
        }
        Ok(Command::Simple(command))
    }

    fn redirect(&mut self, token: Token, redirects: &mut Vec<Redirect>) -> Result<(), ParseError> {
        match token {
            Token::Redirect {
                fd,
                kind,
                dup,
                both,
            } => {
                let target = match dup {
                    Some(target) => RedirectTarget::Fd(target),
                    None => match self.next() {
                        Some(Token::Word(word)) => RedirectTarget::File(word),
                        None => return Err(incomplete("missing file after redirection")),
                        Some(_) => return Err(error("missing file after redirection")),
                    },
                };
                if both {
                    redirects.push(Redirect {
                        fd: 1,
                        kind,
                        target,
                    });
                    redirects.push(Redirect {
                        fd: 2,
                        kind: RedirectKind::Write,
                        target: RedirectTarget::Fd(1),
                    });
                } else {
                    redirects.push(Redirect { fd, kind, target });
                }
            }
            Token::HereDoc { fd, body } => redirects.push(Redirect {
                fd,
                kind: RedirectKind::Read,
                target: RedirectTarget::Text(body.unwrap_or_default()),
            }),
            Token::HereString { fd } => {
                let Some(Token::Word(mut word)) = self.next() else {
                    return Err(error("missing word after `<<<`"));
                };
                word.0.push(Part::Quoted("\n".into()));
                redirects.push(Redirect {
                    fd,
                    kind: RedirectKind::Read,
                    target: RedirectTarget::Text(word),
                });
            }
            _ => {}
        }
        Ok(())
    }

    fn if_clause(&mut self) -> Result<Command, ParseError> {
        self.pos += 1;
        let mut branches = Vec::new();
        let mut otherwise = None;
        loop {
            let test = self.body()?;
            self.expect_keyword("then")?;
            let body = self.body()?;
            branches.push((test, body));
            match self.peek_keyword() {
                Some("elif") => self.pos += 1,
                Some("else") => {
                    self.pos += 1;
                    otherwise = Some(self.body()?);
                    self.expect_keyword("fi")?;
                    break;
                }
                _ => {
                    self.expect_keyword("fi")?;
                    break;
                }
            }
        }
        self.compound(Compound::If {
            branches,
            otherwise,
        })
    }

    fn do_body(&mut self) -> Result<List, ParseError> {
        self.skip_separators();
        self.expect_keyword("do")?;
        let body = self.body()?;
        self.expect_keyword("done")?;
        Ok(body)
    }

    fn for_clause(&mut self) -> Result<Command, ParseError> {
        self.pos += 1;
        if let Some(Token::Arith(source)) = self.peek() {
            let source = source.clone();
            self.pos += 1;
            let mut parts = split_arith_for(&source).into_iter();
            let (Some(init), Some(test), Some(step), None) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                return Err(error("for ((init; test; step)) needs three parts"));
            };
            let body = self.do_body()?;
            return self.compound(Compound::ArithFor {
                init,
                test,
                step,
                body,
            });
        }
        let var = match self.next() {
            Some(Token::Word(word)) => match word.keyword() {
                Some(name) if is_name(name) => name.to_owned(),
                _ => return Err(error("`for` needs a variable name")),
            },
            None => return Err(incomplete("`for` needs a variable name")),
            Some(_) => return Err(error("`for` needs a variable name")),
        };
        self.skip_newlines();
        let mut items = None;
        if self.peek_keyword() == Some("in") {
            self.pos += 1;
            let mut words = Vec::new();
            while let Some(Token::Word(word)) = self.peek() {
                words.push(word.clone());
                self.pos += 1;
            }
            match self.peek() {
                Some(Token::Semi | Token::Newline) | None => {}
                Some(_) => return Err(self.unexpected()),
            }
            items = Some(words);
        }
        let body = self.do_body()?;
        self.compound(Compound::For { var, items, body })
    }

    fn loop_clause(&mut self) -> Result<Command, ParseError> {
        let until = self.peek_keyword() == Some("until");
        self.pos += 1;
        let test = self.body()?;
        self.expect_keyword("do")?;
        let body = self.body()?;
        self.expect_keyword("done")?;
        self.compound(Compound::Loop { until, test, body })
    }

    fn case_clause(&mut self) -> Result<Command, ParseError> {
        self.pos += 1;
        let word = match self.next() {
            Some(Token::Word(word)) => word,
            None => return Err(incomplete("`case` needs a word")),
            Some(_) => return Err(error("`case` needs a word")),
        };
        self.skip_newlines();
        self.expect_keyword("in")?;
        let mut arms = Vec::new();
        loop {
            self.skip_separators();
            if self.peek_keyword() == Some("esac") {
                self.pos += 1;
                break;
            }
            if self.peek() == Some(&Token::Open) {
                self.pos += 1;
            }
            let mut patterns = Vec::new();
            loop {
                match self.next() {
                    Some(Token::Word(word)) => patterns.push(word),
                    None => return Err(incomplete("case pattern expected")),
                    Some(_) => return Err(error("case pattern expected")),
                }
                match self.next() {
                    Some(Token::Pipe) => continue,
                    Some(Token::Close) => break,
                    None => return Err(incomplete("`)` expected after case pattern")),
                    Some(_) => return Err(error("`)` expected after case pattern")),
                }
            }
            let body = self.list()?;
            let end = match self.peek() {
                Some(Token::DoubleSemi) => CaseEnd::Break,
                Some(Token::SemiAmp) => CaseEnd::FallThrough,
                Some(Token::DoubleSemiAmp) => CaseEnd::Continue,
                _ => {
                    arms.push(CaseArm {
                        patterns,
                        body,
                        end: CaseEnd::Break,
                    });
                    self.expect_keyword("esac")?;
                    break;
                }
            };
            self.pos += 1;
            arms.push(CaseArm {
                patterns,
                body,
                end,
            });
        }
        self.compound(Compound::Case { word, arms })
    }

    fn test_clause(&mut self) -> Result<Command, ParseError> {
        self.pos += 1;
        let mut tokens = Vec::new();
        loop {
            let Some(token) = self.next() else {
                return Err(incomplete("`]]` expected"));
            };
            tokens.push(match token {
                Token::Word(word) => match word.keyword() {
                    Some("]]") => break,
                    Some("!") => TestToken::Not,
                    _ => TestToken::Word(word),
                },
                Token::And => TestToken::And,
                Token::Or => TestToken::Or,
                Token::Open => TestToken::Open,
                Token::Close => TestToken::Close,
                Token::Newline => continue,
                Token::Redirect {
                    kind: RedirectKind::Read,
                    dup: None,
                    ..
                } => TestToken::Less,
                Token::Redirect {
                    kind: RedirectKind::Write,
                    dup: None,
                    both: false,
                    ..
                } => TestToken::Greater,
                other => {
                    return Err(error(format!(
                        "syntax error in [[ ]] near {}",
                        describe(&other)
                    )))
                }
            });
        }
        self.compound(Compound::Test(tokens))
    }

    fn function(&mut self, start: usize, keyword: bool) -> Result<Command, ParseError> {
        if keyword {
            self.pos += 1;
        }
        let name = match self.next() {
            Some(Token::Word(word)) => word.keyword().map(str::to_owned),
            None => return Err(incomplete("function name expected")),
            Some(_) => None,
        };
        let Some(name) = name.filter(|name| {
            !name.is_empty()
                && !KEYWORDS.contains(&name.as_str())
                && name
                    .chars()
                    .all(|c| is_name_char(c) || matches!(c, '-' | '.' | ':' | '+' | '@'))
        }) else {
            return Err(error("invalid function name"));
        };
        if self.peek() == Some(&Token::Open) {
            self.pos += 1;
            if self.next() != Some(Token::Close) {
                return Err(error("`()` expected after the function name"));
            }
        }
        self.skip_newlines();
        let body = match self.peek() {
            None => return Err(incomplete("function body expected")),
            Some(Token::Open) => self.command()?,
            Some(Token::Word(word))
                if matches!(
                    word.keyword(),
                    Some("{" | "if" | "for" | "while" | "until" | "case" | "[[")
                ) =>
            {
                self.command()?
            }
            Some(_) => return Err(error("a function body must be a `{ … }` block")),
        };
        let source = self.text(start, self.last_end());
        Ok(Command::Function(Rc::new(Function { name, body, source })))
    }
}

fn describe(token: &Token) -> String {
    match token {
        Token::Word(word) => match word.as_plain() {
            Some(text) => format!("`{text}`"),
            None => "a word".to_owned(),
        },
        Token::Newline => "end of line".to_owned(),
        Token::Semi => "`;`".to_owned(),
        Token::DoubleSemi => "`;;`".to_owned(),
        Token::SemiAmp => "`;&`".to_owned(),
        Token::DoubleSemiAmp => "`;;&`".to_owned(),
        Token::Pipe => "`|`".to_owned(),
        Token::And => "`&&`".to_owned(),
        Token::Or => "`||`".to_owned(),
        Token::Background => "`&`".to_owned(),
        Token::Open => "`(`".to_owned(),
        Token::Close => "`)`".to_owned(),
        Token::Arith(_) => "`((`".to_owned(),
        Token::Redirect { .. } | Token::HereDoc { .. } | Token::HereString { .. } => {
            "a redirection".to_owned()
        }
    }
}

/// Splits `init; test; step` at top-level semicolons.
fn split_arith_for(source: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut depth = 0;
    for c in source.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ';' if depth == 0 => {
                parts.push(String::new());
                continue;
            }
            _ => {}
        }
        if let Some(last) = parts.last_mut() {
            last.push(c);
        }
    }
    parts
        .into_iter()
        .map(|part| part.trim().to_owned())
        .collect()
}

pub fn parse(source: &str) -> Result<List, ParseError> {
    let (tokens, chars) = Lexer::new(source).tokens()?;
    let mut parser = Parser {
        tokens,
        pos: 0,
        chars,
    };
    let list = parser.list()?;
    if parser.pos < parser.tokens.len() {
        return Err(parser.unexpected());
    }
    Ok(list)
}

/// Quotes a string so the parser reads it back as a single literal word.
pub fn quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || "-_./:=+,@%".contains(c))
        && !KEYWORDS.contains(&text)
    {
        return text.to_owned();
    }
    format!("'{}'", text.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple(command: &Command) -> &Simple {
        match command {
            Command::Simple(simple) => simple,
            other => panic!("not a simple command: {other:?}"),
        }
    }

    fn words(source: &str) -> Vec<Vec<String>> {
        parse(source)
            .unwrap()
            .0
            .into_iter()
            .flat_map(|(_, p)| p.commands)
            .map(|c| {
                simple(&c)
                    .words
                    .iter()
                    .map(|w| w.as_plain().unwrap_or_else(|| format!("{w:?}")))
                    .collect()
            })
            .collect()
    }

    fn first(source: &str) -> Command {
        parse(source).unwrap().0.remove(0).1.commands.remove(0)
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
        assert_eq!(list.0[0].1.sources, vec!["a", "b"]);
    }

    #[test]
    fn parses_redirections() {
        let list = parse("cmd > out.txt 2>&1 < in.txt >> log").unwrap();
        let redirects = &simple(&list.0[0].1.commands[0]).redirects;
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
        let command = first("cmd &> all.log");
        assert_eq!(simple(&command).redirects.len(), 2);
    }

    #[test]
    fn parses_expansions() {
        let command = first(r#"echo $HOME "${USER}x" $(pwd) ~/a $?"#);
        let words = &simple(&command).words;
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
    fn parses_parameter_operators() {
        let command = first(r#"echo ${A:-x y} ${#B} ${C%.txt} ${D//a/b} ${E:1:2} $((1 + 2))"#);
        let words = &simple(&command).words;
        assert!(matches!(
            &words[1].0[0],
            Part::Param { op, .. } if matches!(**op, ParamOp::Default { colon: true, .. })
        ));
        assert!(matches!(
            &words[2].0[0],
            Part::Param { op, .. } if **op == ParamOp::Length
        ));
        assert!(matches!(
            &words[3].0[0],
            Part::Param { op, .. } if matches!(**op, ParamOp::RemoveSuffix { longest: false, .. })
        ));
        assert!(matches!(
            &words[4].0[0],
            Part::Param { op, .. } if matches!(**op, ParamOp::Replace { all: true, .. })
        ));
        assert!(matches!(
            &words[5].0[0],
            Part::Param { op, .. } if matches!(&**op, ParamOp::Substring { length: Some(_), .. })
        ));
        assert_eq!(
            words[6].0,
            vec![Part::Arith {
                source: "1 + 2".into(),
                quoted: false
            }]
        );
    }

    #[test]
    fn parses_assignments() {
        let command = first("FOO=bar BAZ= cmd x=y");
        assert_eq!(simple(&command).assignments.len(), 2);
        assert_eq!(simple(&command).words.len(), 2);
    }

    #[test]
    fn reports_incomplete_input() {
        for source in [
            "echo 'abc",
            "echo \"abc",
            "ls |",
            "a &&",
            "echo $(pwd",
            "if true; then",
            "for x in a b; do echo $x",
            "while true",
            "case $x in",
            "f() {",
            "cat <<EOF\nhello",
            "{ echo",
            "[[ -n x",
        ] {
            assert!(parse(source).unwrap_err().incomplete, "{source}");
        }
        for source in ["| ls", "fi", "if; then fi", "done"] {
            assert!(!parse(source).unwrap_err().incomplete, "{source}");
        }
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        assert_eq!(words("# hi\n\nls # list\n"), vec![vec!["ls"]]);
    }

    #[test]
    fn quote_round_trips() {
        for text in [
            "plain",
            "with space",
            "it's",
            "",
            "$HOME",
            "if",
            r"C:\",
            r"a\\b",
        ] {
            assert_eq!(words(&format!("echo {}", quote(text)))[0][1], text);
        }
    }

    #[test]
    fn parses_if_elif_else() {
        let Command::Compound {
            body: Compound::If {
                branches,
                otherwise,
            },
            ..
        } = first("if a; then b; elif c\nthen d; else e; fi")
        else {
            panic!("not an if");
        };
        assert_eq!(branches.len(), 2);
        assert!(otherwise.is_some());
    }

    #[test]
    fn parses_loops() {
        assert!(matches!(
            first("for x in a 'b c'; do echo $x; done"),
            Command::Compound { body: Compound::For { items: Some(ref items), .. }, .. } if items.len() == 2
        ));
        assert!(matches!(
            first("for x\ndo echo; done"),
            Command::Compound {
                body: Compound::For { items: None, .. },
                ..
            }
        ));
        assert!(matches!(
            first("while read l; do echo; done < file"),
            Command::Compound { body: Compound::Loop { until: false, .. }, ref redirects } if redirects.len() == 1
        ));
        assert!(matches!(
            first("for ((i = 0; i < 3; i++)); do echo; done"),
            Command::Compound { body: Compound::ArithFor { ref test, .. }, .. } if test == "i < 3"
        ));
    }

    #[test]
    fn parses_case() {
        let Command::Compound {
            body: Compound::Case { arms, .. },
            ..
        } = first("case $1 in\n  a|b) echo ab;;\n  (c) echo c;&\n  *) echo other\nesac")
        else {
            panic!("not a case");
        };
        assert_eq!(arms.len(), 3);
        assert_eq!(arms[0].patterns.len(), 2);
        assert_eq!(arms[1].end, CaseEnd::FallThrough);
    }

    #[test]
    fn parses_functions() {
        let Command::Function(function) = first("greet() {\n  echo hi $1\n}") else {
            panic!("not a function");
        };
        assert_eq!(function.name, "greet");
        assert_eq!(function.source, "greet() {\n  echo hi $1\n}");
        assert!(matches!(
            first("function deploy { echo; }"),
            Command::Function(_)
        ));
    }

    #[test]
    fn parses_subshells_groups_and_tests() {
        assert!(matches!(
            first("(cd /tmp && ls)"),
            Command::Compound { body: Compound::Subshell { ref source, .. }, .. } if source == "cd /tmp && ls"
        ));
        assert!(matches!(
            first("{ a; b; } > out"),
            Command::Compound { body: Compound::Group(_), ref redirects } if redirects.len() == 1
        ));
        assert!(matches!(
            first("[[ -n $x && $y < b ]]"),
            Command::Compound { body: Compound::Test(ref tokens), .. } if tokens.len() == 6
        ));
        assert!(matches!(
            first("(( x += 2 ))"),
            Command::Compound { body: Compound::Arith(ref text), .. } if text.trim() == "x += 2"
        ));
    }

    #[test]
    fn keywords_are_plain_words_elsewhere() {
        assert_eq!(
            words("echo if then fi"),
            vec![vec!["echo", "if", "then", "fi"]]
        );
    }

    #[test]
    fn reads_here_documents() {
        let command = first("cat <<EOF > out\nhello $USER\nEOF\n");
        let redirects = &simple(&command).redirects;
        assert_eq!(redirects.len(), 2);
        let RedirectTarget::Text(word) = &redirects[0].target else {
            panic!("not a here-document");
        };
        assert_eq!(word.0[0], Part::Quoted("hello ".into()));
        let command = first("cat <<-'END'\n\t$literal\n\tEND\n");
        let RedirectTarget::Text(word) = &simple(&command).redirects[0].target else {
            panic!("not a here-document");
        };
        assert_eq!(word.0, vec![Part::Quoted("$literal\n".into())]);
    }

    #[test]
    fn handles_crlf_scripts() {
        assert_eq!(
            words("echo a\r\necho b\r\n"),
            vec![vec!["echo", "a"], vec!["echo", "b"]]
        );
    }

    #[test]
    fn keeps_windows_path_endings_and_unc_paths() {
        assert_eq!(words(r"cd C:\"), vec![vec!["cd", r"C:\"]]);
        assert_eq!(
            words(r"ls \\server\share"),
            vec![vec!["ls", r"\\server\share"]]
        );
        assert!(parse("echo a \\").unwrap_err().incomplete);
    }

    #[test]
    fn expands_tildes_in_assignments() {
        let command = first("P=~/bin:~/tools:/usr X=a~b");
        let assignments = &simple(&command).assignments;
        assert_eq!(
            assignments[0].1 .0,
            vec![
                Part::Tilde,
                Part::Lit("/bin:".into()),
                Part::Tilde,
                Part::Lit("/tools:/usr".into())
            ]
        );
        assert_eq!(assignments[1].1 .0, vec![Part::Lit("a~b".into())]);
        let command = first("export H=~");
        assert_eq!(
            simple(&command).words[1].0,
            vec![Part::Lit("H=".into()), Part::Tilde]
        );
    }

    #[test]
    fn decodes_ansi_c_quotes() {
        assert_eq!(words(r"echo $'a\tb\x41'"), vec![vec!["echo", "a\tbA"]]);
    }
}

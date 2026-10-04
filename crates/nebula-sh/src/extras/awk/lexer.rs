//! Splits an awk program into tokens. Newlines are tokens: they end statements.

use super::Builtin;

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Number(f64),
    Str(String),
    Regex(String),
    Name(String),
    /// A name written right against `(`: a call to a user function.
    FuncName(String),
    Builtin(Builtin),
    Begin,
    End,
    Function,
    If,
    Else,
    While,
    For,
    Do,
    Break,
    Continue,
    Next,
    NextFile,
    Exit,
    Return,
    Delete,
    Getline,
    Print,
    Printf,
    In,
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Semi,
    Newline,
    Comma,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Not,
    Gt,
    Lt,
    Pipe,
    Question,
    Colon,
    Tilde,
    NoMatch,
    Dollar,
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
    PowAssign,
    Eq,
    Le,
    Ge,
    Ne,
    Incr,
    Decr,
    And,
    Or,
    Append,
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
}

fn keyword(word: &str) -> Option<Tok> {
    Some(match word {
        "BEGIN" => Tok::Begin,
        "END" => Tok::End,
        "function" | "func" => Tok::Function,
        "if" => Tok::If,
        "else" => Tok::Else,
        "while" => Tok::While,
        "for" => Tok::For,
        "do" => Tok::Do,
        "break" => Tok::Break,
        "continue" => Tok::Continue,
        "next" => Tok::Next,
        "nextfile" => Tok::NextFile,
        "exit" => Tok::Exit,
        "return" => Tok::Return,
        "delete" => Tok::Delete,
        "getline" => Tok::Getline,
        "print" => Tok::Print,
        "printf" => Tok::Printf,
        "in" => Tok::In,
        _ => return None,
    })
}

/// After one of these, `/` divides; anywhere else it starts a regular expression.
fn ends_operand(tok: &Tok) -> bool {
    matches!(
        tok,
        Tok::Number(_)
            | Tok::Str(_)
            | Tok::Regex(_)
            | Tok::Name(_)
            | Tok::Builtin(_)
            | Tok::RParen
            | Tok::RBracket
            | Tok::Dollar
            | Tok::Incr
            | Tok::Decr
    )
}

/// The escapes of awk strings. An unknown escape before a regex metacharacter keeps
/// its backslash, so `"\."` still means a literal dot when the string is a pattern.
pub fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(next) = chars.next() else {
            out.push('\\');
            break;
        };
        match next {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'a' => out.push('\x07'),
            'b' => out.push('\x08'),
            'f' => out.push('\x0c'),
            'v' => out.push('\x0b'),
            '\\' => out.push('\\'),
            '"' => out.push('"'),
            '/' => out.push('/'),
            '0'..='7' => {
                let mut value = next.to_digit(8).unwrap_or(0);
                for _ in 0..2 {
                    match chars.peek().and_then(|d| d.to_digit(8)) {
                        Some(digit) => {
                            value = value * 8 + digit;
                            chars.next();
                        }
                        None => break,
                    }
                }
                out.push(char::from_u32(value).unwrap_or('\u{fffd}'));
            }
            '.' | '[' | ']' | '(' | ')' | '*' | '+' | '?' | '{' | '}' | '|' | '^' | '$' => {
                out.push('\\');
                out.push(next);
            }
            other => out.push(other),
        }
    }
    out
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens: Vec<Token> = Vec::new();
    let mut line = 1;
    let mut i = 0;
    let push = |tokens: &mut Vec<Token>, tok: Tok, line: usize| tokens.push(Token { tok, line });

    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\r' => {
                i += 1;
            }
            '\\' if matches!(chars.get(i + 1), Some('\n')) => {
                i += 2;
                line += 1;
            }
            '\\' if matches!(chars.get(i + 1), Some('\r'))
                && matches!(chars.get(i + 2), Some('\n')) =>
            {
                i += 3;
                line += 1;
            }
            '#' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '\n' => {
                push(&mut tokens, Tok::Newline, line);
                line += 1;
                i += 1;
            }
            '"' => {
                let start_line = line;
                let mut raw = String::new();
                i += 1;
                loop {
                    match chars.get(i) {
                        None | Some('\n') => {
                            return Err(format!("line {start_line}: unterminated string"));
                        }
                        Some('"') => {
                            i += 1;
                            break;
                        }
                        Some('\\') if chars.get(i + 1) == Some(&'\n') => {
                            i += 2;
                            line += 1;
                        }
                        Some('\\') => {
                            raw.push('\\');
                            if let Some(&next) = chars.get(i + 1) {
                                raw.push(next);
                            }
                            i += 2;
                        }
                        Some(&other) => {
                            raw.push(other);
                            i += 1;
                        }
                    }
                }
                push(&mut tokens, Tok::Str(unescape(&raw)), start_line);
            }
            '/' if !tokens.last().is_some_and(|last| ends_operand(&last.tok)) => {
                let mut pattern = String::new();
                let mut in_class = false;
                i += 1;
                loop {
                    match chars.get(i) {
                        None | Some('\n') => {
                            return Err(format!("line {line}: unterminated regular expression"));
                        }
                        Some('/') if !in_class => {
                            i += 1;
                            break;
                        }
                        Some('\\') => {
                            match chars.get(i + 1) {
                                Some('/') => pattern.push('/'),
                                Some(&next) => {
                                    pattern.push('\\');
                                    pattern.push(next);
                                }
                                None => pattern.push('\\'),
                            }
                            i += 2;
                        }
                        Some('[') if !in_class => {
                            in_class = true;
                            pattern.push('[');
                            i += 1;
                            // `]` right after `[` or `[^` is a literal member of the class.
                            if chars.get(i) == Some(&'^') {
                                pattern.push('^');
                                i += 1;
                            }
                            if chars.get(i) == Some(&']') {
                                pattern.push(']');
                                i += 1;
                            }
                        }
                        Some('[') if in_class && chars.get(i + 1) == Some(&':') => {
                            // A class like [:alpha:] inside brackets.
                            let close = (i + 2..chars.len().saturating_sub(1))
                                .find(|&j| chars[j] == ':' && chars[j + 1] == ']');
                            match close {
                                Some(end) => {
                                    pattern.extend(&chars[i..end + 2]);
                                    i = end + 2;
                                }
                                None => {
                                    pattern.push('[');
                                    i += 1;
                                }
                            }
                        }
                        Some(']') if in_class => {
                            in_class = false;
                            pattern.push(']');
                            i += 1;
                        }
                        Some(&other) => {
                            pattern.push(other);
                            i += 1;
                        }
                    }
                }
                push(&mut tokens, Tok::Regex(pattern), line);
            }
            '0'..='9' | '.' if c != '.' || chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()) => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if chars.get(i) == Some(&'.') {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                if matches!(chars.get(i), Some('e' | 'E')) {
                    let mut j = i + 1;
                    if matches!(chars.get(j), Some('+' | '-')) {
                        j += 1;
                    }
                    if chars.get(j).is_some_and(|d| d.is_ascii_digit()) {
                        i = j;
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                }
                let text: String = chars[start..i].iter().collect();
                let value = text
                    .parse::<f64>()
                    .map_err(|_| format!("line {line}: bad number {text}"))?;
                push(&mut tokens, Tok::Number(value), line);
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let tok = if let Some(keyword) = keyword(&word) {
                    keyword
                } else if let Some(builtin) = Builtin::from_name(&word) {
                    Tok::Builtin(builtin)
                } else if chars.get(i) == Some(&'(') {
                    Tok::FuncName(word)
                } else {
                    Tok::Name(word)
                };
                push(&mut tokens, tok, line);
            }
            _ => {
                let next = chars.get(i + 1).copied();
                let (tok, len) = match (c, next) {
                    ('*', Some('*')) if chars.get(i + 2) == Some(&'=') => (Tok::PowAssign, 3),
                    ('*', Some('*')) => (Tok::Caret, 2),
                    ('+', Some('+')) => (Tok::Incr, 2),
                    ('-', Some('-')) => (Tok::Decr, 2),
                    ('+', Some('=')) => (Tok::AddAssign, 2),
                    ('-', Some('=')) => (Tok::SubAssign, 2),
                    ('*', Some('=')) => (Tok::MulAssign, 2),
                    ('/', Some('=')) => (Tok::DivAssign, 2),
                    ('%', Some('=')) => (Tok::ModAssign, 2),
                    ('^', Some('=')) => (Tok::PowAssign, 2),
                    ('=', Some('=')) => (Tok::Eq, 2),
                    ('<', Some('=')) => (Tok::Le, 2),
                    ('>', Some('=')) => (Tok::Ge, 2),
                    ('!', Some('=')) => (Tok::Ne, 2),
                    ('!', Some('~')) => (Tok::NoMatch, 2),
                    ('&', Some('&')) => (Tok::And, 2),
                    ('|', Some('|')) => (Tok::Or, 2),
                    ('>', Some('>')) => (Tok::Append, 2),
                    ('{', _) => (Tok::LBrace, 1),
                    ('}', _) => (Tok::RBrace, 1),
                    ('(', _) => (Tok::LParen, 1),
                    (')', _) => (Tok::RParen, 1),
                    ('[', _) => (Tok::LBracket, 1),
                    (']', _) => (Tok::RBracket, 1),
                    (';', _) => (Tok::Semi, 1),
                    (',', _) => (Tok::Comma, 1),
                    ('+', _) => (Tok::Plus, 1),
                    ('-', _) => (Tok::Minus, 1),
                    ('*', _) => (Tok::Star, 1),
                    ('/', _) => (Tok::Slash, 1),
                    ('%', _) => (Tok::Percent, 1),
                    ('^', _) => (Tok::Caret, 1),
                    ('!', _) => (Tok::Not, 1),
                    ('>', _) => (Tok::Gt, 1),
                    ('<', _) => (Tok::Lt, 1),
                    ('|', _) => (Tok::Pipe, 1),
                    ('?', _) => (Tok::Question, 1),
                    (':', _) => (Tok::Colon, 1),
                    ('~', _) => (Tok::Tilde, 1),
                    ('$', _) => (Tok::Dollar, 1),
                    ('=', _) => (Tok::Assign, 1),
                    _ => return Err(format!("line {line}: unexpected character '{c}'")),
                };
                push(&mut tokens, tok, line);
                i += len;
            }
        }
    }
    push(&mut tokens, Tok::Eof, line);
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(source: &str) -> Vec<Tok> {
        tokenize(source)
            .unwrap()
            .into_iter()
            .map(|token| token.tok)
            .collect()
    }

    #[test]
    fn tells_division_from_regex() {
        assert_eq!(
            toks("a / 2 / 1"),
            vec![
                Tok::Name("a".into()),
                Tok::Slash,
                Tok::Number(2.0),
                Tok::Slash,
                Tok::Number(1.0),
                Tok::Eof
            ]
        );
        assert_eq!(
            toks("$0 ~ /a\\/b[/]/"),
            vec![
                Tok::Dollar,
                Tok::Number(0.0),
                Tok::Tilde,
                Tok::Regex("a/b[/]".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn reads_strings_names_and_calls() {
        assert_eq!(
            toks("f(x) g (y) \"a\\tb\\.\""),
            vec![
                Tok::FuncName("f".into()),
                Tok::LParen,
                Tok::Name("x".into()),
                Tok::RParen,
                Tok::Name("g".into()),
                Tok::LParen,
                Tok::Name("y".into()),
                Tok::RParen,
                Tok::Str("a\tb\\.".into()),
                Tok::Eof
            ]
        );
    }
}

//! Conditional expressions: the `test` / `[` builtin and `[[ … ]]`.

use crate::sys;
use std::fs;
use std::path::Path;

/// An operand of a conditional expression.
pub enum Arg {
    /// An operator or a plain word, as typed.
    Text(String),
    /// A `[[ ]]` operand: its value, and the glob pattern it stands for on the
    /// right of `==` / `!=` (quoted parts match literally).
    Value {
        text: String,
        pattern: String,
    },
    /// `[[ ]]` grouping and logic, which `test` spells as words.
    And,
    Or,
    Not,
    Open,
    Close,
}

impl Arg {
    fn text(&self) -> Option<&str> {
        match self {
            Arg::Text(text) | Arg::Value { text, .. } => Some(text),
            _ => None,
        }
    }

    fn pattern(&self) -> Option<&str> {
        match self {
            Arg::Value { pattern, .. } => Some(pattern),
            Arg::Text(text) => Some(text),
            _ => None,
        }
    }
}

fn is_unary(op: &str) -> bool {
    matches!(
        op,
        "-e" | "-f"
            | "-d"
            | "-r"
            | "-w"
            | "-x"
            | "-s"
            | "-L"
            | "-h"
            | "-b"
            | "-c"
            | "-p"
            | "-S"
            | "-t"
            | "-g"
            | "-u"
            | "-k"
            | "-O"
            | "-G"
            | "-N"
            | "-z"
            | "-n"
            | "-v"
            | "-a"
    )
}

fn is_binary(op: &str, extended: bool) -> bool {
    matches!(
        op,
        "=" | "=="
            | "!="
            | "<"
            | ">"
            | "-eq"
            | "-ne"
            | "-lt"
            | "-le"
            | "-gt"
            | "-ge"
            | "-nt"
            | "-ot"
            | "-ef"
    ) || (extended && op == "=~")
}

fn integer(text: &str) -> Result<i64, String> {
    text.trim()
        .parse()
        .map_err(|_| format!("{text}: integer expression expected"))
}

fn path(text: &str) -> String {
    sys::translate_path(text)
}

#[cfg(windows)]
fn is_executable(path: &Path, meta: &fs::Metadata) -> bool {
    if meta.is_dir() {
        return true;
    }
    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase());
    matches!(
        extension.as_deref(),
        Some("exe" | "com" | "bat" | "cmd" | "ps1" | "sh")
    )
}

#[cfg(not(windows))]
fn is_executable(_: &Path, meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

fn unary(op: &str, operand: &str, is_set: &dyn Fn(&str) -> bool) -> bool {
    let file = path(operand);
    let file = Path::new(&file);
    let meta = || fs::metadata(file);
    match op {
        "-z" => operand.is_empty(),
        "-n" => !operand.is_empty(),
        "-v" => is_set(operand),
        "-e" | "-a" => meta().is_ok(),
        "-f" => meta().is_ok_and(|m| m.is_file()),
        "-d" => meta().is_ok_and(|m| m.is_dir()),
        "-s" => meta().is_ok_and(|m| m.len() > 0),
        "-r" | "-O" | "-G" => meta().is_ok(),
        "-w" => meta().is_ok_and(|m| !m.permissions().readonly()),
        "-x" => meta().is_ok_and(|m| is_executable(file, &m)),
        "-L" | "-h" => fs::symlink_metadata(file).is_ok_and(|m| m.file_type().is_symlink()),
        "-t" => operand.parse::<i32>().is_ok_and(|fd| {
            use std::io::IsTerminal;
            match fd {
                0 => std::io::stdin().is_terminal(),
                1 => std::io::stdout().is_terminal(),
                2 => std::io::stderr().is_terminal(),
                _ => false,
            }
        }),
        "-N" => meta()
            .and_then(|m| Ok(m.modified()? > m.accessed()?))
            .unwrap_or(false),
        // Block and character devices, pipes, sockets, setuid bits: not on Windows files.
        _ => false,
    }
}

fn modified(text: &str) -> Option<std::time::SystemTime> {
    fs::metadata(path(text)).ok()?.modified().ok()
}

fn binary(op: &str, left: &Arg, right: &Arg) -> Result<bool, String> {
    let l = left.text().unwrap_or_default();
    let r = right.text().unwrap_or_default();
    Ok(match op {
        "=" | "==" => match right {
            Arg::Value { pattern, .. } => crate::expand::matches(pattern, l),
            _ => l == r,
        },
        "!=" => match right {
            Arg::Value { pattern, .. } => !crate::expand::matches(pattern, l),
            _ => l != r,
        },
        "<" => l < r,
        ">" => l > r,
        "=~" => {
            let pattern = right.pattern().unwrap_or_default();
            match regex::Regex::new(pattern) {
                Ok(regex) => regex.is_match(l),
                Err(_) => return Err(format!("{pattern}: invalid regular expression")),
            }
        }
        "-eq" => integer(l)? == integer(r)?,
        "-ne" => integer(l)? != integer(r)?,
        "-lt" => integer(l)? < integer(r)?,
        "-le" => integer(l)? <= integer(r)?,
        "-gt" => integer(l)? > integer(r)?,
        "-ge" => integer(l)? >= integer(r)?,
        "-nt" => match (modified(l), modified(r)) {
            (Some(a), Some(b)) => a > b,
            (Some(_), None) => true,
            _ => false,
        },
        "-ot" => match (modified(l), modified(r)) {
            (Some(a), Some(b)) => a < b,
            (None, Some(_)) => true,
            _ => false,
        },
        "-ef" => match (fs::canonicalize(path(l)), fs::canonicalize(path(r))) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        },
        _ => return Err(format!("{op}: unknown operator")),
    })
}

struct Eval<'a> {
    args: &'a [Arg],
    pos: usize,
    /// `[[ ]]`: `&&`, `||`, `(` come as tokens and `=~` is allowed.
    extended: bool,
    is_set: &'a dyn Fn(&str) -> bool,
}

impl Eval<'_> {
    fn peek(&self) -> Option<&Arg> {
        self.args.get(self.pos)
    }

    fn word_at(&self, offset: usize) -> Option<&str> {
        match self.args.get(self.pos + offset)? {
            Arg::Text(text) => Some(text),
            // In `[[ ]]`, only unquoted words act as operators; they arrive as `Value`
            // whose pattern equals the text.
            Arg::Value { text, pattern } if text == pattern => Some(text),
            _ => None,
        }
    }

    fn is_or(&self) -> bool {
        match self.peek() {
            Some(Arg::Or) => true,
            Some(Arg::Text(text)) => !self.extended && text == "-o",
            _ => false,
        }
    }

    fn is_and(&self) -> bool {
        match self.peek() {
            Some(Arg::And) => true,
            Some(Arg::Text(text)) => !self.extended && text == "-a",
            _ => false,
        }
    }

    fn or(&mut self) -> Result<bool, String> {
        let mut value = self.and()?;
        while self.is_or() {
            self.pos += 1;
            let right = self.and()?;
            value = value || right;
        }
        Ok(value)
    }

    fn and(&mut self) -> Result<bool, String> {
        let mut value = self.not()?;
        while self.is_and() {
            self.pos += 1;
            let right = self.not()?;
            value = value && right;
        }
        Ok(value)
    }

    fn not(&mut self) -> Result<bool, String> {
        let negate = match self.peek() {
            Some(Arg::Not) => true,
            Some(Arg::Text(text)) => text == "!" && self.args.len() - self.pos > 1,
            _ => false,
        };
        if negate {
            self.pos += 1;
            return Ok(!self.not()?);
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<bool, String> {
        let open = match self.peek() {
            Some(Arg::Open) => true,
            Some(Arg::Text(text)) => !self.extended && text == "(",
            _ => false,
        };
        if open {
            self.pos += 1;
            let value = self.or()?;
            let closed = match self.peek() {
                Some(Arg::Close) => true,
                Some(Arg::Text(text)) => text == ")",
                _ => false,
            };
            if !closed {
                return Err("`)` expected".to_owned());
            }
            self.pos += 1;
            return Ok(value);
        }
        let remaining = self.args.len() - self.pos;
        if remaining >= 3 {
            if let Some(op) = self.word_at(1).filter(|op| is_binary(op, self.extended)) {
                let op = op.to_owned();
                let value = binary(&op, &self.args[self.pos], &self.args[self.pos + 2])?;
                self.pos += 3;
                return Ok(value);
            }
        }
        if remaining >= 2 {
            if let Some(op) = self.word_at(0).filter(|op| is_unary(op)) {
                let op = op.to_owned();
                let operand = self.args[self.pos + 1]
                    .text()
                    .unwrap_or_default()
                    .to_owned();
                self.pos += 2;
                return Ok(unary(&op, &operand, self.is_set));
            }
        }
        match self.peek() {
            Some(arg @ (Arg::Text(_) | Arg::Value { .. })) => {
                let value = !arg.text().unwrap_or_default().is_empty();
                self.pos += 1;
                Ok(value)
            }
            Some(_) => Err("syntax error".to_owned()),
            None => Err("argument expected".to_owned()),
        }
    }
}

/// Evaluates a conditional expression. `Err` is a syntax error (exit status 2).
pub fn evaluate(
    args: &[Arg],
    extended: bool,
    is_set: &dyn Fn(&str) -> bool,
) -> Result<bool, String> {
    if args.is_empty() {
        return Ok(false);
    }
    let mut eval = Eval {
        args,
        pos: 0,
        extended,
        is_set,
    };
    let value = eval.or()?;
    if eval.pos < args.len() {
        let near = args[eval.pos].text().unwrap_or("operator").to_owned();
        return Err(format!("syntax error near `{near}`"));
    }
    Ok(value)
}

/// The `test` and `[` builtins: 0 when true, 1 when false, 2 on error.
pub fn builtin(name: &str, args: &[String], is_set: &dyn Fn(&str) -> bool) -> Result<bool, String> {
    let args = if name == "[" {
        match args.split_last() {
            Some((last, rest)) if last == "]" => rest,
            _ => return Err("missing `]`".to_owned()),
        }
    } else {
        args
    };
    let args: Vec<Arg> = args.iter().map(|arg| Arg::Text(arg.clone())).collect();
    evaluate(&args, false, is_set)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test(args: &[&str]) -> Result<bool, String> {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
        builtin("test", &args, &|name| name == "SET")
    }

    #[test]
    fn compares_strings_and_numbers() {
        assert_eq!(test(&["abc", "=", "abc"]), Ok(true));
        assert_eq!(test(&["abc", "!=", "abc"]), Ok(false));
        assert_eq!(test(&["10", "-gt", "9"]), Ok(true));
        assert_eq!(test(&["a", "<", "b"]), Ok(true));
        assert!(test(&["x", "-lt", "1"]).is_err());
    }

    #[test]
    fn handles_unary_and_single_arguments() {
        assert_eq!(test(&["-z", ""]), Ok(true));
        assert_eq!(test(&["-n", ""]), Ok(false));
        assert_eq!(test(&["-n"]), Ok(true));
        assert_eq!(test(&[""]), Ok(false));
        assert_eq!(test(&[]), Ok(false));
        assert_eq!(test(&["-v", "SET"]), Ok(true));
        assert_eq!(test(&["-d", "."]), Ok(true));
        assert_eq!(test(&["-f", "."]), Ok(false));
        assert_eq!(test(&["-e", "/definitely/missing"]), Ok(false));
    }

    #[test]
    fn combines_with_logic() {
        assert_eq!(test(&["!", "a", "=", "b"]), Ok(true));
        assert_eq!(test(&["a", "=", "b", "-o", "1", "-eq", "1"]), Ok(true));
        assert_eq!(test(&["(", "a", "=", "a", ")", "-a", "-z", "x"]), Ok(false));
        assert_eq!(test(&["!"]), Ok(true));
    }

    #[test]
    fn needs_closing_bracket() {
        let args: Vec<String> = vec!["1".into(), "-eq".into(), "1".into()];
        assert!(builtin("[", &args, &|_| false).is_err());
    }

    #[test]
    fn matches_patterns_in_double_brackets() {
        let value = |text: &str, pattern: &str| Arg::Value {
            text: text.to_owned(),
            pattern: pattern.to_owned(),
        };
        let args = [
            value("main.rs", "main.rs"),
            value("==", "=="),
            value("*.rs", "*.rs"),
        ];
        assert_eq!(evaluate(&args, true, &|_| false), Ok(true));
        let args = [
            value("v1.2", "v1.2"),
            value("=~", "=~"),
            value("^v[0-9]+", "^v[0-9]+"),
        ];
        assert_eq!(evaluate(&args, true, &|_| false), Ok(true));
        let args = [value("", ""), Arg::Or, value("x", "x")];
        assert_eq!(evaluate(&args, true, &|_| false), Ok(true));
    }
}

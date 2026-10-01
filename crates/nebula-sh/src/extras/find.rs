//! `find` with the GNU expression syntax for the common tests and actions.

use glob::{MatchOptions, Pattern};
use std::fs::Metadata;
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, SystemTime};
use walkdir::WalkDir;

#[derive(Debug)]
enum Cmp {
    Less(u64),
    Exactly(u64),
    More(u64),
}

impl Cmp {
    fn parse(text: &str) -> Option<Self> {
        let (kind, digits) = match text.as_bytes().first()? {
            b'+' => ('+', &text[1..]),
            b'-' => ('-', &text[1..]),
            _ => ('=', text),
        };
        let value = digits.parse().ok()?;
        Some(match kind {
            '+' => Cmp::More(value),
            '-' => Cmp::Less(value),
            _ => Cmp::Exactly(value),
        })
    }

    fn test(&self, value: u64) -> bool {
        match self {
            Cmp::Less(n) => value < *n,
            Cmp::Exactly(n) => value == *n,
            Cmp::More(n) => value > *n,
        }
    }
}

#[derive(Debug)]
enum Expr {
    True,
    Name(Pattern, bool),
    Path(Pattern, bool),
    Regex(regex::Regex),
    Type(char),
    Size(Cmp, u64),
    Empty,
    MTime(Cmp, u64),
    Newer(SystemTime),
    Print(bool),
    Delete,
    /// Command, batched with `+`, ask first (`-ok`).
    Exec(Vec<String>, bool, bool),
    Prune,
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

struct Parser {
    tokens: Vec<String>,
    pos: usize,
    has_action: bool,
    /// `-delete` needs children visited before their directory.
    deletes: bool,
    max_depth: Option<usize>,
    min_depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.pos).map(String::as_str)
    }

    fn next(&mut self) -> Option<String> {
        let token = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        token
    }

    fn arg(&mut self, option: &str) -> Result<String, String> {
        self.next()
            .ok_or_else(|| format!("missing argument to `{option}'"))
    }

    fn or(&mut self) -> Result<Expr, String> {
        let mut left = self.and()?;
        while matches!(self.peek(), Some("-o" | "-or")) {
            self.pos += 1;
            left = Expr::Or(Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Expr, String> {
        let mut left = self.unary()?;
        loop {
            match self.peek() {
                Some("-a" | "-and") => {
                    self.pos += 1;
                    left = Expr::And(Box::new(left), Box::new(self.unary()?));
                }
                Some(token) if token != "-o" && token != "-or" && token != ")" => {
                    left = Expr::And(Box::new(left), Box::new(self.unary()?));
                }
                _ => return Ok(left),
            }
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some("!" | "-not") => {
                self.pos += 1;
                Ok(Expr::Not(Box::new(self.unary()?)))
            }
            Some("(") => {
                self.pos += 1;
                let inner = self.or()?;
                if self.next().as_deref() != Some(")") {
                    return Err("missing closing `)'".into());
                }
                Ok(inner)
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<Expr, String> {
        let token = self.next().ok_or("expected an expression")?;
        let glob = |text: &str| {
            Pattern::new(text).map_err(|error| format!("invalid pattern `{text}': {error}"))
        };
        Ok(match token.as_str() {
            "-name" => Expr::Name(glob(&self.arg(&token)?)?, false),
            "-iname" => Expr::Name(glob(&self.arg(&token)?)?, true),
            "-path" | "-wholename" => Expr::Path(glob(&self.arg(&token)?)?, false),
            "-ipath" | "-iwholename" => Expr::Path(glob(&self.arg(&token)?)?, true),
            "-regex" => {
                let pattern = self.arg(&token)?;
                Expr::Regex(
                    regex::Regex::new(&format!("^(?:{pattern})$")).map_err(|e| e.to_string())?,
                )
            }
            "-type" => {
                let kind = self.arg(&token)?;
                match kind.as_str() {
                    "f" | "d" | "l" => Expr::Type(kind.chars().next().unwrap_or('f')),
                    other => return Err(format!("unknown argument to -type: {other}")),
                }
            }
            "-size" => {
                let spec = self.arg(&token)?;
                let (number, unit) = match spec.chars().last() {
                    Some(c @ ('c' | 'k' | 'M' | 'G' | 'b')) => (&spec[..spec.len() - 1], c),
                    _ => (spec.as_str(), 'b'),
                };
                let unit = match unit {
                    'c' => 1,
                    'k' => 1024,
                    'M' => 1024 * 1024,
                    'G' => 1024 * 1024 * 1024,
                    _ => 512,
                };
                Expr::Size(
                    Cmp::parse(number).ok_or_else(|| format!("invalid -size `{spec}'"))?,
                    unit,
                )
            }
            "-empty" => Expr::Empty,
            "-mtime" | "-mmin" => {
                let spec = self.arg(&token)?;
                let unit = if token == "-mtime" { 86_400 } else { 60 };
                Expr::MTime(
                    Cmp::parse(&spec).ok_or_else(|| format!("invalid {token} `{spec}'"))?,
                    unit,
                )
            }
            "-newer" => {
                let file = self.arg(&token)?;
                let time = std::fs::metadata(&file)
                    .and_then(|meta| meta.modified())
                    .map_err(|_| format!("cannot stat `{file}'"))?;
                Expr::Newer(time)
            }
            "-print" => {
                self.has_action = true;
                Expr::Print(false)
            }
            "-print0" => {
                self.has_action = true;
                Expr::Print(true)
            }
            "-delete" => {
                self.has_action = true;
                self.deletes = true;
                Expr::Delete
            }
            "-exec" | "-ok" => {
                self.has_action = true;
                let ask = token == "-ok";
                let mut command = Vec::new();
                loop {
                    let part = self
                        .next()
                        .ok_or("missing argument to `-exec' (end it with \\; or +)")?;
                    match part.as_str() {
                        ";" => return Ok(Expr::Exec(command, false, ask)),
                        "+" if !ask && command.last().map(String::as_str) == Some("{}") => {
                            command.pop();
                            return Ok(Expr::Exec(command, true, false));
                        }
                        _ => command.push(part),
                    }
                }
            }
            "-prune" => Expr::Prune,
            "-true" => Expr::True,
            "-false" => Expr::Not(Box::new(Expr::True)),
            "-maxdepth" | "-mindepth" => {
                let value = self
                    .arg(&token)?
                    .parse::<usize>()
                    .map_err(|_| format!("invalid argument to {token}"))?;
                if token == "-maxdepth" {
                    self.max_depth = Some(value);
                } else {
                    self.min_depth = value;
                }
                Expr::True
            }
            other => return Err(format!("unknown predicate `{other}'")),
        })
    }
}

struct Run {
    out: io::StdoutLock<'static>,
    batch: Vec<(Vec<String>, Vec<String>)>,
    status: i32,
    prune: bool,
}

fn options(ignore_case: bool) -> MatchOptions {
    MatchOptions {
        case_sensitive: !ignore_case,
        require_literal_separator: false,
        require_literal_leading_dot: false,
    }
}

fn evaluate(
    expr: &Expr,
    path: &str,
    name: &str,
    meta: &Metadata,
    is_symlink: bool,
    run: &mut Run,
) -> bool {
    match expr {
        Expr::True => true,
        Expr::Name(pattern, ignore_case) => pattern.matches_with(name, options(*ignore_case)),
        Expr::Path(pattern, ignore_case) => pattern.matches_with(path, options(*ignore_case)),
        Expr::Regex(regex) => regex.is_match(path),
        Expr::Type('d') => meta.is_dir() && !is_symlink,
        Expr::Type('l') => is_symlink,
        Expr::Type(_) => meta.is_file() && !is_symlink,
        Expr::Size(cmp, unit) => cmp.test(meta.len().div_ceil(*unit)),
        Expr::Empty => {
            if meta.is_dir() {
                std::fs::read_dir(path)
                    .map(|mut entries| entries.next().is_none())
                    .unwrap_or(false)
            } else {
                meta.len() == 0
            }
        }
        Expr::MTime(cmp, unit) => {
            let age = meta
                .modified()
                .ok()
                .and_then(|time| SystemTime::now().duration_since(time).ok())
                .unwrap_or(Duration::ZERO);
            cmp.test(age.as_secs() / unit)
        }
        Expr::Newer(time) => meta.modified().is_ok_and(|modified| modified > *time),
        Expr::Print(nul) => {
            let _ = write!(run.out, "{path}{}", if *nul { '\0' } else { '\n' });
            true
        }
        Expr::Delete => {
            let result = if meta.is_dir() && !is_symlink {
                std::fs::remove_dir(path)
            } else {
                std::fs::remove_file(path)
            };
            if let Err(error) = result {
                eprintln!(
                    "find: cannot delete `{path}': {}",
                    crate::exec::describe_io_error(&error)
                );
                run.status = 1;
                return false;
            }
            true
        }
        Expr::Exec(command, batch, ask) => {
            if *batch {
                match run.batch.iter_mut().find(|(cmd, _)| cmd == command) {
                    Some((_, paths)) => paths.push(path.to_owned()),
                    None => run.batch.push((command.clone(), vec![path.to_owned()])),
                }
                return true;
            }
            let argv: Vec<String> = command
                .iter()
                .map(|part| part.replace("{}", path))
                .collect();
            let _ = run.out.flush();
            if *ask && !super::misc::confirm(&format!("< {} > ?", argv.join(" "))) {
                return false;
            }
            super::misc::run_command(&argv) == 0
        }
        Expr::Prune => {
            run.prune = true;
            true
        }
        Expr::Not(inner) => !evaluate(inner, path, name, meta, is_symlink, run),
        Expr::And(left, right) => {
            evaluate(left, path, name, meta, is_symlink, run)
                && evaluate(right, path, name, meta, is_symlink, run)
        }
        Expr::Or(left, right) => {
            evaluate(left, path, name, meta, is_symlink, run)
                || evaluate(right, path, name, meta, is_symlink, run)
        }
    }
}

pub fn run(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("--help") {
        println!("Usage: find [path…] [expression]\n\nTests: -name -iname -path -ipath -regex -type f|d|l -size [+-]N[ckMG] -empty -mtime [+-]N -mmin [+-]N -newer FILE\nActions: -print -print0 -delete -exec CMD {{}} \\; -exec CMD {{}} + -prune\nOperators: ( ) ! -not -a -and -o -or\nOptions: -maxdepth N -mindepth N");
        return 0;
    }
    let split = args
        .iter()
        .position(|arg| arg.starts_with('-') || arg == "(" || arg == "!")
        .unwrap_or(args.len());
    let mut roots: Vec<String> = args[..split]
        .iter()
        .map(|root| crate::sys::translate_path(root))
        .collect();
    if roots.is_empty() {
        roots.push(".".into());
    }
    let mut parser = Parser {
        tokens: args[split..].to_vec(),
        pos: 0,
        has_action: false,
        deletes: false,
        max_depth: None,
        min_depth: 0,
    };
    let expr = if parser.tokens.is_empty() {
        Expr::True
    } else {
        match parser.or() {
            Ok(expr) if parser.pos >= parser.tokens.len() => expr,
            Ok(_) => {
                eprintln!("find: unexpected `{}'", parser.tokens[parser.pos]);
                return 1;
            }
            Err(message) => {
                eprintln!("find: {message}");
                return 1;
            }
        }
    };
    let expr = if parser.has_action {
        expr
    } else {
        Expr::And(Box::new(expr), Box::new(Expr::Print(false)))
    };
    let deletes = parser.deletes;

    let mut run = Run {
        out: io::stdout().lock(),
        batch: Vec::new(),
        status: 0,
        prune: false,
    };
    for root in &roots {
        if !Path::new(root).exists() {
            eprintln!("find: `{root}': No such file or directory");
            run.status = 1;
            continue;
        }
        let mut walker = WalkDir::new(root)
            .follow_links(false)
            .contents_first(deletes)
            .sort_by_file_name();
        if let Some(max) = parser.max_depth {
            walker = walker.max_depth(max);
        }
        let mut iter = walker.min_depth(parser.min_depth).into_iter();
        while let Some(entry) = iter.next() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    eprintln!("find: {error}");
                    run.status = 1;
                    continue;
                }
            };
            let path = if entry.depth() == 0 {
                root.clone()
            } else {
                entry.path().to_string_lossy().replace('\\', "/")
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(meta) = entry.path().symlink_metadata() else {
                continue;
            };
            run.prune = false;
            evaluate(
                &expr,
                &path,
                &name,
                &meta,
                entry.path_is_symlink(),
                &mut run,
            );
            if run.prune && entry.file_type().is_dir() {
                iter.skip_current_dir();
            }
        }
    }
    let _ = run.out.flush();
    for (command, paths) in std::mem::take(&mut run.batch) {
        for group in super::misc::batches(&command, paths, None) {
            let argv: Vec<String> = command.iter().cloned().chain(group).collect();
            if super::misc::run_command(&argv) != 0 {
                run.status = 1;
            }
        }
    }
    run.status
}

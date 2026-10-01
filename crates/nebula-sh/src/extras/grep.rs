//! `grep` with the options people use every day, GNU colors and context lines.
//! Input is processed line by line, so `tail -f log | grep error` streams.

use super::{expand_short_flags, stdout_is_terminal};
use regex::bytes::{Regex, RegexBuilder};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;

#[derive(Default)]
struct Options {
    patterns: Vec<String>,
    ignore_case: bool,
    invert: bool,
    line_numbers: bool,
    count: bool,
    files_with_matches: bool,
    files_without_match: bool,
    quiet: bool,
    no_messages: bool,
    only_matching: bool,
    word: bool,
    line: bool,
    recursive: bool,
    extended: bool,
    fixed: bool,
    with_filename: Option<bool>,
    max_count: Option<usize>,
    after: usize,
    before: usize,
    color: bool,
    skip_binary: bool,
    text: bool,
    include: Vec<glob::Pattern>,
    exclude: Vec<glob::Pattern>,
    exclude_dir: Vec<glob::Pattern>,
}

const USAGE: &str =
    "Usage: grep [OPTION]... PATTERNS [FILE]...\nTry 'grep --help' for more information.";

const HELP: &str = "Usage: grep [OPTION]... PATTERNS [FILE]...
Search for PATTERNS in each FILE (or standard input).

  -E, --extended-regexp     PATTERNS are extended regular expressions
  -F, --fixed-strings       PATTERNS are strings
  -G, --basic-regexp        PATTERNS are basic regular expressions (default)
  -e, --regexp=PATTERNS     use PATTERNS for matching
  -f, --file=FILE           take PATTERNS from FILE
  -i, --ignore-case         ignore case distinctions
  -w, --word-regexp         match only whole words
  -x, --line-regexp         match only whole lines
  -v, --invert-match        select non-matching lines
  -m, --max-count=NUM       stop after NUM selected lines
  -n, --line-number         print line numbers
  -H, --with-filename       print file names
  -h, --no-filename         never print file names
  -o, --only-matching       show only the matching part of lines
  -q, --quiet, --silent     print nothing, exit 0 on the first match
  -s, --no-messages         hide errors about unreadable files
  -r, -R, --recursive       search directories recursively
      --include=GLOB        only search files that match GLOB
      --exclude=GLOB        skip files that match GLOB
      --exclude-dir=GLOB    skip directories that match GLOB
  -I                        ignore binary files
  -a, --text                treat binary files as text
  -l, --files-with-matches  print only names of files with matches
  -L, --files-without-match print only names of files without matches
  -c, --count               print only a count of matching lines per file
  -A, --after-context=NUM   print NUM lines after each match
  -B, --before-context=NUM  print NUM lines before each match
  -C, --context=NUM         print NUM lines around each match
      --color[=WHEN]        highlight matches: always, never or auto

Exit status is 0 if a line is selected, 1 if none, 2 on error.";

fn number(name: &str, value: Option<String>) -> Result<usize, String> {
    value
        .as_deref()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| format!("invalid {name} argument"))
}

fn parse(args: &[String]) -> Result<(Options, Vec<String>), String> {
    let mut options = Options {
        color: stdout_is_terminal() && crate::style::enabled(),
        ..Options::default()
    };
    let mut operands = Vec::new();
    let mut explicit_patterns = false;
    let mut iter = expand_short_flags(args, &['e', 'f', 'm', 'A', 'B', 'C']).into_iter();
    let mut operands_only = false;
    while let Some(arg) = iter.next() {
        if operands_only || !arg.starts_with('-') || arg == "-" {
            operands.push(arg);
            continue;
        }
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) if arg.starts_with("--") => {
                (flag.to_owned(), Some(value.to_owned()))
            }
            _ => (arg.clone(), None),
        };
        let mut value = || inline.clone().or_else(|| iter.next());
        match flag.as_str() {
            "--" => operands_only = true,
            "-e" | "--regexp" => {
                options
                    .patterns
                    .push(value().ok_or("option requires an argument -- 'e'")?);
                explicit_patterns = true;
            }
            "-f" | "--file" => {
                let file = value().ok_or("option requires an argument -- 'f'")?;
                let text = std::fs::read_to_string(&file)
                    .map_err(|e| format!("{file}: {}", crate::exec::describe_io_error(&e)))?;
                options.patterns.extend(text.lines().map(str::to_owned));
                explicit_patterns = true;
            }
            "-i" | "-y" | "--ignore-case" => options.ignore_case = true,
            "--no-ignore-case" => options.ignore_case = false,
            "-v" | "--invert-match" => options.invert = true,
            "-n" | "--line-number" => options.line_numbers = true,
            "-c" | "--count" => options.count = true,
            "-l" | "--files-with-matches" => options.files_with_matches = true,
            "-L" | "--files-without-match" => options.files_without_match = true,
            "-q" | "--quiet" | "--silent" => options.quiet = true,
            "-s" | "--no-messages" => options.no_messages = true,
            "-o" | "--only-matching" => options.only_matching = true,
            "-w" | "--word-regexp" => options.word = true,
            "-x" | "--line-regexp" => options.line = true,
            "-r" | "-R" | "--recursive" | "--dereference-recursive" => options.recursive = true,
            "-E" | "--extended-regexp" | "-P" | "--perl-regexp" => options.extended = true,
            "-F" | "--fixed-strings" => options.fixed = true,
            "-G" | "--basic-regexp" => options.extended = false,
            "-H" | "--with-filename" => options.with_filename = Some(true),
            "-h" | "--no-filename" => options.with_filename = Some(false),
            "-I" => options.skip_binary = true,
            "-a" | "--text" => options.text = true,
            "-m" | "--max-count" => options.max_count = Some(number("max count", value())?),
            "-A" | "--after-context" => options.after = number("context length", value())?,
            "-B" | "--before-context" => options.before = number("context length", value())?,
            "-C" | "--context" => {
                let n = number("context length", value())?;
                options.after = n;
                options.before = n;
            }
            "--color" | "--colour" => {
                options.color = match inline.as_deref() {
                    None | Some("auto") => stdout_is_terminal(),
                    Some("always") => true,
                    Some("never") => false,
                    Some(other) => return Err(format!("invalid argument '{other}' for '--color'")),
                }
            }
            "--include" => options
                .include
                .push(glob::Pattern::new(&value().unwrap_or_default()).map_err(|e| e.to_string())?),
            "--exclude" => options
                .exclude
                .push(glob::Pattern::new(&value().unwrap_or_default()).map_err(|e| e.to_string())?),
            "--exclude-dir" => options
                .exclude_dir
                .push(glob::Pattern::new(&value().unwrap_or_default()).map_err(|e| e.to_string())?),
            "--help" => {
                println!("{HELP}");
                std::process::exit(0);
            }
            other => {
                return Err(format!(
                    "invalid option -- '{}'\n{USAGE}",
                    other.trim_start_matches('-')
                ))
            }
        }
    }
    if !explicit_patterns {
        if operands.is_empty() {
            return Err(USAGE.to_owned());
        }
        options.patterns.push(operands.remove(0));
    }
    Ok((options, operands))
}

/// Converts GNU basic/extended regular expressions to the `regex` crate's syntax.
pub(crate) fn translate(pattern: &str, extended: bool) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars().peekable();
    let mut in_bracket = false;
    while let Some(c) = chars.next() {
        if in_bracket {
            out.push(c);
            if c == ']' {
                in_bracket = false;
            }
            continue;
        }
        match c {
            '[' => {
                in_bracket = true;
                out.push(c);
                if chars.peek() == Some(&'^') {
                    out.push(chars.next().unwrap_or('^'));
                }
                if chars.peek() == Some(&']') {
                    out.push_str("\\]");
                    chars.next();
                }
            }
            '\\' => match chars.next() {
                Some('<') | Some('>') => out.push_str("\\b"),
                Some(special @ ('(' | ')' | '{' | '}' | '|' | '+' | '?')) if !extended => {
                    out.push(special)
                }
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push_str("\\\\"),
            },
            '(' | ')' | '{' | '}' | '|' | '+' | '?' if !extended => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn build_regex(options: &Options) -> Result<Regex, String> {
    let alternatives: Vec<String> = options
        .patterns
        .iter()
        .flat_map(|pattern| pattern.split('\n'))
        .map(|pattern| {
            if options.fixed {
                regex::escape(pattern)
            } else {
                translate(pattern, options.extended)
            }
        })
        .map(|pattern| format!("(?:{pattern})"))
        .collect();
    let mut joined = alternatives.join("|");
    if options.word {
        joined = format!(r"\b(?:{joined})\b");
    }
    if options.line {
        joined = format!("^(?:{joined})$");
    }
    RegexBuilder::new(&joined)
        .case_insensitive(options.ignore_case)
        .unicode(true)
        .build()
        .map_err(|error| {
            let message = error.to_string();
            message
                .lines()
                .last()
                .unwrap_or("invalid pattern")
                .trim()
                .to_owned()
        })
}

struct Painter {
    color: bool,
}

impl Painter {
    fn paint(&self, code: &str, text: &[u8], out: &mut Vec<u8>) {
        if self.color {
            out.extend_from_slice(format!("\x1b[{code}m").as_bytes());
            out.extend_from_slice(text);
            out.extend_from_slice(b"\x1b[m");
        } else {
            out.extend_from_slice(text);
        }
    }
}

struct Searcher<'a> {
    options: &'a Options,
    regex: Regex,
    painter: Painter,
    out: io::StdoutLock<'a>,
    show_names: bool,
    any_match: bool,
    had_error: bool,
    /// File index and line number of the last line printed, to place `--` separators.
    last_output: Option<(usize, usize)>,
    file_index: usize,
}

impl Searcher<'_> {
    /// Writes output; once the reader is gone (`grep x | head -1`), there is no
    /// point in searching further.
    fn emit(&mut self, bytes: &[u8]) {
        if let Err(error) = self.out.write_all(bytes) {
            if error.kind() != io::ErrorKind::BrokenPipe {
                eprintln!("grep: write error: {error}");
            }
            std::process::exit(2);
        }
    }

    fn prefix(&self, name: &str, number: usize, separator: u8, buffer: &mut Vec<u8>) {
        if self.show_names {
            self.painter.paint("35", name.as_bytes(), buffer);
            self.painter.paint("36", &[separator], buffer);
        }
        if self.options.line_numbers {
            self.painter
                .paint("32", number.to_string().as_bytes(), buffer);
            self.painter.paint("36", &[separator], buffer);
        }
    }

    fn selected(&self, line: &[u8]) -> bool {
        self.regex.is_match(line) != self.options.invert
    }

    fn write_line(&mut self, name: &str, number: usize, line: &[u8], matched: bool) {
        let mut buffer = Vec::with_capacity(line.len() + 32);
        let context = self.options.before > 0 || self.options.after > 0;
        if context
            && self
                .last_output
                .is_some_and(|(file, last)| file != self.file_index || number != last + 1)
        {
            self.painter.paint("36", b"--", &mut buffer);
            buffer.push(b'\n');
        }
        self.last_output = Some((self.file_index, number));
        if self.options.only_matching {
            if !matched || self.options.invert {
                return;
            }
            for found in self
                .regex
                .find_iter(line)
                .filter(|m| !m.as_bytes().is_empty())
            {
                self.prefix(name, number, b':', &mut buffer);
                self.painter.paint("01;31", found.as_bytes(), &mut buffer);
                buffer.push(b'\n');
            }
        } else {
            self.prefix(name, number, if matched { b':' } else { b'-' }, &mut buffer);
            if matched && !self.options.invert && self.painter.color {
                let mut last = 0;
                for found in self
                    .regex
                    .find_iter(line)
                    .filter(|m| !m.as_bytes().is_empty())
                {
                    buffer.extend_from_slice(&line[last..found.start()]);
                    self.painter.paint("01;31", found.as_bytes(), &mut buffer);
                    last = found.end();
                }
                buffer.extend_from_slice(&line[last..]);
            } else {
                buffer.extend_from_slice(line);
            }
            buffer.push(b'\n');
        }
        self.emit(&buffer);
    }

    /// Searches one input. Returns the number of selected lines.
    fn search(&mut self, name: &str, reader: impl Read) -> usize {
        let options = self.options;
        let mut reader = BufReader::new(reader);
        let mut line = Vec::new();
        let mut number = 0;
        let mut selected = 0;
        let mut binary = false;
        let mut before: VecDeque<(usize, Vec<u8>)> = VecDeque::new();
        let mut after_left = 0;
        self.file_index += 1;
        let listing = options.count
            || options.files_with_matches
            || options.files_without_match
            || options.quiet;

        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) => break,
                Ok(_) => {}
                Err(error) => {
                    if !options.no_messages {
                        eprintln!("grep: {name}: {error}");
                    }
                    self.had_error = true;
                    break;
                }
            }
            number += 1;
            while matches!(line.last(), Some(b'\n' | b'\r')) {
                line.pop();
            }
            if !binary && !options.text && line.contains(&0) {
                if options.skip_binary {
                    return 0;
                }
                binary = true;
            }
            let is_match = self.selected(&line);
            if is_match {
                selected += 1;
                self.any_match = true;
                if options.quiet {
                    return selected;
                }
                if options.files_with_matches {
                    break;
                }
                if !listing && !binary {
                    for (context_number, context_line) in before.drain(..) {
                        self.write_line(name, context_number, &context_line, false);
                    }
                    self.write_line(name, number, &line, true);
                    after_left = options.after;
                }
                if options.max_count.is_some_and(|max| selected >= max) {
                    // Like GNU grep, still print trailing context after the last match.
                    while after_left > 0 {
                        line.clear();
                        if reader.read_until(b'\n', &mut line).map_or(true, |n| n == 0) {
                            break;
                        }
                        number += 1;
                        while matches!(line.last(), Some(b'\n' | b'\r')) {
                            line.pop();
                        }
                        self.write_line(name, number, &line, false);
                        after_left -= 1;
                    }
                    break;
                }
            } else if !listing && !binary {
                if after_left > 0 {
                    self.write_line(name, number, &line, false);
                    after_left -= 1;
                } else if options.before > 0 {
                    before.push_back((number, line.clone()));
                    if before.len() > options.before {
                        before.pop_front();
                    }
                }
            }
        }

        if binary && selected > 0 && !listing {
            self.emit(format!("grep: {name}: binary file matches\n").as_bytes());
        }
        if options.count {
            let mut buffer = Vec::new();
            if self.show_names {
                self.painter.paint("35", name.as_bytes(), &mut buffer);
                self.painter.paint("36", b":", &mut buffer);
            }
            buffer.extend_from_slice(format!("{selected}\n").as_bytes());
            self.emit(&buffer);
        }
        if (options.files_with_matches && selected > 0)
            || (options.files_without_match && selected == 0)
        {
            let mut buffer = Vec::new();
            self.painter.paint("35", name.as_bytes(), &mut buffer);
            buffer.push(b'\n');
            self.emit(&buffer);
        }
        selected
    }
}

pub fn run(args: &[String]) -> i32 {
    let (options, mut operands) = match parse(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("grep: {message}");
            return 2;
        }
    };
    let regex = match build_regex(&options) {
        Ok(regex) => regex,
        Err(message) => {
            eprintln!("grep: {message}");
            return 2;
        }
    };
    let implicit_root = operands.is_empty() && options.recursive;
    if implicit_root {
        operands.push(".".into());
    }
    let show_names = options
        .with_filename
        .unwrap_or(operands.len() > 1 || options.recursive);
    let stdout = io::stdout();
    let mut searcher = Searcher {
        options: &options,
        regex,
        painter: Painter {
            color: options.color,
        },
        out: stdout.lock(),
        show_names,
        any_match: false,
        had_error: false,
        last_output: None,
        file_index: 0,
    };

    if operands.is_empty() {
        searcher.search("(standard input)", io::stdin().lock());
    }
    for operand in &operands {
        if operand == "-" {
            searcher.search("(standard input)", io::stdin().lock());
            continue;
        }
        let path_text = crate::sys::translate_path(operand);
        let path = Path::new(&path_text);
        if path.is_dir() {
            if !options.recursive {
                if !options.no_messages {
                    eprintln!("grep: {operand}: Is a directory");
                }
                searcher.had_error = true;
                continue;
            }
            let walker = walkdir::WalkDir::new(path)
                .sort_by_file_name()
                .into_iter()
                .filter_entry(|entry| {
                    entry.depth() == 0
                        || !entry.file_type().is_dir()
                        || !options
                            .exclude_dir
                            .iter()
                            .any(|p| p.matches(&entry.file_name().to_string_lossy()))
                });
            for entry in walker {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        if !options.no_messages {
                            eprintln!("grep: {error}");
                        }
                        searcher.had_error = true;
                        continue;
                    }
                };
                if !entry.file_type().is_file() {
                    continue;
                }
                let file_name = entry.file_name().to_string_lossy();
                if (!options.include.is_empty()
                    && !options.include.iter().any(|p| p.matches(&file_name)))
                    || options.exclude.iter().any(|p| p.matches(&file_name))
                {
                    continue;
                }
                let mut shown = entry.path().to_string_lossy().replace('\\', "/");
                if implicit_root {
                    shown = shown.trim_start_matches("./").to_owned();
                }
                match File::open(entry.path()) {
                    Ok(file) => {
                        searcher.search(&shown, file);
                    }
                    Err(error) => {
                        if !options.no_messages {
                            eprintln!("grep: {shown}: {}", crate::exec::describe_io_error(&error));
                        }
                        searcher.had_error = true;
                    }
                }
                if options.quiet && searcher.any_match {
                    return 0;
                }
            }
        } else {
            match File::open(path) {
                Ok(file) => {
                    searcher.search(operand, file);
                }
                Err(error) => {
                    if !options.no_messages {
                        eprintln!(
                            "grep: {operand}: {}",
                            crate::exec::describe_io_error(&error)
                        );
                    }
                    searcher.had_error = true;
                }
            }
        }
        if options.quiet && searcher.any_match {
            return 0;
        }
    }

    let _ = searcher.out.flush();
    // GNU: an error wins over a match, except with -q.
    if searcher.had_error && !(options.quiet && searcher.any_match) {
        2
    } else if searcher.any_match {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::translate;

    #[test]
    fn converts_basic_regular_expressions() {
        assert_eq!(translate(r"a\|b", false), "a|b");
        assert_eq!(translate("a|b", false), r"a\|b");
        assert_eq!(translate(r"\(ab\)\+", false), "(ab)+");
        assert_eq!(translate("x{2}", false), r"x\{2\}");
        assert_eq!(translate(r"\<word\>", false), r"\bword\b");
    }

    #[test]
    fn keeps_extended_expressions() {
        assert_eq!(translate("(a|b)+c?", true), "(a|b)+c?");
        assert_eq!(translate("[(|)]", false), "[(|)]");
        assert_eq!(translate("[]a]", true), r"[\]a]");
    }
}

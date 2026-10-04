//! Runs a parsed awk program over its input.

use super::ast::*;
use super::value::{self, Value};
use super::{
    Builtin, ARGC, ARGV, CONVFMT, ENVIRON, FILENAME, FNR, FS, NR, OFMT, OFS, ORS, RLENGTH, RS,
    RSTART, SUBSEP,
};
use indexmap::IndexMap;
use regex::Regex;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, IsTerminal, Read, Write};
use std::process::{Child, Command, Stdio};
use std::rc::Rc;

type Array = Rc<RefCell<IndexMap<Rc<str>, Value>>>;

#[derive(Clone)]
enum Cell {
    Uninit,
    Scalar(Value),
    Array(Array),
}

/// How a statement ended, when it didn't simply run to completion.
enum Flow {
    Normal,
    Break,
    Continue,
    Return(Value),
}

/// Leaving the current rule or the whole program, or failing.
pub enum Unwind {
    Next,
    NextFile,
    Exit,
    /// Output went to a closed pipe (`awk … | head -1`): stop quietly.
    Quit,
    Fatal(String),
}

type Result<T> = std::result::Result<T, Unwind>;

fn fatal<T>(message: impl Into<String>) -> Result<T> {
    Err(Unwind::Fatal(message.into()))
}

/// Deep enough for any sane recursion, shallow enough for the thread's stack.
const MAX_DEPTH: usize = 20_000;

enum RecordSep {
    Newline,
    Byte(u8),
    Paragraph,
    Regex(Regex),
}

/// Records from one input: the main files, a `getline < file` or a `cmd | getline`.
struct Records {
    reader: Box<dyn BufRead>,
    /// Unread text, for a regular-expression `RS`, which needs to look ahead.
    pending: Option<String>,
}

impl Records {
    fn new(reader: Box<dyn BufRead>) -> Records {
        Records {
            reader,
            pending: None,
        }
    }

    fn read_line(&mut self, delimiter: u8) -> io::Result<Option<Vec<u8>>> {
        let mut buffer = Vec::new();
        if self.reader.read_until(delimiter, &mut buffer)? == 0 {
            return Ok(None);
        }
        if buffer.last() == Some(&delimiter) {
            buffer.pop();
        }
        Ok(Some(buffer))
    }

    fn next(&mut self, separator: &RecordSep) -> io::Result<Option<String>> {
        match separator {
            RecordSep::Newline => Ok(self.read_line(b'\n')?.map(|mut line| {
                // Windows files end lines with CRLF; the CR isn't part of the record.
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                String::from_utf8_lossy(&line).into_owned()
            })),
            RecordSep::Byte(byte) => Ok(self
                .read_line(*byte)?
                .map(|record| String::from_utf8_lossy(&record).into_owned())),
            RecordSep::Paragraph => {
                let mut record = String::new();
                loop {
                    let Some(mut line) = self.read_line(b'\n')? else {
                        return Ok((!record.is_empty()).then_some(record));
                    };
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    if line.is_empty() {
                        if record.is_empty() {
                            continue;
                        }
                        return Ok(Some(record));
                    }
                    if !record.is_empty() {
                        record.push('\n');
                    }
                    record.push_str(&String::from_utf8_lossy(&line));
                }
            }
            RecordSep::Regex(regex) => {
                if self.pending.is_none() {
                    let mut all = Vec::new();
                    self.reader.read_to_end(&mut all)?;
                    self.pending = Some(String::from_utf8_lossy(&all).into_owned());
                }
                let pending = self.pending.as_mut().expect("filled above");
                if pending.is_empty() {
                    return Ok(None);
                }
                let found = regex
                    .find_iter(pending)
                    .find(|found| found.end() > found.start())
                    .map(|found| (found.start(), found.end()));
                match found {
                    Some((start, end)) => {
                        let record = pending[..start].to_owned();
                        pending.replace_range(..end, "");
                        Ok(Some(record))
                    }
                    None => Ok(Some(std::mem::take(pending))),
                }
            }
        }
    }
}

enum Input {
    File(Records),
    Command(Child, Records),
}

enum Output {
    Stdout,
    Stderr,
    Null,
    File(BufWriter<File>),
    Pipe(Child),
}

/// The files named on the command line, read one after the other.
struct MainInput {
    next_arg: usize,
    current: Option<Records>,
    opened_any: bool,
}

/// A small generator for `rand()`, seeded like POSIX awk: 0 until `srand`.
struct Rand {
    state: u64,
    seed: f64,
}

impl Rand {
    fn reseed(&mut self, seed: f64) {
        self.seed = seed;
        self.state = (seed.to_bits() ^ 0x9e37_79b9_7f4a_7c15).max(1);
    }

    fn next(&mut self) -> f64 {
        // xorshift64*
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        let value = x.wrapping_mul(0x2545_f491_4f6c_dd1d);
        (value >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub struct Interp<'p> {
    prog: &'p Program,
    globals: Vec<Cell>,
    frames: Vec<Vec<Cell>>,
    record: Rc<str>,
    fields: Vec<Rc<str>>,
    split_done: bool,
    literal_regexes: Vec<Regex>,
    dynamic_regexes: HashMap<String, Regex>,
    stdout: BufWriter<io::Stdout>,
    flush_each_line: bool,
    outputs: HashMap<String, Output>,
    inputs: HashMap<String, Input>,
    main: MainInput,
    ranges: Vec<bool>,
    rand: Rand,
    exit_code: i32,
}

/// Turns an awk extended regular expression into one the `regex` crate reads.
fn translate_regex(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len() + 4);
    out.push_str("(?s)");
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                // gawk's word boundaries.
                Some('y' | '<' | '>') => out.push_str("\\b"),
                Some('/') => out.push('/'),
                Some('"') => out.push('"'),
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push_str("\\\\"),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn compile_regex(pattern: &str) -> std::result::Result<Regex, String> {
    let translated = translate_regex(pattern);
    Regex::new(&translated).or_else(|first| {
        // awk takes a `{` that starts no interval as a literal brace.
        let braces = translated.replace('{', "\\{").replace('}', "\\}");
        Regex::new(&braces).map_err(|_| {
            let detail = first.to_string();
            let reason = detail.lines().last().unwrap_or("").trim();
            format!("bad regular expression /{pattern}/: {reason}")
        })
    })
}

/// A command run through Nebula itself, for `system()`, `print | cmd` and `cmd | getline`.
fn shell(command: &str) -> Command {
    let exe = std::env::current_exe().unwrap_or_else(|_| "nebula-sh".into());
    let mut process = Command::new(exe);
    process.arg("-c").arg(command);
    process
}

fn is_assignment(arg: &str) -> Option<(&str, &str)> {
    let (name, value) = arg.split_once('=')?;
    let mut chars = name.chars();
    let first = chars.next()?;
    if (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        Some((name, value))
    } else {
        None
    }
}

/// `&` stands for the matched text, `\&` for a literal ampersand.
fn substitute(replacement: &str, matched: &str, out: &mut String) {
    let mut chars = replacement.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if matches!(chars.peek(), Some('&') | Some('\\')) => {
                out.push(chars.next().expect("peeked"));
            }
            '&' => out.push_str(matched),
            _ => out.push(c),
        }
    }
}

/// `gensub`'s replacement: `\0` and `&` for the match, `\1`…`\9` for groups.
fn substitute_groups(replacement: &str, caps: &regex::Captures, out: &mut String) {
    let mut chars = replacement.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.peek().copied() {
                Some(d @ '0'..='9') => {
                    chars.next();
                    let group = d.to_digit(10).unwrap_or(0) as usize;
                    out.push_str(caps.get(group).map_or("", |m| m.as_str()));
                }
                Some('&') => {
                    chars.next();
                    out.push('&');
                }
                Some('\\') => {
                    chars.next();
                    out.push('\\');
                }
                _ => out.push('\\'),
            },
            '&' => out.push_str(caps.get(0).map_or("", |m| m.as_str())),
            _ => out.push(c),
        }
    }
}

fn char_index(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}

impl<'p> Interp<'p> {
    pub fn new(
        prog: &'p Program,
        assignments: Vec<(String, String)>,
        fs: Option<String>,
        operands: Vec<String>,
    ) -> std::result::Result<Interp<'p>, String> {
        let literal_regexes = prog
            .regexes
            .iter()
            .map(|pattern| compile_regex(pattern))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut interp = Interp {
            prog,
            globals: vec![Cell::Uninit; prog.globals.len()],
            frames: Vec::new(),
            record: Rc::from(""),
            fields: Vec::new(),
            split_done: true,
            literal_regexes,
            dynamic_regexes: HashMap::new(),
            stdout: BufWriter::with_capacity(64 * 1024, io::stdout()),
            flush_each_line: io::stdout().is_terminal(),
            outputs: HashMap::new(),
            inputs: HashMap::new(),
            main: MainInput {
                next_arg: 1,
                current: None,
                opened_any: false,
            },
            ranges: vec![false; prog.items.len()],
            rand: Rand {
                state: 0,
                seed: 0.0,
            },
            exit_code: 0,
        };
        interp.rand.reseed(0.0);
        let defaults = [
            (FS, " "),
            (OFS, " "),
            (ORS, "\n"),
            (RS, "\n"),
            (SUBSEP, "\x1c"),
            (CONVFMT, "%.6g"),
            (OFMT, "%.6g"),
            (FILENAME, ""),
        ];
        for (slot, text) in defaults {
            interp.globals[slot] = Cell::Scalar(Value::str(text));
        }
        for slot in [NR, FNR, RSTART] {
            interp.globals[slot] = Cell::Scalar(Value::Num(0.0));
        }
        interp.globals[RLENGTH] = Cell::Scalar(Value::Num(-1.0));
        if let Some(fs) = fs {
            interp.globals[FS] = Cell::Scalar(Value::str(fs));
        }

        let environ: IndexMap<Rc<str>, Value> = std::env::vars_os()
            .map(|(key, value)| {
                (
                    Rc::from(key.to_string_lossy().as_ref()),
                    Value::strnum(value.to_string_lossy().as_ref()),
                )
            })
            .collect();
        interp.globals[ENVIRON] = Cell::Array(Rc::new(RefCell::new(environ)));
        let mut argv: IndexMap<Rc<str>, Value> = IndexMap::new();
        argv.insert(Rc::from("0"), Value::str("awk"));
        for (index, operand) in operands.iter().enumerate() {
            argv.insert(
                Rc::from((index + 1).to_string()),
                Value::strnum(operand.as_str()),
            );
        }
        interp.globals[ARGV] = Cell::Array(Rc::new(RefCell::new(argv)));
        interp.globals[ARGC] = Cell::Scalar(Value::Num((operands.len() + 1) as f64));

        for (name, value) in assignments {
            interp.assign_global(&name, &value)?;
        }
        Ok(interp)
    }

    fn assign_global(&mut self, name: &str, value: &str) -> std::result::Result<(), String> {
        let value = Value::strnum(super::lexer::unescape(value));
        match self.prog.globals.iter().position(|global| global == name) {
            Some(slot) => match self.globals[slot] {
                Cell::Array(_) => Err(format!("can't assign to `{name}', which is an array")),
                _ => {
                    self.globals[slot] = Cell::Scalar(value);
                    Ok(())
                }
            },
            // A variable the program never mentions: nothing can read it.
            None => Ok(()),
        }
    }

    // ---- variables -------------------------------------------------------------

    fn var_name(&self, var: Var) -> String {
        match var {
            Var::Global(index) => self.prog.globals[index].clone(),
            Var::Local(index) => format!("parameter #{}", index + 1),
        }
    }

    fn cell(&mut self, var: Var) -> &mut Cell {
        match var {
            Var::Global(index) => &mut self.globals[index],
            Var::Local(index) => &mut self.frames.last_mut().expect("inside a function")[index],
        }
    }

    fn get_var(&mut self, var: Var) -> Result<Value> {
        match self.cell(var) {
            Cell::Uninit => Ok(Value::Uninit),
            Cell::Scalar(value) => Ok(value.clone()),
            Cell::Array(_) => {
                let name = self.var_name(var);
                fatal(format!("can't use array `{name}' as a value"))
            }
        }
    }

    fn set_var(&mut self, var: Var, value: Value) -> Result<()> {
        let cell = self.cell(var);
        if let Cell::Array(_) = cell {
            let name = self.var_name(var);
            return fatal(format!("can't assign to array `{name}'"));
        }
        *cell = Cell::Scalar(value);
        Ok(())
    }

    fn array(&mut self, var: Var) -> Result<Array> {
        let cell = self.cell(var);
        match cell {
            Cell::Array(array) => Ok(array.clone()),
            Cell::Uninit => {
                let array: Array = Rc::default();
                *cell = Cell::Array(array.clone());
                Ok(array)
            }
            Cell::Scalar(_) => {
                let name = self.var_name(var);
                fatal(format!("can't use `{name}' as an array: it holds a value"))
            }
        }
    }

    fn global_str(&self, slot: usize) -> Rc<str> {
        match &self.globals[slot] {
            Cell::Scalar(value) => value.to_str(&self.convfmt()),
            _ => Rc::from(""),
        }
    }

    fn convfmt(&self) -> Rc<str> {
        match &self.globals[CONVFMT] {
            Cell::Scalar(Value::Str(text) | Value::StrNum(text)) => text.clone(),
            _ => Rc::from("%.6g"),
        }
    }

    fn to_str(&self, value: &Value) -> Rc<str> {
        match value {
            Value::Num(_) => value.to_str(&self.convfmt()),
            Value::Uninit => Rc::from(""),
            Value::Str(text) | Value::StrNum(text) => text.clone(),
        }
    }

    /// Numbers print with `OFMT` rather than `CONVFMT`.
    fn to_output(&self, value: &Value) -> Rc<str> {
        match value {
            Value::Num(_) => value.to_str(&self.global_str(OFMT)),
            other => self.to_str(other),
        }
    }

    fn bump(&mut self, slot: usize) {
        let value = match &self.globals[slot] {
            Cell::Scalar(value) => value.to_num(),
            _ => 0.0,
        };
        self.globals[slot] = Cell::Scalar(Value::Num(value + 1.0));
    }

    fn subscript(&mut self, subscripts: &[Expr]) -> Result<Rc<str>> {
        if subscripts.len() == 1 {
            let value = self.eval(&subscripts[0])?;
            return Ok(self.to_str(&value));
        }
        let separator = self.global_str(SUBSEP);
        let mut key = String::new();
        for (index, expr) in subscripts.iter().enumerate() {
            if index > 0 {
                key.push_str(&separator);
            }
            let value = self.eval(expr)?;
            key.push_str(&self.to_str(&value));
        }
        Ok(Rc::from(key))
    }

    // ---- the record and its fields -----------------------------------------------

    fn set_record(&mut self, text: impl Into<Rc<str>>) {
        self.record = text.into();
        self.fields.clear();
        self.split_done = false;
    }

    fn split_fields(&mut self) -> Result<()> {
        if self.split_done {
            return Ok(());
        }
        let fs = self.global_str(FS);
        let paragraph = self.global_str(RS).is_empty();
        let record = self.record.clone();
        self.fields = self.split_text(&record, &fs, paragraph)?;
        self.split_done = true;
        Ok(())
    }

    /// Splits like awk: `" "` on runs of blanks, one other character literally,
    /// `""` into characters, anything longer as a regular expression.
    fn split_text(&mut self, text: &str, fs: &str, paragraph: bool) -> Result<Vec<Rc<str>>> {
        if fs == " " {
            return Ok(text
                .split([' ', '\t', '\n'])
                .filter(|part| !part.is_empty())
                .map(Rc::from)
                .collect());
        }
        if text.is_empty() {
            return Ok(Vec::new());
        }
        if fs.is_empty() {
            return Ok(text.chars().map(|c| Rc::from(c.to_string())).collect());
        }
        let mut chars = fs.chars();
        let single = match (chars.next(), chars.next()) {
            (Some(single), None) => Some(single),
            _ => None,
        };
        if let Some(single) = single {
            // In paragraph mode a newline also separates fields.
            if !paragraph || single == '\n' {
                return Ok(text.split(single).map(Rc::from).collect());
            }
        }
        let base = match single {
            Some(_) => regex::escape(fs),
            None => fs.to_owned(),
        };
        let pattern = if paragraph {
            format!("(?:{base})|\n")
        } else {
            base
        };
        let regex = self.dynamic_regex(&pattern)?;
        Ok(regex.split(text).map(Rc::from).collect())
    }

    fn rebuild_record(&mut self) {
        let separator = self.global_str(OFS);
        let joined = self
            .fields
            .iter()
            .map(|field| &**field)
            .collect::<Vec<_>>()
            .join(&separator);
        self.record = Rc::from(joined);
    }

    fn field_index(&mut self, expr: &Expr) -> Result<usize> {
        let index = self.eval(expr)?.to_num();
        if index < 0.0 {
            return fatal(format!("there is no field ${}", index as i64));
        }
        Ok(index as usize)
    }

    fn get_field(&mut self, index: usize) -> Result<Value> {
        if index == 0 {
            return Ok(Value::StrNum(self.record.clone()));
        }
        self.split_fields()?;
        Ok(match self.fields.get(index - 1) {
            Some(field) => Value::StrNum(field.clone()),
            None => Value::Uninit,
        })
    }

    fn set_field(&mut self, index: usize, text: Rc<str>) -> Result<()> {
        if index == 0 {
            self.set_record(text);
            return Ok(());
        }
        self.split_fields()?;
        if index > self.fields.len() {
            self.fields.resize(index, Rc::from(""));
        }
        self.fields[index - 1] = text;
        self.rebuild_record();
        Ok(())
    }

    fn get_nf(&mut self) -> Result<f64> {
        self.split_fields()?;
        Ok(self.fields.len() as f64)
    }

    fn set_nf(&mut self, value: f64) -> Result<()> {
        if value < 0.0 {
            return fatal("NF can't be negative");
        }
        self.split_fields()?;
        self.fields.resize(value as usize, Rc::from(""));
        self.rebuild_record();
        Ok(())
    }

    // ---- lvalues ---------------------------------------------------------------------

    fn read(&mut self, target: &LValue) -> Result<Value> {
        match target {
            LValue::Var(var) => self.get_var(*var),
            LValue::Index(var, subscripts) => {
                let key = self.subscript(subscripts)?;
                let array = self.array(*var)?;
                let mut array = array.borrow_mut();
                // Reading an element creates it, as in every awk.
                Ok(array.entry(key).or_insert(Value::Uninit).clone())
            }
            LValue::Field(expr) => {
                let index = self.field_index(expr)?;
                self.get_field(index)
            }
            LValue::Nf => Ok(Value::Num(self.get_nf()?)),
        }
    }

    fn write(&mut self, target: &LValue, value: Value) -> Result<()> {
        match target {
            LValue::Var(var) => self.set_var(*var, value),
            LValue::Index(var, subscripts) => {
                let key = self.subscript(subscripts)?;
                let array = self.array(*var)?;
                array.borrow_mut().insert(key, value);
                Ok(())
            }
            LValue::Field(expr) => {
                let index = self.field_index(expr)?;
                let text = self.to_str(&value);
                self.set_field(index, text)
            }
            LValue::Nf => self.set_nf(value.to_num()),
        }
    }

    // ---- regular expressions -----------------------------------------------------------

    fn dynamic_regex(&mut self, pattern: &str) -> Result<Regex> {
        if let Some(regex) = self.dynamic_regexes.get(pattern) {
            return Ok(regex.clone());
        }
        match compile_regex(pattern) {
            Ok(regex) => {
                if self.dynamic_regexes.len() > 500 {
                    self.dynamic_regexes.clear();
                }
                self.dynamic_regexes
                    .insert(pattern.to_owned(), regex.clone());
                Ok(regex)
            }
            Err(message) => fatal(message),
        }
    }

    /// The regex an argument stands for: a `/literal/`, or a string read as a pattern.
    fn regex_of(&mut self, expr: &Expr) -> Result<Regex> {
        match expr {
            Expr::Regex(index) => Ok(self.literal_regexes[*index].clone()),
            Expr::Group(inner) => self.regex_of(inner),
            _ => {
                let value = self.eval(expr)?;
                let pattern = self.to_str(&value);
                self.dynamic_regex(&pattern)
            }
        }
    }

    // ---- expressions -------------------------------------------------------------------

    fn compare(&self, op: BinOp, left: &Value, right: &Value) -> bool {
        let ordering = match (left.numeric(), right.numeric()) {
            (Some(a), Some(b)) => match a.partial_cmp(&b) {
                Some(ordering) => ordering,
                None => return op == BinOp::Ne,
            },
            _ => self.to_str(left).cmp(&self.to_str(right)),
        };
        use std::cmp::Ordering::*;
        match op {
            BinOp::Lt => ordering == Less,
            BinOp::Le => ordering != Greater,
            BinOp::Gt => ordering == Greater,
            BinOp::Ge => ordering != Less,
            BinOp::Eq => ordering == Equal,
            BinOp::Ne => ordering != Equal,
            _ => false,
        }
    }

    fn arithmetic(op: BinOp, a: f64, b: f64) -> Result<f64> {
        Ok(match op {
            BinOp::Add => a + b,
            BinOp::Sub => a - b,
            BinOp::Mul => a * b,
            BinOp::Div => {
                if b == 0.0 {
                    return fatal("division by zero");
                }
                a / b
            }
            BinOp::Mod => {
                if b == 0.0 {
                    return fatal("division by zero in %");
                }
                a % b
            }
            BinOp::Pow => a.powf(b),
            _ => 0.0,
        })
    }

    fn bool_value(value: bool) -> Value {
        Value::Num(if value { 1.0 } else { 0.0 })
    }

    pub fn eval(&mut self, expr: &Expr) -> Result<Value> {
        match expr {
            Expr::Num(value) => Ok(Value::Num(*value)),
            Expr::Str(text) => Ok(Value::Str(text.clone())),
            Expr::Regex(index) => {
                let matched = self.literal_regexes[*index].is_match(&self.record);
                Ok(Self::bool_value(matched))
            }
            Expr::Read(target) => self.read(target),
            Expr::Assign(op, target, value) => {
                let value = self.eval(value)?;
                let result = match op {
                    Some(op) => {
                        let current = self.read(target)?.to_num();
                        Value::Num(Self::arithmetic(*op, current, value.to_num())?)
                    }
                    None => value,
                };
                self.write(target, result.clone())?;
                Ok(result)
            }
            Expr::Cond(cond, then, otherwise) => {
                if self.eval(cond)?.truthy() {
                    self.eval(then)
                } else {
                    self.eval(otherwise)
                }
            }
            Expr::And(a, b) => {
                let result = self.eval(a)?.truthy() && self.eval(b)?.truthy();
                Ok(Self::bool_value(result))
            }
            Expr::Or(a, b) => {
                let result = self.eval(a)?.truthy() || self.eval(b)?.truthy();
                Ok(Self::bool_value(result))
            }
            Expr::In(subscripts, var) => {
                let key = self.subscript(subscripts)?;
                let array = self.array(*var)?;
                let found = array.borrow().contains_key(&key);
                Ok(Self::bool_value(found))
            }
            Expr::Match(negate, subject, pattern) => {
                let subject = self.eval(subject)?;
                let text = self.to_str(&subject);
                let regex = self.regex_of(pattern)?;
                Ok(Self::bool_value(regex.is_match(&text) != *negate))
            }
            Expr::Binary(op, a, b) => {
                let left = self.eval(a)?;
                let right = self.eval(b)?;
                match op {
                    BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::Eq | BinOp::Ne => {
                        Ok(Self::bool_value(self.compare(*op, &left, &right)))
                    }
                    _ => Ok(Value::Num(Self::arithmetic(
                        *op,
                        left.to_num(),
                        right.to_num(),
                    )?)),
                }
            }
            Expr::Concat(a, b) => {
                let left = self.eval(a)?;
                let right = self.eval(b)?;
                let mut text = String::from(&*self.to_str(&left));
                text.push_str(&self.to_str(&right));
                Ok(Value::str(text))
            }
            Expr::Neg(a) => Ok(Value::Num(-self.eval(a)?.to_num())),
            Expr::Plus(a) => Ok(Value::Num(self.eval(a)?.to_num())),
            Expr::Not(a) => Ok(Self::bool_value(!self.eval(a)?.truthy())),
            Expr::IncDec {
                target,
                delta,
                prefix,
            } => {
                let old = self.read(target)?.to_num();
                self.write(target, Value::Num(old + delta))?;
                Ok(Value::Num(if *prefix { old + delta } else { old }))
            }
            Expr::Call(index, args) => self.call(*index, args),
            Expr::Builtin(builtin, args) => self.builtin(*builtin, args),
            Expr::Getline { source, target } => self.getline(source, target.as_ref()),
            Expr::Group(inner) => self.eval(inner),
        }
    }

    fn call(&mut self, index: usize, args: &[Expr]) -> Result<Value> {
        let prog = self.prog;
        let function = &prog.functions[index];
        if args.len() > function.params {
            return fatal(format!(
                "function `{}' takes {} arguments, not {}",
                function.name,
                function.params,
                args.len()
            ));
        }
        if self.frames.len() >= MAX_DEPTH {
            return fatal(format!(
                "function `{}' calls itself too deeply",
                function.name
            ));
        }
        let mut locals = vec![Cell::Uninit; function.params];
        for (position, arg) in args.iter().enumerate() {
            locals[position] = match arg {
                Expr::Read(LValue::Var(var)) => {
                    let wants_array = function.array_params[position];
                    match self.cell(*var).clone() {
                        Cell::Array(array) => Cell::Array(array),
                        Cell::Uninit if wants_array => Cell::Array(self.array(*var)?),
                        Cell::Uninit => Cell::Uninit,
                        Cell::Scalar(value) => Cell::Scalar(value),
                    }
                }
                _ => Cell::Scalar(self.eval(arg)?),
            };
        }
        self.frames.push(locals);
        let mut result = Value::Uninit;
        for stmt in &function.body {
            match self.exec(stmt) {
                Ok(Flow::Normal) => {}
                Ok(Flow::Return(value)) => {
                    result = value;
                    break;
                }
                Ok(Flow::Break | Flow::Continue) => break,
                Err(unwind) => {
                    self.frames.pop();
                    return Err(unwind);
                }
            }
        }
        self.frames.pop();
        Ok(result)
    }

    // ---- built-in functions ----------------------------------------------------------

    fn arg_num(&mut self, args: &[Expr], index: usize) -> Result<f64> {
        match args.get(index) {
            Some(expr) => Ok(self.eval(expr)?.to_num()),
            None => Ok(0.0),
        }
    }

    fn arg_str(&mut self, args: &[Expr], index: usize) -> Result<Rc<str>> {
        match args.get(index) {
            Some(expr) => {
                let value = self.eval(expr)?;
                Ok(self.to_str(&value))
            }
            None => Ok(Rc::from("")),
        }
    }

    fn check_args(builtin: Builtin, args: &[Expr], min: usize, max: usize) -> Result<()> {
        if args.len() < min || args.len() > max {
            return fatal(format!(
                "{} takes {}",
                builtin.name(),
                if min == max {
                    format!("{min} arguments")
                } else {
                    format!("{min} to {max} arguments")
                }
            ));
        }
        Ok(())
    }

    fn array_arg(&mut self, builtin: Builtin, expr: Option<&Expr>) -> Result<Array> {
        match expr {
            Some(Expr::Read(LValue::Var(var))) => {
                let var = *var;
                // Arrays given to these functions start over.
                let array = self.array(var)?;
                Ok(array)
            }
            _ => fatal(format!("{} needs an array name", builtin.name())),
        }
    }

    fn builtin(&mut self, builtin: Builtin, args: &[Expr]) -> Result<Value> {
        use Builtin::*;
        match builtin {
            Length => {
                Self::check_args(builtin, args, 0, 1)?;
                if let Some(Expr::Read(LValue::Var(var))) = args.first() {
                    if let Cell::Array(array) = self.cell(*var) {
                        let len = array.borrow().len();
                        return Ok(Value::Num(len as f64));
                    }
                }
                let text = match args.first() {
                    Some(_) => self.arg_str(args, 0)?,
                    None => self.record.clone(),
                };
                Ok(Value::Num(text.chars().count() as f64))
            }
            Substr => {
                // As gawk: positions are truncated, and a start before 1 counts as 1.
                Self::check_args(builtin, args, 2, 3)?;
                let text = self.arg_str(args, 0)?;
                let length = text.chars().count() as f64;
                let start = self.arg_num(args, 1)?;
                let first = if start.is_nan() {
                    1.0
                } else {
                    start.trunc().max(1.0)
                };
                let end = match args.get(2) {
                    Some(_) => {
                        let count = self.arg_num(args, 2)?;
                        if count.is_nan() {
                            first
                        } else {
                            first + count.trunc()
                        }
                    }
                    None => length + 1.0,
                };
                let last = end.min(length + 1.0);
                if last <= first {
                    return Ok(Value::str(""));
                }
                let piece: String = text
                    .chars()
                    .skip(first as usize - 1)
                    .take((last - first) as usize)
                    .collect();
                Ok(Value::str(piece))
            }
            Index => {
                Self::check_args(builtin, args, 2, 2)?;
                let text = self.arg_str(args, 0)?;
                let needle = self.arg_str(args, 1)?;
                if needle.is_empty() {
                    return Ok(Value::Num(1.0));
                }
                let position = text
                    .find(&*needle)
                    .map_or(0, |byte| char_index(&text, byte) + 1);
                Ok(Value::Num(position as f64))
            }
            Split => {
                Self::check_args(builtin, args, 2, 3)?;
                let text = self.arg_str(args, 0)?;
                let array = self.array_arg(builtin, args.get(1))?;
                let parts = match args.get(2) {
                    Some(Expr::Regex(index)) => {
                        let regex = self.literal_regexes[*index].clone();
                        if text.is_empty() {
                            Vec::new()
                        } else {
                            regex.split(&text).map(Rc::from).collect()
                        }
                    }
                    Some(_) => {
                        let fs = self.arg_str(args, 2)?;
                        self.split_text(&text, &fs, false)?
                    }
                    None => {
                        let fs = self.global_str(FS);
                        self.split_text(&text, &fs, false)?
                    }
                };
                let mut array = array.borrow_mut();
                array.clear();
                for (index, part) in parts.iter().enumerate() {
                    array.insert(
                        Rc::from((index + 1).to_string()),
                        Value::StrNum(part.clone()),
                    );
                }
                Ok(Value::Num(parts.len() as f64))
            }
            Sub | Gsub => {
                Self::check_args(builtin, args, 2, 3)?;
                let regex = self.regex_of(&args[0])?;
                let replacement = self.arg_str(args, 1)?;
                let target = match args.get(2) {
                    Some(Expr::Read(target)) => target.clone(),
                    Some(Expr::Group(inner)) => match &**inner {
                        Expr::Read(target) => target.clone(),
                        _ => {
                            return fatal(format!("{} needs a variable to change", builtin.name()))
                        }
                    },
                    Some(_) => {
                        return fatal(format!("{} needs a variable to change", builtin.name()))
                    }
                    None => LValue::Field(Box::new(Expr::Num(0.0))),
                };
                let current = self.read(&target)?;
                let text = self.to_str(&current);
                let (result, count) = replace(&regex, &text, &replacement, builtin == Gsub);
                if count > 0 {
                    self.write(&target, Value::str(result))?;
                }
                Ok(Value::Num(count as f64))
            }
            Gensub => {
                Self::check_args(builtin, args, 3, 4)?;
                let regex = self.regex_of(&args[0])?;
                let replacement = self.arg_str(args, 1)?;
                let how = self.arg_str(args, 2)?;
                let text = match args.get(3) {
                    Some(_) => self.arg_str(args, 3)?,
                    None => self.record.clone(),
                };
                let which = if how.starts_with(['g', 'G']) {
                    0
                } else {
                    (value::str_to_num(&how) as usize).max(1)
                };
                let mut out = String::new();
                let mut last = 0;
                for (count, caps) in regex.captures_iter(&text).enumerate() {
                    let whole = caps.get(0).expect("group 0");
                    if which == 0 || count + 1 == which {
                        out.push_str(&text[last..whole.start()]);
                        substitute_groups(&replacement, &caps, &mut out);
                        last = whole.end();
                        if which != 0 {
                            break;
                        }
                    }
                }
                out.push_str(&text[last..]);
                Ok(Value::str(out))
            }
            Match => {
                Self::check_args(builtin, args, 2, 3)?;
                let text = self.arg_str(args, 0)?;
                let regex = self.regex_of(&args[1])?;
                let groups = match args.get(2) {
                    Some(expr) => Some(self.array_arg(builtin, Some(expr))?),
                    None => None,
                };
                let (start, length) = match regex.captures(&text) {
                    Some(caps) => {
                        let whole = caps.get(0).expect("group 0");
                        if let Some(groups) = &groups {
                            let mut groups = groups.borrow_mut();
                            groups.clear();
                            for (index, group) in caps.iter().enumerate() {
                                if let Some(group) = group {
                                    groups.insert(
                                        Rc::from(index.to_string()),
                                        Value::strnum(group.as_str()),
                                    );
                                }
                            }
                        }
                        (
                            char_index(&text, whole.start()) as f64 + 1.0,
                            whole.as_str().chars().count() as f64,
                        )
                    }
                    None => {
                        if let Some(groups) = &groups {
                            groups.borrow_mut().clear();
                        }
                        (0.0, -1.0)
                    }
                };
                self.globals[RSTART] = Cell::Scalar(Value::Num(start));
                self.globals[RLENGTH] = Cell::Scalar(Value::Num(length));
                Ok(Value::Num(start))
            }
            Sprintf => {
                if args.is_empty() {
                    return fatal("sprintf needs a format");
                }
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(arg)?);
                }
                let format = self.to_str(&values[0]);
                match value::sprintf(&format, &values[1..], &self.convfmt()) {
                    Ok(text) => Ok(Value::str(text)),
                    Err(message) => fatal(format!("sprintf: {message}")),
                }
            }
            Sin | Cos | Exp | Log | Sqrt | Int => {
                Self::check_args(builtin, args, 1, 1)?;
                let x = self.arg_num(args, 0)?;
                Ok(Value::Num(match builtin {
                    Sin => x.sin(),
                    Cos => x.cos(),
                    Exp => x.exp(),
                    Log => x.ln(),
                    Sqrt => x.sqrt(),
                    _ => x.trunc(),
                }))
            }
            Atan2 => {
                Self::check_args(builtin, args, 2, 2)?;
                let y = self.arg_num(args, 0)?;
                let x = self.arg_num(args, 1)?;
                Ok(Value::Num(y.atan2(x)))
            }
            Rand => Ok(Value::Num(self.rand.next())),
            Srand => {
                Self::check_args(builtin, args, 0, 1)?;
                let previous = self.rand.seed;
                let seed = match args.first() {
                    Some(_) => self.arg_num(args, 0)?,
                    None => std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0.0, |elapsed| elapsed.as_secs() as f64),
                };
                self.rand.reseed(seed);
                Ok(Value::Num(previous))
            }
            Tolower | Toupper => {
                Self::check_args(builtin, args, 1, 1)?;
                let text = self.arg_str(args, 0)?;
                Ok(Value::str(if builtin == Tolower {
                    text.to_lowercase()
                } else {
                    text.to_uppercase()
                }))
            }
            System => {
                Self::check_args(builtin, args, 1, 1)?;
                let command = self.arg_str(args, 0)?;
                self.flush_all();
                let status = shell(&command)
                    .status()
                    .map(|status| status.code().unwrap_or(-1))
                    .unwrap_or(127);
                Ok(Value::Num(status as f64))
            }
            Close => {
                Self::check_args(builtin, args, 1, 1)?;
                let name = self.arg_str(args, 0)?;
                Ok(Value::Num(self.close(&name) as f64))
            }
            Fflush => {
                self.flush_all();
                Ok(Value::Num(0.0))
            }
            Systime => Ok(Value::Num(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0.0, |elapsed| elapsed.as_secs() as f64),
            )),
            Strftime => {
                Self::check_args(builtin, args, 0, 3)?;
                let format = match args.first() {
                    Some(_) => self.arg_str(args, 0)?,
                    None => Rc::from("%a %b %e %H:%M:%S %Z %Y"),
                };
                let seconds = match args.get(1) {
                    Some(_) => self.arg_num(args, 1)? as i64,
                    None => std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |elapsed| elapsed.as_secs() as i64),
                };
                let utc = match args.get(2) {
                    Some(_) => self.eval(&args[2])?.truthy(),
                    None => false,
                };
                Ok(Value::str(strftime(&format, seconds, utc)))
            }
            Asort | Asorti => {
                Self::check_args(builtin, args, 1, 2)?;
                let source = self.array_arg(builtin, args.first())?;
                let mut items: Vec<Value> = if builtin == Asort {
                    source.borrow().values().cloned().collect()
                } else {
                    source
                        .borrow()
                        .keys()
                        .map(|key| Value::StrNum(key.clone()))
                        .collect()
                };
                // Numbers first, in order, then strings in order.
                items.sort_by(|a, b| match (a.numeric(), b.numeric()) {
                    (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => self.to_str(a).cmp(&self.to_str(b)),
                });
                let target = match args.get(1) {
                    Some(expr) => self.array_arg(builtin, Some(expr))?,
                    None => source,
                };
                let mut target = target.borrow_mut();
                target.clear();
                let count = items.len();
                for (index, item) in items.into_iter().enumerate() {
                    target.insert(Rc::from((index + 1).to_string()), item);
                }
                Ok(Value::Num(count as f64))
            }
        }
    }

    // ---- input ---------------------------------------------------------------------------

    fn record_separator(&mut self) -> Result<RecordSep> {
        let rs = self.global_str(RS);
        let mut chars = rs.chars();
        Ok(match (chars.next(), chars.next()) {
            (None, _) => RecordSep::Paragraph,
            (Some('\n'), None) => RecordSep::Newline,
            (Some(c), None) if c.is_ascii() => RecordSep::Byte(c as u8),
            _ => RecordSep::Regex(self.dynamic_regex(&rs)?),
        })
    }

    fn argv(&self, index: usize) -> Option<Rc<str>> {
        match &self.globals[ARGV] {
            Cell::Array(array) => array
                .borrow()
                .get(index.to_string().as_str())
                .map(|value| self.to_str(value)),
            _ => None,
        }
    }

    /// The next record of the main input, moving through the files on the command line.
    fn next_main_record(&mut self) -> Result<Option<String>> {
        loop {
            if self.main.current.is_none() {
                let argc = match &self.globals[ARGC] {
                    Cell::Scalar(value) => value.to_num().max(0.0) as usize,
                    _ => 0,
                };
                while self.main.next_arg < argc && self.main.current.is_none() {
                    let index = self.main.next_arg;
                    self.main.next_arg += 1;
                    let Some(arg) = self.argv(index) else {
                        continue;
                    };
                    if arg.is_empty() {
                        continue;
                    }
                    if let Some((name, value)) = is_assignment(&arg) {
                        if let Err(message) = self.assign_global(name, value) {
                            return fatal(message);
                        }
                        continue;
                    }
                    let reader: Box<dyn BufRead> = if &*arg == "-" {
                        Box::new(io::stdin().lock())
                    } else {
                        match File::open(crate::sys::translate_path(&arg)) {
                            Ok(file) => Box::new(BufReader::with_capacity(64 * 1024, file)),
                            Err(error) => {
                                return fatal(format!(
                                    "can't open \"{arg}\": {}",
                                    crate::exec::describe_io_error(&error)
                                ))
                            }
                        }
                    };
                    self.globals[FILENAME] = Cell::Scalar(Value::Str(arg));
                    self.globals[FNR] = Cell::Scalar(Value::Num(0.0));
                    self.main.current = Some(Records::new(reader));
                    self.main.opened_any = true;
                }
                if self.main.current.is_none() {
                    if self.main.opened_any {
                        return Ok(None);
                    }
                    // No file named: read standard input, once.
                    self.main.opened_any = true;
                    self.main.current = Some(Records::new(Box::new(io::stdin().lock())));
                }
            }
            let separator = self.record_separator()?;
            let records = self.main.current.as_mut().expect("opened above");
            match records.next(&separator) {
                Ok(Some(record)) => return Ok(Some(record)),
                Ok(None) => self.main.current = None,
                Err(error) => return fatal(format!("read error: {error}")),
            }
        }
    }

    fn getline(&mut self, source: &GetlineSource, target: Option<&LValue>) -> Result<Value> {
        let record = match source {
            GetlineSource::Input => match self.next_main_record()? {
                Some(record) => {
                    self.bump(NR);
                    self.bump(FNR);
                    record
                }
                None => return Ok(Value::Num(0.0)),
            },
            GetlineSource::File(expr) | GetlineSource::Command(expr) => {
                let value = self.eval(expr)?;
                let name = self.to_str(&value);
                let is_command = matches!(source, GetlineSource::Command(_));
                if !self.inputs.contains_key(&*name) {
                    let input = if is_command {
                        self.flush_all();
                        let child = shell(&name)
                            .stdout(Stdio::piped())
                            .stdin(Stdio::inherit())
                            .spawn();
                        match child {
                            Ok(mut child) => {
                                let stdout = child.stdout.take().expect("piped");
                                Input::Command(
                                    child,
                                    Records::new(Box::new(BufReader::new(stdout))),
                                )
                            }
                            Err(_) => return Ok(Value::Num(-1.0)),
                        }
                    } else {
                        let reader: Box<dyn BufRead> = if &*name == "-" || &*name == "/dev/stdin" {
                            Box::new(io::stdin().lock())
                        } else {
                            match File::open(crate::sys::translate_path(&name)) {
                                Ok(file) => Box::new(BufReader::new(file)),
                                Err(_) => return Ok(Value::Num(-1.0)),
                            }
                        };
                        Input::File(Records::new(reader))
                    };
                    self.inputs.insert(name.to_string(), input);
                }
                let separator = self.record_separator()?;
                let records = match self.inputs.get_mut(&*name).expect("opened above") {
                    Input::File(records) | Input::Command(_, records) => records,
                };
                match records.next(&separator) {
                    Ok(Some(record)) => {
                        if is_command {
                            self.bump(NR);
                        }
                        record
                    }
                    Ok(None) => return Ok(Value::Num(0.0)),
                    Err(_) => return Ok(Value::Num(-1.0)),
                }
            }
        };
        match target {
            Some(target) => self.write(target, Value::strnum(record))?,
            None => self.set_record(record),
        }
        Ok(Value::Num(1.0))
    }

    // ---- output ----------------------------------------------------------------------

    fn write_out(&mut self, dest: Option<&(Redirect, Expr)>, text: &str) -> Result<()> {
        let Some((kind, expr)) = dest else {
            if let Err(error) = self.stdout.write_all(text.as_bytes()) {
                return Err(write_error(error));
            }
            if self.flush_each_line {
                let _ = self.stdout.flush();
            }
            return Ok(());
        };
        let value = self.eval(expr)?;
        let name = self.to_str(&value);
        if !self.outputs.contains_key(&*name) {
            let output = match (&*name, kind) {
                ("/dev/stdout" | "-", Redirect::File | Redirect::Append) => Output::Stdout,
                ("/dev/stderr", Redirect::File | Redirect::Append) => Output::Stderr,
                ("/dev/null", Redirect::File | Redirect::Append) => Output::Null,
                (_, Redirect::Pipe) => {
                    self.flush_all();
                    match shell(&name).stdin(Stdio::piped()).spawn() {
                        Ok(child) => Output::Pipe(child),
                        Err(error) => return fatal(format!("can't run \"{name}\": {error}")),
                    }
                }
                (path, _) => {
                    let mut options = OpenOptions::new();
                    if *kind == Redirect::Append {
                        options.append(true).create(true);
                    } else {
                        options.write(true).create(true).truncate(true);
                    }
                    match options.open(crate::sys::translate_path(path)) {
                        Ok(file) => Output::File(BufWriter::new(file)),
                        Err(error) => {
                            return fatal(format!(
                                "can't write to \"{name}\": {}",
                                crate::exec::describe_io_error(&error)
                            ))
                        }
                    }
                }
            };
            self.outputs.insert(name.to_string(), output);
        }
        let result = match self.outputs.get_mut(&*name).expect("opened above") {
            Output::Stdout => self.stdout.write_all(text.as_bytes()),
            Output::Stderr => {
                let _ = self.stdout.flush();
                io::stderr().write_all(text.as_bytes())
            }
            Output::Null => Ok(()),
            Output::File(file) => file.write_all(text.as_bytes()),
            Output::Pipe(child) => match child.stdin.as_mut() {
                Some(stdin) => stdin.write_all(text.as_bytes()),
                None => Ok(()),
            },
        };
        result.map_err(write_error)
    }

    pub fn flush_all(&mut self) {
        let _ = self.stdout.flush();
        for output in self.outputs.values_mut() {
            match output {
                Output::File(file) => {
                    let _ = file.flush();
                }
                Output::Pipe(child) => {
                    if let Some(stdin) = child.stdin.as_mut() {
                        let _ = stdin.flush();
                    }
                }
                _ => {}
            }
        }
    }

    /// `close(name)`: the exit status for a command, 0 for a file, -1 if nothing was open.
    fn close(&mut self, name: &str) -> i32 {
        let mut status = -1;
        if let Some(output) = self.outputs.remove(name) {
            status = finish_output(output);
        }
        if let Some(input) = self.inputs.remove(name) {
            status = match input {
                Input::File(_) => 0,
                Input::Command(mut child, records) => {
                    drop(records);
                    child
                        .wait()
                        .map_or(-1, |status| status.code().unwrap_or(-1))
                }
            };
        }
        status
    }

    pub fn finish(&mut self) {
        let _ = self.stdout.flush();
        let names: Vec<String> = self.outputs.keys().cloned().collect();
        for name in names {
            if let Some(output) = self.outputs.remove(&name) {
                finish_output(output);
            }
        }
        let names: Vec<String> = self.inputs.keys().cloned().collect();
        for name in names {
            self.close(&name);
        }
    }

    // ---- statements ------------------------------------------------------------------

    fn exec_block(&mut self, stmts: &[Stmt]) -> Result<Flow> {
        for stmt in stmts {
            let flow = self.exec(stmt)?;
            if !matches!(flow, Flow::Normal) {
                return Ok(flow);
            }
        }
        Ok(Flow::Normal)
    }

    fn print_args(&mut self, args: &[Expr]) -> Result<String> {
        if args.is_empty() {
            let mut text = String::from(&*self.record);
            text.push_str(&self.global_str(ORS));
            return Ok(text);
        }
        let separator = self.global_str(OFS);
        let mut text = String::new();
        for (index, arg) in args.iter().enumerate() {
            if index > 0 {
                text.push_str(&separator);
            }
            let value = self.eval(arg)?;
            text.push_str(&self.to_output(&value));
        }
        text.push_str(&self.global_str(ORS));
        Ok(text)
    }

    fn exec(&mut self, stmt: &Stmt) -> Result<Flow> {
        match stmt {
            Stmt::Expr(expr) => {
                self.eval(expr)?;
            }
            Stmt::Print { args, dest } => {
                let text = self.print_args(args)?;
                self.write_out(dest.as_ref(), &text)?;
            }
            Stmt::Printf { args, dest } => {
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(arg)?);
                }
                let format = self.to_str(&values[0]);
                let text = match value::sprintf(&format, &values[1..], &self.convfmt()) {
                    Ok(text) => text,
                    Err(message) => return fatal(format!("printf: {message}")),
                };
                self.write_out(dest.as_ref(), &text)?;
            }
            Stmt::If(cond, then, otherwise) => {
                if self.eval(cond)?.truthy() {
                    return self.exec(then);
                } else if let Some(otherwise) = otherwise {
                    return self.exec(otherwise);
                }
            }
            Stmt::While(cond, body) => {
                while self.eval(cond)?.truthy() {
                    match self.exec(body)? {
                        Flow::Break => break,
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                        Flow::Normal | Flow::Continue => {}
                    }
                }
            }
            Stmt::DoWhile(body, cond) => loop {
                match self.exec(body)? {
                    Flow::Break => break,
                    Flow::Return(value) => return Ok(Flow::Return(value)),
                    Flow::Normal | Flow::Continue => {}
                }
                if !self.eval(cond)?.truthy() {
                    break;
                }
            },
            Stmt::For {
                init,
                cond,
                step,
                body,
            } => {
                if let Some(init) = init {
                    self.exec(init)?;
                }
                loop {
                    if let Some(cond) = cond {
                        if !self.eval(cond)?.truthy() {
                            break;
                        }
                    }
                    match self.exec(body)? {
                        Flow::Break => break,
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                        Flow::Normal | Flow::Continue => {}
                    }
                    if let Some(step) = step {
                        self.exec(step)?;
                    }
                }
            }
            Stmt::ForIn(key, array, body) => {
                let array = self.array(*array)?;
                let keys: Vec<Rc<str>> = array.borrow().keys().cloned().collect();
                for name in keys {
                    // Elements deleted by the loop body are skipped.
                    if !array.borrow().contains_key(&name) {
                        continue;
                    }
                    self.set_var(*key, Value::StrNum(name))?;
                    match self.exec(body)? {
                        Flow::Break => break,
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                        Flow::Normal | Flow::Continue => {}
                    }
                }
            }
            Stmt::Block(stmts) => return self.exec_block(stmts),
            Stmt::Next => return Err(Unwind::Next),
            Stmt::NextFile => return Err(Unwind::NextFile),
            Stmt::Exit(code) => {
                if let Some(code) = code {
                    self.exit_code = self.eval(code)?.to_num() as i32;
                }
                return Err(Unwind::Exit);
            }
            Stmt::Return(value) => {
                let value = match value {
                    Some(expr) => self.eval(expr)?,
                    None => Value::Uninit,
                };
                return Ok(Flow::Return(value));
            }
            Stmt::Break => return Ok(Flow::Break),
            Stmt::Continue => return Ok(Flow::Continue),
            Stmt::Delete(var, subscripts) => {
                let array = self.array(*var)?;
                match subscripts {
                    Some(subscripts) => {
                        let key = self.subscript(subscripts)?;
                        array.borrow_mut().shift_remove(&key);
                    }
                    None => array.borrow_mut().clear(),
                }
            }
        }
        Ok(Flow::Normal)
    }

    fn run_action(&mut self, action: &Option<Vec<Stmt>>) -> Result<()> {
        match action {
            Some(stmts) => {
                self.exec_block(stmts)?;
            }
            None => {
                let text = self.print_args(&[])?;
                self.write_out(None, &text)?;
            }
        }
        Ok(())
    }

    fn run_rules(&mut self) -> Result<()> {
        let prog = self.prog;
        for (index, item) in prog.items.iter().enumerate() {
            let matched = match &item.pattern {
                Pattern::Begin | Pattern::End => continue,
                Pattern::All => true,
                Pattern::Expr(expr) => self.eval(expr)?.truthy(),
                Pattern::Range(start, end) => {
                    if self.ranges[index] {
                        if self.eval(end)?.truthy() {
                            self.ranges[index] = false;
                        }
                        true
                    } else if self.eval(start)?.truthy() {
                        if !self.eval(end)?.truthy() {
                            self.ranges[index] = true;
                        }
                        true
                    } else {
                        false
                    }
                }
            };
            if matched {
                self.run_action(&item.action)?;
            }
        }
        Ok(())
    }

    /// Runs BEGIN, the records, then END. Returns the exit status.
    pub fn run(&mut self) -> std::result::Result<i32, String> {
        let prog = self.prog;
        let begins: Vec<&Item> = prog
            .items
            .iter()
            .filter(|item| matches!(item.pattern, Pattern::Begin))
            .collect();
        let ends: Vec<&Item> = prog
            .items
            .iter()
            .filter(|item| matches!(item.pattern, Pattern::End))
            .collect();
        let reads_input = prog
            .items
            .iter()
            .any(|item| !matches!(item.pattern, Pattern::Begin));

        let mut exiting = false;
        for item in begins {
            match self.run_action(&item.action) {
                Ok(()) => {}
                Err(Unwind::Exit) => {
                    exiting = true;
                    break;
                }
                Err(Unwind::Next | Unwind::NextFile) => {
                    return Err("next can't be used in BEGIN".into())
                }
                Err(Unwind::Quit) => return Ok(self.exit_code),
                Err(Unwind::Fatal(message)) => return Err(message),
            }
        }
        if !exiting && reads_input {
            loop {
                let record = match self.next_main_record() {
                    Ok(Some(record)) => record,
                    Ok(None) => break,
                    Err(Unwind::Fatal(message)) => return Err(message),
                    Err(_) => break,
                };
                self.bump(NR);
                self.bump(FNR);
                self.set_record(record);
                match self.run_rules() {
                    Ok(()) | Err(Unwind::Next) => {}
                    Err(Unwind::NextFile) => self.main.current = None,
                    Err(Unwind::Exit) => break,
                    Err(Unwind::Quit) => return Ok(self.exit_code),
                    Err(Unwind::Fatal(message)) => return Err(message),
                }
            }
        }
        for item in ends {
            match self.run_action(&item.action) {
                Ok(()) => {}
                Err(Unwind::Exit) => break,
                Err(Unwind::Next | Unwind::NextFile) => {
                    return Err("next can't be used in END".into())
                }
                Err(Unwind::Quit) => return Ok(self.exit_code),
                Err(Unwind::Fatal(message)) => return Err(message),
            }
        }
        Ok(self.exit_code)
    }
}

fn write_error(error: io::Error) -> Unwind {
    if error.kind() == io::ErrorKind::BrokenPipe {
        Unwind::Quit
    } else {
        Unwind::Fatal(format!("write error: {error}"))
    }
}

fn finish_output(output: Output) -> i32 {
    match output {
        Output::File(mut file) => {
            if file.flush().is_ok() {
                0
            } else {
                -1
            }
        }
        Output::Pipe(mut child) => {
            drop(child.stdin.take());
            child
                .wait()
                .map_or(-1, |status| status.code().unwrap_or(-1))
        }
        Output::Stdout | Output::Stderr | Output::Null => 0,
    }
}

/// `sub` and `gsub`, with awk's rule for empty matches: one may not directly follow
/// a non-empty one.
fn replace(regex: &Regex, text: &str, replacement: &str, global: bool) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut count = 0;
    let mut position = 0;
    let mut previous_end: Option<usize> = None;
    while position <= text.len() {
        let Some(found) = regex.find_at(text, position) else {
            break;
        };
        if found.start() == found.end() {
            if previous_end == Some(found.start()) {
                // Right after the last match: step over one character instead.
                match text[found.start()..].chars().next() {
                    Some(c) => {
                        out.push(c);
                        position = found.start() + c.len_utf8();
                        continue;
                    }
                    None => break,
                }
            }
            out.push_str(&text[position..found.start()]);
            substitute(replacement, "", &mut out);
            count += 1;
            match text[found.start()..].chars().next() {
                Some(c) => {
                    out.push(c);
                    position = found.start() + c.len_utf8();
                }
                None => {
                    position = text.len() + 1;
                }
            }
            previous_end = None;
        } else {
            out.push_str(&text[position..found.start()]);
            substitute(replacement, found.as_str(), &mut out);
            count += 1;
            position = found.end();
            previous_end = Some(found.end());
        }
        if !global {
            break;
        }
    }
    if position <= text.len() {
        out.push_str(&text[position..]);
    }
    (out, count)
}

/// gawk's `strftime`, through jiff's strftime-style formatting.
fn strftime(format: &str, seconds: i64, utc: bool) -> String {
    let Ok(timestamp) = jiff::Timestamp::from_second(seconds) else {
        return String::new();
    };
    let zoned = if utc {
        timestamp.to_zoned(jiff::tz::TimeZone::UTC)
    } else {
        timestamp.to_zoned(jiff::tz::TimeZone::system())
    };
    jiff::fmt::strtime::format(format, &zoned).unwrap_or_default()
}

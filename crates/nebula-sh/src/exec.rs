//! Runs parsed command lines: expansion, redirections, pipelines, control flow,
//! functions and builtins.

use crate::arith;
use crate::commands;
use crate::expand::{self, Context};
use crate::parse::{
    self, CaseEnd, Command, Compound, Connector, Function, List, Pipeline, Redirect, RedirectKind,
    RedirectTarget, Simple, TestToken,
};
use crate::suggest;
use crate::sys;
use crate::test;
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

/// Deep enough for real scripts, shallow enough to stop runaway recursion
/// before the interpreter's stack runs out.
const MAX_CALL_DEPTH: usize = 1000;

pub enum Input {
    Inherit,
    File(File),
    Pipe(os_pipe::PipeReader),
}

pub enum Output {
    Stdout,
    Stderr,
    File(File),
    Pipe(os_pipe::PipeWriter),
    /// Collects the output of a builtin that runs inside a pipeline.
    Memory(Arc<Mutex<Vec<u8>>>),
}

impl Input {
    pub fn try_clone(&self) -> io::Result<Input> {
        Ok(match self {
            Input::Inherit => Input::Inherit,
            Input::File(file) => Input::File(file.try_clone()?),
            Input::Pipe(pipe) => Input::Pipe(pipe.try_clone()?),
        })
    }

    fn into_stdio(self) -> Stdio {
        match self {
            Input::Inherit => Stdio::inherit(),
            Input::File(file) => file.into(),
            Input::Pipe(pipe) => pipe.into(),
        }
    }

    /// A reader for builtins like `read`. Reads must go one byte at a time so that
    /// whatever follows the line stays available to the next command.
    pub fn reader(&self) -> io::Result<Box<dyn Read>> {
        Ok(match self {
            Input::Inherit => {
                #[cfg(windows)]
                let handle = {
                    use std::os::windows::io::AsHandle;
                    io::stdin().as_handle().try_clone_to_owned()?
                };
                #[cfg(not(windows))]
                let handle = {
                    use std::os::fd::AsFd;
                    io::stdin().as_fd().try_clone_to_owned()?
                };
                Box::new(File::from(handle))
            }
            Input::File(file) => Box::new(file.try_clone()?),
            Input::Pipe(pipe) => Box::new(pipe.try_clone()?),
        })
    }

    pub fn is_terminal(&self) -> bool {
        use std::io::IsTerminal;
        matches!(self, Input::Inherit) && io::stdin().is_terminal()
    }
}

struct MemoryWriter(Arc<Mutex<Vec<u8>>>);

impl Write for MemoryWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if let Ok(mut buffer) = self.0.lock() {
            buffer.extend_from_slice(data);
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Output {
    pub fn try_clone(&self) -> io::Result<Output> {
        Ok(match self {
            Output::Stdout => Output::Stdout,
            Output::Stderr => Output::Stderr,
            Output::File(file) => Output::File(file.try_clone()?),
            Output::Pipe(pipe) => Output::Pipe(pipe.try_clone()?),
            Output::Memory(buffer) => Output::Memory(buffer.clone()),
        })
    }

    fn into_stdio(self) -> Stdio {
        match self {
            Output::Stdout => Stdio::inherit(),
            Output::Stderr => io::stderr().into(),
            Output::File(file) => file.into(),
            Output::Pipe(pipe) => pipe.into(),
            Output::Memory(_) => Stdio::null(),
        }
    }

    fn into_stdio_for_stderr(self) -> Stdio {
        match self {
            Output::Stdout => io::stdout().into(),
            Output::Stderr => Stdio::inherit(),
            other => other.into_stdio(),
        }
    }

    fn into_writer(self) -> Box<dyn Write + Send> {
        match self {
            Output::Stdout => Box::new(io::stdout()),
            Output::Stderr => Box::new(io::stderr()),
            Output::File(file) => Box::new(file),
            Output::Pipe(pipe) => Box::new(pipe),
            Output::Memory(buffer) => Box::new(MemoryWriter(buffer)),
        }
    }

    /// A writer on a copy of this output.
    pub fn writer(&self) -> Box<dyn Write + Send> {
        match self.try_clone() {
            Ok(output) => output.into_writer(),
            Err(_) => Box::new(io::sink()),
        }
    }
}

/// The standard input, output and error of a command.
pub struct Io {
    pub stdin: Input,
    pub stdout: Output,
    pub stderr: Output,
}

impl Io {
    pub fn standard() -> Io {
        Io {
            stdin: Input::Inherit,
            stdout: Output::Stdout,
            stderr: Output::Stderr,
        }
    }

    pub fn try_clone(&self) -> io::Result<Io> {
        Ok(Io {
            stdin: self.stdin.try_clone()?,
            stdout: self.stdout.try_clone()?,
            stderr: self.stderr.try_clone()?,
        })
    }
}

enum Running {
    Child(Child),
    Thread(JoinHandle<i32>),
    Done(i32),
}

impl Running {
    fn wait(self) -> i32 {
        match self {
            Running::Child(mut child) => child.wait().map(|status| exit_code(&status)).unwrap_or(1),
            Running::Thread(handle) => handle.join().unwrap_or(1),
            Running::Done(code) => code,
        }
    }
}

fn exit_code(status: &std::process::ExitStatus) -> i32 {
    match status.code() {
        // STATUS_CONTROL_C_EXIT: the program was stopped with Ctrl+C.
        Some(code) if code as u32 == 0xC000_013A => 130,
        Some(code) => code,
        None => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                128 + status.signal().unwrap_or(0)
            }
            #[cfg(not(unix))]
            1
        }
    }
}

/// How a command name resolves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Alias(String),
    Function(Rc<Function>),
    Builtin,
    Util,
    External(PathBuf),
    Missing,
}

/// `set -e`, `set -u`, `set -x` and `set -o pipefail`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub errexit: bool,
    pub nounset: bool,
    pub xtrace: bool,
    pub pipefail: bool,
}

/// `break`, `continue` and `return` unwind through the lists that contain them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Break(usize),
    Continue(usize),
    Return,
}

/// What a command substitution must leave untouched.
struct SavedState {
    cwd: Option<PathBuf>,
    vars: Vec<(OsString, OsString)>,
    aliases: BTreeMap<String, String>,
    functions: BTreeMap<String, Rc<Function>>,
    positional: Vec<String>,
    options: Options,
    exit_code: Option<i32>,
    flow: Option<Flow>,
    dir_stack: Vec<PathBuf>,
}

pub struct Shell {
    pub last_status: i32,
    pub aliases: BTreeMap<String, String>,
    pub functions: BTreeMap<String, Rc<Function>>,
    pub history: Vec<String>,
    pub exit_code: Option<i32>,
    pub interrupted: Arc<AtomicBool>,
    pub positional: Vec<String>,
    /// `$0`.
    pub script_name: String,
    pub options: Options,
    pub interactive: bool,
    pub flow: Option<Flow>,
    pub loop_depth: usize,
    dir_stack: Vec<PathBuf>,
    exe: PathBuf,
    expanding_aliases: Vec<String>,
    local_frames: Vec<Vec<(String, Option<OsString>)>>,
    call_depth: usize,
    condition_depth: usize,
    expansion_failed: bool,
    subst_status: Option<i32>,
    /// Standard input for `$(…)` while a command's words are expanded.
    subst_stdin: Option<Input>,
    started: Instant,
    random: u64,
}

impl Shell {
    pub fn new() -> Self {
        let exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("nebula-sh"));
        let mut aliases = BTreeMap::new();
        aliases.insert("ll".to_owned(), "ls -alh".to_owned());
        aliases.insert("la".to_owned(), "ls -A".to_owned());
        aliases.insert("..".to_owned(), "cd ..".to_owned());
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1)
            ^ u64::from(std::process::id());
        Self {
            last_status: 0,
            aliases,
            functions: BTreeMap::new(),
            history: Vec::new(),
            exit_code: None,
            interrupted: Arc::new(AtomicBool::new(false)),
            positional: Vec::new(),
            script_name: "nebula".to_owned(),
            options: Options::default(),
            interactive: false,
            flow: None,
            loop_depth: 0,
            dir_stack: Vec::new(),
            exe,
            expanding_aliases: Vec::new(),
            local_frames: Vec::new(),
            call_depth: 0,
            condition_depth: 0,
            expansion_failed: false,
            subst_status: None,
            subst_stdin: None,
            started: Instant::now(),
            random: seed | 1,
        }
    }

    pub fn resolve(&self, name: &str) -> Resolution {
        if !self.expanding_aliases.iter().any(|alias| alias == name) {
            if let Some(value) = self.aliases.get(name) {
                return Resolution::Alias(value.clone());
            }
        }
        if let Some(function) = self.functions.get(name) {
            return Resolution::Function(function.clone());
        }
        self.resolve_command(name)
    }

    /// Resolution that skips aliases and functions, for the `command` builtin.
    pub fn resolve_command(&self, name: &str) -> Resolution {
        if commands::is_builtin(name) {
            return Resolution::Builtin;
        }
        if commands::is_util(name) {
            return Resolution::Util;
        }
        match sys::find_executable(name) {
            Some(path) => Resolution::External(path),
            None => Resolution::Missing,
        }
    }

    fn interrupted(&self) -> bool {
        self.interrupted.load(Ordering::SeqCst)
    }

    /// True once `exit`, `break`, `continue`, `return` or Ctrl+C stops the current list.
    fn stopped(&self) -> bool {
        self.exit_code.is_some() || self.flow.is_some() || self.interrupted()
    }

    /// Runs one line (or a whole script) from the top level and returns its exit status.
    pub fn run_line(&mut self, line: &str) -> i32 {
        self.interrupted.store(false, Ordering::SeqCst);
        match parse::parse(line) {
            Ok(list) => {
                let status = self.run_list(&list, &Io::standard());
                self.flow = None;
                self.last_status = status;
                status
            }
            Err(error) => {
                eprintln!("nebula: {error}");
                self.last_status = 2;
                2
            }
        }
    }

    /// Runs code inside the current shell with the given I/O, for `source`, `eval` and aliases.
    pub fn run_source(&mut self, source: &str, io: &Io, context: &str) -> i32 {
        match parse::parse(source) {
            Ok(list) => self.run_list(&list, io),
            Err(error) => {
                let _ = writeln!(io.stderr.writer(), "nebula: {context}{error}");
                2
            }
        }
    }

    pub fn run_list(&mut self, list: &List, io: &Io) -> i32 {
        let mut status = 0;
        for (index, (connector, pipeline)) in list.0.iter().enumerate() {
            if index > 0 {
                let run = match connector {
                    Connector::And => status == 0,
                    Connector::Or => status != 0,
                    Connector::Seq => true,
                };
                if !run {
                    continue;
                }
            }
            if self.stopped() {
                if self.interrupted() && self.exit_code.is_none() && self.flow.is_none() {
                    status = 130;
                }
                break;
            }
            // The left side of `&&` / `||` may fail without triggering `set -e`.
            let guarded = matches!(
                list.0.get(index + 1),
                Some((Connector::And | Connector::Or, _))
            );
            if guarded {
                self.condition_depth += 1;
            }
            status = self.run_pipeline(pipeline, io);
            if guarded {
                self.condition_depth -= 1;
            }
            self.last_status = status;
            if status != 0
                && self.options.errexit
                && self.condition_depth == 0
                && !guarded
                && !pipeline.negate
                && !self.stopped()
            {
                self.exit_code = Some(status);
            }
        }
        status
    }

    fn run_pipeline(&mut self, pipeline: &Pipeline, io: &Io) -> i32 {
        let status = if pipeline.commands.len() == 1 {
            self.run_command(&pipeline.commands[0], io)
        } else {
            self.run_stages(pipeline, io)
        };
        if pipeline.negate {
            i32::from(status == 0)
        } else {
            status
        }
    }

    fn run_stages(&mut self, pipeline: &Pipeline, io: &Io) -> i32 {
        let count = pipeline.commands.len();
        let mut running = Vec::with_capacity(count);
        let mut previous: Option<os_pipe::PipeReader> = None;
        let mut failed = false;

        for (index, command) in pipeline.commands.iter().enumerate() {
            let last = index + 1 == count;
            let stdin = match previous.take() {
                Some(reader) => Input::Pipe(reader),
                None => io.stdin.try_clone().unwrap_or(Input::Inherit),
            };
            let stdout = if last {
                io.stdout.try_clone().unwrap_or(Output::Stdout)
            } else {
                match os_pipe::pipe() {
                    Ok((reader, writer)) => {
                        previous = Some(reader);
                        Output::Pipe(writer)
                    }
                    Err(error) => {
                        eprintln!("nebula: cannot create pipe: {error}");
                        failed = true;
                        break;
                    }
                }
            };
            let stderr = io.stderr.try_clone().unwrap_or(Output::Stderr);
            let source = pipeline.sources.get(index).map_or("", String::as_str);
            running.push(self.start_stage(
                command,
                source,
                Io {
                    stdin,
                    stdout,
                    stderr,
                },
            ));
        }
        drop(previous);

        let statuses: Vec<i32> = running.into_iter().map(Running::wait).collect();
        if failed {
            return 1;
        }
        if self.options.pipefail {
            statuses
                .iter()
                .rev()
                .find(|s| **s != 0)
                .copied()
                .unwrap_or(0)
        } else {
            statuses.last().copied().unwrap_or(0)
        }
    }

    /// Starts one stage of a multi-command pipeline. Everything that would need the
    /// shell's own state while running concurrently goes to a sub-shell process.
    fn start_stage(&mut self, command: &Command, source: &str, io: Io) -> Running {
        match command {
            Command::Simple(simple) => self.start_simple(simple, io, true),
            Command::Compound { .. } => self.spawn_subshell(source, io),
            Command::Function(function) => {
                self.define(function);
                Running::Done(0)
            }
        }
    }

    /// Runs a command in the foreground, inside this shell.
    fn run_command(&mut self, command: &Command, io: &Io) -> i32 {
        match command {
            Command::Simple(simple) => match io.try_clone() {
                Ok(io) => self.start_simple(simple, io, false).wait(),
                Err(error) => {
                    eprintln!("nebula: {error}");
                    1
                }
            },
            Command::Compound { body, redirects } => {
                let io = match io
                    .try_clone()
                    .map_err(|error| error.to_string())
                    .and_then(|io| self.apply_redirects(redirects, io))
                {
                    Ok(io) => io,
                    Err(message) => {
                        eprintln!("nebula: {message}");
                        return 1;
                    }
                };
                self.run_compound(body, &io)
            }
            Command::Function(function) => {
                self.define(function);
                0
            }
        }
    }

    pub fn define(&mut self, function: &Rc<Function>) {
        self.functions
            .insert(function.name.clone(), Rc::clone(function));
    }

    /// Runs a list as an `if` / `while` condition, where failures don't trigger `set -e`.
    fn condition(&mut self, list: &List, io: &Io) -> i32 {
        self.condition_depth += 1;
        let status = self.run_list(list, io);
        self.condition_depth -= 1;
        status
    }

    /// After a loop body: handles `break` / `continue` and says whether to leave the loop.
    fn leave_loop(&mut self) -> bool {
        match self.flow {
            Some(Flow::Break(levels)) => {
                self.flow = (levels > 1).then_some(Flow::Break(levels - 1));
                true
            }
            Some(Flow::Continue(levels)) => {
                if levels > 1 {
                    self.flow = Some(Flow::Continue(levels - 1));
                    true
                } else {
                    self.flow = None;
                    false
                }
            }
            Some(Flow::Return) => true,
            None => self.exit_code.is_some() || self.interrupted(),
        }
    }

    fn run_compound(&mut self, compound: &Compound, io: &Io) -> i32 {
        match compound {
            Compound::Group(list) => self.run_list(list, io),
            Compound::Subshell { source, .. } => match io.try_clone() {
                Ok(io) => self.spawn_subshell(source, io).wait(),
                Err(error) => {
                    eprintln!("nebula: {error}");
                    1
                }
            },
            Compound::If {
                branches,
                otherwise,
            } => {
                for (test, body) in branches {
                    let status = self.condition(test, io);
                    if self.stopped() {
                        return status;
                    }
                    if status == 0 {
                        return self.run_list(body, io);
                    }
                }
                match otherwise {
                    Some(list) => self.run_list(list, io),
                    None => 0,
                }
            }
            Compound::Loop { until, test, body } => {
                let mut status = 0;
                self.loop_depth += 1;
                loop {
                    let result = self.condition(test, io);
                    if self.leave_loop() || (result == 0) == *until {
                        break;
                    }
                    status = self.run_list(body, io);
                    if self.leave_loop() {
                        break;
                    }
                }
                self.loop_depth -= 1;
                self.loop_status(status)
            }
            Compound::For { var, items, body } => {
                self.expansion_failed = false;
                let values: Vec<String> = match items {
                    Some(words) => {
                        let mut values = Vec::new();
                        for word in words {
                            values.extend(expand::expand(word, self, true));
                        }
                        values
                    }
                    None => self.positional.clone(),
                };
                if self.expansion_failed {
                    return 1;
                }
                let mut status = 0;
                self.loop_depth += 1;
                for value in values {
                    env::set_var(var, value);
                    status = self.run_list(body, io);
                    if self.leave_loop() {
                        break;
                    }
                }
                self.loop_depth -= 1;
                self.loop_status(status)
            }
            Compound::ArithFor {
                init,
                test,
                step,
                body,
            } => {
                if self.arith(init).is_none() {
                    return 1;
                }
                let mut status = 0;
                self.loop_depth += 1;
                loop {
                    if !test.trim().is_empty() {
                        match self.arith(test) {
                            Some(0) => break,
                            Some(_) => {}
                            None => {
                                status = 1;
                                break;
                            }
                        }
                    }
                    status = self.run_list(body, io);
                    if self.leave_loop() {
                        break;
                    }
                    if self.arith(step).is_none() {
                        status = 1;
                        break;
                    }
                }
                self.loop_depth -= 1;
                self.loop_status(status)
            }
            Compound::Case { word, arms } => {
                let value = expand::expand_single(word, self);
                let mut status = 0;
                let mut run_next = false;
                for arm in arms {
                    let hit = run_next
                        || arm.patterns.iter().any(|pattern| {
                            let pattern = expand::pattern(pattern, self);
                            expand::matches(&pattern, &value)
                        });
                    if !hit {
                        continue;
                    }
                    status = self.run_list(&arm.body, io);
                    if self.stopped() {
                        break;
                    }
                    match arm.end {
                        CaseEnd::Break => break,
                        CaseEnd::FallThrough => run_next = true,
                        CaseEnd::Continue => run_next = false,
                    }
                }
                status
            }
            Compound::Arith(source) => match self.arith(source) {
                Some(value) => i32::from(value == 0),
                None => 1,
            },
            Compound::Test(tokens) => self.double_bracket(tokens),
        }
    }

    fn loop_status(&self, status: i32) -> i32 {
        if self.interrupted() && self.exit_code.is_none() {
            130
        } else {
            status
        }
    }

    fn double_bracket(&mut self, tokens: &[TestToken]) -> i32 {
        self.expansion_failed = false;
        let mut args = Vec::with_capacity(tokens.len());
        for token in tokens {
            args.push(match token {
                TestToken::Word(word) => {
                    let (text, pattern) = expand::single_with_pattern(word, self);
                    test::Arg::Value { text, pattern }
                }
                TestToken::And => test::Arg::And,
                TestToken::Or => test::Arg::Or,
                TestToken::Not => test::Arg::Not,
                TestToken::Open => test::Arg::Open,
                TestToken::Close => test::Arg::Close,
                TestToken::Less => test::Arg::Text("<".into()),
                TestToken::Greater => test::Arg::Text(">".into()),
            });
        }
        if self.expansion_failed {
            return 1;
        }
        match test::evaluate(&args, true, &|name| env::var_os(name).is_some()) {
            Ok(true) => 0,
            Ok(false) => 1,
            Err(message) => {
                eprintln!("nebula: [[: {message}");
                2
            }
        }
    }

    fn start_simple(&mut self, simple: &Simple, io: Io, stage: bool) -> Running {
        self.expansion_failed = false;
        self.subst_status = None;
        let outer_stdin = std::mem::replace(&mut self.subst_stdin, io.stdin.try_clone().ok());
        let mut argv: Vec<String> = Vec::new();
        for word in &simple.words {
            argv.extend(expand::expand(word, self, true));
        }
        let assignments: Vec<(String, String)> = simple
            .assignments
            .iter()
            .map(|(name, value)| (name.clone(), expand::expand_single(value, self)))
            .collect();
        self.subst_stdin = outer_stdin;
        if self.expansion_failed {
            return Running::Done(1);
        }

        let io = match self.apply_redirects(&simple.redirects, io) {
            Ok(io) => io,
            Err(message) => {
                eprintln!("nebula: {message}");
                return Running::Done(1);
            }
        };

        if argv.is_empty() {
            for (name, value) in assignments {
                env::set_var(name, value);
            }
            return Running::Done(self.subst_status.unwrap_or(0));
        }
        if self.options.xtrace {
            let words: Vec<String> = assignments
                .iter()
                .map(|(name, value)| format!("{name}={}", parse::quote(value)))
                .chain(argv.iter().map(|arg| parse::quote(arg)))
                .collect();
            let _ = writeln!(io.stderr.writer(), "+ {}", words.join(" "));
        }
        self.dispatch(argv, assignments, io, stage, false)
    }

    /// Runs an expanded command line.
    fn dispatch(
        &mut self,
        argv: Vec<String>,
        assignments: Vec<(String, String)>,
        io: Io,
        stage: bool,
        bypass: bool,
    ) -> Running {
        let name = argv[0].clone();
        let resolution = if bypass {
            self.resolve_command(&name)
        } else {
            self.resolve(&name)
        };
        match resolution {
            Resolution::Alias(_) | Resolution::Function(_) if stage => {
                self.spawn_subshell(&quote_argv(&argv), io)
            }
            Resolution::Alias(value) => {
                let rest: Vec<String> = argv[1..].iter().map(|arg| parse::quote(arg)).collect();
                let line = if rest.is_empty() {
                    value
                } else {
                    format!("{value} {}", rest.join(" "))
                };
                self.expanding_aliases.push(name.clone());
                let status = self.run_source(&line, &io, &format!("alias {name}: "));
                self.expanding_aliases.pop();
                Running::Done(status)
            }
            Resolution::Function(function) => {
                let saved = self.set_temporarily(&assignments);
                let status = self.call_function(&function, argv[1..].to_vec(), &io);
                restore_vars(saved);
                Running::Done(status)
            }
            Resolution::Builtin if stage && commands::runs_code(&name) => {
                self.spawn_subshell(&quote_argv(&argv), io)
            }
            Resolution::Builtin if stage => self.buffered_builtin(&argv, &assignments, io),
            Resolution::Builtin => {
                let saved = self.set_temporarily(&assignments);
                let status = self.builtin(&argv, &io);
                restore_vars(saved);
                Running::Done(status)
            }
            Resolution::Util => {
                let args: Vec<String> = argv[1..]
                    .iter()
                    .map(|arg| {
                        if arg.starts_with('-') {
                            arg.clone()
                        } else {
                            sys::translate_path(arg)
                        }
                    })
                    .collect();
                let mut process = std::process::Command::new(&self.exe);
                process.arg(&name).args(args);
                spawn(process, &name, assignments, io)
            }
            Resolution::External(path) => {
                let mut process = self.external_command(&path);
                process.args(&argv[1..]);
                spawn(process, &name, assignments, io)
            }
            Resolution::Missing => {
                let message = suggest::not_found(&name, self);
                let _ = writeln!(io.stderr.writer(), "{message}");
                Running::Done(127)
            }
        }
    }

    /// Runs `argv` as a command, skipping aliases and functions (the `command` builtin).
    pub fn run_bypassing_functions(&mut self, argv: Vec<String>, io: &Io) -> i32 {
        match io.try_clone() {
            Ok(io) => self.dispatch(argv, Vec::new(), io, false, true).wait(),
            Err(error) => {
                eprintln!("nebula: {error}");
                1
            }
        }
    }

    /// A builtin inside a pipeline: its output is collected first and written from a
    /// thread, so the next stage can start reading without a deadlock.
    fn buffered_builtin(
        &mut self,
        argv: &[String],
        assignments: &[(String, String)],
        io: Io,
    ) -> Running {
        let Io {
            stdin,
            stdout,
            stderr,
        } = io;
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let stage = Io {
            stdin,
            stdout: Output::Memory(buffer.clone()),
            stderr,
        };
        let saved = self.set_temporarily(assignments);
        let status = self.builtin(argv, &stage);
        restore_vars(saved);
        drop(stage);
        let bytes = buffer
            .lock()
            .map(|mut buffer| std::mem::take(&mut *buffer))
            .unwrap_or_default();
        let mut out = stdout.into_writer();
        Running::Thread(thread::spawn(move || {
            let _ = out.write_all(&bytes);
            let _ = out.flush();
            status
        }))
    }

    fn builtin(&mut self, argv: &[String], io: &Io) -> i32 {
        crate::builtins::run(self, argv, io)
    }

    pub fn call_function(&mut self, function: &Function, args: Vec<String>, io: &Io) -> i32 {
        if self.call_depth >= MAX_CALL_DEPTH {
            eprintln!(
                "nebula: {}: maximum function nesting level exceeded ({MAX_CALL_DEPTH})",
                function.name
            );
            return 1;
        }
        let saved_args = std::mem::replace(&mut self.positional, args);
        let saved_loops = std::mem::replace(&mut self.loop_depth, 0);
        self.local_frames.push(Vec::new());
        self.call_depth += 1;
        let status = self.run_command(&function.body, io);
        self.call_depth -= 1;
        if let Some(frame) = self.local_frames.pop() {
            restore_vars(frame);
        }
        self.loop_depth = saved_loops;
        self.positional = saved_args;
        // `return` ends here; a stray `break` or `continue` does not leak to the caller.
        if self.flow.is_some() {
            self.flow = None;
        }
        status
    }

    /// `local name[=value]`: the previous value comes back when the function returns.
    pub fn declare_local(&mut self, name: &str, value: Option<&str>) -> Result<(), String> {
        let Some(frame) = self.local_frames.last_mut() else {
            return Err("can only be used in a function".to_owned());
        };
        if !frame.iter().any(|(saved, _)| saved == name) {
            frame.push((name.to_owned(), env::var_os(name)));
        }
        match value {
            Some(value) => env::set_var(name, value),
            None => env::remove_var(name),
        }
        Ok(())
    }

    pub fn in_function(&self) -> bool {
        !self.local_frames.is_empty()
    }

    /// `NAME=value command`: the variables only last for that command.
    fn set_temporarily(
        &mut self,
        assignments: &[(String, String)],
    ) -> Vec<(String, Option<OsString>)> {
        assignments
            .iter()
            .map(|(name, value)| {
                let previous = env::var_os(name);
                env::set_var(name, value);
                (name.clone(), previous)
            })
            .collect()
    }

    /// Runs `body` in a separate `nebula-sh` process that starts with this shell's
    /// functions, aliases, options and arguments.
    fn spawn_subshell(&mut self, body: &str, io: Io) -> Running {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let path = env::temp_dir().join(format!(
            "nebula-sub-{}-{}.sh",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let script = format!("{}{body}\n", self.prelude());
        if let Err(error) = std::fs::write(&path, script) {
            eprintln!("nebula: cannot start a sub-shell: {error}");
            return Running::Done(1);
        }
        let mut process = std::process::Command::new(&self.exe);
        process
            .arg("--subshell")
            .arg(&path)
            .arg(self.last_status.to_string())
            .arg(&self.script_name);
        let running = spawn(process, "sub-shell", Vec::new(), io);
        if matches!(running, Running::Done(_)) {
            let _ = std::fs::remove_file(&path);
        }
        running
    }

    /// The state a sub-shell inherits, as Nebula code. Variables are environment
    /// variables, so the process inherits those by itself.
    fn prelude(&self) -> String {
        let mut text = String::new();
        for (name, value) in &self.aliases {
            text.push_str(&format!(
                "alias {}\n",
                parse::quote(&format!("{name}={value}"))
            ));
        }
        for function in self.functions.values() {
            text.push_str(&function.source);
            text.push('\n');
        }
        let flags = self.option_flags();
        if !flags.is_empty() {
            text.push_str(&format!("set -{flags}\n"));
        }
        if self.options.pipefail {
            text.push_str("set -o pipefail\n");
        }
        text.push_str("set --");
        for arg in &self.positional {
            text.push(' ');
            text.push_str(&parse::quote(arg));
        }
        text.push('\n');
        text
    }

    fn option_flags(&self) -> String {
        let mut flags = String::new();
        if self.options.errexit {
            flags.push('e');
        }
        if self.options.nounset {
            flags.push('u');
        }
        if self.options.xtrace {
            flags.push('x');
        }
        flags
    }

    fn apply_redirects(&mut self, redirects: &[Redirect], mut io: Io) -> Result<Io, String> {
        for redirect in redirects {
            match &redirect.target {
                RedirectTarget::File(word) => {
                    let target = sys::translate_path(&expand::expand_single(word, self));
                    let path = Path::new(&target);
                    match redirect.kind {
                        RedirectKind::Read => {
                            let file = File::open(path).map_err(|error| {
                                format!("{target}: {}", describe_io_error(&error))
                            })?;
                            io.stdin = Input::File(file);
                        }
                        RedirectKind::Write | RedirectKind::Append => {
                            let file = OpenOptions::new()
                                .write(true)
                                .create(true)
                                .append(redirect.kind == RedirectKind::Append)
                                .truncate(redirect.kind == RedirectKind::Write)
                                .open(path)
                                .map_err(|error| {
                                    format!("{target}: {}", describe_io_error(&error))
                                })?;
                            if redirect.fd == 2 {
                                io.stderr = Output::File(file);
                            } else {
                                io.stdout = Output::File(file);
                            }
                        }
                    }
                }
                RedirectTarget::Fd(target) => {
                    let source = match target {
                        1 => io.stdout.try_clone(),
                        2 => io.stderr.try_clone(),
                        other => return Err(format!("bad file descriptor {other}")),
                    }
                    .map_err(|error| error.to_string())?;
                    match redirect.fd {
                        1 => io.stdout = source,
                        2 => io.stderr = source,
                        other => return Err(format!("cannot redirect file descriptor {other}")),
                    }
                }
                RedirectTarget::Text(word) => {
                    let text = expand::expand_single(word, self);
                    io.stdin = Input::Pipe(text_pipe(text).map_err(|error| error.to_string())?);
                }
            }
        }
        Ok(io)
    }

    /// Builds a process for an external program, routing scripts through their interpreter.
    fn external_command(&self, path: &Path) -> std::process::Command {
        let extension = path
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase());
        match extension.as_deref() {
            Some("ps1") => {
                let host = if sys::find_executable("pwsh").is_some() {
                    "pwsh"
                } else {
                    "powershell"
                };
                let mut command = std::process::Command::new(host);
                command
                    .args([
                        "-NoLogo",
                        "-NoProfile",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-File",
                    ])
                    .arg(path);
                command
            }
            // Shell scripts have no runner on Windows; Nebula runs them itself.
            Some("sh") if cfg!(windows) => {
                let mut command = std::process::Command::new(&self.exe);
                command.arg(path);
                command
            }
            _ => std::process::Command::new(path),
        }
    }

    pub fn change_dir(&mut self, target: &Path) -> Result<(), String> {
        let previous = env::current_dir().ok();
        env::set_current_dir(target).map_err(|error| describe_io_error(&error))?;
        if let Some(previous) = previous {
            env::set_var("OLDPWD", previous);
        }
        if let Ok(current) = env::current_dir() {
            env::set_var("PWD", current);
        }
        Ok(())
    }

    pub fn dir_stack(&mut self) -> &mut Vec<PathBuf> {
        &mut self.dir_stack
    }

    fn save_state(&self) -> SavedState {
        SavedState {
            cwd: env::current_dir().ok(),
            vars: env::vars_os().collect(),
            aliases: self.aliases.clone(),
            functions: self.functions.clone(),
            positional: self.positional.clone(),
            options: self.options,
            exit_code: self.exit_code,
            flow: self.flow,
            dir_stack: self.dir_stack.clone(),
        }
    }

    fn restore_state(&mut self, saved: SavedState) {
        if let Some(cwd) = saved.cwd {
            if env::current_dir().ok().as_ref() != Some(&cwd) {
                let _ = env::set_current_dir(cwd);
            }
        }
        let wanted: BTreeMap<OsString, OsString> = saved.vars.into_iter().collect();
        for (name, value) in env::vars_os() {
            match wanted.get(&name) {
                Some(old) if *old == value => {}
                Some(old) => env::set_var(&name, old),
                None => env::remove_var(&name),
            }
        }
        for (name, value) in &wanted {
            if env::var_os(name).is_none() {
                env::set_var(name, value);
            }
        }
        self.aliases = saved.aliases;
        self.functions = saved.functions;
        self.positional = saved.positional;
        self.options = saved.options;
        self.exit_code = saved.exit_code;
        self.flow = saved.flow;
        self.dir_stack = saved.dir_stack;
    }

    /// Runs `source` like a sub-shell would and returns what it printed on stdout.
    /// Changes it makes to the directory, variables or functions are undone afterwards.
    pub fn capture(&mut self, source: &str) -> String {
        let list = match parse::parse(source) {
            Ok(list) => list,
            Err(error) => {
                eprintln!("nebula: {error}");
                self.subst_status = Some(2);
                return String::new();
            }
        };
        let Ok((mut reader, writer)) = os_pipe::pipe() else {
            return String::new();
        };
        let collector = thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = reader.read_to_end(&mut bytes);
            bytes
        });
        let saved = self.save_state();
        let failed = self.expansion_failed;
        let stdin = self
            .subst_stdin
            .as_ref()
            .and_then(|input| input.try_clone().ok())
            .unwrap_or(Input::Inherit);
        let io = Io {
            stdin,
            stdout: Output::Pipe(writer),
            stderr: Output::Stderr,
        };
        let status = self.run_list(&list, &io);
        drop(io);
        let status = self.exit_code.unwrap_or(status);
        self.restore_state(saved);
        self.expansion_failed = failed;
        let bytes = collector.join().unwrap_or_default();
        self.subst_status = Some(status);
        self.last_status = status;
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn next_random(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.random;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.random = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

fn restore_vars(saved: Vec<(String, Option<OsString>)>) {
    for (name, value) in saved.into_iter().rev() {
        match value {
            Some(value) => env::set_var(name, value),
            None => env::remove_var(name),
        }
    }
}

fn quote_argv(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| parse::quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A pipe whose other end receives `text` (here-documents and here-strings).
fn text_pipe(text: String) -> io::Result<os_pipe::PipeReader> {
    let (reader, mut writer) = os_pipe::pipe()?;
    thread::spawn(move || {
        let _ = writer.write_all(text.as_bytes());
    });
    Ok(reader)
}

fn spawn(
    mut process: std::process::Command,
    name: &str,
    assignments: Vec<(String, String)>,
    io: Io,
) -> Running {
    process
        .envs(assignments)
        .stdin(io.stdin.into_stdio())
        .stdout(io.stdout.into_stdio())
        .stderr(io.stderr.into_stdio_for_stderr());
    match process.spawn() {
        Ok(child) => Running::Child(child),
        Err(error) => {
            eprintln!("nebula: {name}: {}", describe_io_error(&error));
            Running::Done(if error.kind() == io::ErrorKind::NotFound {
                127
            } else {
                126
            })
        }
    }
}

impl arith::Vars for Shell {
    fn get(&mut self, name: &str) -> Option<String> {
        self.var(name)
    }

    fn set(&mut self, name: &str, value: i64) {
        env::set_var(name, value.to_string());
    }
}

impl Context for Shell {
    fn var(&mut self, name: &str) -> Option<String> {
        match name {
            "?" => Some(self.last_status.to_string()),
            "$" => Some(std::process::id().to_string()),
            "0" => Some(self.script_name.clone()),
            "-" => {
                let mut flags = self.option_flags();
                if self.interactive {
                    flags.push('i');
                }
                Some(flags)
            }
            "!" => None,
            "RANDOM" => Some((self.next_random() % 32768).to_string()),
            "SECONDS" => Some(self.started.elapsed().as_secs().to_string()),
            _ if name.chars().all(|c| c.is_ascii_digit()) => name
                .parse::<usize>()
                .ok()
                .and_then(|index| index.checked_sub(1))
                .and_then(|index| self.positional.get(index).cloned()),
            _ => env::var(name).ok(),
        }
    }

    fn unset(&mut self, name: &str) {
        if self.options.nounset {
            self.fail(format!("{name}: unbound variable"));
            if !self.interactive {
                self.exit_code.get_or_insert(1);
            }
        }
    }

    fn set_var(&mut self, name: &str, value: &str) {
        env::set_var(name, value);
    }

    fn positional(&self) -> Vec<String> {
        self.positional.clone()
    }

    fn substitute(&mut self, source: &str) -> String {
        self.capture(source)
    }

    fn arith(&mut self, source: &str) -> Option<i64> {
        let word = match parse::parse_arith_text(source) {
            Ok(word) => word,
            Err(error) => {
                self.fail(format!("{}: {error}", source.trim()));
                return None;
            }
        };
        let text = expand::expand_single(&word, self);
        match arith::eval(&text, self) {
            Ok(value) => Some(value),
            Err(error) => {
                self.fail(format!("{}: {error}", text.trim()));
                None
            }
        }
    }

    fn home(&self) -> Option<String> {
        sys::home().map(|path| {
            let text = path.to_string_lossy().into_owned();
            if cfg!(windows) {
                text.replace('\\', "/")
            } else {
                text
            }
        })
    }

    fn fail(&mut self, message: String) {
        eprintln!("nebula: {message}");
        self.expansion_failed = true;
    }

    fn required(&mut self, message: String) {
        self.fail(message);
        if !self.interactive {
            self.exit_code.get_or_insert(1);
        }
    }
}

pub fn describe_io_error(error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::NotFound => "no such file or directory".to_owned(),
        io::ErrorKind::PermissionDenied => "permission denied".to_owned(),
        io::ErrorKind::AlreadyExists => "already exists".to_owned(),
        io::ErrorKind::NotADirectory => "not a directory".to_owned(),
        io::ErrorKind::IsADirectory => "is a directory".to_owned(),
        _ => {
            let text = error.to_string();
            match text.find(" (os error") {
                Some(index) => text[..index].to_lowercase(),
                None => text,
            }
        }
    }
}

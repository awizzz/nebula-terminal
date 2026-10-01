//! Runs parsed command lines: expansion, redirection, pipelines and builtins.

use crate::commands;
use crate::expand::{self, Context};
use crate::parse::{self, Command, Connector, List, Pipeline, RedirectKind, RedirectTarget};
use crate::suggest;
use crate::sys;
use std::collections::BTreeMap;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

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
}

impl Input {
    fn into_stdio(self) -> Stdio {
        match self {
            Input::Inherit => Stdio::inherit(),
            Input::File(file) => file.into(),
            Input::Pipe(pipe) => pipe.into(),
        }
    }
}

impl Output {
    fn try_clone(&self) -> io::Result<Output> {
        Ok(match self {
            Output::Stdout => Output::Stdout,
            Output::Stderr => Output::Stderr,
            Output::File(file) => Output::File(file.try_clone()?),
            Output::Pipe(pipe) => Output::Pipe(pipe.try_clone()?),
        })
    }

    fn into_stdio(self) -> Stdio {
        match self {
            Output::Stdout => Stdio::inherit(),
            Output::Stderr => io::stderr().into(),
            Output::File(file) => file.into(),
            Output::Pipe(pipe) => pipe.into(),
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
        }
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
    Builtin,
    Util,
    External(PathBuf),
    Missing,
}

pub struct Shell {
    pub last_status: i32,
    pub aliases: BTreeMap<String, String>,
    pub history: Vec<String>,
    pub exit_code: Option<i32>,
    pub interrupted: Arc<AtomicBool>,
    dir_stack: Vec<PathBuf>,
    exe: PathBuf,
    alias_depth: usize,
}

impl Shell {
    pub fn new() -> Self {
        let exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("nebula-sh"));
        let mut aliases = BTreeMap::new();
        aliases.insert("ll".to_owned(), "ls -alh".to_owned());
        aliases.insert("la".to_owned(), "ls -A".to_owned());
        aliases.insert("..".to_owned(), "cd ..".to_owned());
        Self {
            last_status: 0,
            aliases,
            history: Vec::new(),
            exit_code: None,
            interrupted: Arc::new(AtomicBool::new(false)),
            dir_stack: Vec::new(),
            exe,
            alias_depth: 0,
        }
    }

    pub fn resolve(&self, name: &str) -> Resolution {
        if let Some(value) = self.aliases.get(name) {
            return Resolution::Alias(value.clone());
        }
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

    /// Runs one line of input and returns its exit status.
    pub fn run_line(&mut self, line: &str) -> i32 {
        match parse::parse(line) {
            Ok(list) => {
                let status = self.run_list(&list, Output::Stdout);
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

    fn run_list(&mut self, list: &List, stdout: Output) -> i32 {
        let mut status = 0;
        for (index, (connector, pipeline)) in list.0.iter().enumerate() {
            if index > 0 {
                let run = match connector {
                    Connector::And => status == 0,
                    Connector::Or => status != 0,
                    Connector::Seq => true,
                };
                if !run || self.exit_code.is_some() {
                    continue;
                }
                if self.interrupted.load(Ordering::SeqCst) && *connector != Connector::Seq {
                    break;
                }
            }
            let target = match stdout.try_clone() {
                Ok(target) => target,
                Err(error) => {
                    eprintln!("nebula: {error}");
                    return 1;
                }
            };
            status = self.run_pipeline(pipeline, target);
            self.last_status = status;
        }
        status
    }

    fn run_pipeline(&mut self, pipeline: &Pipeline, stdout: Output) -> i32 {
        self.interrupted.store(false, Ordering::SeqCst);
        let count = pipeline.commands.len();
        let mut running = Vec::with_capacity(count);
        let mut previous: Option<os_pipe::PipeReader> = None;
        let mut stdout = Some(stdout);

        for (index, command) in pipeline.commands.iter().enumerate() {
            let last = index + 1 == count;
            let stdin = previous.take().map(Input::Pipe).unwrap_or(Input::Inherit);
            let out = if last {
                stdout.take().unwrap_or(Output::Stdout)
            } else {
                match os_pipe::pipe() {
                    Ok((reader, writer)) => {
                        previous = Some(reader);
                        Output::Pipe(writer)
                    }
                    Err(error) => {
                        eprintln!("nebula: cannot create pipe: {error}");
                        return 1;
                    }
                }
            };
            running.push(self.start_command(command, stdin, out, !last || count > 1));
        }

        let mut status = 0;
        for task in running {
            status = task.wait();
        }
        if pipeline.negate {
            i32::from(status == 0)
        } else {
            status
        }
    }

    fn start_command(
        &mut self,
        command: &Command,
        stdin: Input,
        stdout: Output,
        in_pipeline: bool,
    ) -> Running {
        let mut argv: Vec<String> = Vec::new();
        for word in &command.words {
            argv.extend(expand::expand(word, self, true));
        }
        let assignments: Vec<(String, String)> = command
            .assignments
            .iter()
            .map(|(name, value)| (name.clone(), expand::expand_single(value, self)))
            .collect();

        let (stdin, stdout, stderr) = match self.apply_redirects(command, stdin, stdout) {
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
            return Running::Done(0);
        }

        let name = argv[0].clone();
        match self.resolve(&name) {
            Resolution::Alias(value) if self.alias_depth < 8 => {
                let rest: Vec<String> = argv[1..].iter().map(|arg| parse::quote(arg)).collect();
                let line = if rest.is_empty() {
                    value
                } else {
                    format!("{value} {}", rest.join(" "))
                };
                let list = match parse::parse(&line) {
                    Ok(list) => list,
                    Err(error) => {
                        eprintln!("nebula: alias {name}: {error}");
                        return Running::Done(2);
                    }
                };
                // Aliases use the redirections of the command that invoked them.
                if !matches!(stdin, Input::Inherit) || !matches!(stderr, Output::Stderr) {
                    eprintln!(
                        "nebula: {name}: input and error redirections are ignored for aliases"
                    );
                }
                self.alias_depth += 1;
                let status = self.run_list(&list, stdout);
                self.alias_depth -= 1;
                Running::Done(status)
            }
            Resolution::Alias(_) => {
                eprintln!("nebula: {name}: alias loop");
                Running::Done(1)
            }
            Resolution::Builtin => self.start_builtin(&argv, stdout, stderr, in_pipeline),
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
                self.spawn(process, &name, assignments, stdin, stdout, stderr)
            }
            Resolution::External(path) => {
                let mut process = external_command(&path);
                process.args(&argv[1..]);
                self.spawn(process, &name, assignments, stdin, stdout, stderr)
            }
            Resolution::Missing => {
                let mut err = stderr.into_writer();
                let _ = writeln!(err, "{}", suggest::not_found(&name, self));
                Running::Done(127)
            }
        }
    }

    fn spawn(
        &self,
        mut process: std::process::Command,
        name: &str,
        assignments: Vec<(String, String)>,
        stdin: Input,
        stdout: Output,
        stderr: Output,
    ) -> Running {
        process
            .envs(assignments)
            .stdin(stdin.into_stdio())
            .stdout(stdout.into_stdio())
            .stderr(stderr.into_stdio_for_stderr());
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

    fn apply_redirects(
        &mut self,
        command: &Command,
        mut stdin: Input,
        mut stdout: Output,
    ) -> Result<(Input, Output, Output), String> {
        let mut stderr = Output::Stderr;
        for redirect in &command.redirects {
            match &redirect.target {
                RedirectTarget::File(word) => {
                    let target = sys::translate_path(&expand::expand_single(word, self));
                    let path = Path::new(&target);
                    match redirect.kind {
                        RedirectKind::Read => {
                            let file = File::open(path).map_err(|error| {
                                format!("{target}: {}", describe_io_error(&error))
                            })?;
                            stdin = Input::File(file);
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
                                stderr = Output::File(file);
                            } else {
                                stdout = Output::File(file);
                            }
                        }
                    }
                }
                RedirectTarget::Fd(target) => {
                    let source = match target {
                        1 => stdout.try_clone(),
                        2 => stderr.try_clone(),
                        other => return Err(format!("bad file descriptor {other}")),
                    }
                    .map_err(|error| error.to_string())?;
                    match redirect.fd {
                        1 => stdout = source,
                        2 => stderr = source,
                        other => return Err(format!("cannot redirect file descriptor {other}")),
                    }
                }
            }
        }
        Ok((stdin, stdout, stderr))
    }

    fn start_builtin(
        &mut self,
        argv: &[String],
        stdout: Output,
        stderr: Output,
        in_pipeline: bool,
    ) -> Running {
        let piped = matches!(stdout, Output::Pipe(_));
        if in_pipeline && piped {
            // Buffer the output and write it from a thread so the next stage can start reading.
            let mut buffer = Vec::new();
            let mut err = stderr.into_writer();
            let status = self.builtin(argv, &mut buffer, &mut err);
            let mut out = stdout.into_writer();
            return Running::Thread(thread::spawn(move || {
                let _ = out.write_all(&buffer);
                status
            }));
        }
        let mut out = stdout.into_writer();
        let mut err = stderr.into_writer();
        let status = self.builtin(argv, &mut out, &mut err);
        let _ = out.flush();
        Running::Done(status)
    }

    fn builtin(&mut self, argv: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
        crate::builtins::run(self, argv, out, err)
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

    /// Runs `source` and returns what it printed on stdout.
    pub fn capture(&mut self, source: &str) -> String {
        let list = match parse::parse(source) {
            Ok(list) => list,
            Err(error) => {
                eprintln!("nebula: {error}");
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
        let saved = self.last_status;
        let status = self.run_list(&list, Output::Pipe(writer));
        let bytes = collector.join().unwrap_or_default();
        self.last_status = if status == 0 { saved } else { status };
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

impl Context for Shell {
    fn var(&self, name: &str) -> Option<String> {
        match name {
            "?" => Some(self.last_status.to_string()),
            "$" => Some(std::process::id().to_string()),
            "0" => Some("nebula".to_owned()),
            _ if name.chars().all(|c| c.is_ascii_digit()) => None,
            _ => env::var(name).ok(),
        }
    }

    fn substitute(&mut self, source: &str) -> String {
        self.capture(source)
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
}

/// Builds a process for an external program, routing scripts through their interpreter.
fn external_command(path: &Path) -> std::process::Command {
    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase());
    if extension.as_deref() == Some("ps1") {
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
        return command;
    }
    std::process::Command::new(path)
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

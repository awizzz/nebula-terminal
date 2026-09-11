use crate::platform;
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Connector {
    And,
    Or,
    Sequence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    Pipe,
    And,
    Or,
    Sequence,
    In,
    Out(bool),
    ErrOut(bool),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CommandSpec {
    argv: Vec<String>,
    stdin: Option<PathBuf>,
    stdout: Option<(PathBuf, bool)>,
    stderr: Option<(PathBuf, bool)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChainPart {
    gate: Option<Connector>,
    pipeline: Vec<CommandSpec>,
}

struct StageResult {
    code: i32,
    stdout: Vec<u8>,
}

pub fn execute(line: &str, cwd: &Path) -> Result<i32, String> {
    let chain = parse_line(line)?;
    let mut last_code = 0;

    for part in chain {
        let should_run = match part.gate {
            None => true,
            Some(Connector::And) => last_code == 0,
            Some(Connector::Or) => last_code != 0,
        };

        if should_run {
            last_code = execute_pipeline(&part.pipeline, cwd)?;
        }
    }

    Ok(last_code)
}

fn parse_line(line: &str) -> Result<Vec<ChainPart>, String> {
    let tokens = lex(line)?;
    if tokens.is_empty() {
        return Ok(Vec::new());
    }

    let mut chain = Vec::new();
    let mut pipeline = Vec::new();
    let mut command = CommandSpec::default();
    let mut gate = None;
    let mut index = 0;

    while index < tokens.len() {
        match &tokens[index] {
            Token::Word(value) => command.argv.push(platform::expand_environment(value)),
            Token::In | Token::Out(_) | Token::ErrOut(_) => {
                let token = tokens[index].clone();
                index += 1;
                let Some(Token::Word(path)) = tokens.get(index) else {
                    return Err("redirection operator must be followed by a path".into());
                };
                let path = PathBuf::from(platform::expand_environment(path));
                match token {
                    Token::In => command.stdin = Some(path),
                    Token::Out(append) => command.stdout = Some((path, append)),
                    Token::ErrOut(append) => command.stderr = Some((path, append)),
                    _ => unreachable!(),
                }
            }
            Token::Pipe => {
                finish_command(&mut pipeline, &mut command)?;
            }
            Token::And | Token::Or | Token::Sequence => {
                finish_command(&mut pipeline, &mut command)?;
                if pipeline.is_empty() {
                    return Err("conditional operator requires a command".into());
                }
                chain.push(ChainPart {
                    gate,
                    pipeline: std::mem::take(&mut pipeline),
                });
                gate = match tokens[index] {
                    Token::And => Some(Connector::And),
                    Token::Or => Some(Connector::Or),
                    Token::Sequence => None,
                    _ => unreachable!(),
                };
            }
        }
        index += 1;
    }

    finish_command(&mut pipeline, &mut command)?;
    if pipeline.is_empty() {
        return Err("command line cannot end with an operator".into());
    }
    chain.push(ChainPart { gate, pipeline });
    Ok(chain)
}

fn finish_command(
    pipeline: &mut Vec<CommandSpec>,
    command: &mut CommandSpec,
) -> Result<(), String> {
    if command.argv.is_empty() {
        return Err("pipeline operator requires a command on both sides".into());
    }
    pipeline.push(std::mem::take(command));
    Ok(())
}

fn lex(line: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0;

    let flush_word = |tokens: &mut Vec<Token>, word: &mut String| {
        if !word.is_empty() {
            tokens.push(Token::Word(std::mem::take(word)));
        }
    };

    while index < chars.len() {
        let ch = chars[index];

        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            } else if ch == '\\'
                && active_quote == '"'
                && chars.get(index + 1).copied() == Some('"')
            {
                word.push('"');
                index += 1;
            } else {
                word.push(ch);
            }
            index += 1;
            continue;
        }

        match ch {
            '"' | '\'' => quote = Some(ch),
            value if value.is_whitespace() => flush_word(&mut tokens, &mut word),
            '&' if chars.get(index + 1).copied() == Some('&') => {
                flush_word(&mut tokens, &mut word);
                tokens.push(Token::And);
                index += 1;
            }
            '|' if chars.get(index + 1).copied() == Some('|') => {
                flush_word(&mut tokens, &mut word);
                tokens.push(Token::Or);
                index += 1;
            }
            '|' => {
                flush_word(&mut tokens, &mut word);
                tokens.push(Token::Pipe);
            }
            ';' => {
                flush_word(&mut tokens, &mut word);
                tokens.push(Token::Sequence);
            }
            '2' if word.is_empty() && chars.get(index + 1).copied() == Some('>') => {
                flush_word(&mut tokens, &mut word);
                let append = chars.get(index + 2).copied() == Some('>');
                tokens.push(Token::ErrOut(append));
                index += if append { 2 } else { 1 };
            }
            '>' => {
                flush_word(&mut tokens, &mut word);
                let append = chars.get(index + 1).copied() == Some('>');
                tokens.push(Token::Out(append));
                if append {
                    index += 1;
                }
            }
            '<' => {
                flush_word(&mut tokens, &mut word);
                tokens.push(Token::In);
            }
            _ => word.push(ch),
        }

        index += 1;
    }

    if quote.is_some() {
        return Err("unterminated quoted string".into());
    }

    flush_word(&mut tokens, &mut word);
    Ok(tokens)
}

fn execute_pipeline(pipeline: &[CommandSpec], cwd: &Path) -> Result<i32, String> {
    let mut input = None;
    let mut last_code = 0;

    for (index, spec) in pipeline.iter().enumerate() {
        let capture = index + 1 < pipeline.len();
        let result = execute_stage(spec, cwd, input.take(), capture)?;
        last_code = result.code;
        if capture {
            input = Some(result.stdout);
        }
    }

    Ok(last_code)
}

fn execute_stage(
    spec: &CommandSpec,
    cwd: &Path,
    piped_input: Option<Vec<u8>>,
    capture_stdout: bool,
) -> Result<StageResult, String> {
    if let Some(result) = run_builtin(spec, cwd, piped_input.as_deref()) {
        return finish_builtin(spec, cwd, capture_stdout, result?);
    }

    let program = spec
        .argv
        .first()
        .ok_or_else(|| "missing program name".to_string())?;
    if is_batch_file(program) {
        return Err(format!(
            "'{program}' is a CMD script. Run it through `cmd {program}` or switch to `backend cmd`."
        ));
    }

    let mut command = compatibility_command(&spec.argv).unwrap_or_else(|| {
        let mut command = Command::new(program);
        command.args(&spec.argv[1..]);
        command
    });
    command.current_dir(cwd);

    let stdin_data = if let Some(path) = &spec.stdin {
        Some(fs::read(resolve_path(cwd, path)).map_err(|e| e.to_string())?)
    } else {
        piped_input
    };

    if stdin_data.is_some() {
        command.stdin(Stdio::piped());
    }

    if let Some((path, append)) = &spec.stdout {
        command.stdout(Stdio::from(open_output(cwd, path, *append)?));
    } else if capture_stdout {
        command.stdout(Stdio::piped());
    }

    if let Some((path, append)) = &spec.stderr {
        command.stderr(Stdio::from(open_output(cwd, path, *append)?));
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("failed to start '{program}': {e}"))?;

    if let Some(data) = stdin_data {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(&data).map_err(|e| e.to_string())?;
        }
    }

    if capture_stdout && spec.stdout.is_none() {
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        Ok(StageResult {
            code: output.status.code().unwrap_or(1),
            stdout: output.stdout,
        })
    } else {
        let status = child.wait().map_err(|e| e.to_string())?;
        Ok(StageResult {
            code: status.code().unwrap_or(1),
            stdout: Vec::new(),
        })
    }
}

fn compatibility_command(argv: &[String]) -> Option<Command> {
    let program = argv.first()?.to_ascii_lowercase();
    match program.as_str() {
        "cmd" => {
            let mut command = Command::new("cmd.exe");
            if argv.len() > 1 {
                let joined = join_command(&argv[1..]);
                command.args(["/d", "/s", "/c", joined.as_str()]);
            }
            Some(command)
        }
        "powershell" => {
            let mut command = Command::new("powershell.exe");
            if argv.len() > 1 {
                let joined = join_command(&argv[1..]);
                command.args(["-NoLogo", "-NoProfile", "-Command", joined.as_str()]);
            }
            Some(command)
        }
        "pwsh" => {
            let mut command = Command::new("pwsh.exe");
            if argv.len() > 1 {
                let joined = join_command(&argv[1..]);
                command.args(["-NoLogo", "-NoProfile", "-Command", joined.as_str()]);
            }
            Some(command)
        }
        _ => None,
    }
}

fn join_command(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if arg.chars().any(char::is_whitespace) {
                format!("\"{}\"", arg.replace('"', "\\\""))
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn run_builtin(
    spec: &CommandSpec,
    cwd: &Path,
    input: Option<&[u8]>,
) -> Option<Result<(i32, Vec<u8>, Vec<u8>), String>> {
    let command = spec.argv.first()?.to_ascii_lowercase();
    let args = &spec.argv[1..];

    let result = match command.as_str() {
        "echo" => Ok((0, format!("{}\n", args.join(" ")).into_bytes(), Vec::new())),
        "dir" | "ls" => builtin_dir(args, cwd),
        "type" | "cat" => builtin_cat(args, cwd, input),
        "mkdir" | "md" => builtin_mkdir(args, cwd),
        "touch" => builtin_touch(args, cwd),
        "where" | "which" => builtin_which(args, cwd),
        "copy" | "cp" => builtin_copy(args, cwd),
        "move" | "mv" => builtin_move(args, cwd),
        _ => return None,
    };

    Some(result)
}

fn finish_builtin(
    spec: &CommandSpec,
    cwd: &Path,
    capture_stdout: bool,
    (code, stdout, stderr): (i32, Vec<u8>, Vec<u8>),
) -> Result<StageResult, String> {
    if !stderr.is_empty() {
        if let Some((path, append)) = &spec.stderr {
            let mut file = open_output(cwd, path, *append)?;
            file.write_all(&stderr).map_err(|e| e.to_string())?;
        } else {
            std::io::stderr()
                .write_all(&stderr)
                .map_err(|e| e.to_string())?;
        }
    }

    let mut captured = Vec::new();
    if let Some((path, append)) = &spec.stdout {
        let mut file = open_output(cwd, path, *append)?;
        file.write_all(&stdout).map_err(|e| e.to_string())?;
    } else if capture_stdout {
        captured = stdout;
    } else {
        std::io::stdout()
            .write_all(&stdout)
            .map_err(|e| e.to_string())?;
    }

    Ok(StageResult {
        code,
        stdout: captured,
    })
}

fn builtin_dir(args: &[String], cwd: &Path) -> Result<(i32, Vec<u8>, Vec<u8>), String> {
    if args.len() > 1 {
        return Err("dir accepts at most one path".into());
    }
    let path = args
        .first()
        .map(PathBuf::from)
        .map(|path| resolve_path(cwd, &path))
        .unwrap_or_else(|| cwd.to_path_buf());

    let mut entries = fs::read_dir(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());

    let mut output = format!("Directory: {}\n\n", path.display());
    for entry in entries {
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if metadata.is_dir() {
            output.push_str(&format!("<DIR>          {name}\n"));
        } else {
            output.push_str(&format!("{:>13}  {name}\n", metadata.len()));
        }
    }
    Ok((0, output.into_bytes(), Vec::new()))
}

fn builtin_cat(
    args: &[String],
    cwd: &Path,
    input: Option<&[u8]>,
) -> Result<(i32, Vec<u8>, Vec<u8>), String> {
    if args.is_empty() {
        return Ok((0, input.unwrap_or_default().to_vec(), Vec::new()));
    }

    let mut output = Vec::new();
    for arg in args {
        let path = resolve_path(cwd, Path::new(arg));
        let mut file = File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        file.read_to_end(&mut output).map_err(|e| e.to_string())?;
    }
    Ok((0, output, Vec::new()))
}

fn builtin_mkdir(args: &[String], cwd: &Path) -> Result<(i32, Vec<u8>, Vec<u8>), String> {
    if args.is_empty() {
        return Err("mkdir requires at least one path".into());
    }
    for arg in args {
        fs::create_dir_all(resolve_path(cwd, Path::new(arg))).map_err(|e| e.to_string())?;
    }
    Ok((0, Vec::new(), Vec::new()))
}

fn builtin_touch(args: &[String], cwd: &Path) -> Result<(i32, Vec<u8>, Vec<u8>), String> {
    if args.is_empty() {
        return Err("touch requires at least one path".into());
    }
    for arg in args {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(resolve_path(cwd, Path::new(arg)))
            .map_err(|e| e.to_string())?;
    }
    Ok((0, Vec::new(), Vec::new()))
}

fn builtin_which(args: &[String], cwd: &Path) -> Result<(i32, Vec<u8>, Vec<u8>), String> {
    if args.is_empty() {
        return Err("which requires at least one command".into());
    }

    let mut output = String::new();
    let mut missing = Vec::new();
    for arg in args {
        match resolve_program(arg, cwd) {
            Some(path) => output.push_str(&format!("{}\n", path.display())),
            None => missing.push(arg.clone()),
        }
    }

    let code = if missing.is_empty() { 0 } else { 1 };
    let stderr = if missing.is_empty() {
        Vec::new()
    } else {
        format!("not found: {}\n", missing.join(", ")).into_bytes()
    };
    Ok((code, output.into_bytes(), stderr))
}

fn builtin_copy(args: &[String], cwd: &Path) -> Result<(i32, Vec<u8>, Vec<u8>), String> {
    if args.len() != 2 {
        return Err("copy requires a source and destination".into());
    }
    let source = resolve_path(cwd, Path::new(&args[0]));
    let destination = resolve_path(cwd, Path::new(&args[1]));
    fs::copy(&source, &destination)
        .map_err(|e| format!("{} -> {}: {e}", source.display(), destination.display()))?;
    Ok((0, Vec::new(), Vec::new()))
}

fn builtin_move(args: &[String], cwd: &Path) -> Result<(i32, Vec<u8>, Vec<u8>), String> {
    if args.len() != 2 {
        return Err("move requires a source and destination".into());
    }
    let source = resolve_path(cwd, Path::new(&args[0]));
    let destination = resolve_path(cwd, Path::new(&args[1]));
    fs::rename(&source, &destination)
        .map_err(|e| format!("{} -> {}: {e}", source.display(), destination.display()))?;
    Ok((0, Vec::new(), Vec::new()))
}

fn resolve_program(command: &str, cwd: &Path) -> Option<PathBuf> {
    let raw = PathBuf::from(command);
    if raw.components().count() > 1 || raw.is_absolute() {
        let path = resolve_path(cwd, &raw);
        return path.exists().then_some(path);
    }

    let extensions = if raw.extension().is_some() {
        vec![String::new()]
    } else {
        env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .map(str::to_string)
            .collect()
    };

    let mut directories = vec![cwd.to_path_buf()];
    if let Some(path) = env::var_os("PATH") {
        directories.extend(env::split_paths(&path));
    }

    for directory in directories {
        for extension in &extensions {
            let candidate = if extension.is_empty() {
                directory.join(command)
            } else {
                directory.join(format!("{command}{extension}"))
            };
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn resolve_path(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn open_output(cwd: &Path, path: &Path, append: bool) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.create(true).write(true);
    if append {
        options.append(true);
    } else {
        options.truncate(true);
    }
    options
        .open(resolve_path(cwd, path))
        .map_err(|e| e.to_string())
}

fn is_batch_file(program: &str) -> bool {
    Path::new(program)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "bat" | "cmd"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_quotes_and_operators() {
        assert_eq!(
            lex("echo \"hello world\" | tool > out.txt && next").unwrap(),
            vec![
                Token::Word("echo".into()),
                Token::Word("hello world".into()),
                Token::Pipe,
                Token::Word("tool".into()),
                Token::Out(false),
                Token::Word("out.txt".into()),
                Token::And,
                Token::Word("next".into()),
            ]
        );
    }

    #[test]
    fn parses_pipeline_redirections_and_conditionals() {
        let parsed = parse_line("first a | second >> out.txt || third 2> err.txt").unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].pipeline.len(), 2);
        assert_eq!(
            parsed[0].pipeline[1].stdout,
            Some((PathBuf::from("out.txt"), true))
        );
        assert_eq!(parsed[1].gate, Some(Connector::Or));
        assert_eq!(
            parsed[1].pipeline[0].stderr,
            Some((PathBuf::from("err.txt"), false))
        );
    }

    #[test]
    fn rejects_incomplete_syntax() {
        assert!(parse_line("echo hi |").is_err());
        assert!(parse_line("echo hi >").is_err());
        assert!(lex("echo \"unterminated").is_err());
    }
}

//! `awk`: the POSIX language with the gawk functions people reach for most
//! (`gensub`, `strftime`, `systime`, `asort`, `match` with an array, `length` of an
//! array, `nextfile`, `/dev/stderr`).
//!
//! `awk [-F fs] [-v var=value]... [-f progfile | 'program'] [file | var=value]...`

mod ast;
mod interp;
mod lexer;
mod parser;
mod value;

/// Special variables, in the global slots the parser gives them first.
pub const NR: usize = 0;
pub const FNR: usize = 1;
pub const FS: usize = 2;
pub const OFS: usize = 3;
pub const ORS: usize = 4;
pub const RS: usize = 5;
pub const FILENAME: usize = 6;
pub const SUBSEP: usize = 7;
pub const RSTART: usize = 8;
pub const RLENGTH: usize = 9;
pub const CONVFMT: usize = 10;
pub const OFMT: usize = 11;
pub const ENVIRON: usize = 12;
pub const ARGC: usize = 13;
pub const ARGV: usize = 14;

pub const SPECIAL_GLOBALS: [&str; 15] = [
    "NR", "FNR", "FS", "OFS", "ORS", "RS", "FILENAME", "SUBSEP", "RSTART", "RLENGTH", "CONVFMT",
    "OFMT", "ENVIRON", "ARGC", "ARGV",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    Length,
    Substr,
    Index,
    Split,
    Sub,
    Gsub,
    Gensub,
    Match,
    Sprintf,
    Sin,
    Cos,
    Atan2,
    Exp,
    Log,
    Sqrt,
    Int,
    Rand,
    Srand,
    Tolower,
    Toupper,
    System,
    Close,
    Fflush,
    Systime,
    Strftime,
    Asort,
    Asorti,
}

const BUILTINS: [(&str, Builtin); 27] = [
    ("length", Builtin::Length),
    ("substr", Builtin::Substr),
    ("index", Builtin::Index),
    ("split", Builtin::Split),
    ("sub", Builtin::Sub),
    ("gsub", Builtin::Gsub),
    ("gensub", Builtin::Gensub),
    ("match", Builtin::Match),
    ("sprintf", Builtin::Sprintf),
    ("sin", Builtin::Sin),
    ("cos", Builtin::Cos),
    ("atan2", Builtin::Atan2),
    ("exp", Builtin::Exp),
    ("log", Builtin::Log),
    ("sqrt", Builtin::Sqrt),
    ("int", Builtin::Int),
    ("rand", Builtin::Rand),
    ("srand", Builtin::Srand),
    ("tolower", Builtin::Tolower),
    ("toupper", Builtin::Toupper),
    ("system", Builtin::System),
    ("close", Builtin::Close),
    ("fflush", Builtin::Fflush),
    ("systime", Builtin::Systime),
    ("strftime", Builtin::Strftime),
    ("asort", Builtin::Asort),
    ("asorti", Builtin::Asorti),
];

impl Builtin {
    pub fn from_name(name: &str) -> Option<Builtin> {
        BUILTINS
            .iter()
            .find(|(builtin, _)| *builtin == name)
            .map(|(_, builtin)| *builtin)
    }

    pub fn name(self) -> &'static str {
        BUILTINS
            .iter()
            .find(|(_, builtin)| *builtin == self)
            .map_or("?", |(name, _)| name)
    }
}

const USAGE: &str = "usage: awk [-F fs] [-v var=value] [-f progfile | 'program'] [file ...]";

const HELP: &str = "\
Usage: awk [-F fs] [-v var=value]... [-f progfile | 'program'] [file | var=value]...
Scan files line by line and run a program on the lines that match its patterns.

  -F fs          field separator: one character, or a regular expression
  -v var=value   set a variable before the program starts
  -f progfile    read the program from a file (may be repeated)
  --help         show this help
  --version      show the version

Examples:
  awk '{print $1}' file           first column
  awk -F, '$3 > 100' data.csv     rows whose third column is over 100
  awk '{s += $2} END {print s}'   sum of the second column
  awk '!seen[$0]++'               drop repeated lines, keeping the order";

struct Options {
    fs: Option<String>,
    assignments: Vec<(String, String)>,
    program: String,
    operands: Vec<String>,
}

fn parse_options(args: &[String]) -> Result<Option<Options>, String> {
    let mut fs = None;
    let mut assignments = Vec::new();
    let mut program_files: Vec<String> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--" {
            index += 1;
            break;
        }
        if arg == "--help" {
            println!("{HELP}");
            return Ok(None);
        }
        if arg == "--version" || arg == "-V" {
            println!("awk (Nebula) {}", env!("CARGO_PKG_VERSION"));
            return Ok(None);
        }
        if arg.len() < 2 || !arg.starts_with('-') {
            break;
        }
        let (flag, attached) = arg.split_at(2);
        let value = |index: &mut usize| -> Result<String, String> {
            if !attached.is_empty() {
                return Ok(attached.to_owned());
            }
            *index += 1;
            args.get(*index)
                .cloned()
                .ok_or_else(|| format!("option {flag} needs a value\n{USAGE}"))
        };
        match flag {
            "-F" => {
                let separator = value(&mut index)?;
                // `-Ft` means a tab, as in gawk.
                fs = Some(if separator == "t" {
                    "\t".to_owned()
                } else {
                    lexer::unescape(&separator)
                });
            }
            "-v" => {
                let assignment = value(&mut index)?;
                match assignment.split_once('=') {
                    Some((name, value)) if !name.is_empty() => {
                        assignments.push((name.to_owned(), value.to_owned()))
                    }
                    _ => return Err(format!("-v needs var=value, not \"{assignment}\"")),
                }
            }
            "-f" => program_files.push(value(&mut index)?),
            _ => return Err(format!("unknown option {arg}\n{USAGE}")),
        }
        index += 1;
    }
    let program = if program_files.is_empty() {
        match args.get(index) {
            Some(program) => {
                index += 1;
                program.clone()
            }
            None => return Err(USAGE.to_owned()),
        }
    } else {
        let mut program = String::new();
        for file in &program_files {
            let path = crate::sys::translate_path(file);
            let text = std::fs::read_to_string(&path).map_err(|error| {
                format!(
                    "can't read the program file {file}: {}",
                    crate::exec::describe_io_error(&error)
                )
            })?;
            program.push_str(&text);
            program.push('\n');
        }
        program
    };
    Ok(Some(Options {
        fs,
        assignments,
        program,
        operands: args[index..].to_vec(),
    }))
}

fn run_program(args: Vec<String>) -> i32 {
    let options = match parse_options(&args) {
        Ok(Some(options)) => options,
        Ok(None) => return 0,
        Err(message) => {
            eprintln!("awk: {message}");
            return 2;
        }
    };
    let program = match lexer::tokenize(&options.program).and_then(parser::parse) {
        Ok(program) => program,
        Err(message) => {
            eprintln!("awk: {message}");
            return 2;
        }
    };
    let mut interp =
        match interp::Interp::new(&program, options.assignments, options.fs, options.operands) {
            Ok(interp) => interp,
            Err(message) => {
                eprintln!("awk: {message}");
                return 2;
            }
        };
    let result = interp.run();
    interp.finish();
    match result {
        Ok(code) => code,
        Err(message) => {
            eprintln!("awk: {message}");
            2
        }
    }
}

/// Programs can recurse; run them with more stack than the main thread has on Windows.
pub fn run(args: &[String]) -> i32 {
    let args = args.to_vec();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || run_program(args))
        .ok()
        .and_then(|handle| handle.join().ok())
        .unwrap_or(2)
}

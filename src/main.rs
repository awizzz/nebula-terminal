mod config;
mod editor;
mod i18n;
mod native;
mod platform;
mod shell;
mod ui;

use std::{env, process};

const APP_NAME: &str = "Nebula";
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_cli_help() {
    println!(
        "{APP_NAME} {VERSION}\n\
A fast, customizable Windows shell frontend.\n\n\
Usage:\n  nebula [OPTION]\n\n\
Options:\n  -h, --help       Show this help and exit\n  -V, --version    Show the version and exit\n      --admin      Relaunch Nebula with administrator privileges\n\n\
Run Nebula without arguments to start the interactive shell."
    );
}

enum CliAction {
    Interactive,
    Exit,
    ElevatedRun(String),
}

fn decode_hex(value: &str) -> Result<String, String> {
    if value.len() % 2 != 0 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid internal command payload".into());
    }
    let bytes = (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "invalid internal command payload".to_string())?;
    String::from_utf8(bytes).map_err(|_| "invalid internal command payload".into())
}

fn validate_cli_args(args: &[String]) -> Result<CliAction, String> {
    match args {
        [] => Ok(CliAction::Interactive),
        [arg] if matches!(arg.as_str(), "-h" | "--help") => {
            print_cli_help();
            Ok(CliAction::Exit)
        }
        [arg] if matches!(arg.as_str(), "-V" | "--version") => {
            println!("{APP_NAME} {VERSION}");
            Ok(CliAction::Exit)
        }
        [arg] if matches!(arg.as_str(), "--admin" | "--elevated-child") => {
            Ok(CliAction::Interactive)
        }
        [flag, payload] if flag == "--elevated-run" => {
            Ok(CliAction::ElevatedRun(decode_hex(payload)?))
        }
        [arg] => Err(format!("unknown option: {arg}")),
        _ => Err("invalid startup arguments".into()),
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let action = match validate_cli_args(&args) {
        Ok(action) => action,
        Err(error) => {
            eprintln!("Nebula: {error}");
            eprintln!("Try 'nebula --help' for usage information.");
            process::exit(2);
        }
    };

    let result = match action {
        CliAction::Exit => return,
        CliAction::Interactive => shell::run().map(|_| 0),
        CliAction::ElevatedRun(command) => shell::run_once(&command),
    };

    match result {
        Ok(code) => process::exit(code),
        Err(error) => {
            eprintln!("Nebula: {error}");
            process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_mode_accepts_no_arguments() {
        assert!(matches!(validate_cli_args(&[]), Ok(CliAction::Interactive)));
    }

    #[test]
    fn admin_modes_are_forwarded_to_the_shell() {
        assert!(matches!(
            validate_cli_args(&["--admin".into()]),
            Ok(CliAction::Interactive)
        ));
        assert!(matches!(
            validate_cli_args(&["--elevated-child".into()]),
            Ok(CliAction::Interactive)
        ));
    }

    #[test]
    fn elevated_payload_is_decoded() {
        match validate_cli_args(&["--elevated-run".into(), "6563686f206869".into()]).unwrap() {
            CliAction::ElevatedRun(command) => assert_eq!(command, "echo hi"),
            _ => panic!("unexpected CLI action"),
        }
    }

    #[test]
    fn invalid_options_are_rejected() {
        assert!(validate_cli_args(&["--wat".into()]).is_err());
        assert!(validate_cli_args(&["--elevated-run".into(), "xyz".into()]).is_err());
    }
}

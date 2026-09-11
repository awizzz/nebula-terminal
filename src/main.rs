mod config;
mod editor;
mod i18n;
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

fn validate_cli_args(args: &[String]) -> Result<bool, String> {
    match args {
        [] => Ok(false),
        [arg] if matches!(arg.as_str(), "-h" | "--help") => {
            print_cli_help();
            Ok(true)
        }
        [arg] if matches!(arg.as_str(), "-V" | "--version") => {
            println!("{APP_NAME} {VERSION}");
            Ok(true)
        }
        [arg] if matches!(arg.as_str(), "--admin" | "--elevated-child") => Ok(false),
        [arg] => Err(format!("unknown option: {arg}")),
        _ => Err("Nebula accepts at most one startup option".into()),
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match validate_cli_args(&args) {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("Nebula: {error}");
            eprintln!("Try 'nebula --help' for usage information.");
            process::exit(2);
        }
    }

    if let Err(error) = shell::run() {
        eprintln!("Nebula: {error}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_mode_accepts_no_arguments() {
        assert!(matches!(validate_cli_args(&[]), Ok(false)));
    }

    #[test]
    fn admin_modes_are_forwarded_to_the_shell() {
        assert!(matches!(validate_cli_args(&["--admin".into()]), Ok(false)));
        assert!(matches!(
            validate_cli_args(&["--elevated-child".into()]),
            Ok(false)
        ));
    }

    #[test]
    fn invalid_options_are_rejected() {
        assert!(validate_cli_args(&["--wat".into()]).is_err());
        assert!(validate_cli_args(&["--admin".into(), "--wat".into()]).is_err());
    }
}

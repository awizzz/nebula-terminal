mod config;
mod editor;
mod i18n;
mod platform;
mod shell;
mod ui;

fn main() {
    if let Err(error) = shell::run() {
        eprintln!("Nebula: {error}");
        std::process::exit(1);
    }
}

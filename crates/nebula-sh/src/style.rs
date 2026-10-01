//! Small ANSI styling helpers. Colors are the 16 terminal colors so they follow the
//! user's theme; `NO_COLOR` turns them off.

use nu_ansi_term::{Color, Style};
use std::sync::OnceLock;

pub const ACCENT: Color = Color::Blue;
pub const COMMAND: Color = Color::Green;
pub const ERROR: Color = Color::Red;
pub const WARN: Color = Color::Yellow;

pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        use std::io::IsTerminal;
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty());
        !no_color && (std::io::stdout().is_terminal() || std::io::stderr().is_terminal())
    })
}

pub fn apply(style: Style, text: &str) -> String {
    if enabled() {
        style.paint(text).to_string()
    } else {
        text.to_owned()
    }
}

pub trait Paint {
    fn paint(&self, color: Color) -> String;
    fn bold(&self) -> String;
    fn dim(&self) -> String;
}

impl<T: AsRef<str>> Paint for T {
    fn paint(&self, color: Color) -> String {
        apply(Style::new().fg(color), self.as_ref())
    }

    fn bold(&self) -> String {
        apply(Style::new().bold(), self.as_ref())
    }

    fn dim(&self) -> String {
        apply(Style::new().dimmed(), self.as_ref())
    }
}

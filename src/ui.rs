use crate::{
    config::{Config, ThemeConfig, UiConfig},
    i18n::Translator,
    platform,
};
use nu_ansi_term::{Color, Style};
use std::{
    io::{self, IsTerminal, Write},
    thread,
    time::Duration,
};

pub fn parse_color(value: &str) -> Color {
    let value = value.trim().trim_start_matches('#');
    if value.len() == 6 {
        if let Ok(rgb) = u32::from_str_radix(value, 16) {
            return Color::Rgb(
                ((rgb >> 16) & 0xff) as u8,
                ((rgb >> 8) & 0xff) as u8,
                (rgb & 0xff) as u8,
            );
        }
    }
    Color::White
}

pub fn paint(color: &str, text: &str, bold: bool) -> String {
    let mut style = Style::new().fg(parse_color(color));
    if bold {
        style = style.bold();
    }
    style.paint(text).to_string()
}

pub fn animations_available() -> bool {
    io::stdout().is_terminal()
}

pub fn startup(config: &Config) {
    if !config.ui.animations || !animations_available() {
        return;
    }

    let frames = [
        ("·", &config.theme.muted),
        ("◇", &config.theme.panel),
        ("◈", &config.theme.git),
        ("◆", &config.theme.accent),
    ];
    let delay = Duration::from_millis(config.ui.animation_speed_ms.clamp(8, 120));

    for (glyph, color) in frames {
        print!(
            "\r\x1b[2K  {} {}",
            paint(color, glyph, true),
            paint(color, "NEBULA", true)
        );
        let _ = io::stdout().flush();
        thread::sleep(delay);
    }

    print!("\r\x1b[2K");
    let _ = io::stdout().flush();
}

pub fn banner(config: &Config, translator: &Translator, version: &str) {
    let admin = platform::is_admin();
    let mode_text = translator.text(if admin { "status.admin" } else { "status.user" });
    let mode_color = if admin {
        &config.theme.admin
    } else {
        &config.theme.success
    };

    match config.ui.banner_style.to_ascii_lowercase().as_str() {
        "off" => {}
        "compact" => {
            println!();
            println!(
                "  {} {}  {}  {}  {}",
                paint(&config.theme.accent, "NEBULA", true),
                paint(&config.theme.muted, &format!("v{version}"), false),
                paint(mode_color, &mode_text, true),
                paint(&config.theme.muted, &config.general.backend, false),
                paint(&config.theme.muted, translator.language(), false),
            );
            println!();
        }
        "minimal" => {
            println!();
            println!(
                "  {}  {}",
                paint(&config.theme.accent, "NEBULA", true),
                paint(&config.theme.muted, &format!("v{version}"), false)
            );
            println!(
                "  {}",
                paint(
                    &config.theme.foreground,
                    &translator.text("app.tagline"),
                    false
                )
            );
            println!(
                "  {} · {} · {}",
                paint(mode_color, &mode_text, true),
                paint(&config.theme.muted, &config.general.backend, false),
                paint(&config.theme.muted, translator.language(), false)
            );
            println!();
        }
        _ => aurora_banner(config, translator, version, &mode_text, mode_color),
    }

    if config.ui.show_tips && !config.ui.banner_style.eq_ignore_ascii_case("off") {
        println!(
            "  {}",
            paint(&config.theme.muted, &translator.text("banner.hint"), false)
        );
        println!();
    }
}

fn aurora_banner(
    config: &Config,
    translator: &Translator,
    version: &str,
    mode_text: &str,
    mode_color: &str,
) {
    let width = 62usize;
    let title = format!("NEBULA  v{version}");
    let meta = format!(
        "{}  ·  {}  ·  {}",
        mode_text,
        config.general.backend,
        translator.language()
    );
    let tagline = translator.text("app.tagline");

    println!();
    println!(
        "  {}",
        paint(
            &config.theme.panel,
            &format!("╭{}╮", "─".repeat(width)),
            false
        )
    );
    println!(
        "  {}{}{}",
        paint(&config.theme.panel, "│ ", false),
        paint(
            &config.theme.accent,
            &format!("{title:<width$}", width = width - 2),
            true
        ),
        paint(&config.theme.panel, " │", false)
    );
    println!(
        "  {}{}{}",
        paint(&config.theme.panel, "│ ", false),
        paint(
            &config.theme.foreground,
            &format!(
                "{:<width$}",
                truncate(&tagline, width - 2),
                width = width - 2
            ),
            false
        ),
        paint(&config.theme.panel, " │", false)
    );
    println!(
        "  {}",
        paint(
            &config.theme.panel,
            &format!("├{}┤", "─".repeat(width)),
            false
        )
    );
    println!(
        "  {}{}{}",
        paint(&config.theme.panel, "│ ", false),
        paint(
            mode_color,
            &format!("{:<width$}", truncate(&meta, width - 2), width = width - 2),
            true
        ),
        paint(&config.theme.panel, " │", false)
    );
    println!(
        "  {}",
        paint(
            &config.theme.panel,
            &format!("╰{}╯", "─".repeat(width)),
            false
        )
    );
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    if max <= 1 {
        return "…".into();
    }
    let mut result: String = value.chars().take(max - 1).collect();
    result.push('…');
    result
}

pub fn separator(config: &Config) {
    if config.ui.command_separator {
        println!(
            "{}",
            paint(
                &config.theme.panel,
                "  ────────────────────────────────────────────────────────────",
                false
            )
        );
    }
}

pub fn theme_preview(theme: &ThemeConfig, current: &str, translator: &Translator) {
    println!();
    println!(
        "  {}",
        paint(&theme.accent, &translator.text("theme.gallery"), true)
    );
    println!();
    for name in [
        "hypr",
        "tokyo-night",
        "catppuccin",
        "nord",
        "dracula",
        "rose-pine",
        "gruvbox",
    ] {
        let marker = if name.eq_ignore_ascii_case(current) {
            "●"
        } else {
            "○"
        };
        println!(
            "  {}  {:<14} {}",
            paint(&theme.accent, marker, true),
            name,
            palette_bar(name)
        );
    }
    println!();
}

fn palette_bar(name: &str) -> String {
    let colors: &[&str] = match name {
        "tokyo-night" => &["#7AA2F7", "#BB9AF7", "#9ECE6A", "#E0AF68", "#F7768E"],
        "catppuccin" => &["#89B4FA", "#CBA6F7", "#A6E3A1", "#F9E2AF", "#F38BA8"],
        "nord" => &["#81A1C1", "#B48EAD", "#A3BE8C", "#EBCB8B", "#BF616A"],
        "dracula" => &["#8BE9FD", "#BD93F9", "#50FA7B", "#F1FA8C", "#FF5555"],
        "rose-pine" => &["#C4A7E7", "#EBBCBA", "#9CCFD8", "#F6C177", "#EB6F92"],
        "gruvbox" => &["#83A598", "#D3869B", "#B8BB26", "#FABD2F", "#FB4934"],
        _ => &["#89B4FA", "#CBA6F7", "#A6E3A1", "#F9E2AF", "#F38BA8"],
    };

    colors
        .iter()
        .map(|color| paint(color, "██", true))
        .collect::<Vec<_>>()
        .join("")
}

pub fn ui_status(config: &Config, translator: &Translator) {
    println!();
    println!(
        "  {}",
        paint(&config.theme.accent, &translator.text("ui.title"), true)
    );
    println!(
        "  {:<18} {}",
        translator.text("ui.animations"),
        on_off(config.ui.animations, &config.theme, translator)
    );
    println!(
        "  {:<18} {} ms",
        translator.text("ui.speed"),
        config.ui.animation_speed_ms
    );
    println!(
        "  {:<18} {}",
        translator.text("ui.banner"),
        config.ui.banner_style
    );
    println!(
        "  {:<18} {}",
        translator.text("ui.tips"),
        on_off(config.ui.show_tips, &config.theme, translator)
    );
    println!(
        "  {:<18} {}",
        translator.text("ui.separator"),
        on_off(config.ui.command_separator, &config.theme, translator)
    );
    println!();
}

fn on_off(value: bool, theme: &ThemeConfig, translator: &Translator) -> String {
    if value {
        paint(&theme.success, &translator.text("ui.on"), true)
    } else {
        paint(&theme.muted, &translator.text("ui.off"), false)
    }
}

pub fn pulse(config: &Config, text: &str) {
    if !config.ui.animations || !animations_available() {
        println!("{}", paint(&config.theme.success, text, false));
        return;
    }

    let delay = Duration::from_millis(config.ui.animation_speed_ms.clamp(8, 100));
    for glyph in ["·", "◦", "○", "●"] {
        print!(
            "\r\x1b[2K  {} {}",
            paint(&config.theme.accent, glyph, true),
            text
        );
        let _ = io::stdout().flush();
        thread::sleep(delay);
    }
    println!();
}

pub fn about(config: &Config, translator: &Translator, version: &str) {
    let admin = if platform::is_admin() {
        translator.text("status.admin")
    } else {
        translator.text("status.user")
    };

    println!();
    println!(
        "  {} {}",
        paint(&config.theme.accent, "Nebula", true),
        paint(&config.theme.muted, &format!("v{version}"), false)
    );
    println!("  {:<14} {}", translator.text("about.mode"), admin);
    println!(
        "  {:<14} {}",
        translator.text("about.backend"),
        config.general.backend
    );
    println!(
        "  {:<14} {}",
        translator.text("about.language"),
        translator.language()
    );
    println!(
        "  {:<14} {}",
        translator.text("about.theme"),
        config.theme.preset
    );
    println!(
        "  {:<14} {}",
        translator.text("about.terminal"),
        platform::terminal_name()
    );
    println!(
        "  {:<14} {}",
        translator.text("about.config"),
        Config::path().display()
    );
    println!();
}

pub fn reset_ui(ui: &mut UiConfig) {
    *ui = UiConfig::default();
}

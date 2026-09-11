use crate::{config::Config, i18n::Translator, platform, ui};
use nu_ansi_term::Style;
use reedline::{
    default_emacs_keybindings, ColumnarMenu, DefaultCompleter, DefaultHinter, Emacs,
    FileBackedHistory, KeyCode, KeyModifiers, MenuBuilder, Prompt, PromptEditMode,
    PromptHistorySearch, Reedline, ReedlineEvent, ReedlineMenu,
};
use std::{
    borrow::Cow,
    collections::BTreeSet,
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

pub struct NebulaPrompt {
    left: String,
    right: String,
    indicator: String,
    multiline: String,
    search: String,
}

impl Prompt for NebulaPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.left)
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.right)
    }

    fn render_prompt_indicator(&self, _prompt_mode: PromptEditMode) -> Cow<'_, str> {
        Cow::Borrowed(&self.indicator)
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.multiline)
    }

    fn render_prompt_history_search_indicator(
        &self,
        _history_search: PromptHistorySearch,
    ) -> Cow<'_, str> {
        Cow::Borrowed(&self.search)
    }
}

pub fn history_path() -> PathBuf {
    platform::data_dir().join("history.txt")
}

pub fn build_editor(config: &Config, cwd: &Path) -> Reedline {
    let mut editor = Reedline::create();

    if config.general.history_enabled {
        let path = history_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(history) = FileBackedHistory::with_file(config.general.history_limit, path) {
            editor = editor.with_history(Box::new(history));
        }
        if config.general.history_ignore_leading_space {
            editor = editor.with_history_exclusion_prefix(Some(" ".into()));
        }
    }

    let completer = Box::new(DefaultCompleter::new_with_wordlen(
        completion_values(config, cwd),
        1,
    ));
    let completion_menu = Box::new(ColumnarMenu::default().with_name("completion_menu"));
    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".into()),
            ReedlineEvent::MenuNext,
        ]),
    );

    let edit_mode = Box::new(Emacs::new(keybindings));
    let hinter = Box::new(
        DefaultHinter::default().with_style(
            Style::new()
                .italic()
                .fg(ui::parse_color(&config.theme.muted)),
        ),
    );

    editor
        .with_hinter(hinter)
        .with_completer(completer)
        .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
        .with_edit_mode(edit_mode)
}

fn completion_values(config: &Config, cwd: &Path) -> Vec<String> {
    let mut values = BTreeSet::new();
    for command in [
        "about",
        "admin",
        "alias",
        "backend",
        "cat",
        "cd",
        "chdir",
        "clear",
        "cls",
        "config",
        "copy",
        "cp",
        "dir",
        "doctor",
        "echo",
        "exit",
        "git",
        "help",
        "history",
        "ipconfig",
        "lang",
        "language",
        "ls",
        "md",
        "mkdir",
        "move",
        "mv",
        "netsh",
        "ping",
        "popd",
        "powershell",
        "pushd",
        "pwsh",
        "pwd",
        "python",
        "reload",
        "set",
        "ssh",
        "sudo",
        "systeminfo",
        "theme",
        "touch",
        "type",
        "ui",
        "version",
        "where",
        "which",
        "winget",
    ] {
        values.insert(command.to_string());
    }

    for alias in config.aliases.keys() {
        values.insert(alias.clone());
    }

    if let Some(path) = env::var_os("PATH") {
        for directory in env::split_paths(&path) {
            let Ok(entries) = fs::read_dir(directory) else {
                continue;
            };

            for entry in entries.flatten() {
                let path = entry.path();
                let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
                    continue;
                };
                if !matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "exe" | "cmd" | "bat" | "com"
                ) {
                    continue;
                }
                if let Some(stem) = path.file_stem().and_then(|name| name.to_str()) {
                    values.insert(stem.to_string());
                }
            }
        }
    }

    if let Ok(entries) = fs::read_dir(cwd) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let mut value = name.to_string();
            if path.is_dir() {
                value.push('\\');
            }
            if value.contains(' ') {
                value = format!("\"{value}\"");
            }
            values.insert(value);
        }
    }

    values.into_iter().collect()
}

pub fn build_prompt(
    cwd: &Path,
    last_code: i32,
    last_duration: Duration,
    config: &Config,
    translator: &Translator,
) -> NebulaPrompt {
    let admin = platform::is_admin();
    let status = if admin {
        ui::paint(&config.theme.admin, &translator.text("status.admin"), true)
    } else {
        ui::paint(&config.theme.success, &translator.text("status.user"), true)
    };

    let identity = match (config.prompt.show_user, config.prompt.show_hostname) {
        (true, true) => format!(
            "{} ",
            ui::paint(
                &config.theme.accent,
                &format!("{}@{}", platform::username(), platform::hostname()),
                false,
            )
        ),
        (true, false) => format!(
            "{} ",
            ui::paint(&config.theme.accent, &platform::username(), false)
        ),
        (false, true) => format!(
            "{} ",
            ui::paint(&config.theme.accent, &platform::hostname(), false)
        ),
        (false, false) => String::new(),
    };

    let cwd_text = ui::paint(&config.theme.path, &compact_path(cwd), true);
    let git = if config.prompt.show_git {
        git_branch(cwd)
            .map(|branch| {
                format!(
                    "  {}",
                    ui::paint(&config.theme.git, &format!("git:{branch}"), false)
                )
            })
            .unwrap_or_default()
    } else {
        String::new()
    };

    let duration = if config.prompt.show_duration && last_duration > Duration::from_millis(10) {
        format!(
            "  {}",
            ui::paint(&config.theme.muted, &format_duration(last_duration), false,)
        )
    } else {
        String::new()
    };

    let exit = if config.prompt.show_exit_code && last_code != 0 {
        format!(
            "  {}",
            ui::paint(&config.theme.error, &format!("exit:{last_code}"), false,)
        )
    } else {
        String::new()
    };

    let indicator = ui::paint(&config.theme.accent, &config.prompt.indicator, true);
    let mut first_line = config.prompt.template.clone();
    for (token, value) in [
        ("{status}", status.as_str()),
        ("{identity}", identity.as_str()),
        ("{cwd}", cwd_text.as_str()),
        ("{git}", git.as_str()),
        ("{duration}", duration.as_str()),
        ("{exit}", exit.as_str()),
    ] {
        first_line = first_line.replace(token, value);
    }

    let (prefix, suffix) = config
        .prompt
        .indicator_line
        .split_once("{indicator}")
        .unwrap_or((&config.prompt.indicator_line, ""));
    let left = format!(
        "{first_line}\n{}",
        ui::paint(&config.theme.muted, prefix, false)
    );
    let right = if config.prompt.show_time {
        ui::paint(&config.theme.muted, &platform::local_time(), false)
    } else {
        String::new()
    };

    NebulaPrompt {
        left,
        right,
        indicator: format!("{indicator}{suffix}"),
        multiline: ui::paint(
            &config.theme.muted,
            &config.prompt.multiline_indicator,
            false,
        ),
        search: ui::paint(&config.theme.muted, "? ", false),
    }
}

pub fn compact_path(path: &Path) -> String {
    if let Some(home) = home_dir() {
        if path == home {
            return "~".into();
        }
        if let Ok(relative) = path.strip_prefix(&home) {
            return format!("~/{}", relative.display()).replace('\\', "/");
        }
    }
    path.display().to_string().replace('\\', "/")
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

fn git_branch(cwd: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!branch.is_empty()).then_some(branch)
}

pub fn format_duration(duration: Duration) -> String {
    if duration < Duration::from_secs(1) {
        format!("{}ms", duration.as_millis())
    } else if duration < Duration::from_secs(60) {
        format!("{:.2}s", duration.as_secs_f64())
    } else {
        format!("{:.1}m", duration.as_secs_f64() / 60.0)
    }
}

pub fn set_terminal_title(cwd: &Path) {
    let admin = if platform::is_admin() { " [ADMIN]" } else { "" };
    print!("\x1b]0;Nebula{admin} — {}\x07", compact_path(cwd));
    let _ = io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_formatting_is_stable() {
        assert_eq!(format_duration(Duration::from_millis(42)), "42ms");
        assert_eq!(format_duration(Duration::from_millis(1500)), "1.50s");
        assert_eq!(format_duration(Duration::from_secs(120)), "2.0m");
    }
}

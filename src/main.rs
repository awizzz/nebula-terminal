mod config;
mod i18n;
mod platform;

use config::{Config, ThemeConfig};
use i18n::Translator;
use nu_ansi_term::{Color, Style};
use reedline::{
    default_emacs_keybindings, ColumnarMenu, DefaultCompleter, DefaultHinter, Emacs,
    FileBackedHistory, KeyCode, KeyModifiers, MenuBuilder, Prompt, PromptEditMode,
    PromptHistorySearch, Reedline, ReedlineEvent, ReedlineMenu, Signal,
};
use std::{
    borrow::Cow,
    collections::BTreeSet,
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

const APP_NAME: &str = "Nebula";
const VERSION: &str = env!("CARGO_PKG_VERSION");

struct ShellState {
    cwd: PathBuf,
    directory_stack: Vec<PathBuf>,
    last_code: i32,
    last_duration: Duration,
}

enum LoopAction {
    Continue,
    RebuildEditor,
    Exit,
}

struct NebulaPrompt {
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

fn main() {
    if let Err(error) = run() {
        eprintln!("Nebula: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    if env::args().any(|arg| arg == "--version" || arg == "-V") {
        println!("{APP_NAME} {VERSION}");
        return Ok(());
    }

    if env::args().any(|arg| arg == "--admin") && !platform::is_admin() {
        platform::relaunch_elevated()?;
        return Ok(());
    }

    let mut config = match Config::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Nebula: {error}");
            Config::default()
        }
    };
    config.apply_environment();

    let mut translator = Translator::from_config(&config);
    let mut state = ShellState {
        cwd: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        directory_stack: Vec::new(),
        last_code: 0,
        last_duration: Duration::ZERO,
    };

    let mut editor = build_editor(&config);
    set_terminal_title(&state.cwd);

    if config.general.show_banner {
        print_banner(&config, &translator);
    }

    loop {
        let prompt = build_prompt(&state, &config, &translator);
        let signal = editor
            .read_line(&prompt)
            .map_err(|e| format!("input error: {e}"))?;

        let line = match signal {
            Signal::Success(line) => line.trim().to_string(),
            Signal::CtrlC => {
                println!("^C");
                state.last_code = 130;
                state.last_duration = Duration::ZERO;
                continue;
            }
            Signal::CtrlD => break,
            _ => continue,
        };

        if line.is_empty() {
            continue;
        }

        let started = Instant::now();
        let action = dispatch(&line, &mut state, &mut config, &mut translator);
        state.last_duration = started.elapsed();
        set_terminal_title(&state.cwd);

        match action {
            LoopAction::Continue => {}
            LoopAction::RebuildEditor => editor = build_editor(&config),
            LoopAction::Exit => break,
        }
    }

    println!("{}", translator.text("msg.goodbye"));
    Ok(())
}

fn build_editor(config: &Config) -> Reedline {
    let mut editor = Reedline::create();
    let history_path = platform::data_dir().join("history.txt");

    if let Some(parent) = history_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(history) = FileBackedHistory::with_file(config.general.history_limit, history_path) {
        editor = editor.with_history(Box::new(history));
    }

    let completer = Box::new(DefaultCompleter::new_with_wordlen(completion_commands(), 1));
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
        DefaultHinter::default()
            .with_style(Style::new().italic().fg(Color::Rgb(127, 132, 156))),
    );

    editor
        .with_hinter(hinter)
        .with_completer(completer)
        .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
        .with_edit_mode(edit_mode)
}

fn completion_commands() -> Vec<String> {
    let mut values = BTreeSet::new();
    for command in [
        "admin", "cd", "chdir", "clear", "cls", "config", "dir", "echo", "exit",
        "git", "help", "ipconfig", "lang", "language", "netsh", "ping", "popd", "powershell",
        "pushd", "pwd", "python", "reload", "set", "ssh", "sudo", "systeminfo", "theme",
        "version", "where", "winget",
    ] {
        values.insert(command.to_string());
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

    values.into_iter().collect()
}

fn build_prompt(state: &ShellState, config: &Config, translator: &Translator) -> NebulaPrompt {
    let admin = platform::is_admin();
    let status = if admin {
        paint(
            &config.theme.admin,
            &translator.text("status.admin"),
            true,
        )
    } else {
        paint(
            &config.theme.success,
            &translator.text("status.user"),
            true,
        )
    };

    let identity = match (config.prompt.show_user, config.prompt.show_hostname) {
        (true, true) => format!(
            "{} ",
            paint(
                &config.theme.accent,
                &format!("{}@{}", platform::username(), platform::hostname()),
                false,
            )
        ),
        (true, false) => format!(
            "{} ",
            paint(&config.theme.accent, &platform::username(), false)
        ),
        (false, true) => format!(
            "{} ",
            paint(&config.theme.accent, &platform::hostname(), false)
        ),
        (false, false) => String::new(),
    };

    let cwd = paint(&config.theme.path, &compact_path(&state.cwd), true);
    let git = if config.prompt.show_git {
        git_branch(&state.cwd)
            .map(|branch| {
                format!(
                    "  {}",
                    paint(&config.theme.git, &format!("git:{branch}"), false)
                )
            })
            .unwrap_or_default()
    } else {
        String::new()
    };

    let duration = if config.prompt.show_duration && state.last_duration > Duration::from_millis(10)
    {
        format!(
            "  {}",
            paint(
                &config.theme.muted,
                &format_duration(state.last_duration),
                false,
            )
        )
    } else {
        String::new()
    };

    let exit = if config.prompt.show_exit_code && state.last_code != 0 {
        format!(
            "  {}",
            paint(
                &config.theme.error,
                &format!("exit:{}", state.last_code),
                false,
            )
        )
    } else {
        String::new()
    };

    let indicator = paint(&config.theme.accent, &config.prompt.indicator, true);
    let mut first_line = config.prompt.template.clone();
    for (token, value) in [
        ("{status}", status.as_str()),
        ("{identity}", identity.as_str()),
        ("{cwd}", cwd.as_str()),
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
        paint(&config.theme.muted, prefix, false)
    );

    NebulaPrompt {
        left,
        right: String::new(),
        indicator: format!("{indicator}{suffix}"),
        multiline: paint(
            &config.theme.muted,
            &config.prompt.multiline_indicator,
            false,
        ),
        search: paint(&config.theme.muted, "? ", false),
    }
}

fn dispatch(
    original: &str,
    state: &mut ShellState,
    config: &mut Config,
    translator: &mut Translator,
) -> LoopAction {
    let line = expand_alias(original, &config.aliases);
    let (command, rest) = split_command(&line);
    let lower = command.to_ascii_lowercase();

    if rest.is_empty() && is_drive_selector(command) {
        switch_drive(command, state, translator, &config.theme);
        return LoopAction::Continue;
    }

    match lower.as_str() {
        "exit" | "quit" if rest.is_empty() => LoopAction::Exit,
        "help" if rest.is_empty() => {
            print_help(translator, &config.theme);
            state.last_code = 0;
            LoopAction::Continue
        }
        "version" if rest.is_empty() => {
            println!("{APP_NAME} {VERSION}");
            state.last_code = 0;
            LoopAction::Continue
        }
        "clear" | "cls" if rest.is_empty() => {
            print!("\x1b[2J\x1b[H");
            let _ = io::stdout().flush();
            state.last_code = 0;
            LoopAction::Continue
        }
        "pwd" if rest.is_empty() => {
            println!("{}", state.cwd.display());
            state.last_code = 0;
            LoopAction::Continue
        }
        "cd" | "chdir" => {
            change_directory(rest, state, translator, &config.theme);
            LoopAction::Continue
        }
        "pushd" => {
            let previous = state.cwd.clone();
            if change_directory(rest, state, translator, &config.theme) {
                state.directory_stack.push(previous);
            }
            LoopAction::Continue
        }
        "popd" if rest.is_empty() => {
            if let Some(path) = state.directory_stack.pop() {
                state.cwd = path;
                let _ = env::set_current_dir(&state.cwd);
                state.last_code = 0;
            } else {
                state.last_code = 1;
            }
            LoopAction::Continue
        }
        "set" => {
            handle_set(rest, state, translator, &config.theme);
            LoopAction::Continue
        }
        "admin" if rest.is_empty() => {
            if platform::is_admin() {
                println!(
                    "{}",
                    paint(
                        &config.theme.warning,
                        &translator.text("msg.already_admin"),
                        false,
                    )
                );
                state.last_code = 0;
                LoopAction::Continue
            } else {
                match platform::relaunch_elevated() {
                    Ok(()) => {
                        println!(
                            "{}",
                            paint(
                                &config.theme.success,
                                &translator.text("msg.elevated_started"),
                                false,
                            )
                        );
                        state.last_code = 0;
                        LoopAction::Exit
                    }
                    Err(error) => {
                        eprintln!(
                            "{}",
                            paint(
                                &config.theme.error,
                                &translator.value("msg.elevation_failed", error),
                                false,
                            )
                        );
                        state.last_code = 1;
                        LoopAction::Continue
                    }
                }
            }
        }
        "sudo" => {
            if rest.is_empty() {
                println!("{}", translator.text("msg.sudo_usage"));
                state.last_code = 1;
            } else {
                state.last_code = match platform::run_command_elevated(rest, &state.cwd) {
                    Ok(()) => 0,
                    Err(error) => {
                        eprintln!(
                            "{}",
                            paint(
                                &config.theme.error,
                                &translator.value("msg.elevation_failed", error),
                                false,
                            )
                        );
                        1
                    }
                };
            }
            LoopAction::Continue
        }
        "config" => handle_config(rest, state, translator, &config.theme),
        "reload" if rest.is_empty() => match Config::load() {
            Ok(new_config) => {
                *config = new_config;
                config.apply_environment();
                *translator = Translator::from_config(config);
                println!(
                    "{}",
                    paint(
                        &config.theme.success,
                        &translator.text("msg.config_reloaded"),
                        false,
                    )
                );
                state.last_code = 0;
                LoopAction::RebuildEditor
            }
            Err(error) => {
                eprintln!(
                    "{}",
                    paint(
                        &config.theme.error,
                        &translator.value("msg.config_error", error),
                        false,
                    )
                );
                state.last_code = 1;
                LoopAction::Continue
            }
        },
        "language" | "lang" => handle_language(rest, state, config, translator),
        "theme" => handle_theme(rest, state, config, translator),
        _ => {
            state.last_code = execute_external(&line, &state.cwd, config, translator);
            LoopAction::Continue
        }
    }
}

fn handle_config(
    rest: &str,
    state: &mut ShellState,
    translator: &Translator,
    theme: &ThemeConfig,
) -> LoopAction {
    let path = Config::path();
    match rest.trim().to_ascii_lowercase().as_str() {
        "" | "edit" => {
            if let Err(error) = platform::open_in_editor(&path) {
                eprintln!("{}", paint(&theme.error, &error, false));
                state.last_code = 1;
            } else {
                state.last_code = 0;
            }
        }
        "path" => {
            println!(
                "{}",
                translator.value("msg.config_path", path.display().to_string())
            );
            state.last_code = 0;
        }
        _ => {
            println!("config [edit|path]");
            state.last_code = 1;
        }
    }
    LoopAction::Continue
}

fn handle_language(
    rest: &str,
    state: &mut ShellState,
    config: &mut Config,
    translator: &mut Translator,
) -> LoopAction {
    let requested = rest.trim();
    if requested.is_empty() {
        let value = if config.general.language.eq_ignore_ascii_case("auto") {
            format!("auto → {}", translator.language())
        } else {
            translator.language().to_string()
        };
        println!("{}", translator.value("msg.language_current", value));
        state.last_code = 0;
        return LoopAction::Continue;
    }

    config.general.language = requested.to_string();
    if let Err(error) = config.save() {
        eprintln!(
            "{}",
            paint(
                &config.theme.error,
                &translator.value("msg.config_error", error),
                false,
            )
        );
        state.last_code = 1;
        return LoopAction::Continue;
    }

    *translator = Translator::from_config(config);
    println!(
        "{}",
        paint(
            &config.theme.success,
            &translator.value("msg.language_set", translator.language()),
            false,
        )
    );
    state.last_code = 0;
    LoopAction::RebuildEditor
}

fn handle_theme(
    rest: &str,
    state: &mut ShellState,
    config: &mut Config,
    translator: &Translator,
) -> LoopAction {
    let requested = rest.trim();
    if requested.is_empty() {
        println!("{}", translator.text("msg.themes"));
        println!("current: {}", config.theme.preset);
        state.last_code = 0;
        return LoopAction::Continue;
    }

    if !config.apply_theme_preset(requested) {
        println!(
            "{}",
            paint(
                &config.theme.error,
                &translator.value("msg.theme_unknown", requested),
                false,
            )
        );
        println!("{}", translator.text("msg.themes"));
        state.last_code = 1;
        return LoopAction::Continue;
    }

    match config.save() {
        Ok(()) => {
            println!(
                "{}",
                paint(
                    &config.theme.success,
                    &translator.value("msg.theme_set", requested),
                    false,
                )
            );
            state.last_code = 0;
        }
        Err(error) => {
            eprintln!(
                "{}",
                paint(
                    &config.theme.error,
                    &translator.value("msg.config_error", error),
                    false,
                )
            );
            state.last_code = 1;
        }
    }
    LoopAction::Continue
}

fn change_directory(
    rest: &str,
    state: &mut ShellState,
    translator: &Translator,
    theme: &ThemeConfig,
) -> bool {
    let target = resolve_directory(rest, &state.cwd);
    let Some(target) = target else {
        println!(
            "{}",
            paint(
                &theme.error,
                &translator.text("msg.path_not_found"),
                false,
            )
        );
        state.last_code = 1;
        return false;
    };

    if !target.exists() {
        println!(
            "{}",
            paint(
                &theme.error,
                &translator.text("msg.path_not_found"),
                false,
            )
        );
        state.last_code = 1;
        return false;
    }

    if !target.is_dir() {
        println!(
            "{}",
            paint(
                &theme.error,
                &translator.value("msg.not_directory", target.display().to_string()),
                false,
            )
        );
        state.last_code = 1;
        return false;
    }

    match env::set_current_dir(&target) {
        Ok(()) => {
            state.cwd = target;
            state.last_code = 0;
            true
        }
        Err(error) => {
            eprintln!("{}", paint(&theme.error, &error.to_string(), false));
            state.last_code = 1;
            false
        }
    }
}

fn is_drive_selector(command: &str) -> bool {
    let bytes = command.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn switch_drive(
    drive: &str,
    state: &mut ShellState,
    translator: &Translator,
    theme: &ThemeConfig,
) {
    let root = format!("{}\\", drive.to_ascii_uppercase());
    if !Path::new(&root).exists() {
        println!(
            "{}",
            paint(
                &theme.error,
                &translator.text("msg.path_not_found"),
                false,
            )
        );
        state.last_code = 1;
        return;
    }

    let target = PathBuf::from(root);
    match env::set_current_dir(&target) {
        Ok(()) => {
            state.cwd = target;
            state.last_code = 0;
        }
        Err(error) => {
            eprintln!("{}", paint(&theme.error, &error.to_string(), false));
            state.last_code = 1;
        }
    }
}

fn resolve_directory(raw: &str, cwd: &Path) -> Option<PathBuf> {
    let mut raw = raw.trim();
    if raw.to_ascii_lowercase().starts_with("/d ") {
        raw = raw[3..].trim();
    }

    if raw.is_empty() {
        return home_dir();
    }

    let raw = raw.trim_matches('"');
    let expanded = expand_environment(raw);
    let path = PathBuf::from(expanded);
    let path = if path.is_absolute() { path } else { cwd.join(path) };
    Some(path)
}

fn handle_set(
    rest: &str,
    state: &mut ShellState,
    translator: &Translator,
    theme: &ThemeConfig,
) {
    let rest = rest.trim();
    if rest.is_empty() {
        let mut vars: Vec<_> = env::vars().collect();
        vars.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, value) in vars {
            println!("{key}={value}");
        }
        state.last_code = 0;
        return;
    }

    if let Some((name, value)) = rest.split_once('=') {
        let name = name.trim();
        if name.is_empty() {
            println!(
                "{}",
                paint(
                    &theme.error,
                    &translator.text("msg.invalid_variable"),
                    false,
                )
            );
            state.last_code = 1;
        } else if value.is_empty() {
            env::remove_var(name);
            state.last_code = 0;
        } else {
            env::set_var(name, value);
            state.last_code = 0;
        }
        return;
    }

    let needle = rest.to_ascii_uppercase();
    let matches: Vec<_> = env::vars()
        .filter(|(key, _)| key.to_ascii_uppercase().starts_with(&needle))
        .collect();
    if matches.is_empty() {
        println!("{}", translator.text("msg.variable_not_found"));
        state.last_code = 1;
    } else {
        for (key, value) in matches {
            println!("{key}={value}");
        }
        state.last_code = 0;
    }
}

fn execute_external(line: &str, cwd: &Path, config: &Config, translator: &Translator) -> i32 {
    let backend = config.general.backend.to_ascii_lowercase();
    let status = match backend.as_str() {
        "cmd" => Command::new("cmd.exe")
            .args(["/d", "/s", "/c", line])
            .current_dir(cwd)
            .status(),
        "powershell" => Command::new("powershell.exe")
            .args(["-NoLogo", "-NoProfile", "-Command", line])
            .current_dir(cwd)
            .status(),
        "pwsh" => Command::new("pwsh.exe")
            .args(["-NoLogo", "-NoProfile", "-Command", line])
            .current_dir(cwd)
            .status(),
        _ => {
            eprintln!(
                "{}",
                translator.value("msg.backend_unknown", &config.general.backend)
            );
            Command::new("cmd.exe")
                .args(["/d", "/s", "/c", line])
                .current_dir(cwd)
                .status()
        }
    };

    match status {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("Nebula: {error}");
            1
        }
    }
}

fn expand_alias(
    line: &str,
    aliases: &std::collections::BTreeMap<String, String>,
) -> String {
    let (command, rest) = split_command(line);
    if let Some(replacement) = aliases.get(command) {
        if rest.is_empty() {
            replacement.clone()
        } else {
            format!("{replacement} {rest}")
        }
    } else {
        line.to_string()
    }
}

fn split_command(line: &str) -> (&str, &str) {
    match line.find(char::is_whitespace) {
        Some(index) => (&line[..index], line[index..].trim_start()),
        None => (line, ""),
    }
}

fn expand_environment(input: &str) -> String {
    let mut output = input.to_string();
    for (key, value) in env::vars() {
        output = output.replace(&format!("%{key}%"), &value);
    }

    if let Some(home) = home_dir() {
        if output == "~" {
            return home.display().to_string();
        }
        if output.starts_with("~/") || output.starts_with("~\\") {
            return home.join(&output[2..]).display().to_string();
        }
    }
    output
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

fn compact_path(path: &Path) -> String {
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

fn format_duration(duration: Duration) -> String {
    if duration < Duration::from_secs(1) {
        format!("{}ms", duration.as_millis())
    } else if duration < Duration::from_secs(60) {
        format!("{:.2}s", duration.as_secs_f64())
    } else {
        format!("{:.1}m", duration.as_secs_f64() / 60.0)
    }
}

fn print_banner(config: &Config, translator: &Translator) {
    let mode = if platform::is_admin() {
        paint(
            &config.theme.admin,
            &translator.text("status.admin"),
            true,
        )
    } else {
        paint(
            &config.theme.success,
            &translator.text("status.user"),
            true,
        )
    };

    println!();
    println!(
        "  {}  {}",
        paint(&config.theme.accent, APP_NAME, true),
        paint(&config.theme.muted, &format!("v{VERSION}"), false)
    );
    println!(
        "  {}",
        paint(
            &config.theme.foreground,
            &translator.text("app.tagline"),
            false,
        )
    );
    println!(
        "  {} · {} · {}",
        mode,
        paint(&config.theme.muted, &config.general.backend, false),
        paint(&config.theme.muted, translator.language(), false)
    );
    println!(
        "  {}",
        paint(
            &config.theme.muted,
            &translator.text("banner.hint"),
            false,
        )
    );
    println!();
}

fn print_help(translator: &Translator, theme: &ThemeConfig) {
    println!(
        "{}",
        paint(&theme.accent, &translator.text("help.title"), true)
    );
    println!();
    for key in [
        "help.admin",
        "help.sudo",
        "help.cd",
        "help.pwd",
        "help.config",
        "help.reload",
        "help.language",
        "help.theme",
        "help.clear",
        "help.version",
        "help.exit",
    ] {
        println!("  {}", translator.text(key));
    }
    println!();
    println!(
        "{}",
        paint(&theme.muted, &translator.text("help.external"), false)
    );
}

fn parse_color(value: &str) -> Color {
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

fn paint(color: &str, text: &str, bold: bool) -> String {
    let mut style = Style::new().fg(parse_color(color));
    if bold {
        style = style.bold();
    }
    style.paint(text).to_string()
}

fn set_terminal_title(cwd: &Path) {
    let admin = if platform::is_admin() { " [ADMIN]" } else { "" };
    print!(
        "\x1b]0;{APP_NAME}{admin} — {}\x07",
        compact_path(cwd)
    );
    let _ = io::stdout().flush();
}

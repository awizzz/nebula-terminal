use crate::{
    config::{Config, ThemeConfig},
    editor,
    i18n::Translator,
    platform, ui,
};
use reedline::Signal;
use std::{
    collections::{BTreeMap, BTreeSet},
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
    previous_directory: Option<PathBuf>,
    directory_stack: Vec<PathBuf>,
    drive_directories: BTreeMap<char, PathBuf>,
    last_code: i32,
    last_duration: Duration,
}

impl ShellState {
    fn new() -> Self {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut state = Self {
            cwd,
            previous_directory: None,
            directory_stack: Vec::new(),
            drive_directories: BTreeMap::new(),
            last_code: 0,
            last_duration: Duration::ZERO,
        };
        state.remember_drive();
        state
    }

    fn remember_drive(&mut self) {
        if let Some(letter) = drive_letter(&self.cwd) {
            self.drive_directories.insert(letter, self.cwd.clone());
        }
    }
}

enum LoopAction {
    Continue,
    RebuildEditor,
    Exit,
}

pub fn run() -> Result<(), String> {
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
    let mut state = ShellState::new();
    let mut line_editor = editor::build_editor(&config, &state.cwd);
    editor::set_terminal_title(&state.cwd);

    if config.general.show_banner && !config.ui.banner_style.eq_ignore_ascii_case("off") {
        ui::startup(&config);
        ui::banner(&config, &translator, VERSION);
    }

    loop {
        let prompt = editor::build_prompt(
            &state.cwd,
            state.last_code,
            state.last_duration,
            &config,
            &translator,
        );
        let signal = line_editor
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
        editor::set_terminal_title(&state.cwd);

        match action {
            LoopAction::Continue => ui::separator(&config),
            LoopAction::RebuildEditor => {
                ui::separator(&config);
                line_editor = editor::build_editor(&config, &state.cwd);
            }
            LoopAction::Exit => break,
        }
    }

    println!("{}", translator.text("msg.goodbye"));
    Ok(())
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
        let changed = switch_drive(command, state, translator, &config.theme);
        return if changed {
            LoopAction::RebuildEditor
        } else {
            LoopAction::Continue
        };
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
        "about" if rest.is_empty() => {
            ui::about(config, translator, VERSION);
            state.last_code = 0;
            LoopAction::Continue
        }
        "doctor" if rest.is_empty() => {
            run_doctor(state, config, translator);
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
            let changed = change_directory(rest, state, translator, &config.theme);
            if changed {
                LoopAction::RebuildEditor
            } else {
                LoopAction::Continue
            }
        }
        "pushd" => {
            let previous = state.cwd.clone();
            if change_directory(rest, state, translator, &config.theme) {
                state.directory_stack.push(previous);
                LoopAction::RebuildEditor
            } else {
                LoopAction::Continue
            }
        }
        "popd" if rest.is_empty() => {
            if let Some(path) = state.directory_stack.pop() {
                let previous = state.cwd.clone();
                match env::set_current_dir(&path) {
                    Ok(()) => {
                        state.previous_directory = Some(previous);
                        state.cwd = env::current_dir().unwrap_or(path);
                        state.remember_drive();
                        state.last_code = 0;
                        LoopAction::RebuildEditor
                    }
                    Err(error) => {
                        eprintln!(
                            "{}",
                            ui::paint(&config.theme.error, &error.to_string(), false)
                        );
                        state.last_code = 1;
                        LoopAction::Continue
                    }
                }
            } else {
                println!("{}", translator.text("msg.directory_stack_empty"));
                state.last_code = 1;
                LoopAction::Continue
            }
        }
        "set" => {
            handle_set(rest, state, translator, &config.theme);
            LoopAction::Continue
        }
        "admin" if rest.is_empty() => handle_admin(state, config, translator),
        "sudo" => {
            handle_sudo(rest, state, config, translator);
            LoopAction::Continue
        }
        "config" => handle_config(rest, state, translator, &config.theme),
        "reload" if rest.is_empty() => handle_reload(state, config, translator),
        "language" | "lang" => handle_language(rest, state, config, translator),
        "backend" => handle_backend(rest, state, config, translator),
        "theme" => handle_theme(rest, state, config, translator),
        "ui" => handle_ui(rest, state, config, translator),
        "alias" => handle_alias(rest, state, config, translator),
        "history" => handle_history(rest, state, config, translator),
        _ => {
            state.last_code = execute_external(&line, &state.cwd, config, translator);
            LoopAction::Continue
        }
    }
}

fn handle_admin(state: &mut ShellState, config: &Config, translator: &Translator) -> LoopAction {
    if platform::is_admin() {
        println!(
            "{}",
            ui::paint(
                &config.theme.warning,
                &translator.text("msg.already_admin"),
                false,
            )
        );
        state.last_code = 0;
        return LoopAction::Continue;
    }

    match platform::relaunch_elevated() {
        Ok(()) => {
            println!(
                "{}",
                ui::paint(
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
                ui::paint(
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

fn handle_sudo(rest: &str, state: &mut ShellState, config: &Config, translator: &Translator) {
    if rest.is_empty() {
        println!("{}", translator.text("msg.sudo_usage"));
        state.last_code = 1;
        return;
    }

    state.last_code =
        match platform::run_command_elevated(rest, &state.cwd, &config.general.backend) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!(
                    "{}",
                    ui::paint(
                        &config.theme.error,
                        &translator.value("msg.elevation_failed", error),
                        false,
                    )
                );
                1
            }
        };
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
                eprintln!("{}", ui::paint(&theme.error, &error, false));
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
        "check" => match Config::load() {
            Ok(config) => match config.validate() {
                Ok(()) => {
                    println!("{}", translator.text("msg.config_valid"));
                    state.last_code = 0;
                }
                Err(error) => {
                    println!(
                        "{}",
                        ui::paint(
                            &theme.error,
                            &translator.value("msg.config_error", error),
                            false,
                        )
                    );
                    state.last_code = 1;
                }
            },
            Err(error) => {
                println!(
                    "{}",
                    ui::paint(
                        &theme.error,
                        &translator.value("msg.config_error", error),
                        false,
                    )
                );
                state.last_code = 1;
            }
        },
        _ => {
            println!("{}", translator.text("msg.config_usage"));
            state.last_code = 1;
        }
    }
    LoopAction::Continue
}

fn handle_reload(
    state: &mut ShellState,
    config: &mut Config,
    translator: &mut Translator,
) -> LoopAction {
    match Config::load() {
        Ok(new_config) => {
            *config = new_config;
            config.apply_environment();
            *translator = Translator::from_config(config);
            ui::pulse(config, &translator.text("msg.config_reloaded"));
            state.last_code = 0;
            LoopAction::RebuildEditor
        }
        Err(error) => {
            eprintln!(
                "{}",
                ui::paint(
                    &config.theme.error,
                    &translator.value("msg.config_error", error),
                    false,
                )
            );
            state.last_code = 1;
            LoopAction::Continue
        }
    }
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
            ui::paint(
                &config.theme.error,
                &translator.value("msg.config_error", error),
                false,
            )
        );
        state.last_code = 1;
        return LoopAction::Continue;
    }

    *translator = Translator::from_config(config);
    ui::pulse(
        config,
        &translator.value("msg.language_set", translator.language()),
    );
    state.last_code = 0;
    LoopAction::RebuildEditor
}

fn handle_backend(
    rest: &str,
    state: &mut ShellState,
    config: &mut Config,
    translator: &Translator,
) -> LoopAction {
    let requested = rest.trim().to_ascii_lowercase();
    if requested.is_empty() {
        println!(
            "{}",
            translator.value("msg.backend_current", &config.general.backend)
        );
        state.last_code = 0;
        return LoopAction::Continue;
    }

    let executable = match requested.as_str() {
        "cmd" => "cmd.exe",
        "powershell" => "powershell.exe",
        "pwsh" => "pwsh.exe",
        _ => {
            println!(
                "{}",
                ui::paint(
                    &config.theme.error,
                    &translator.value("msg.backend_unknown", requested),
                    false,
                )
            );
            state.last_code = 1;
            return LoopAction::Continue;
        }
    };

    if !platform::command_exists(executable) {
        println!(
            "{}",
            ui::paint(
                &config.theme.error,
                &translator.value("msg.backend_unavailable", requested),
                false,
            )
        );
        state.last_code = 1;
        return LoopAction::Continue;
    }

    config.general.backend = requested.clone();
    match config.save() {
        Ok(()) => {
            ui::pulse(config, &translator.value("msg.backend_set", requested));
            state.last_code = 0;
        }
        Err(error) => {
            eprintln!("{}", ui::paint(&config.theme.error, &error, false));
            state.last_code = 1;
        }
    }
    LoopAction::Continue
}

fn handle_theme(
    rest: &str,
    state: &mut ShellState,
    config: &mut Config,
    translator: &Translator,
) -> LoopAction {
    let requested = rest.trim();
    if requested.is_empty() {
        ui::theme_preview(&config.theme, &config.theme.preset, translator);
        state.last_code = 0;
        return LoopAction::Continue;
    }

    if !config.apply_theme_preset(requested) {
        println!(
            "{}",
            ui::paint(
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
            ui::pulse(config, &translator.value("msg.theme_set", requested));
            ui::theme_preview(&config.theme, &config.theme.preset, translator);
            state.last_code = 0;
        }
        Err(error) => {
            eprintln!(
                "{}",
                ui::paint(
                    &config.theme.error,
                    &translator.value("msg.config_error", error),
                    false,
                )
            );
            state.last_code = 1;
        }
    }
    LoopAction::RebuildEditor
}

fn handle_ui(
    rest: &str,
    state: &mut ShellState,
    config: &mut Config,
    translator: &Translator,
) -> LoopAction {
    let parts: Vec<_> = rest.split_whitespace().collect();
    if parts.is_empty() || parts[0].eq_ignore_ascii_case("status") {
        ui::ui_status(config, translator);
        state.last_code = 0;
        return LoopAction::Continue;
    }

    if parts[0].eq_ignore_ascii_case("demo") {
        ui::startup(config);
        ui::banner(config, translator, VERSION);
        ui::theme_preview(&config.theme, &config.theme.preset, translator);
        state.last_code = 0;
        return LoopAction::Continue;
    }

    if parts[0].eq_ignore_ascii_case("help") {
        for key in [
            "ui.help.animations",
            "ui.help.speed",
            "ui.help.banner",
            "ui.help.tips",
            "ui.help.separator",
            "ui.help.demo",
            "ui.help.reset",
        ] {
            println!("{}", translator.text(key));
        }
        state.last_code = 0;
        return LoopAction::Continue;
    }

    let mut changed = false;
    match parts.as_slice() {
        ["animations", value] => {
            if let Some(value) = parse_on_off(value) {
                config.ui.animations = value;
                changed = true;
            }
        }
        ["speed", value] => {
            if let Ok(value) = value.parse::<u64>() {
                config.ui.animation_speed_ms = value.clamp(8, 250);
                changed = true;
            }
        }
        ["banner", value]
            if matches!(
                value.to_ascii_lowercase().as_str(),
                "aurora" | "minimal" | "compact" | "off"
            ) =>
        {
            config.ui.banner_style = value.to_ascii_lowercase();
            changed = true;
        }
        ["tips", value] => {
            if let Some(value) = parse_on_off(value) {
                config.ui.show_tips = value;
                changed = true;
            }
        }
        ["separator", value] => {
            if let Some(value) = parse_on_off(value) {
                config.ui.command_separator = value;
                changed = true;
            }
        }
        ["reset"] => {
            ui::reset_ui(&mut config.ui);
            changed = true;
        }
        _ => {}
    }

    if !changed {
        println!(
            "{}",
            ui::paint(
                &config.theme.error,
                &translator.text("msg.ui_invalid"),
                false,
            )
        );
        state.last_code = 1;
        return LoopAction::Continue;
    }

    match config.save() {
        Ok(()) => {
            let message = if parts.first().is_some_and(|value| *value == "reset") {
                translator.text("msg.ui_reset")
            } else {
                translator.text("msg.ui_saved")
            };
            ui::pulse(config, &message);
            state.last_code = 0;
            LoopAction::RebuildEditor
        }
        Err(error) => {
            eprintln!("{}", ui::paint(&config.theme.error, &error, false));
            state.last_code = 1;
            LoopAction::Continue
        }
    }
}

fn parse_on_off(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "yes" | "oui" => Some(true),
        "off" | "false" | "0" | "no" | "non" => Some(false),
        _ => None,
    }
}

fn handle_alias(
    rest: &str,
    state: &mut ShellState,
    config: &mut Config,
    translator: &Translator,
) -> LoopAction {
    let rest = rest.trim();
    if rest.is_empty() {
        if config.aliases.is_empty() {
            println!("{}", translator.text("msg.alias_none"));
        } else {
            for (name, command) in &config.aliases {
                println!("{name} = {command}");
            }
        }
        state.last_code = 0;
        return LoopAction::Continue;
    }

    if let Some(name) = rest.strip_prefix("rm ").map(str::trim) {
        let key = name.to_ascii_lowercase();
        if config.aliases.remove(&key).is_some() {
            match config.save() {
                Ok(()) => {
                    ui::pulse(config, &translator.value("msg.alias_removed", name));
                    state.last_code = 0;
                    return LoopAction::RebuildEditor;
                }
                Err(error) => {
                    eprintln!("{}", ui::paint(&config.theme.error, &error, false));
                    state.last_code = 1;
                    return LoopAction::Continue;
                }
            }
        }
        println!("{}", translator.value("msg.alias_missing", name));
        state.last_code = 1;
        return LoopAction::Continue;
    }

    if let Some((name, value)) = rest.split_once('=') {
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        if name.is_empty() || value.is_empty() {
            state.last_code = 1;
            return LoopAction::Continue;
        }
        config.aliases.insert(name.clone(), value.to_string());
        match config.save() {
            Ok(()) => {
                ui::pulse(config, &translator.value("msg.alias_set", &name));
                state.last_code = 0;
                LoopAction::RebuildEditor
            }
            Err(error) => {
                eprintln!("{}", ui::paint(&config.theme.error, &error, false));
                state.last_code = 1;
                LoopAction::Continue
            }
        }
    } else {
        let key = rest.to_ascii_lowercase();
        if let Some(value) = config.aliases.get(&key) {
            println!("{key} = {value}");
            state.last_code = 0;
        } else {
            println!("{}", translator.value("msg.alias_missing", rest));
            state.last_code = 1;
        }
        LoopAction::Continue
    }
}

fn handle_history(
    rest: &str,
    state: &mut ShellState,
    config: &mut Config,
    translator: &Translator,
) -> LoopAction {
    let path = editor::history_path();
    match rest.trim().to_ascii_lowercase().as_str() {
        "" | "status" => {
            let status = if config.general.history_enabled {
                translator.text("ui.on")
            } else {
                translator.text("ui.off")
            };
            println!("{}", translator.value("msg.history_status", status));
            println!(
                "{}",
                translator.value("msg.history_path", path.display().to_string())
            );
            state.last_code = 0;
            LoopAction::Continue
        }
        "path" => {
            println!(
                "{}",
                translator.value("msg.history_path", path.display().to_string())
            );
            state.last_code = 0;
            LoopAction::Continue
        }
        "clear" => {
            let _ = fs::remove_file(&path);
            println!("{}", translator.text("msg.history_cleared"));
            state.last_code = 0;
            LoopAction::RebuildEditor
        }
        "on" | "off" => {
            config.general.history_enabled = rest.trim().eq_ignore_ascii_case("on");
            match config.save() {
                Ok(()) => {
                    let key = if config.general.history_enabled {
                        "msg.history_enabled"
                    } else {
                        "msg.history_disabled"
                    };
                    ui::pulse(config, &translator.text(key));
                    state.last_code = 0;
                    LoopAction::RebuildEditor
                }
                Err(error) => {
                    eprintln!("{}", ui::paint(&config.theme.error, &error, false));
                    state.last_code = 1;
                    LoopAction::Continue
                }
            }
        }
        _ => {
            println!("{}", translator.text("msg.history_usage"));
            state.last_code = 1;
            LoopAction::Continue
        }
    }
}

fn run_doctor(state: &mut ShellState, config: &Config, translator: &Translator) {
    println!();
    println!(
        "  {}",
        ui::paint(&config.theme.accent, &translator.text("doctor.title"), true)
    );
    println!();

    let data_dir = platform::data_dir();
    let config_dir_ok = fs::create_dir_all(&data_dir).is_ok();
    print_check(
        config_dir_ok,
        &translator.text("doctor.config"),
        &data_dir.display().to_string(),
        config,
        translator,
    );

    let config_ok = config.validate().is_ok();
    print_check(
        config_ok,
        &translator.text("doctor.config_valid"),
        &Config::path().display().to_string(),
        config,
        translator,
    );

    let backend_exe = match config.general.backend.as_str() {
        "powershell" => "powershell.exe",
        "pwsh" => "pwsh.exe",
        _ => "cmd.exe",
    };
    let backend_ok = platform::command_exists(backend_exe);
    print_check(
        backend_ok,
        &translator.text("doctor.backend"),
        backend_exe,
        config,
        translator,
    );

    print_check(
        platform::command_exists("git.exe"),
        &translator.text("doctor.git"),
        "git.exe",
        config,
        translator,
    );
    print_check(
        true,
        &translator.text("doctor.terminal"),
        &platform::terminal_name(),
        config,
        translator,
    );
    print_check(
        ui::animations_available(),
        &translator.text("doctor.animations"),
        &translator.text(if ui::animations_available() {
            "doctor.tty"
        } else {
            "doctor.non_tty"
        }),
        config,
        translator,
    );
    print_check(
        true,
        &translator.text("doctor.history"),
        &translator.text(if config.general.history_enabled {
            "ui.on"
        } else {
            "ui.off"
        }),
        config,
        translator,
    );
    print_check(
        true,
        &translator.text("doctor.admin"),
        &translator.text(if platform::is_admin() {
            "status.admin"
        } else {
            "status.user"
        }),
        config,
        translator,
    );
    println!();

    state.last_code = if config_dir_ok && config_ok && backend_ok {
        0
    } else {
        1
    };
}

fn print_check(ok: bool, label: &str, detail: &str, config: &Config, translator: &Translator) {
    let (color, mark, status_key) = if ok {
        (&config.theme.success, "●", "doctor.ok")
    } else {
        (&config.theme.warning, "○", "doctor.warn")
    };
    println!(
        "  {} {:<24} {}  {}",
        ui::paint(color, mark, true),
        label,
        ui::paint(color, &translator.text(status_key), true),
        ui::paint(&config.theme.muted, detail, false)
    );
}

fn change_directory(
    rest: &str,
    state: &mut ShellState,
    translator: &Translator,
    theme: &ThemeConfig,
) -> bool {
    let target = if rest.trim() == "-" {
        match state.previous_directory.clone() {
            Some(path) => path,
            None => {
                println!(
                    "{}",
                    ui::paint(
                        &theme.warning,
                        &translator.text("msg.previous_directory_missing"),
                        false,
                    )
                );
                state.last_code = 1;
                return false;
            }
        }
    } else {
        let Some(target) = resolve_directory(rest, &state.cwd) else {
            println!(
                "{}",
                ui::paint(&theme.error, &translator.text("msg.path_not_found"), false)
            );
            state.last_code = 1;
            return false;
        };
        target
    };

    if !target.exists() {
        println!(
            "{}",
            ui::paint(&theme.error, &translator.text("msg.path_not_found"), false)
        );
        state.last_code = 1;
        return false;
    }

    if !target.is_dir() {
        println!(
            "{}",
            ui::paint(
                &theme.error,
                &translator.value("msg.not_directory", target.display().to_string()),
                false,
            )
        );
        state.last_code = 1;
        return false;
    }

    let previous = state.cwd.clone();
    match env::set_current_dir(&target) {
        Ok(()) => {
            state.previous_directory = Some(previous);
            state.cwd = env::current_dir().unwrap_or(target);
            state.remember_drive();
            state.last_code = 0;
            true
        }
        Err(error) => {
            eprintln!("{}", ui::paint(&theme.error, &error.to_string(), false));
            state.last_code = 1;
            false
        }
    }
}

fn is_drive_selector(command: &str) -> bool {
    let bytes = command.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn drive_letter(path: &Path) -> Option<char> {
    let text = path.to_string_lossy();
    let bytes = text.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        Some((bytes[0] as char).to_ascii_uppercase())
    } else {
        None
    }
}

fn switch_drive(
    drive: &str,
    state: &mut ShellState,
    translator: &Translator,
    theme: &ThemeConfig,
) -> bool {
    let letter = drive
        .chars()
        .next()
        .map(|value| value.to_ascii_uppercase())
        .unwrap_or('C');
    let root = PathBuf::from(format!("{letter}:\\"));
    let target = state
        .drive_directories
        .get(&letter)
        .filter(|path| path.exists())
        .cloned()
        .unwrap_or(root);

    if !target.exists() {
        println!(
            "{}",
            ui::paint(&theme.error, &translator.text("msg.path_not_found"), false)
        );
        state.last_code = 1;
        return false;
    }

    let previous = state.cwd.clone();
    match env::set_current_dir(&target) {
        Ok(()) => {
            state.previous_directory = Some(previous);
            state.cwd = env::current_dir().unwrap_or(target);
            state.remember_drive();
            state.last_code = 0;
            true
        }
        Err(error) => {
            eprintln!("{}", ui::paint(&theme.error, &error.to_string(), false));
            state.last_code = 1;
            false
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
    let mut expanded = platform::expand_environment(raw);
    if let Some(home) = home_dir() {
        if expanded == "~" {
            expanded = home.display().to_string();
        } else if expanded.starts_with("~/") || expanded.starts_with("~\\") {
            expanded = home.join(&expanded[2..]).display().to_string();
        }
    }

    let path = PathBuf::from(expanded);
    Some(if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    })
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

fn handle_set(rest: &str, state: &mut ShellState, translator: &Translator, theme: &ThemeConfig) {
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
                ui::paint(
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

fn expand_alias(line: &str, aliases: &BTreeMap<String, String>) -> String {
    let mut current = line.to_string();
    let mut seen = BTreeSet::new();

    for _ in 0..8 {
        let (command, rest) = split_command(&current);
        let key = command.to_ascii_lowercase();
        let Some(replacement) = aliases.get(&key) else {
            break;
        };
        if !seen.insert(key) {
            break;
        }
        current = if rest.is_empty() {
            replacement.clone()
        } else {
            format!("{replacement} {rest}")
        };
    }

    current
}

fn split_command(line: &str) -> (&str, &str) {
    match line.find(char::is_whitespace) {
        Some(index) => (&line[..index], line[index..].trim_start()),
        None => (line, ""),
    }
}

fn print_help(translator: &Translator, theme: &ThemeConfig) {
    println!(
        "{}",
        ui::paint(&theme.accent, &translator.text("help.title"), true)
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
        "help.backend",
        "help.theme",
        "help.ui",
        "help.alias",
        "help.history",
        "help.about",
        "help.doctor",
        "help.clear",
        "help.version",
        "help.exit",
    ] {
        println!("  {}", translator.text(key));
    }
    println!();
    println!(
        "{}",
        ui::paint(&theme.muted, &translator.text("help.external"), false)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_drive_selectors() {
        assert!(is_drive_selector("C:"));
        assert!(is_drive_selector("d:"));
        assert!(!is_drive_selector("C:\\"));
        assert!(!is_drive_selector("cd"));
    }

    #[test]
    fn splits_command_and_arguments() {
        assert_eq!(split_command("git status"), ("git", "status"));
        assert_eq!(split_command("pwd"), ("pwd", ""));
    }

    #[test]
    fn expands_nested_aliases_without_looping_forever() {
        let aliases = BTreeMap::from([
            ("g".to_string(), "git".to_string()),
            ("gs".to_string(), "g status".to_string()),
        ]);
        assert_eq!(expand_alias("gs --short", &aliases), "git status --short");

        let looping = BTreeMap::from([
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "a".to_string()),
        ]);
        assert!(matches!(expand_alias("a", &looping).as_str(), "a" | "b"));
    }

    #[test]
    fn parses_boolean_ui_values() {
        assert_eq!(parse_on_off("on"), Some(true));
        assert_eq!(parse_on_off("non"), Some(false));
        assert_eq!(parse_on_off("maybe"), None);
    }
}

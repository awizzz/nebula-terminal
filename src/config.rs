use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf};

use crate::platform;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub ui: UiConfig,
    pub prompt: PromptConfig,
    pub theme: ThemeConfig,
    pub aliases: BTreeMap<String, String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub language: String,
    pub backend: String,
    pub show_banner: bool,
    pub history_limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub animations: bool,
    pub animation_speed_ms: u64,
    pub banner_style: String,
    pub show_tips: bool,
    pub command_separator: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PromptConfig {
    pub template: String,
    pub indicator_line: String,
    pub indicator: String,
    pub multiline_indicator: String,
    pub show_git: bool,
    pub show_duration: bool,
    pub show_exit_code: bool,
    pub show_user: bool,
    pub show_hostname: bool,
    pub show_time: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub preset: String,
    pub foreground: String,
    pub muted: String,
    pub accent: String,
    pub path: String,
    pub git: String,
    pub success: String,
    pub warning: String,
    pub error: String,
    pub admin: String,
    pub panel: String,
}

impl Default for Config {
    fn default() -> Self {
        let mut aliases = BTreeMap::new();
        aliases.insert("ll".into(), "dir".into());
        aliases.insert("gs".into(), "git status".into());

        Self {
            general: GeneralConfig::default(),
            ui: UiConfig::default(),
            prompt: PromptConfig::default(),
            theme: ThemeConfig::default(),
            aliases,
            env: BTreeMap::new(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            language: "auto".into(),
            backend: "cmd".into(),
            show_banner: true,
            history_limit: 10_000,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            animations: true,
            animation_speed_ms: 28,
            banner_style: "aurora".into(),
            show_tips: true,
            command_separator: false,
        }
    }
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            template: "╭─ {status} {identity}{cwd}{git}{duration}{exit}".into(),
            indicator_line: "╰─{indicator} ".into(),
            indicator: "❯".into(),
            multiline_indicator: "· ".into(),
            show_git: true,
            show_duration: true,
            show_exit_code: true,
            show_user: false,
            show_hostname: false,
            show_time: false,
        }
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            preset: "hypr".into(),
            foreground: "#CDD6F4".into(),
            muted: "#7F849C".into(),
            accent: "#89DCEB".into(),
            path: "#89B4FA".into(),
            git: "#CBA6F7".into(),
            success: "#A6E3A1".into(),
            warning: "#F9E2AF".into(),
            error: "#F38BA8".into(),
            admin: "#F38BA8".into(),
            panel: "#313244".into(),
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        platform::data_dir().join("config.toml")
    }

    pub fn load() -> Result<Self, String> {
        let path = Self::path();
        if !path.exists() {
            let cfg = Self::default();
            cfg.save()?;
            return Ok(cfg);
        }

        let raw = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        toml::from_str(&raw).map_err(|e| format!("invalid config {}: {e}", path.display()))
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
        }

        let raw = toml::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize config: {e}"))?;
        fs::write(&path, raw).map_err(|e| format!("failed to write {}: {e}", path.display()))
    }

    pub fn apply_theme_preset(&mut self, name: &str) -> bool {
        let colors = match name.to_ascii_lowercase().as_str() {
            "hypr" => ["#CDD6F4", "#7F849C", "#89DCEB", "#89B4FA", "#CBA6F7", "#A6E3A1", "#F9E2AF", "#F38BA8", "#F38BA8", "#313244"],
            "tokyo-night" | "tokyo" => ["#C0CAF5", "#565F89", "#7DCFFF", "#7AA2F7", "#BB9AF7", "#9ECE6A", "#E0AF68", "#F7768E", "#F7768E", "#24283B"],
            "catppuccin" | "mocha" => ["#CDD6F4", "#7F849C", "#89DCEB", "#89B4FA", "#CBA6F7", "#A6E3A1", "#F9E2AF", "#F38BA8", "#F38BA8", "#313244"],
            "nord" => ["#D8DEE9", "#7B88A1", "#88C0D0", "#81A1C1", "#B48EAD", "#A3BE8C", "#EBCB8B", "#BF616A", "#BF616A", "#3B4252"],
            "dracula" => ["#F8F8F2", "#6272A4", "#8BE9FD", "#8BE9FD", "#BD93F9", "#50FA7B", "#F1FA8C", "#FF5555", "#FF5555", "#44475A"],
            "rose-pine" | "rose" => ["#E0DEF4", "#908CAA", "#9CCFD8", "#C4A7E7", "#EBBCBA", "#9CCFD8", "#F6C177", "#EB6F92", "#EB6F92", "#26233A"],
            "gruvbox" => ["#EBDBB2", "#928374", "#83A598", "#FABD2F", "#D3869B", "#B8BB26", "#FABD2F", "#FB4934", "#FB4934", "#3C3836"],
            _ => return false,
        };

        self.theme.preset = name.to_ascii_lowercase();
        self.theme.foreground = colors[0].into();
        self.theme.muted = colors[1].into();
        self.theme.accent = colors[2].into();
        self.theme.path = colors[3].into();
        self.theme.git = colors[4].into();
        self.theme.success = colors[5].into();
        self.theme.warning = colors[6].into();
        self.theme.error = colors[7].into();
        self.theme.admin = colors[8].into();
        self.theme.panel = colors[9].into();
        true
    }

    pub fn apply_environment(&self) {
        for (key, value) in &self.env {
            std::env::set_var(key, value);
        }
    }
}

//! The prompt: current folder, Git branch and state, duration and exit status by default,
//! and the segments, colors and symbol `~/.nebularc` asks for (see [`PromptConfig`]).
//!
//! ```text
//! ~/projects/app  on  main +1 !2 ?3 ⇡1  took 4s
//! ❯
//! ```

use crate::sys;
use nu_ansi_term::{Color, Style};
use reedline::{Prompt, PromptEditMode, PromptHistorySearch, PromptHistorySearchStatus};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const BRANCH: char = '\u{e725}';
const GIT_TIMEOUT: Duration = Duration::from_millis(300);

#[derive(Debug, Default, PartialEq, Eq)]
pub struct GitStatus {
    pub branch: String,
    pub staged: usize,
    pub modified: usize,
    pub untracked: usize,
    pub conflicts: usize,
    pub ahead: usize,
    pub behind: usize,
    /// `git status` did not answer in time; only the branch is known.
    pub partial: bool,
}

fn find_git_dir(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let candidate = dir.join(".git");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if candidate.is_file() {
            // Worktrees and submodules: ".git" is a file pointing at the real directory.
            let text = std::fs::read_to_string(&candidate).ok()?;
            let target = text.strip_prefix("gitdir:")?.trim();
            return Some(dir.join(target));
        }
    }
    None
}

fn branch_from_head(git_dir: &Path) -> Option<String> {
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    Some(match head.strip_prefix("ref: refs/heads/") {
        Some(branch) => branch.to_owned(),
        None => head.chars().take(7).collect(),
    })
}

/// Parses `git status --porcelain=v1 --branch`.
pub fn parse_porcelain(output: &str, status: &mut GitStatus) {
    for line in output.lines() {
        if let Some(header) = line.strip_prefix("## ") {
            if let Some(start) = header.find('[') {
                let counts = &header[start + 1..header.len().saturating_sub(1)];
                for part in counts.split(", ") {
                    if let Some(n) = part.strip_prefix("ahead ") {
                        status.ahead = n.parse().unwrap_or(0);
                    } else if let Some(n) = part.strip_prefix("behind ") {
                        status.behind = n.parse().unwrap_or(0);
                    }
                }
            }
            continue;
        }
        let bytes = line.as_bytes();
        if bytes.len() < 2 {
            continue;
        }
        let (x, y) = (bytes[0], bytes[1]);
        if x == b'?' {
            status.untracked += 1;
        } else if x == b'U' || y == b'U' || (x == b'A' && y == b'A') || (x == b'D' && y == b'D') {
            status.conflicts += 1;
        } else {
            if x != b' ' {
                status.staged += 1;
            }
            if y != b' ' {
                status.modified += 1;
            }
        }
    }
}

pub fn git_status(cwd: &Path) -> Option<GitStatus> {
    let git_dir = find_git_dir(cwd)?;
    let mut status = GitStatus {
        branch: branch_from_head(&git_dir)?,
        ..GitStatus::default()
    };
    let (sender, receiver) = mpsc::channel();
    let cwd = cwd.to_path_buf();
    std::thread::spawn(move || {
        let output = Command::new("git")
            .args([
                "--no-optional-locks",
                "status",
                "--porcelain=v1",
                "--branch",
                "--ignore-submodules=dirty",
            ])
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        let _ = sender.send(output);
    });
    match receiver.recv_timeout(GIT_TIMEOUT) {
        Ok(Ok(output)) if output.status.success() => {
            parse_porcelain(&String::from_utf8_lossy(&output.stdout), &mut status)
        }
        _ => status.partial = true,
    }
    Some(status)
}

fn shorten(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 5 {
        return path.to_owned();
    }
    format!("{}/…/{}", parts[0], parts[parts.len() - 3..].join("/"))
}

fn paint(style: Style, text: &str) -> String {
    crate::style::apply(style, text)
}

/// The parts of the information line, in the order `NEBULA_PROMPT` lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Segment {
    Folder,
    Git,
    Duration,
    Status,
    Time,
    User,
    Host,
    Admin,
    Jobs,
    Node,
    Python,
    Rust,
}

impl Segment {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "folder" | "dir" => Segment::Folder,
            "git" => Segment::Git,
            "duration" | "took" => Segment::Duration,
            "status" | "exit" => Segment::Status,
            "time" => Segment::Time,
            "user" => Segment::User,
            "host" => Segment::Host,
            "admin" => Segment::Admin,
            "jobs" => Segment::Jobs,
            "node" => Segment::Node,
            "python" => Segment::Python,
            "rust" => Segment::Rust,
            _ => return None,
        })
    }

    fn default_style(self) -> Style {
        match self {
            Segment::Folder => Style::new().bold().fg(Color::Blue),
            Segment::Git => Style::new().bold().fg(Color::Magenta),
            Segment::Duration => Style::new().fg(Color::Yellow),
            Segment::Status | Segment::Rust => Style::new().fg(Color::Red),
            Segment::Time => Style::new().dimmed(),
            Segment::User | Segment::Host | Segment::Node => Style::new().fg(Color::Green),
            Segment::Admin => Style::new().bold().fg(Color::Red),
            Segment::Jobs => Style::new().fg(Color::Cyan),
            Segment::Python => Style::new().fg(Color::Yellow),
        }
    }
}

const DEFAULT_SEGMENTS: [Segment; 4] = [
    Segment::Folder,
    Segment::Git,
    Segment::Duration,
    Segment::Status,
];

/// "bold+cyan", "#ff8800", "dim", "214": a style for `NEBULA_PROMPT_COLORS`.
fn parse_style(spec: &str) -> Option<Style> {
    let mut style = Style::new();
    for word in spec.split('+').map(str::trim) {
        style = match word {
            "bold" => style.bold(),
            "dim" | "dimmed" => style.dimmed(),
            "italic" => style.italic(),
            "underline" => style.underline(),
            _ => style.fg(parse_color(word)?),
        };
    }
    Some(style)
}

fn parse_color(word: &str) -> Option<Color> {
    if let Some(hex) = word.strip_prefix('#').filter(|hex| hex.len() == 6) {
        let value = u32::from_str_radix(hex, 16).ok()?;
        return Some(Color::Rgb(
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        ));
    }
    if let Ok(index) = word.parse::<u8>() {
        return Some(Color::Fixed(index));
    }
    Some(match word {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" | "purple" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "gray" | "grey" | "bright-black" => Color::DarkGray,
        "bright-red" => Color::LightRed,
        "bright-green" => Color::LightGreen,
        "bright-yellow" => Color::LightYellow,
        "bright-blue" => Color::LightBlue,
        "bright-magenta" | "bright-purple" => Color::LightMagenta,
        "bright-cyan" => Color::LightCyan,
        "bright-white" => Color::LightGray,
        _ => return None,
    })
}

/// How the prompt looks, from the variables `~/.nebularc` can set:
///
/// ```sh
/// NEBULA_PROMPT="folder git node duration status time"
/// NEBULA_PROMPT_COLORS="folder=bold+cyan time=dim"
/// NEBULA_PROMPT_SYMBOL="λ"
/// NEBULA_PROMPT_LINES=1
/// ```
///
/// `NEBULA_PROMPT=starship` hands the whole prompt to Starship.
#[derive(Debug, PartialEq)]
pub struct PromptConfig {
    pub segments: Vec<Segment>,
    styles: Vec<(Segment, Style)>,
    pub symbol: String,
    pub one_line: bool,
    pub starship: bool,
}

impl PromptConfig {
    pub fn from_env() -> Self {
        let var = |name: &str| std::env::var(name).unwrap_or_default();
        Self::parse(
            &var("NEBULA_PROMPT"),
            &var("NEBULA_PROMPT_COLORS"),
            &var("NEBULA_PROMPT_SYMBOL"),
            &var("NEBULA_PROMPT_LINES"),
        )
    }

    pub fn parse(segments: &str, colors: &str, symbol: &str, lines: &str) -> Self {
        let starship = segments.trim() == "starship";
        let mut list: Vec<Segment> = segments
            .split([' ', ',', ':'])
            .filter_map(Segment::from_name)
            .collect();
        list.dedup();
        if list.is_empty() && !starship {
            list = DEFAULT_SEGMENTS.to_vec();
        }
        let styles = colors
            .split_whitespace()
            .filter_map(|entry| {
                let (name, spec) = entry.split_once('=')?;
                Some((Segment::from_name(name)?, parse_style(spec)?))
            })
            .collect();
        let symbol = symbol.trim();
        Self {
            segments: list,
            styles,
            symbol: if symbol.is_empty() || symbol.chars().count() > 8 {
                "❯".to_owned()
            } else {
                symbol.to_owned()
            },
            one_line: lines.trim() == "1",
            starship,
        }
    }

    fn style(&self, segment: Segment) -> Style {
        self.styles
            .iter()
            .rev()
            .find(|(candidate, _)| *candidate == segment)
            .map_or_else(|| segment.default_style(), |(_, style)| *style)
    }
}

/// What the prompt needs from the shell that draws it.
pub struct PromptState {
    pub status: i32,
    pub duration: Option<Duration>,
    pub jobs: usize,
}

fn git_segment(git: &GitStatus, style: Style) -> String {
    let icon = if sys::icons_enabled() {
        format!("{BRANCH} ")
    } else {
        String::new()
    };
    let mut text = format!(
        "{} {}",
        paint(Style::new().dimmed(), "on"),
        paint(style, &format!("{icon}{}", git.branch))
    );
    let marks: Vec<String> = [
        (git.conflicts, '=', Style::new().fg(Color::Red)),
        (git.staged, '+', Style::new().fg(Color::Green)),
        (git.modified, '!', Style::new().fg(Color::Yellow)),
        (git.untracked, '?', Style::new().dimmed()),
        (git.ahead, '⇡', Style::new().fg(Color::Cyan)),
        (git.behind, '⇣', Style::new().fg(Color::Cyan)),
    ]
    .into_iter()
    .filter(|(count, _, _)| *count > 0)
    .map(|(count, mark, style)| paint(style, &format!("{mark}{count}")))
    .collect();
    if !marks.is_empty() {
        text.push(' ');
        text.push_str(&marks.join(" "));
    }
    text
}

fn duration_text(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs >= 3600 {
        format!("{}h{}m", secs / 3600, secs % 3600 / 60)
    } else if secs >= 60 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{:.1}s", duration.as_secs_f32())
    }
}

/// The first folder from `cwd` up that holds one of `markers`, up to a few levels.
fn project_has(cwd: &Path, markers: &[&str]) -> bool {
    cwd.ancestors()
        .take(6)
        .any(|dir| markers.iter().any(|marker| dir.join(marker).exists()))
}

/// `program --version`, asked once per session: a version doesn't change under a shell.
fn tool_version(program: &'static str) -> Option<String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static VERSIONS: OnceLock<Mutex<HashMap<&'static str, Option<String>>>> = OnceLock::new();
    let versions = VERSIONS.get_or_init(Mutex::default);
    if let Some(known) = versions.lock().ok()?.get(program) {
        return known.clone();
    }
    let version = run_with_timeout(program, &["--version"], GIT_TIMEOUT).and_then(|output| {
        output
            .split_whitespace()
            .find(|word| {
                word.trim_start_matches('v')
                    .starts_with(|c: char| c.is_ascii_digit())
            })
            .map(|word| word.trim_start_matches('v').to_owned())
    });
    versions.lock().ok()?.insert(program, version.clone());
    version
}

fn run_with_timeout(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let path = sys::find_executable(program)?;
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let output = Command::new(path)
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        let _ = sender.send(output);
    });
    match receiver.recv_timeout(timeout) {
        Ok(Ok(output)) if output.status.success() => {
            Some(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        _ => None,
    }
}

fn segment_text(segment: Segment, style: Style, state: &PromptState, cwd: &Path) -> Option<String> {
    Some(match segment {
        Segment::Folder => paint(style, &shorten(&sys::display_path(cwd))),
        Segment::Git => git_segment(&git_status(cwd)?, style),
        Segment::Duration => {
            let duration = state.duration.filter(|d| *d >= Duration::from_secs(2))?;
            format!(
                "{} {}",
                paint(Style::new().dimmed(), "took"),
                paint(style, &duration_text(duration))
            )
        }
        Segment::Status => {
            (state.status != 0).then_some(())?;
            paint(style, &format!("✗ {}", state.status))
        }
        Segment::Time => {
            let now = jiff::Zoned::now();
            format!(
                "{} {}",
                paint(Style::new().dimmed(), "at"),
                paint(style, &now.strftime("%H:%M").to_string())
            )
        }
        Segment::User => paint(
            style,
            &std::env::var("USERNAME")
                .or_else(|_| std::env::var("USER"))
                .ok()?,
        ),
        Segment::Host => paint(
            style,
            &format!("@{}", std::env::var("COMPUTERNAME").ok()?.to_lowercase()),
        ),
        Segment::Admin => {
            sys::is_elevated().then_some(())?;
            paint(style, "admin")
        }
        Segment::Jobs => {
            (state.jobs > 0).then_some(())?;
            paint(style, &format!("✦{}", state.jobs))
        }
        Segment::Node => {
            project_has(cwd, &["package.json"]).then_some(())?;
            format!(
                "{} {}",
                paint(Style::new().dimmed(), "via"),
                paint(style, &format!("node {}", tool_version("node")?))
            )
        }
        Segment::Rust => {
            project_has(cwd, &["Cargo.toml"]).then_some(())?;
            format!(
                "{} {}",
                paint(Style::new().dimmed(), "via"),
                paint(style, &format!("rust {}", tool_version("rustc")?))
            )
        }
        Segment::Python => {
            let venv = std::env::var("VIRTUAL_ENV").ok().and_then(|path| {
                Path::new(&path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            });
            (venv.is_some()
                || project_has(
                    cwd,
                    &[
                        "pyproject.toml",
                        "requirements.txt",
                        "setup.py",
                        ".python-version",
                    ],
                ))
            .then_some(())?;
            let version = tool_version("python").unwrap_or_default();
            let text = match venv {
                Some(venv) => format!("py {version} ({venv})"),
                None => format!("py {version}"),
            };
            format!(
                "{} {}",
                paint(Style::new().dimmed(), "via"),
                paint(style, text.trim())
            )
        }
    })
}

/// The segments with a space between them, except "user" and "host", which read as user@host.
fn join(parts: &[(Segment, String)]) -> String {
    let mut line = String::new();
    for (index, (segment, text)) in parts.iter().enumerate() {
        let glued = *segment == Segment::Host && index > 0 && parts[index - 1].0 == Segment::User;
        if index > 0 && !glued {
            line.push(' ');
        }
        line.push_str(text);
    }
    line
}

/// Starship's prompt for this state, or None when it isn't installed or too slow.
fn starship_prompt(state: &PromptState) -> Option<String> {
    let millis = state.duration.map_or(0, |duration| duration.as_millis());
    let args = [
        "prompt".to_owned(),
        format!("--status={}", state.status),
        format!("--cmd-duration={millis}"),
        format!("--jobs={}", state.jobs),
        format!("--terminal-width={}", sys::terminal_width()),
    ];
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run_with_timeout("starship", &args, Duration::from_secs(2))
}

pub struct NebulaPrompt {
    line: String,
    failed: bool,
    symbol: String,
    one_line: bool,
    /// Starship draws everything, indicator included.
    complete: bool,
}

impl NebulaPrompt {
    pub fn new(state: &PromptState) -> Self {
        Self::with_config(&PromptConfig::from_env(), state)
    }

    pub fn with_config(config: &PromptConfig, state: &PromptState) -> Self {
        let failed = state.status != 0;
        if config.starship {
            if let Some(line) = starship_prompt(state) {
                return Self {
                    line,
                    failed,
                    symbol: String::new(),
                    one_line: true,
                    complete: true,
                };
            }
        }
        let cwd = std::env::current_dir().unwrap_or_default();
        let segments = if config.segments.is_empty() {
            &DEFAULT_SEGMENTS[..]
        } else {
            &config.segments[..]
        };
        let parts: Vec<(Segment, String)> = segments
            .iter()
            .filter_map(|segment| {
                Some((
                    *segment,
                    segment_text(*segment, config.style(*segment), state, &cwd)?,
                ))
            })
            .collect();
        Self {
            line: join(&parts),
            failed,
            symbol: config.symbol.clone(),
            one_line: config.one_line,
            complete: false,
        }
    }
}

impl Prompt for NebulaPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        if self.complete {
            return Cow::Borrowed(&self.line);
        }
        Cow::Owned(format!(
            "{}{}",
            self.line,
            if self.one_line { " " } else { "\n" }
        ))
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, _mode: PromptEditMode) -> Cow<'_, str> {
        if self.complete {
            return Cow::Borrowed("");
        }
        let color = if self.failed {
            Color::Red
        } else {
            Color::Green
        };
        Cow::Owned(paint(
            Style::new().bold().fg(color),
            &format!("{} ", self.symbol),
        ))
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Owned(paint(Style::new().dimmed(), "· "))
    }

    fn render_prompt_history_search_indicator(&self, search: PromptHistorySearch) -> Cow<'_, str> {
        let prefix = if matches!(search.status, PromptHistorySearchStatus::Failing) {
            "no match"
        } else {
            "search"
        };
        Cow::Owned(format!(
            "{} {} ",
            paint(Style::new().dimmed(), &format!("({prefix})")),
            search.term
        ))
    }

    fn get_prompt_color(&self) -> reedline::Color {
        reedline::Color::Default
    }

    fn get_indicator_color(&self) -> reedline::Color {
        reedline::Color::Default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_output() {
        let mut status = GitStatus::default();
        parse_porcelain("## main...origin/main [ahead 2, behind 1]\nM  a.rs\n M b.rs\nMM c.rs\n?? d.rs\nUU e.rs\n", &mut status);
        assert_eq!(
            (
                status.staged,
                status.modified,
                status.untracked,
                status.conflicts,
                status.ahead,
                status.behind
            ),
            (2, 2, 1, 1, 2, 1)
        );
    }

    #[test]
    fn reads_the_prompt_variables() {
        let config = PromptConfig::parse("", "", "", "");
        assert_eq!(config.segments, DEFAULT_SEGMENTS);
        assert_eq!(config.symbol, "❯");
        assert!(!config.one_line && !config.starship);

        let config = PromptConfig::parse(
            "time folder,git  bogus node:node",
            "folder=bold+cyan time=#ff8800 git=nope jobs=214",
            " λ ",
            "1",
        );
        assert_eq!(
            config.segments,
            [Segment::Time, Segment::Folder, Segment::Git, Segment::Node]
        );
        assert_eq!(
            config.style(Segment::Folder),
            Style::new().bold().fg(Color::Cyan)
        );
        assert_eq!(
            config.style(Segment::Time),
            Style::new().fg(Color::Rgb(0xff, 0x88, 0x00))
        );
        assert_eq!(config.style(Segment::Git), Segment::Git.default_style());
        assert_eq!(
            config.style(Segment::Jobs),
            Style::new().fg(Color::Fixed(214))
        );
        assert_eq!(config.symbol, "λ");
        assert!(config.one_line);

        assert!(PromptConfig::parse("starship", "", "", "").starship);
        assert_eq!(
            PromptConfig::parse("", "", "a very long symbol", "").symbol,
            "❯"
        );
    }

    fn plain(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                chars.by_ref().find(|c| c.is_ascii_alphabetic());
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn builds_the_line_from_segments() {
        let state = PromptState {
            status: 2,
            duration: Some(Duration::from_secs(75)),
            jobs: 1,
        };
        let config = PromptConfig::parse("status duration jobs", "", "$", "1");
        let prompt = NebulaPrompt::with_config(&config, &state);
        assert_eq!(plain(&prompt.line), "✗ 2 took 1m15s ✦1");
        assert_eq!(plain(&prompt.render_prompt_left()), "✗ 2 took 1m15s ✦1 ");
        assert_eq!(
            plain(&prompt.render_prompt_indicator(PromptEditMode::Default)),
            "$ "
        );
        let quiet = NebulaPrompt::with_config(
            &config,
            &PromptState {
                status: 0,
                duration: None,
                jobs: 0,
            },
        );
        assert_eq!(quiet.line, "");

        let parts = [
            (Segment::User, "jane".to_owned()),
            (Segment::Host, "@pc".to_owned()),
            (Segment::Folder, "~".to_owned()),
        ];
        assert_eq!(join(&parts), "jane@pc ~");
        assert_eq!(join(&parts[1..]), "@pc ~");
    }

    #[test]
    fn writes_durations_for_people() {
        assert_eq!(duration_text(Duration::from_millis(2500)), "2.5s");
        assert_eq!(duration_text(Duration::from_secs(125)), "2m5s");
        assert_eq!(duration_text(Duration::from_secs(3720)), "1h2m");
    }

    #[test]
    fn shortens_deep_paths() {
        assert_eq!(shorten("~/a/b"), "~/a/b");
        assert_eq!(shorten("~/a/b/c/d/e/f"), "~/…/d/e/f");
    }
}

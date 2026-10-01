//! The prompt: current folder, Git branch and state, duration and exit status.
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

pub struct NebulaPrompt {
    line: String,
    failed: bool,
}

impl NebulaPrompt {
    pub fn new(last_status: i32, duration: Option<Duration>) -> Self {
        let cwd = std::env::current_dir().unwrap_or_default();
        let mut line = paint(
            Style::new().bold().fg(Color::Blue),
            &shorten(&sys::display_path(&cwd)),
        );
        if let Some(git) = git_status(&cwd) {
            let icon = if sys::icons_enabled() {
                format!("{BRANCH} ")
            } else {
                String::new()
            };
            line.push_str(&format!(
                " {} {}",
                paint(Style::new().dimmed(), "on"),
                paint(
                    Style::new().bold().fg(Color::Magenta),
                    &format!("{icon}{}", git.branch)
                )
            ));
            let mut marks = Vec::new();
            if git.conflicts > 0 {
                marks.push(paint(
                    Style::new().fg(Color::Red),
                    &format!("={}", git.conflicts),
                ));
            }
            if git.staged > 0 {
                marks.push(paint(
                    Style::new().fg(Color::Green),
                    &format!("+{}", git.staged),
                ));
            }
            if git.modified > 0 {
                marks.push(paint(
                    Style::new().fg(Color::Yellow),
                    &format!("!{}", git.modified),
                ));
            }
            if git.untracked > 0 {
                marks.push(paint(Style::new().dimmed(), &format!("?{}", git.untracked)));
            }
            if git.ahead > 0 {
                marks.push(paint(
                    Style::new().fg(Color::Cyan),
                    &format!("⇡{}", git.ahead),
                ));
            }
            if git.behind > 0 {
                marks.push(paint(
                    Style::new().fg(Color::Cyan),
                    &format!("⇣{}", git.behind),
                ));
            }
            if !marks.is_empty() {
                line.push(' ');
                line.push_str(&marks.join(" "));
            }
        }
        if let Some(duration) = duration.filter(|d| *d >= Duration::from_secs(2)) {
            let secs = duration.as_secs();
            let text = if secs >= 60 {
                format!("{}m{}s", secs / 60, secs % 60)
            } else {
                format!("{:.1}s", duration.as_secs_f32())
            };
            line.push_str(&format!(
                " {} {}",
                paint(Style::new().dimmed(), "took"),
                paint(Style::new().fg(Color::Yellow), &text)
            ));
        }
        if last_status != 0 {
            line.push_str(&format!(
                " {}",
                paint(Style::new().fg(Color::Red), &format!("✗ {last_status}"))
            ));
        }
        Self {
            line,
            failed: last_status != 0,
        }
    }
}

impl Prompt for NebulaPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Owned(format!("{}\n", self.line))
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, _mode: PromptEditMode) -> Cow<'_, str> {
        let color = if self.failed {
            Color::Red
        } else {
            Color::Green
        };
        Cow::Owned(paint(Style::new().bold().fg(color), "❯ "))
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
    fn shortens_deep_paths() {
        assert_eq!(shorten("~/a/b"), "~/a/b");
        assert_eq!(shorten("~/a/b/c/d/e/f"), "~/…/d/e/f");
    }
}

//! The folders `z` knows: each `cd` in an interactive shell raises a folder's rank,
//! and `z words` goes to the best match, weighing rank by how recent the visit was
//! (the rule of rupa's z). Kept in `dirs.txt` next to the history.

use crate::sys;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub path: String,
    pub rank: f64,
    pub time: u64,
}

/// Past this total, every rank shrinks a little, so old favourites fade.
const MAX_TOTAL_RANK: f64 = 9000.0;

fn file() -> PathBuf {
    sys::data_dir().join("dirs.txt")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn parse(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.rsplitn(3, '|');
            let time = fields.next()?.parse().ok()?;
            let rank = fields.next()?.parse().ok()?;
            let path = fields.next()?.to_owned();
            Some(Entry { path, rank, time })
        })
        .collect()
}

fn format(entries: &[Entry]) -> String {
    entries
        .iter()
        .map(|entry| format!("{}|{}|{}\n", entry.path, entry.rank, entry.time))
        .collect()
}

pub fn load() -> Vec<Entry> {
    std::fs::read_to_string(file())
        .map(|text| parse(&text))
        .unwrap_or_default()
}

fn save(entries: &[Entry]) {
    let path = file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, format(entries));
}

fn same_path(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

/// Adds a visit to `path`. The home folder is everywhere already and isn't kept.
pub fn record(path: &Path) {
    if sys::home().is_some_and(|home| home == path) {
        return;
    }
    let path = path.to_string_lossy().into_owned();
    let mut entries = load();
    visit(&mut entries, &path, now());
    save(&entries);
}

fn visit(entries: &mut Vec<Entry>, path: &str, time: u64) {
    match entries
        .iter_mut()
        .find(|entry| same_path(&entry.path, path))
    {
        Some(entry) => {
            entry.rank += 1.0;
            entry.time = time;
        }
        None => entries.push(Entry {
            path: path.to_owned(),
            rank: 1.0,
            time,
        }),
    }
    let total: f64 = entries.iter().map(|entry| entry.rank).sum();
    if total > MAX_TOTAL_RANK {
        for entry in entries.iter_mut() {
            entry.rank *= 0.99;
        }
        entries.retain(|entry| entry.rank >= 1.0);
    }
}

/// Rank weighed by age: four times as much within the hour, a quarter after a week.
fn frecency(entry: &Entry, now: u64) -> f64 {
    let age = now.saturating_sub(entry.time);
    if age < 3600 {
        entry.rank * 4.0
    } else if age < 86_400 {
        entry.rank * 2.0
    } else if age < 604_800 {
        entry.rank / 2.0
    } else {
        entry.rank / 4.0
    }
}

/// Whether every word appears in the path, in order, ignoring case.
fn matches(path: &str, words: &[String]) -> bool {
    let lower = path.to_lowercase().replace('\\', "/");
    let mut rest = lower.as_str();
    for word in words {
        let word = word.to_lowercase().replace('\\', "/");
        match rest.find(&word) {
            Some(index) => rest = &rest[index + word.len()..],
            None => return false,
        }
    }
    true
}

/// The folders that match, best first, as (score, path). Folders that no longer
/// exist are left out.
pub fn ranked(words: &[String]) -> Vec<(f64, String)> {
    let now = now();
    let mut found: Vec<(f64, String)> = load()
        .into_iter()
        .filter(|entry| matches(&entry.path, words) && Path::new(&entry.path).is_dir())
        .map(|entry| {
            let mut score = frecency(&entry, now);
            // The last word in the folder's own name beats a match higher up.
            if let Some(last) = words.last() {
                let name = Path::new(&entry.path)
                    .file_name()
                    .map(|name| name.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if name.contains(&last.to_lowercase()) {
                    score *= 2.0;
                }
            }
            (score, entry.path)
        })
        .collect();
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_ages_visits() {
        let mut entries = Vec::new();
        visit(&mut entries, "/work/app", 100);
        visit(&mut entries, "/work/app", 200);
        visit(&mut entries, "/tmp", 150);
        assert_eq!(entries[0].rank, 2.0);
        assert_eq!(entries[0].time, 200);
        assert_eq!(parse(&format(&entries)), entries);
        let recent = Entry {
            path: "/a".into(),
            rank: 1.0,
            time: 1000,
        };
        assert_eq!(frecency(&recent, 1000), 4.0);
        assert_eq!(frecency(&recent, 1000 + 700_000), 0.25);
    }

    #[test]
    fn matches_words_in_order() {
        let words = |items: &[&str]| items.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
        assert!(matches(
            "C:\\Users\\me\\Projects\\Nebula",
            &words(&["proj", "neb"])
        ));
        assert!(!matches(
            "C:\\Users\\me\\Projects\\Nebula",
            &words(&["neb", "proj"])
        ));
        assert!(matches("/home/me/src", &words(&[])));
    }
}

//! Friendly "command not found" messages.

use crate::commands;
use crate::exec::Shell;
use crate::style::{self, Paint};
use std::collections::BTreeSet;
use std::sync::OnceLock;

fn path_commands() -> &'static BTreeSet<String> {
    static COMMANDS: OnceLock<BTreeSet<String>> = OnceLock::new();
    COMMANDS.get_or_init(crate::sys::path_commands)
}

/// The closest known command to a mistyped one, if it is close enough.
pub fn closest(name: &str, shell: &Shell) -> Option<String> {
    let lower = name.to_lowercase();
    let candidates = commands::all_names()
        .map(str::to_owned)
        .chain(shell.aliases.keys().cloned())
        .chain(shell.functions.keys().cloned())
        .chain(path_commands().iter().cloned());
    let mut best: Option<(usize, String)> = None;
    for candidate in candidates {
        if candidate.len() < 2 {
            continue;
        }
        let distance = strsim::damerau_levenshtein(&lower, &candidate.to_lowercase());
        let limit = if name.len() <= 3 { 1 } else { 2 };
        if distance > 0 && distance <= limit && best.as_ref().is_none_or(|(d, _)| distance < *d) {
            best = Some((distance, candidate));
        }
    }
    best.map(|(_, candidate)| candidate)
}

pub fn not_found(name: &str, shell: &Shell) -> String {
    let mut message = format!(
        "{} command not found: {name}",
        "nebula:".paint(style::ERROR)
    );
    if let Some(linux) = commands::windows_equivalent(name) {
        message.push_str(&format!(
            "\n  {} on Nebula, use {}",
            "tip:".paint(style::WARN),
            linux.paint(style::COMMAND).bold()
        ));
    } else if let Some(candidate) = closest(name, shell) {
        message.push_str(&format!(
            "\n  {} did you mean {}?",
            "tip:".paint(style::WARN),
            candidate.paint(style::COMMAND).bold()
        ));
    }
    message
}

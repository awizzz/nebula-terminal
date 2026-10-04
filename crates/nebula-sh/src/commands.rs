//! Catalog of the commands Nebula provides itself.

use crate::coreutils;

/// Run inside the shell process because they change or read its state.
pub const BUILTINS: &[&str] = &[
    "alias", "cd", "clear", "dirs", "exit", "export", "help", "history", "popd", "pushd", "source",
    ".", "type", "unalias", "unset", "which", "echo", "test", "[", "true", "false", ":", "local",
    "declare", "typeset", "return", "break", "continue", "shift", "set", "read", "eval", "command",
    "let",
];

/// Builtins that run other commands; inside a pipeline they need a sub-shell.
pub fn runs_code(name: &str) -> bool {
    matches!(name, "eval" | "source" | "." | "command")
}

/// Commands implemented by Nebula on top of coreutils.
pub const EXTRAS: &[&str] = &[
    "awk", "cmp", "diff", "find", "grep", "kill", "killall", "less", "ls", "open", "pkill", "ps",
    "sed", "tree", "xargs", "xdg-open",
];

pub fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

/// Commands run by re-executing `nebula-sh <name>`.
pub fn is_util(name: &str) -> bool {
    EXTRAS.contains(&name) || coreutils::NAMES.contains(&name)
}

pub fn all_names() -> impl Iterator<Item = &'static str> {
    let mut names: Vec<&'static str> = BUILTINS
        .iter()
        .chain(EXTRAS)
        .chain(coreutils::NAMES)
        .copied()
        .collect();
    names.sort_unstable();
    names.dedup();
    names.into_iter()
}

pub fn describe(name: &str) -> Option<&'static str> {
    Some(match name {
        "alias" => "define or list aliases",
        "cd" => "change directory",
        "clear" => "clear the screen",
        "dirs" | "pushd" | "popd" => "directory stack",
        "exit" => "close this shell",
        "export" => "set environment variables",
        "help" => "list Nebula commands",
        "history" => "show command history",
        "source" | "." => "run commands from a file",
        "type" | "which" => "show what a command is",
        "unalias" => "remove an alias",
        "unset" => "remove a variable or function",
        "test" | "[" => "check files, strings and numbers",
        "true" | ":" => "do nothing, successfully",
        "false" => "do nothing, unsuccessfully",
        "local" => "variable local to a function",
        "declare" | "typeset" => "set variables, list functions",
        "return" => "leave a function",
        "break" => "leave a loop",
        "continue" => "next loop iteration",
        "shift" => "drop the first arguments",
        "set" => "shell options and arguments",
        "read" => "read a line into variables",
        "eval" => "run text as a command",
        "command" => "run a command, skipping functions",
        "let" => "arithmetic",
        "cat" => "print files",
        "cp" => "copy files",
        "mv" => "move or rename",
        "rm" => "remove files",
        "mkdir" => "create directories",
        "rmdir" => "remove empty directories",
        "touch" => "create files or update times",
        "ls" => "list directory contents",
        "tree" => "show a directory tree",
        "find" => "search for files",
        "grep" => "search text with patterns",
        "head" => "first lines of a file",
        "tail" => "last lines of a file",
        "less" | "more" => "page through text",
        "wc" => "count lines, words, bytes",
        "sort" => "sort lines",
        "uniq" => "remove repeated lines",
        "cut" => "select columns",
        "tr" => "translate characters",
        "tee" => "copy input to files",
        "echo" => "print text",
        "printf" => "formatted print",
        "pwd" => "print current directory",
        "env" | "printenv" => "show environment",
        "date" => "print the date",
        "du" => "disk usage",
        "df" => "free disk space",
        "ln" => "create links",
        "realpath" => "absolute path",
        "basename" | "dirname" => "split paths",
        "ps" => "list processes",
        "kill" | "killall" | "pkill" => "stop processes",
        "open" | "xdg-open" => "open with the default app",
        "xargs" => "build commands from input",
        "sleep" => "wait for a while",
        "seq" => "print number sequences",
        "whoami" => "current user",
        "hostname" => "computer name",
        "uname" => "system information",
        "sha256sum" | "sha1sum" | "sha512sum" | "md5sum" | "cksum" => "checksums",
        "base64" => "encode or decode base64",
        "diff" => "compare files line by line",
        "cmp" => "compare files byte by byte",
        "sed" => "edit text with a script",
        "awk" => "process text by columns",
        _ => return None,
    })
}

/// Linux equivalents for Windows commands people type out of habit.
pub fn windows_equivalent(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_lowercase().as_str() {
        "dir" => "ls",
        "cls" => "clear",
        "type" => "cat",
        "del" | "erase" => "rm",
        "copy" | "xcopy" | "robocopy" => "cp",
        "move" | "ren" | "rename" => "mv",
        "md" => "mkdir",
        "rd" => "rmdir",
        "findstr" => "grep",
        "fc" | "comp" => "diff",
        "where" => "which",
        "tasklist" => "ps",
        "taskkill" => "kill",
        "start" => "open",
        "set" => "export",
        _ => return None,
    })
}

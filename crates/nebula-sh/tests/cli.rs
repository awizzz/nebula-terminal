//! End-to-end checks: run `nebula-sh -c` and compare its output, on every platform.

use std::path::PathBuf;
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nebula-sh-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_in(dir: &PathBuf, line: &str) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_nebula-sh"))
        .arg("-c")
        .arg(line)
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .output()
        .expect("run nebula-sh");
    let normalize = |bytes: &[u8]| String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    (
        output.status.code().unwrap_or(-1),
        normalize(&output.stdout),
        normalize(&output.stderr),
    )
}

fn run(line: &str) -> (i32, String, String) {
    run_in(&std::env::temp_dir(), line)
}

#[test]
fn echoes_and_expands() {
    assert_eq!(
        run("echo hello 'single $X' \"dq\"").1,
        "hello single $X dq\n"
    );
    assert_eq!(run("export A=1; echo \"a=$A\"").1, "a=1\n");
    assert_eq!(run("X=$(echo inner); echo $X").1, "inner\n");
}

#[test]
fn chains_and_status() {
    assert_eq!(run("false || echo b && echo c").1, "b\nc\n");
    assert_eq!(run("true && false; echo $?").1, "1\n");
    assert_eq!(run("! false && echo negated").1, "negated\n");
    assert_eq!(run("exit 3").0, 3);
}

#[test]
fn pipes_through_coreutils() {
    assert_eq!(
        run("printf 'b\\na\\nb\\n' | sort | uniq -c | head -1")
            .1
            .trim(),
        "1 a"
    );
    assert_eq!(run("seq 1 5 | tail -2 | wc -l").1.trim(), "2");
    assert_eq!(run("echo Hello | tr a-z A-Z").1, "HELLO\n");
}

#[test]
fn redirects_to_files() {
    let dir = scratch("redirect");
    assert_eq!(
        run_in(&dir, "echo one > f.txt; echo two >> f.txt; cat < f.txt").1,
        "one\ntwo\n"
    );
    let (_, out, err) = run_in(&dir, "cat missing.txt 2> err.txt; cat err.txt");
    assert!(out.contains("missing.txt"), "{out} / {err}");
    assert_eq!(
        run_in(&dir, "ls nope > /dev/null 2>&1; echo $?").1.trim(),
        "2"
    );
}

#[test]
fn files_and_directories() {
    let dir = scratch("files");
    let (code, out, err) = run_in(
        &dir,
        "mkdir -p a/b && touch a/b/x.rs a/y.txt && cd a && ls && pwd",
    );
    assert_eq!(code, 0, "{err}");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(&lines[..2], ["b", "y.txt"]);
    assert!(lines[2].ends_with('a'));
    assert_eq!(run_in(&dir, "find a -name '*.rs'").1, "a/b/x.rs\n");
    assert_eq!(run_in(&dir, "find a -type d | sort").1, "a\na/b\n");
    assert_eq!(
        run_in(
            &dir,
            "cp a/y.txt z.txt && mv z.txt w.txt && rm w.txt && ls w.txt 2>/dev/null; echo $?"
        )
        .1
        .trim(),
        "2"
    );
    assert_eq!(run_in(&dir, "echo *.nothing").1, "*.nothing\n");
    assert_eq!(run_in(&dir, "cd a; echo *.txt").1, "y.txt\n");
}

#[test]
fn grep_matches_like_gnu() {
    let dir = scratch("grep");
    std::fs::write(dir.join("notes.txt"), "alpha\nbeta\ngamma\nBeta again\n").unwrap();
    assert_eq!(
        run_in(&dir, "grep -n eta notes.txt").1,
        "2:beta\n4:Beta again\n"
    );
    assert_eq!(run_in(&dir, "grep -ic beta notes.txt").1, "2\n");
    assert_eq!(run_in(&dir, "grep -v a notes.txt; echo $?").1, "1\n");
    assert_eq!(
        run_in(&dir, "grep -E 'alpha|gamma' notes.txt").1,
        "alpha\ngamma\n"
    );
    assert_eq!(
        run_in(&dir, "grep 'alpha\\|gamma' notes.txt").1,
        "alpha\ngamma\n"
    );
    assert_eq!(run_in(&dir, "grep -A1 alpha notes.txt").1, "alpha\nbeta\n");
    assert_eq!(run_in(&dir, "grep -r gamma .").1, "./notes.txt:gamma\n");
    assert_eq!(run_in(&dir, "grep -q zeta notes.txt; echo $?").1, "1\n");
}

#[test]
fn tree_draws_a_tree() {
    let dir = scratch("tree");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/main.rs"), "").unwrap();
    std::fs::write(dir.join("README.md"), "").unwrap();
    assert_eq!(
        run_in(&dir, "tree").1,
        ".\n├── README.md\n└── src\n    └── main.rs\n\n1 directory, 2 files\n"
    );
}

#[test]
fn aliases_and_builtins() {
    assert_eq!(run("alias hi='echo hi there'; hi you").1, "hi there you\n");
    assert!(run("type cd").1.contains("built-in"));
    assert!(run("type ls").1.contains("Nebula command"));
}

#[test]
fn explains_unknown_commands() {
    let (code, _, err) = run("gti status");
    assert_eq!(code, 127);
    assert!(err.contains("command not found: gti"), "{err}");
    let (_, _, err) = run("cls");
    assert!(err.contains("use clear"), "{err}");
}

#[test]
fn reports_syntax_errors() {
    let (code, _, err) = run("echo 'open");
    assert_eq!(code, 2);
    assert!(err.contains("unclosed single quote"));
}

#[test]
fn runs_utilities_directly() {
    let output = Command::new(env!("CARGO_BIN_EXE_nebula-sh"))
        .args(["echo", "direct"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "direct");
}

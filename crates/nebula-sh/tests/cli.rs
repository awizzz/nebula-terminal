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
    assert!(run("type cd").1.contains("builtin"));
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

#[test]
fn runs_control_flow() {
    assert_eq!(
        run("for i in 1 2 3; do if [ $i -eq 2 ]; then continue; fi; echo $i; done").1,
        "1\n3\n"
    );
    assert_eq!(
        run("n=0; while (( n < 3 )); do n=$((n + 1)); done; echo $n").1,
        "3\n"
    );
    assert_eq!(run("until true; do echo never; done; echo ok").1, "ok\n");
    assert_eq!(
        run("for ((i = 0; i < 3; i++)); do printf '%s' $i; done").1,
        "012"
    );
    assert_eq!(
        run("case app.tar.gz in *.zip) echo zip;; *.tar.*) echo tar;; esac").1,
        "tar\n"
    );
    assert_eq!(
        run("for a in x y; do for b in 1 2; do [ $b = 2 ] && break 2; echo $a$b; done; done").1,
        "x1\n"
    );
}

#[test]
fn runs_functions() {
    assert_eq!(
        run("greet() { local who=${1:-world}; echo \"hi $who ($#)\"; }; greet; greet Ada x").1,
        "hi world (0)\nhi Ada (2)\n"
    );
    assert_eq!(run("f() { return 4; }; f; echo $?").1, "4\n");
    assert_eq!(
        run("x=1; f() { local x=2; }; f; echo $x").1,
        "1\n"
    );
    assert_eq!(
        run("fact() { if (( $1 <= 1 )); then echo 1; else echo $(( $1 * $(fact $(( $1 - 1 ))) )); fi; }; fact 5").1,
        "120\n"
    );
    assert_eq!(run("up() { tr a-z A-Z; }; echo hi | up").1, "HI\n");
    let (code, _, err) = run("f() { f; }; f");
    assert_eq!(code, 1);
    assert!(err.contains("maximum function nesting"), "{err}");
}

#[test]
fn expands_parameters_and_arithmetic() {
    assert_eq!(
        run("f=report.tar.gz; echo ${f%%.*} ${f#*.} ${#f} ${f/tar/zip} ${f^^}").1,
        "report tar.gz 13 report.zip.gz REPORT.TAR.GZ\n"
    );
    assert_eq!(run("echo $((7 * (3 + 1) % 5)) $((2 ** 10))").1, "3 1024\n");
    assert_eq!(run("set -- a 'b c' d; for x in \"$@\"; do echo \"[$x]\"; done").1, "[a]\n[b c]\n[d]\n");
    let (_, out, err) = run("echo $((1 / 0)); echo next");
    assert_eq!(out, "next\n");
    assert!(err.contains("division by 0"), "{err}");
}

#[test]
fn tests_conditions() {
    assert_eq!(run("[ -d / ] && [ ! -f / ] && echo dir").1, "dir\n");
    assert_eq!(run("test 10 -gt 9 && test abc = abc && echo yes").1, "yes\n");
    assert_eq!(
        run("v=1.20; [[ $v == 1.* && $v =~ ^[0-9]+\\.[0-9]+$ ]] && echo match").1,
        "match\n"
    );
    assert_eq!(run("[[ a < b ]] && echo less").1, "less\n");
    assert_eq!(run("[ 1 -eq ]; echo $?").1, "2\n");
}

#[test]
fn reads_input_and_here_documents() {
    assert_eq!(
        run("printf 'a b c\\nd e\\n' | while read first rest; do echo \"$first|$rest\"; done").1,
        "a|b c\nd|e\n"
    );
    assert_eq!(run("read -r x y <<< 'one two three'; echo $y").1, "two three\n");
    assert_eq!(
        run("name=Nebula\ncat <<EOF\nHello $name\n$((1 + 1))\nEOF").1,
        "Hello Nebula\n2\n"
    );
    assert_eq!(run("cat <<'EOF'\n$literal\nEOF").1, "$literal\n");
}

#[test]
fn isolates_subshells_and_substitutions() {
    let dir = scratch("subshell");
    let (_, out, _) = run_in(&dir, "(cd / && pwd); x=$(cd /; echo in); pwd | grep -c subshell; echo $x");
    assert_eq!(out.lines().collect::<Vec<_>>()[1..], ["1", "in"]);
    assert_eq!(run("x=$(false) || echo failed").1, "failed\n");
    assert_eq!(run("(exit 3); echo $?").1, "3\n");
    assert_eq!(run("echo before; (exit 5); echo after").1, "before\nafter\n");
}

#[test]
fn honours_shell_options() {
    let (code, out, _) = run("set -e; echo one; false; echo two");
    assert_eq!((code, out.as_str()), (1, "one\n"));
    assert_eq!(run("set -e; false || true; if false; then :; fi; echo kept").1, "kept\n");
    assert_eq!(run("set -o pipefail; false | true; echo $?").1, "1\n");
    let (code, _, err) = run("set -u; echo $NEBULA_SURELY_UNSET; echo no");
    assert_eq!(code, 1);
    assert!(err.contains("unbound variable"), "{err}");
}

#[test]
fn runs_script_files_with_arguments() {
    let dir = scratch("script");
    std::fs::write(
        dir.join("hello.sh"),
        "#!/usr/bin/env nebula\r\nfor name in \"$@\"; do\r\n  echo \"hello $name from $(basename $0)\"\r\ndone\r\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_nebula-sh"))
        .arg(dir.join("hello.sh"))
        .args(["Ada", "Linus"])
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        "hello Ada from hello.sh\nhello Linus from hello.sh\n"
    );
}

#[test]
fn pipelines_do_not_deadlock_on_aliases() {
    assert_eq!(run("alias big='seq 1 100000'; big | tail -1").1, "100000\n");
    assert_eq!(run("alias ls='ls -a'; ls -d /").1.trim(), "/");
    assert_eq!(run("yes | grep y | head -2").1, "y\ny\n");
}

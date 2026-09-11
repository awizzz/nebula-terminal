use std::process::Command;

fn nebula() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nebula"))
}

#[test]
fn version_flag_reports_package_version() {
    let output = nebula()
        .arg("--version")
        .output()
        .expect("Nebula executable should start");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("version output should be UTF-8");
    assert_eq!(
        stdout.trim(),
        format!("Nebula {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn help_flag_is_non_interactive_and_documents_admin_mode() {
    let output = nebula()
        .arg("--help")
        .output()
        .expect("Nebula executable should start");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help output should be UTF-8");
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("--admin"));
    assert!(stdout.contains("--version"));
}

#[test]
fn short_help_flag_works() {
    let output = nebula()
        .arg("-h")
        .output()
        .expect("Nebula executable should start");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help output should be UTF-8");
    assert!(stdout.contains("Windows shell frontend"));
}

#[test]
fn unknown_option_fails_with_usage_hint() {
    let output = nebula()
        .arg("--definitely-not-a-nebula-option")
        .output()
        .expect("Nebula executable should start");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
    assert!(stderr.contains("unknown option"));
    assert!(stderr.contains("nebula --help"));
}

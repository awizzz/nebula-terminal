use std::process::Command;

#[test]
fn version_flag_reports_package_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_nebula"))
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

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn make_temp_dir(suffix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be valid")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("kooixc-cli-check-{suffix}-{nanos}"));
    fs::create_dir_all(&dir).expect("temp dir should be created");
    dir
}

#[test]
fn check_warning_is_non_fatal_by_default() {
    let dir = make_temp_dir("warning-default");
    let main = dir.join("main.kooix");

    fs::write(
        &main,
        "cap Net<\"example.com\">;\nfn main() -> Int requires [Net<\"example.com\">] { 0 };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("check")
        .arg(&main)
        .output()
        .expect("run check");

    assert!(
        output.status.success(),
        "warnings should not fail by default, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("ok: semantic checks passed with warnings"),
        "unexpected stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("warning["), "unexpected stderr: {stderr}");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_strict_warnings_fails_on_warning() {
    let dir = make_temp_dir("warning-strict");
    let main = dir.join("main.kooix");

    fs::write(
        &main,
        "cap Net<\"example.com\">;\nfn main() -> Int requires [Net<\"example.com\">] { 0 };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("check")
        .arg(&main)
        .arg("--strict-warnings")
        .output()
        .expect("run check --strict-warnings");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("warning["), "unexpected stderr: {stderr}");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_unknown_option_fails_with_usage_error() {
    let dir = make_temp_dir("unknown-option");
    let main = dir.join("main.kooix");

    fs::write(&main, "fn main() -> Int { 0 };\n").expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("check")
        .arg(&main)
        .arg("--json")
        .output()
        .expect("run check --json");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown check option '--json'"),
        "unexpected stderr: {stderr}"
    );
    assert!(stderr.contains("usage: "), "unexpected stderr: {stderr}");

    let _ = fs::remove_dir_all(&dir);
}

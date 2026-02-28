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
        String::from_utf8_lossy(&output.stdout)
            .contains("ok: semantic checks passed with warnings"),
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
fn check_json_output_reports_ok_state() {
    let dir = make_temp_dir("json-pass");
    let main = dir.join("main.kooix");

    fs::write(&main, "fn main() -> Int { 0 };\n").expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("check")
        .arg(&main)
        .arg("--json")
        .output()
        .expect("run check --json");

    assert!(
        output.status.success(),
        "check --json should pass, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"ok\":true"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("\"summary\""),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("\"phase\":\"check\""),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("\"diagnostics\":[]"),
        "unexpected stdout: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_json_warning_is_ok_without_strict() {
    let dir = make_temp_dir("json-warning-default");
    let main = dir.join("main.kooix");

    fs::write(
        &main,
        "cap Net<\"example.com\">;\nfn main() -> Int requires [Net<\"example.com\">] { 0 };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("check")
        .arg(&main)
        .arg("--json")
        .output()
        .expect("run check --json");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"ok\":true"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("\"phase\":\"check\""),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("\"severity\":\"warning\""),
        "unexpected stdout: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_json_warning_fails_with_strict() {
    let dir = make_temp_dir("json-warning-strict");
    let main = dir.join("main.kooix");

    fs::write(
        &main,
        "cap Net<\"example.com\">;\nfn main() -> Int requires [Net<\"example.com\">] { 0 };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("check")
        .arg(&main)
        .arg("--json")
        .arg("--strict-warnings")
        .output()
        .expect("run check --json --strict-warnings");

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"ok\":false"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("\"phase\":\"check\""),
        "unexpected stdout: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_pretty_without_json_fails() {
    let dir = make_temp_dir("pretty-no-json");
    let main = dir.join("main.kooix");

    fs::write(&main, "fn main() -> Int { 0 };\n").expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("check")
        .arg(&main)
        .arg("--pretty")
        .output()
        .expect("run check --pretty");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--pretty requires --json"),
        "unexpected stderr: {stderr}"
    );

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
        .arg("--bad")
        .output()
        .expect("run check --bad");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown check option '--bad'"),
        "unexpected stderr: {stderr}"
    );
    assert!(stderr.contains("usage: "), "unexpected stderr: {stderr}");

    let _ = fs::remove_dir_all(&dir);
}

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn make_temp_dir(suffix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be valid")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("kooixc-cli-{suffix}-{nanos}"));
    fs::create_dir_all(&dir).expect("temp dir should be created");
    dir
}

#[test]
fn check_modules_command_passes_for_qualified_imports() {
    let dir = make_temp_dir("check-modules-pass");
    let lib = dir.join("lib.kooix");
    let main = dir.join("main.kooix");

    fs::write(&lib, "fn helper() -> Int { 41 };").expect("write lib");
    fs::write(
        &main,
        "import \"lib\" as Lib;\n\nfn main() -> Int { Lib::helper() + 1 };",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("check-modules")
        .arg(&main)
        .output()
        .expect("run check-modules");

    assert!(
        output.status.success(),
        "check-modules should pass, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("ok: module semantic checks passed"),
        "unexpected stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_modules_command_reports_qualified_import_errors() {
    let dir = make_temp_dir("check-modules-fail");
    let lib = dir.join("lib.kooix");
    let main = dir.join("main.kooix");

    fs::write(&lib, "fn helper() -> Int { 41 };").expect("write lib");
    fs::write(
        &main,
        "import \"lib\" as Lib;\n\nfn main() -> Int { Lib::missing() + 1 };",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("check-modules")
        .arg(&main)
        .output()
        .expect("run check-modules");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown imported function 'Lib::missing'"),
        "unexpected stderr: {stderr}"
    );
    assert!(
        stderr.contains("main.kooix:"),
        "unexpected stderr: {stderr}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_modules_json_output_reports_ok_state() {
    let dir = make_temp_dir("check-modules-json-pass");
    let lib = dir.join("lib.kooix");
    let main = dir.join("main.kooix");

    fs::write(&lib, "fn helper() -> Int { 41 };\n").expect("write lib");
    fs::write(
        &main,
        "import \"lib\" as Lib;\n\nfn main() -> Int { Lib::helper() + 1 };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("check-modules")
        .arg(&main)
        .arg("--json")
        .output()
        .expect("run check-modules --json");

    assert!(
        output.status.success(),
        "check-modules --json should pass, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"ok\":true"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("\"modules\""),
        "unexpected stdout: {stdout}"
    );
    assert!(stdout.contains("main.kooix"), "unexpected stdout: {stdout}");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_modules_json_output_reports_errors() {
    let dir = make_temp_dir("check-modules-json-fail");
    let lib = dir.join("lib.kooix");
    let main = dir.join("main.kooix");

    fs::write(&lib, "fn helper() -> Int { 41 };\n").expect("write lib");
    fs::write(
        &main,
        "import \"lib\" as Lib;\n\nfn main() -> Int { Lib::missing() + 1 };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("check-modules")
        .arg(&main)
        .arg("--json")
        .output()
        .expect("run check-modules --json");

    assert_eq!(output.status.code(), Some(1));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"ok\":false"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("unknown imported function 'Lib::missing'"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("\"severity\":\"error\""),
        "unexpected stdout: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_modules_json_pretty_output_is_multiline() {
    let dir = make_temp_dir("check-modules-json-pretty");
    let lib = dir.join("lib.kooix");
    let main = dir.join("main.kooix");

    fs::write(&lib, "fn helper() -> Int { 41 };\n").expect("write lib");
    fs::write(
        &main,
        "import \"lib\" as Lib;\n\nfn main() -> Int { Lib::helper() + 1 };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("check-modules")
        .arg(&main)
        .arg("--json")
        .arg("--pretty")
        .output()
        .expect("run check-modules --json --pretty");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\n"), "unexpected stdout: {stdout}");
    assert!(
        stdout.contains("\n  \"modules\""),
        "unexpected stdout: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_modules_pretty_without_json_fails() {
    let dir = make_temp_dir("check-modules-pretty-no-json");
    let main = dir.join("main.kooix");
    fs::write(&main, "fn main() -> Int { 0 };\n").expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("check-modules")
        .arg(&main)
        .arg("--pretty")
        .output()
        .expect("run check-modules --pretty");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--pretty requires --json"),
        "unexpected stderr: {stderr}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_modules_warning_is_non_fatal_by_default() {
    let dir = make_temp_dir("check-modules-warning-default");
    let main = dir.join("main.kooix");

    fs::write(
        &main,
        "cap Net<\"example.com\">;\nfn main() -> Int requires [Net<\"example.com\">] { 0 };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("check-modules")
        .arg(&main)
        .output()
        .expect("run check-modules");

    assert!(
        output.status.success(),
        "warnings should not fail by default, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("warning["), "unexpected stderr: {stderr}");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_modules_strict_warnings_fails_on_warning() {
    let dir = make_temp_dir("check-modules-warning-strict");
    let main = dir.join("main.kooix");

    fs::write(
        &main,
        "cap Net<\"example.com\">;\nfn main() -> Int requires [Net<\"example.com\">] { 0 };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("check-modules")
        .arg(&main)
        .arg("--strict-warnings")
        .output()
        .expect("run check-modules --strict-warnings");

    assert_eq!(output.status.code(), Some(1));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_modules_json_warning_is_ok_without_strict() {
    let dir = make_temp_dir("check-modules-json-warning-default");
    let main = dir.join("main.kooix");

    fs::write(
        &main,
        "cap Net<\"example.com\">;\nfn main() -> Int requires [Net<\"example.com\">] { 0 };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("check-modules")
        .arg(&main)
        .arg("--json")
        .output()
        .expect("run check-modules --json");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"ok\":true"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("\"severity\":\"warning\""),
        "unexpected stdout: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_modules_json_warning_fails_with_strict() {
    let dir = make_temp_dir("check-modules-json-warning-strict");
    let main = dir.join("main.kooix");

    fs::write(
        &main,
        "cap Net<\"example.com\">;\nfn main() -> Int requires [Net<\"example.com\">] { 0 };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("check-modules")
        .arg(&main)
        .arg("--json")
        .arg("--strict-warnings")
        .output()
        .expect("run check-modules --json --strict-warnings");

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"ok\":false"),
        "unexpected stdout: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}

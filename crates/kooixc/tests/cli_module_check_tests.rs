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

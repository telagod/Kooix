use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn make_temp_dir(suffix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be valid")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("kooixc-cli-run-{suffix}-{nanos}"));
    fs::create_dir_all(&dir).expect("temp dir should be created");
    dir
}

#[test]
fn run_namespace_imports_execute_with_duplicate_local_symbols() {
    let dir = make_temp_dir("namespace-duplicate");
    let main = dir.join("main.kooix");
    let lib_a = dir.join("lib_a.kooix");
    let lib_b = dir.join("lib_b.kooix");

    fs::write(
        &lib_a,
        "fn helper() -> Int { 1 };\nfn dup() -> Int { helper() + 1 };\n",
    )
    .expect("write lib_a");
    fs::write(
        &lib_b,
        "fn helper() -> Int { 10 };\nfn dup() -> Int { helper() + 1 };\n",
    )
    .expect("write lib_b");
    fs::write(
        &main,
        "import \"lib_a\" as A;\nimport \"lib_b\" as B;\nfn main() -> Int { A::dup() + B::dup() };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("run")
        .arg(&main)
        .output()
        .expect("run command");

    assert!(
        output.status.success(),
        "run should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ok: run result: 13"),
        "unexpected stdout: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn run_include_style_import_still_works() {
    let dir = make_temp_dir("include-style");
    let main = dir.join("main.kooix");
    let lib = dir.join("lib.kooix");

    fs::write(&lib, "fn helper() -> Int { 41 };\n").expect("write lib");
    fs::write(
        &main,
        "import \"lib\";\nfn main() -> Int { helper() + 1 };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("run")
        .arg(&main)
        .output()
        .expect("run command");

    assert!(
        output.status.success(),
        "include-style run should remain compatible, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ok: run result: 42"),
        "unexpected stdout: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}

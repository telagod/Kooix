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

fn native_toolchain_available() -> bool {
    Command::new("llc").arg("--version").output().is_ok()
        && Command::new("clang").arg("--version").output().is_ok()
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

#[test]
fn run_namespace_imports_execute_with_transitive_alias_calls() {
    let dir = make_temp_dir("namespace-transitive-call");
    let main = dir.join("main.kooix");
    let lib_a = dir.join("lib_a.kooix");
    let lib_core = dir.join("lib_core.kooix");

    fs::write(&lib_core, "fn base() -> Int { 40 };\n").expect("write lib_core");
    fs::write(
        &lib_a,
        "import \"lib_core\" as Core;\nfn calc() -> Int { Core::base() + 2 };\n",
    )
    .expect("write lib_a");
    fs::write(
        &main,
        "import \"lib_a\" as A;\nfn main() -> Int { A::calc() };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("run")
        .arg(&main)
        .output()
        .expect("run command");

    assert!(
        output.status.success(),
        "transitive namespace run should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ok: run result: 42"),
        "unexpected stdout: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn run_namespace_imports_execute_with_imported_enum_local_paths() {
    let dir = make_temp_dir("namespace-enum-local-paths");
    let main = dir.join("main.kooix");
    let lib = dir.join("lib.kooix");

    fs::write(
        &lib,
        "enum Option<T> { Some(T); None; };\nfn mk(x: Int) -> Option<Int> { Option::Some(x) };\nfn read() -> Int {\n  match mk(9) {\n    Option::Some(v) => v;\n    Option::None => 0;\n  }\n};\n",
    )
    .expect("write lib");
    fs::write(
        &main,
        "import \"lib\" as Lib;\nfn main() -> Int { Lib::read() };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("run")
        .arg(&main)
        .output()
        .expect("run command");

    assert!(
        output.status.success(),
        "enum local path rewrite run should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ok: run result: 9"),
        "unexpected stdout: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn run_namespace_imports_execute_with_imported_record_local_types() {
    let dir = make_temp_dir("namespace-record-local-types");
    let main = dir.join("main.kooix");
    let lib = dir.join("lib.kooix");

    fs::write(
        &lib,
        "record Box { value: Int; };\nfn mk(v: Int) -> Box { Box { value: v; } };\nfn calc() -> Int {\n  let b: Box = mk(40);\n  b.value + 2\n};\n",
    )
    .expect("write lib");
    fs::write(
        &main,
        "import \"lib\" as Lib;\nfn main() -> Int { Lib::calc() };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("run")
        .arg(&main)
        .output()
        .expect("run command");

    assert!(
        output.status.success(),
        "record local type rewrite run should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ok: run result: 42"),
        "unexpected stdout: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn native_namespace_imports_execute_with_transitive_alias_calls() {
    if !native_toolchain_available() {
        return;
    }

    let dir = make_temp_dir("native-namespace-transitive-call");
    let main = dir.join("main.kooix");
    let lib_a = dir.join("lib_a.kooix");
    let lib_core = dir.join("lib_core.kooix");
    let out_bin = dir.join("out-native");

    fs::write(&lib_core, "fn base() -> Int { 40 };\n").expect("write lib_core");
    fs::write(
        &lib_a,
        "import \"lib_core\" as Core;\nfn calc() -> Int { Core::base() + 2 };\n",
    )
    .expect("write lib_a");
    fs::write(
        &main,
        "import \"lib_a\" as A;\nfn main() -> Int { if A::calc() == 42 { 0 } else { 1 } };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("native")
        .arg(&main)
        .arg(&out_bin)
        .arg("--run")
        .output()
        .expect("native --run command");

    assert!(
        output.status.success(),
        "native transitive namespace run should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ok: native binary generated at"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("run exit code: 0"),
        "unexpected stdout: {stdout}"
    );

    let _ = fs::remove_file(&out_bin);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn native_include_style_import_still_works() {
    if !native_toolchain_available() {
        return;
    }

    let dir = make_temp_dir("native-include-style");
    let main = dir.join("main.kooix");
    let lib = dir.join("lib.kooix");
    let out_bin = dir.join("out-native");

    fs::write(&lib, "fn helper() -> Int { 41 };\n").expect("write lib");
    fs::write(
        &main,
        "import \"lib\";\nfn main() -> Int { if helper() + 1 == 42 { 0 } else { 1 } };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("native")
        .arg(&main)
        .arg(&out_bin)
        .arg("--run")
        .output()
        .expect("native --run command");

    assert!(
        output.status.success(),
        "native include-style run should remain compatible, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ok: native binary generated at"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("run exit code: 0"),
        "unexpected stdout: {stdout}"
    );

    let _ = fs::remove_file(&out_bin);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn native_namespace_imports_execute_with_imported_enum_local_paths() {
    if !native_toolchain_available() {
        return;
    }

    let dir = make_temp_dir("native-namespace-enum-local-paths");
    let main = dir.join("main.kooix");
    let lib = dir.join("lib.kooix");
    let out_bin = dir.join("out-native");

    fs::write(
        &lib,
        "enum Option<T> { Some(T); None; };\nfn mk(x: Int) -> Option<Int> { Option::Some(x) };\nfn read() -> Int {\n  match mk(9) {\n    Option::Some(v) => v;\n    Option::None => 0;\n  }\n};\n",
    )
    .expect("write lib");
    fs::write(
        &main,
        "import \"lib\" as Lib;\nfn main() -> Int { if Lib::read() == 9 { 0 } else { 1 } };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("native")
        .arg(&main)
        .arg(&out_bin)
        .arg("--run")
        .output()
        .expect("native --run command");

    assert!(
        output.status.success(),
        "native enum local path rewrite run should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ok: native binary generated at"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("run exit code: 0"),
        "unexpected stdout: {stdout}"
    );

    let _ = fs::remove_file(&out_bin);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn native_namespace_imports_execute_with_imported_record_local_types() {
    if !native_toolchain_available() {
        return;
    }

    let dir = make_temp_dir("native-namespace-record-local-types");
    let main = dir.join("main.kooix");
    let lib = dir.join("lib.kooix");
    let out_bin = dir.join("out-native");

    fs::write(
        &lib,
        "record Box { value: Int; };\nfn mk(v: Int) -> Box { Box { value: v; } };\nfn calc() -> Int {\n  let b: Box = mk(40);\n  b.value + 2\n};\n",
    )
    .expect("write lib");
    fs::write(
        &main,
        "import \"lib\" as Lib;\nfn main() -> Int { if Lib::calc() == 42 { 0 } else { 1 } };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("native")
        .arg(&main)
        .arg(&out_bin)
        .arg("--run")
        .output()
        .expect("native --run command");

    assert!(
        output.status.success(),
        "native record local type rewrite run should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ok: native binary generated at"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("run exit code: 0"),
        "unexpected stdout: {stdout}"
    );

    let _ = fs::remove_file(&out_bin);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn run_namespace_imports_execute_with_imported_generic_function_type_args() {
    let dir = make_temp_dir("namespace-generic-inference");
    let main = dir.join("main.kooix");
    let lib = dir.join("lib.kooix");

    fs::write(
        &lib,
        "fn id<T>(x: T) -> T { x };\nfn calc() -> Int { id<Int>(40) + 2 };\n",
    )
    .expect("write lib");
    fs::write(
        &main,
        "import \"lib\" as Lib;\nfn main() -> Int { if Lib::id<Int>(1) == 1 { Lib::calc() } else { 0 } };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("run")
        .arg(&main)
        .output()
        .expect("run command");

    assert!(
        output.status.success(),
        "run generic imported function with type args should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ok: run result: 42"),
        "unexpected stdout: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn native_namespace_imports_reports_generic_type_args_not_supported_yet() {
    if !native_toolchain_available() {
        return;
    }

    let dir = make_temp_dir("native-namespace-generic-inference");
    let main = dir.join("main.kooix");
    let lib = dir.join("lib.kooix");
    let out_bin = dir.join("out-native");

    fs::write(
        &lib,
        "fn id<T>(x: T) -> T { x };\nfn calc() -> Int { id<Int>(40) + 2 };\n",
    )
    .expect("write lib");
    fs::write(
        &main,
        "import \"lib\" as Lib;\nfn main() -> Int { if Lib::id<Int>(1) == 1 { if Lib::calc() == 42 { 0 } else { 1 } } else { 1 } };\n",
    )
    .expect("write main");

    let output = Command::new(env!("CARGO_BIN_EXE_kooixc"))
        .arg("native")
        .arg(&main)
        .arg(&out_bin)
        .arg("--run")
        .output()
        .expect("native --run command");

    assert!(
        !output.status.success(),
        "native generic imported function should fail until generic type args lowering is supported"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("uses generic type arguments, which native lowering does not support yet"),
        "unexpected stderr: {stderr}"
    );

    let _ = fs::remove_file(&out_bin);
    let _ = fs::remove_dir_all(&dir);
}

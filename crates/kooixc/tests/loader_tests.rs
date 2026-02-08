use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use kooixc::loader::load_source_map;

fn make_temp_dir(suffix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be valid")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("kooixc-loader-{suffix}-{nanos}"));
    fs::create_dir_all(&dir).expect("temp dir should be created");
    dir
}

#[test]
fn loads_imports_without_extension() {
    let dir = make_temp_dir("no-ext");
    let lib = dir.join("lib.kooix");
    let main = dir.join("main.kooix");

    fs::write(&lib, "fn helper() -> Int { 41 };").expect("write lib");
    fs::write(
        &main,
        "import \"lib\";\n\nfn main() -> Int { helper() + 1 };",
    )
    .expect("write main");

    let map = load_source_map(&main).expect("load should succeed");
    assert_eq!(map.files.len(), 2);
    assert!(map.combined.contains("fn helper()"));
    assert!(map.combined.contains("fn main()"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn does_not_duplicate_shared_imports() {
    let dir = make_temp_dir("dedup");
    let common = dir.join("common.kooix");
    let a = dir.join("a.kooix");
    let b = dir.join("b.kooix");
    let main = dir.join("main.kooix");

    fs::write(&common, "fn helper() -> Int { 1 };").expect("write common");
    fs::write(&a, "import \"common\";\nfn a() -> Int { helper() };").expect("write a");
    fs::write(&b, "import \"common\";\nfn b() -> Int { helper() };").expect("write b");
    fs::write(
        &main,
        "import \"a\";\nimport \"b\";\nfn main() -> Int { a() + b() };",
    )
    .expect("write main");

    let map = load_source_map(&main).expect("load should succeed");
    let common_count = map
        .files
        .iter()
        .filter(|file| file.path.file_name().and_then(|name| name.to_str()) == Some("common.kooix"))
        .count();
    assert_eq!(common_count, 1);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn import_errors_include_path_context() {
    let dir = make_temp_dir("diag");
    let main = dir.join("main.kooix");

    fs::write(&main, "import \"lib\"").expect("write main");

    let errors = load_source_map(&main).expect_err("load should fail");
    assert_eq!(errors.len(), 1);

    let message = &errors[0].message;
    assert!(
        message.contains("import declaration must end with ';'"),
        "unexpected message: {message}"
    );
    assert!(
        message.contains(&main.display().to_string()),
        "message should include file path: {message}"
    );

    let _ = fs::remove_dir_all(&dir);
}

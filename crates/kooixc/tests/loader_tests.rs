use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use kooixc::check_entry_modules;
use kooixc::error::Severity;
use kooixc::loader::{load_module_programs, load_source_map, load_source_map_with_module_graph};

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

#[test]
fn module_graph_tracks_import_aliases() {
    let dir = make_temp_dir("graph-alias");
    let lib = dir.join("lib.kooix");
    let main = dir.join("main.kooix");

    fs::write(&lib, "fn helper() -> Int { 41 };").expect("write lib");
    fs::write(
        &main,
        "import \"lib\" as Lib;\n\nfn main() -> Int { Lib::helper() + 1 };",
    )
    .expect("write main");

    let (_map, graph) =
        load_source_map_with_module_graph(&main).expect("load should succeed with module graph");

    let main_node = graph
        .modules
        .iter()
        .find(|node| node.path.file_name().and_then(|name| name.to_str()) == Some("main.kooix"))
        .expect("main module should exist in graph");
    assert_eq!(main_node.imports.len(), 1);
    assert_eq!(main_node.imports[0].raw, "lib");
    assert_eq!(main_node.imports[0].ns.as_deref(), Some("Lib"));
    assert_eq!(
        main_node.imports[0]
            .resolved
            .file_name()
            .and_then(|name| name.to_str()),
        Some("lib.kooix")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn load_module_programs_parses_each_file_separately() {
    let dir = make_temp_dir("modules");
    let lib = dir.join("lib.kooix");
    let main = dir.join("main.kooix");

    fs::write(&lib, "fn helper() -> Int { 41 };").expect("write lib");
    fs::write(
        &main,
        "import \"lib\" as Lib;\n\nfn main() -> Int { Lib::helper() + 1 };",
    )
    .expect("write main");

    let (_graph, modules) = load_module_programs(&main).expect("load modules should succeed");
    assert_eq!(modules.len(), 2);

    let main_mod = modules
        .iter()
        .find(|module| module.path.file_name().and_then(|name| name.to_str()) == Some("main.kooix"))
        .expect("main module should exist");
    assert_eq!(main_mod.program.items.len(), 2); // import + fn

    let lib_mod = modules
        .iter()
        .find(|module| module.path.file_name().and_then(|name| name.to_str()) == Some("lib.kooix"))
        .expect("lib module should exist");
    assert_eq!(lib_mod.program.items.len(), 1); // fn

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_entry_modules_does_not_require_include_concatenation() {
    let dir = make_temp_dir("module-check");
    let lib = dir.join("lib.kooix");
    let main = dir.join("main.kooix");

    fs::write(&lib, "fn helper() -> Int { 41 };").expect("write lib");
    fs::write(&main, "import \"lib\" as Lib;\n\nfn main() -> Int { 0 };").expect("write main");

    let results = check_entry_modules(&main).expect("module check should succeed");
    assert_eq!(results.len(), 2);
    assert!(!results.iter().any(|result| {
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_entry_modules_isolates_duplicate_names_across_files() {
    let dir = make_temp_dir("module-check-dupes");
    let lib = dir.join("lib.kooix");
    let main = dir.join("main.kooix");

    fs::write(&lib, "fn helper() -> Int { 41 };").expect("write lib");
    fs::write(
        &main,
        "import \"lib\";\n\nfn helper() -> Int { 1 };\nfn main() -> Int { 0 };",
    )
    .expect("write main");

    let results = check_entry_modules(&main).expect("module check should succeed");
    assert!(!results
        .iter()
        .flat_map(|result| &result.diagnostics)
        .any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic.message.contains("duplicate function")
        }));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_entry_modules_resolves_qualified_import_function_calls() {
    let dir = make_temp_dir("module-check-qualified-call");
    let lib = dir.join("lib.kooix");
    let main = dir.join("main.kooix");

    fs::write(&lib, "fn helper() -> Int { 41 };").expect("write lib");
    fs::write(
        &main,
        "import \"lib\" as Lib;\n\nfn main() -> Int { Lib::helper() + 1 };",
    )
    .expect("write main");

    let results = check_entry_modules(&main).expect("module check should succeed");
    assert_eq!(results.len(), 2);
    assert!(!results.iter().any(|result| {
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_entry_modules_resolves_qualified_import_record_types() {
    let dir = make_temp_dir("module-check-qualified-record");
    let lib = dir.join("lib.kooix");
    let main = dir.join("main.kooix");

    fs::write(&lib, "record Answer { x: Int; };\n").expect("write lib");
    fs::write(
        &main,
        "import \"lib\" as Lib;\n\nfn f(a: Lib::Answer) -> Int { a.x };\nfn main() -> Int { f(Lib::Answer { x: 1; }) };",
    )
    .expect("write main");

    let results = check_entry_modules(&main).expect("module check should succeed");
    assert_eq!(results.len(), 2);
    assert!(!results.iter().any(|result| {
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }));

    let _ = fs::remove_dir_all(&dir);
}

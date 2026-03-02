use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::ast::{
    Block, EnumDecl, Expr, FunctionDecl, Item, MatchArm, MatchArmBody, MatchPattern, Program,
    RecordDecl, Statement, TypeArg, TypeRef,
};
use crate::error::{Diagnostic, Span};
use crate::loader::{ImportEdge, LoadedModule, ModuleGraph};

#[derive(Debug, Clone)]
pub struct ExportIndex {
    functions: HashMap<PathBuf, HashMap<String, FunctionDecl>>,
    records: HashMap<PathBuf, HashMap<String, RecordDecl>>,
    enums: HashMap<PathBuf, HashMap<String, EnumDecl>>,
}

pub fn build_export_index(modules: &[LoadedModule]) -> ExportIndex {
    let mut functions: HashMap<PathBuf, HashMap<String, FunctionDecl>> = HashMap::new();
    let mut records: HashMap<PathBuf, HashMap<String, RecordDecl>> = HashMap::new();
    let mut enums: HashMap<PathBuf, HashMap<String, EnumDecl>> = HashMap::new();

    for module in modules {
        let module_path = canonicalize_lossy(&module.path);

        for item in &module.program.items {
            match item {
                Item::Function(function) => {
                    functions
                        .entry(module_path.clone())
                        .or_default()
                        .insert(function.name.clone(), function.clone());
                }
                Item::Record(record) => {
                    records
                        .entry(module_path.clone())
                        .or_default()
                        .insert(record.name.clone(), record.clone());
                }
                Item::Enum(en) => {
                    enums
                        .entry(module_path.clone())
                        .or_default()
                        .insert(en.name.clone(), en.clone());
                }
                Item::Capability(_) | Item::Import(_) | Item::Workflow(_) | Item::Agent(_) => {}
            }
        }
    }

    ExportIndex {
        functions,
        records,
        enums,
    }
}

pub fn prepare_program_for_module_check(
    module: &LoadedModule,
    graph: &ModuleGraph,
    exports: &ExportIndex,
) -> (Program, Vec<Diagnostic>) {
    prepare_program_for_module_projection(module, graph, exports, false)
}

pub fn prepare_program_for_module_execution(
    module: &LoadedModule,
    graph: &ModuleGraph,
    exports: &ExportIndex,
) -> (Program, Vec<Diagnostic>) {
    prepare_program_for_module_projection(module, graph, exports, true)
}

fn prepare_program_for_module_projection(
    module: &LoadedModule,
    graph: &ModuleGraph,
    exports: &ExportIndex,
    include_import_bodies: bool,
) -> (Program, Vec<Diagnostic>) {
    let module_path = canonicalize_lossy(&module.path);
    let mut diagnostics = Vec::new();

    let alias_map = module_alias_map(&module_path, graph);
    if alias_map.is_empty() {
        return (module.program.clone(), diagnostics);
    }

    let mut program = module.program.clone();
    let mut needed_functions: HashMap<String, (String, PathBuf)> = HashMap::new();
    let mut needed_records: HashMap<String, (String, PathBuf)> = HashMap::new();
    let mut needed_enums: HashMap<String, (String, PathBuf)> = HashMap::new();

    for item in &mut program.items {
        match item {
            Item::Function(function) => {
                normalize_function_body(
                    function,
                    &alias_map,
                    exports,
                    &mut needed_functions,
                    &mut needed_records,
                    &mut needed_enums,
                    &mut diagnostics,
                );
            }
            Item::Workflow(_)
            | Item::Agent(_)
            | Item::Capability(_)
            | Item::Import(_)
            | Item::Record(_)
            | Item::Enum(_) => {}
        }
    }

    let mut inserted: HashSet<String> = HashSet::new();
    let mut record_queue: Vec<String> = needed_records.keys().cloned().collect();
    let mut enum_queue: Vec<String> = needed_enums.keys().cloned().collect();

    let mut fn_queue: Vec<String> = needed_functions.keys().cloned().collect();
    while let Some(internal) = fn_queue.pop() {
        if !inserted.insert(inserted_key("fn", &internal)) {
            continue;
        }
        let Some((original, imported_module)) = needed_functions.get(&internal).cloned() else {
            continue;
        };
        let Some(template) = exports
            .functions
            .get(&imported_module)
            .and_then(|items| items.get(&original))
        else {
            diagnostics.push(Diagnostic::error(
                format!(
                    "module check: unknown imported function '{}::{}' (from '{}')",
                    internal_namespace(&internal),
                    original,
                    imported_module.display(),
                ),
                Span::new(0, 0),
            ));
            continue;
        };

        let alias = internal_namespace(&internal).to_string();
        if include_import_bodies {
            enqueue_module_local_symbols(
                &alias,
                &imported_module,
                exports,
                &mut needed_functions,
                &mut needed_records,
                &mut needed_enums,
                &mut fn_queue,
                &mut record_queue,
                &mut enum_queue,
            );
        }

        let mut stub = if include_import_bodies {
            executable_imported_function(template, &internal)
        } else {
            stub_function(template, &internal)
        };
        rewrite_function_signature_for_imported_module(
            &mut stub,
            &alias,
            &imported_module,
            exports,
            &mut needed_records,
            &mut needed_enums,
            &mut record_queue,
            &mut enum_queue,
        );
        if include_import_bodies {
            rewrite_function_body_for_local_module_symbols(
                &mut stub,
                &alias,
                &imported_module,
                exports,
                graph,
                &mut needed_functions,
                &mut needed_records,
                &mut needed_enums,
                &mut diagnostics,
            );
        }
        program.items.push(Item::Function(stub));
    }

    // Insert record/enum stubs, expanding dependencies discovered while rewriting imported item
    // signatures and schemas.
    while !record_queue.is_empty() || !enum_queue.is_empty() {
        while let Some(internal) = record_queue.pop() {
            if !inserted.insert(inserted_key("record", &internal)) {
                continue;
            }
            let Some((original, imported_module)) = needed_records.get(&internal).cloned() else {
                continue;
            };
            let Some(template) = exports
                .records
                .get(&imported_module)
                .and_then(|items| items.get(&original))
            else {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "module check: unknown imported record '{}::{}' (from '{}')",
                        internal_namespace(&internal),
                        original,
                        imported_module.display(),
                    ),
                    Span::new(0, 0),
                ));
                continue;
            };

            let alias = internal_namespace(&internal).to_string();
            let mut stub = template.clone();
            stub.name = internal.clone();
            stub.span = Span::new(0, 0);
            rewrite_record_decl_for_imported_module(
                &mut stub,
                &alias,
                &imported_module,
                exports,
                &mut needed_records,
                &mut needed_enums,
                &mut record_queue,
                &mut enum_queue,
            );
            program.items.push(Item::Record(stub));
        }

        while let Some(internal) = enum_queue.pop() {
            if !inserted.insert(inserted_key("enum", &internal)) {
                continue;
            }
            let Some((original, imported_module)) = needed_enums.get(&internal).cloned() else {
                continue;
            };
            let Some(template) = exports
                .enums
                .get(&imported_module)
                .and_then(|items| items.get(&original))
            else {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "module check: unknown imported enum '{}::{}' (from '{}')",
                        internal_namespace(&internal),
                        original,
                        imported_module.display(),
                    ),
                    Span::new(0, 0),
                ));
                continue;
            };

            let alias = internal_namespace(&internal).to_string();
            let mut stub = template.clone();
            stub.name = internal.clone();
            stub.span = Span::new(0, 0);
            rewrite_enum_decl_for_imported_module(
                &mut stub,
                &alias,
                &imported_module,
                exports,
                &mut needed_records,
                &mut needed_enums,
                &mut record_queue,
                &mut enum_queue,
            );
            program.items.push(Item::Enum(stub));
        }
    }

    (program, diagnostics)
}

fn enqueue_module_local_symbols(
    alias: &str,
    imported_module: &PathBuf,
    exports: &ExportIndex,
    needed_functions: &mut HashMap<String, (String, PathBuf)>,
    needed_records: &mut HashMap<String, (String, PathBuf)>,
    needed_enums: &mut HashMap<String, (String, PathBuf)>,
    fn_queue: &mut Vec<String>,
    record_queue: &mut Vec<String>,
    enum_queue: &mut Vec<String>,
) {
    if let Some(functions) = exports.functions.get(imported_module) {
        for name in functions.keys() {
            let internal = format!("{alias}__{name}");
            if needed_functions
                .insert(internal.clone(), (name.clone(), imported_module.clone()))
                .is_none()
            {
                fn_queue.push(internal);
            }
        }
    }
    if let Some(records) = exports.records.get(imported_module) {
        for name in records.keys() {
            let internal = format!("{alias}__{name}");
            if needed_records
                .insert(internal.clone(), (name.clone(), imported_module.clone()))
                .is_none()
            {
                record_queue.push(internal);
            }
        }
    }
    if let Some(enums) = exports.enums.get(imported_module) {
        for name in enums.keys() {
            let internal = format!("{alias}__{name}");
            if needed_enums
                .insert(internal.clone(), (name.clone(), imported_module.clone()))
                .is_none()
            {
                enum_queue.push(internal);
            }
        }
    }
}

fn inserted_key(kind: &str, internal: &str) -> String {
    format!("{kind}:{internal}")
}

fn internal_namespace(internal: &str) -> &str {
    internal.split("__").next().unwrap_or(internal)
}

fn canonicalize_lossy(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn module_alias_map(module_path: &PathBuf, graph: &ModuleGraph) -> HashMap<String, PathBuf> {
    let mut out = HashMap::new();
    let Some(module_node) = graph.modules.iter().find(|node| node.path == *module_path) else {
        return out;
    };

    for ImportEdge { resolved, ns, .. } in &module_node.imports {
        let Some(ns) = ns else {
            continue;
        };
        out.insert(ns.clone(), canonicalize_lossy(resolved));
    }

    out
}

fn normalize_function_body(
    function: &mut FunctionDecl,
    alias_map: &HashMap<String, PathBuf>,
    exports: &ExportIndex,
    needed_functions: &mut HashMap<String, (String, PathBuf)>,
    needed_records: &mut HashMap<String, (String, PathBuf)>,
    needed_enums: &mut HashMap<String, (String, PathBuf)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    rewrite_type_ref(
        &mut function.return_type,
        alias_map,
        exports,
        needed_records,
        needed_enums,
        diagnostics,
    );
    for param in &mut function.params {
        rewrite_type_ref(
            &mut param.ty,
            alias_map,
            exports,
            needed_records,
            needed_enums,
            diagnostics,
        );
    }
    for required in &mut function.requires {
        rewrite_type_ref(
            required,
            alias_map,
            exports,
            needed_records,
            needed_enums,
            diagnostics,
        );
    }

    let Some(body) = &mut function.body else {
        return;
    };
    normalize_block(
        body,
        alias_map,
        exports,
        needed_functions,
        needed_records,
        needed_enums,
        diagnostics,
    );
}

fn normalize_block(
    block: &mut Block,
    alias_map: &HashMap<String, PathBuf>,
    exports: &ExportIndex,
    needed_functions: &mut HashMap<String, (String, PathBuf)>,
    needed_records: &mut HashMap<String, (String, PathBuf)>,
    needed_enums: &mut HashMap<String, (String, PathBuf)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for statement in &mut block.statements {
        match statement {
            Statement::Let(let_stmt) => {
                if let Some(ty) = &mut let_stmt.ty {
                    rewrite_type_ref(
                        ty,
                        alias_map,
                        exports,
                        needed_records,
                        needed_enums,
                        diagnostics,
                    );
                }
                normalize_expr(
                    &mut let_stmt.value,
                    alias_map,
                    exports,
                    needed_functions,
                    needed_records,
                    needed_enums,
                    diagnostics,
                )
            }
            Statement::Assign(assign_stmt) => normalize_expr(
                &mut assign_stmt.value,
                alias_map,
                exports,
                needed_functions,
                needed_records,
                needed_enums,
                diagnostics,
            ),
            Statement::Return(ret) => {
                if let Some(expr) = &mut ret.value {
                    normalize_expr(
                        expr,
                        alias_map,
                        exports,
                        needed_functions,
                        needed_records,
                        needed_enums,
                        diagnostics,
                    );
                }
            }
            Statement::Expr(expr) => normalize_expr(
                expr,
                alias_map,
                exports,
                needed_functions,
                needed_records,
                needed_enums,
                diagnostics,
            ),
        }
    }

    if let Some(expr) = &mut block.tail {
        normalize_expr(
            expr,
            alias_map,
            exports,
            needed_functions,
            needed_records,
            needed_enums,
            diagnostics,
        );
    }
}

fn normalize_expr(
    expr: &mut Expr,
    alias_map: &HashMap<String, PathBuf>,
    exports: &ExportIndex,
    needed_functions: &mut HashMap<String, (String, PathBuf)>,
    needed_records: &mut HashMap<String, (String, PathBuf)>,
    needed_enums: &mut HashMap<String, (String, PathBuf)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match expr {
        Expr::Path(_) | Expr::String(_) | Expr::Number(_) | Expr::Bool(_) => {}
        Expr::RecordLit { ty, fields } => {
            rewrite_type_ref(
                ty,
                alias_map,
                exports,
                needed_records,
                needed_enums,
                diagnostics,
            );
            for field in fields {
                normalize_expr(
                    &mut field.value,
                    alias_map,
                    exports,
                    needed_functions,
                    needed_records,
                    needed_enums,
                    diagnostics,
                );
            }
        }
        Expr::Call { target, args, .. } => {
            rewrite_qualified_call_target(
                target,
                alias_map,
                exports,
                needed_functions,
                needed_records,
                needed_enums,
                diagnostics,
            );
            for arg in args {
                normalize_expr(
                    arg,
                    alias_map,
                    exports,
                    needed_functions,
                    needed_records,
                    needed_enums,
                    diagnostics,
                );
            }
        }
        Expr::If {
            cond,
            then_block,
            else_block,
        } => {
            normalize_expr(
                cond,
                alias_map,
                exports,
                needed_functions,
                needed_records,
                needed_enums,
                diagnostics,
            );
            normalize_block(
                then_block,
                alias_map,
                exports,
                needed_functions,
                needed_records,
                needed_enums,
                diagnostics,
            );
            if let Some(block) = else_block {
                normalize_block(
                    block,
                    alias_map,
                    exports,
                    needed_functions,
                    needed_records,
                    needed_enums,
                    diagnostics,
                );
            }
        }
        Expr::While { cond, body } => {
            normalize_expr(
                cond,
                alias_map,
                exports,
                needed_functions,
                needed_records,
                needed_enums,
                diagnostics,
            );
            normalize_block(
                body,
                alias_map,
                exports,
                needed_functions,
                needed_records,
                needed_enums,
                diagnostics,
            );
        }
        Expr::Match { value, arms } => {
            normalize_expr(
                value,
                alias_map,
                exports,
                needed_functions,
                needed_records,
                needed_enums,
                diagnostics,
            );
            for arm in arms {
                normalize_match_arm(
                    arm,
                    alias_map,
                    exports,
                    needed_functions,
                    needed_records,
                    needed_enums,
                    diagnostics,
                );
            }
        }
        Expr::Binary { left, right, .. } => {
            normalize_expr(
                left,
                alias_map,
                exports,
                needed_functions,
                needed_records,
                needed_enums,
                diagnostics,
            );
            normalize_expr(
                right,
                alias_map,
                exports,
                needed_functions,
                needed_records,
                needed_enums,
                diagnostics,
            );
        }
    }
}

fn normalize_match_arm(
    arm: &mut MatchArm,
    alias_map: &HashMap<String, PathBuf>,
    exports: &ExportIndex,
    needed_functions: &mut HashMap<String, (String, PathBuf)>,
    needed_records: &mut HashMap<String, (String, PathBuf)>,
    needed_enums: &mut HashMap<String, (String, PathBuf)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match &mut arm.pattern {
        MatchPattern::Wildcard => {}
        MatchPattern::Variant { path, .. } => {
            rewrite_qualified_enum_path(path, alias_map, exports, needed_enums, diagnostics);
        }
    }

    match &mut arm.body {
        MatchArmBody::Expr(expr) => normalize_expr(
            expr,
            alias_map,
            exports,
            needed_functions,
            needed_records,
            needed_enums,
            diagnostics,
        ),
        MatchArmBody::Block(block) => normalize_block(
            block,
            alias_map,
            exports,
            needed_functions,
            needed_records,
            needed_enums,
            diagnostics,
        ),
    }
}

fn rewrite_qualified_call_target(
    target: &mut Vec<String>,
    alias_map: &HashMap<String, PathBuf>,
    exports: &ExportIndex,
    needed_functions: &mut HashMap<String, (String, PathBuf)>,
    _needed_records: &mut HashMap<String, (String, PathBuf)>,
    needed_enums: &mut HashMap<String, (String, PathBuf)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(head) = target.first() else {
        return;
    };
    let Some(imported_module) = alias_map.get(head).cloned() else {
        return;
    };

    match target.as_slice() {
        [alias, name] => {
            let internal = format!("{alias}__{name}");

            if exports
                .functions
                .get(&imported_module)
                .and_then(|items| items.get(name))
                .is_some()
            {
                needed_functions.insert(internal.clone(), (name.clone(), imported_module));
                *target = vec![internal];
                return;
            }

            diagnostics.push(Diagnostic::error(
                format!(
                    "module check: unknown imported function '{alias}::{name}' (from '{}')",
                    imported_module.display()
                ),
                Span::new(0, 0),
            ));
        }
        [alias, enum_name, variant] => {
            let internal_enum = format!("{alias}__{enum_name}");
            if exports
                .enums
                .get(&imported_module)
                .and_then(|items| items.get(enum_name))
                .is_some()
            {
                needed_enums.insert(internal_enum.clone(), (enum_name.clone(), imported_module));
                *target = vec![internal_enum, variant.clone()];
                return;
            }

            diagnostics.push(Diagnostic::error(
                format!(
                    "module check: unknown imported enum '{alias}::{enum_name}' (from '{}')",
                    imported_module.display()
                ),
                Span::new(0, 0),
            ));
        }
        _ => {}
    }
}

fn rewrite_qualified_enum_path(
    path: &mut Vec<String>,
    alias_map: &HashMap<String, PathBuf>,
    exports: &ExportIndex,
    needed_enums: &mut HashMap<String, (String, PathBuf)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(head) = path.first() else {
        return;
    };
    let Some(imported_module) = alias_map.get(head).cloned() else {
        return;
    };

    match path.as_slice() {
        [alias, enum_name, variant] => {
            let internal_enum = format!("{alias}__{enum_name}");
            if exports
                .enums
                .get(&imported_module)
                .and_then(|items| items.get(enum_name))
                .is_some()
            {
                needed_enums.insert(internal_enum.clone(), (enum_name.clone(), imported_module));
                *path = vec![internal_enum, variant.clone()];
                return;
            }

            diagnostics.push(Diagnostic::error(
                format!(
                    "module check: unknown imported enum '{alias}::{enum_name}' (from '{}')",
                    imported_module.display()
                ),
                Span::new(0, 0),
            ));
        }
        _ => {}
    }
}

fn rewrite_type_ref(
    ty: &mut TypeRef,
    alias_map: &HashMap<String, PathBuf>,
    exports: &ExportIndex,
    needed_records: &mut HashMap<String, (String, PathBuf)>,
    needed_enums: &mut HashMap<String, (String, PathBuf)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for arg in &mut ty.args {
        let TypeArg::Type(nested) = arg else {
            continue;
        };
        rewrite_type_ref(
            nested,
            alias_map,
            exports,
            needed_records,
            needed_enums,
            diagnostics,
        );
    }

    let name = ty.name.clone();
    let mut parts = name.splitn(2, "::");
    let Some(head) = parts.next() else {
        return;
    };
    let Some(rest) = parts.next() else {
        return;
    };
    let Some(imported_module) = alias_map.get(head).cloned() else {
        return;
    };

    if rest.contains("::") {
        diagnostics.push(Diagnostic::error(
            format!(
                "module check: imported type ref must be '<ns>::<Type>' (found '{}')",
                ty.name
            ),
            Span::new(0, 0),
        ));
        return;
    }

    let internal = format!("{head}__{rest}");
    if exports
        .records
        .get(&imported_module)
        .and_then(|items| items.get(rest))
        .is_some()
    {
        ty.name = internal.clone();
        needed_records.insert(internal, (rest.to_string(), imported_module));
        return;
    }

    if exports
        .enums
        .get(&imported_module)
        .and_then(|items| items.get(rest))
        .is_some()
    {
        ty.name = internal.clone();
        needed_enums.insert(internal, (rest.to_string(), imported_module));
        return;
    }

    diagnostics.push(Diagnostic::error(
        format!(
            "module check: unknown imported type '{head}::{rest}' (from '{}')",
            imported_module.display()
        ),
        Span::new(0, 0),
    ));
}

fn rewrite_function_signature_for_imported_module(
    function: &mut FunctionDecl,
    alias: &str,
    imported_module: &PathBuf,
    exports: &ExportIndex,
    needed_records: &mut HashMap<String, (String, PathBuf)>,
    needed_enums: &mut HashMap<String, (String, PathBuf)>,
    record_queue: &mut Vec<String>,
    enum_queue: &mut Vec<String>,
) {
    for generic in &mut function.generics {
        for bound in &mut generic.bounds {
            rewrite_type_ref_for_imported_module(
                bound,
                alias,
                imported_module,
                exports,
                needed_records,
                needed_enums,
                record_queue,
                enum_queue,
            );
        }
    }

    for param in &mut function.params {
        rewrite_type_ref_for_imported_module(
            &mut param.ty,
            alias,
            imported_module,
            exports,
            needed_records,
            needed_enums,
            record_queue,
            enum_queue,
        );
    }

    rewrite_type_ref_for_imported_module(
        &mut function.return_type,
        alias,
        imported_module,
        exports,
        needed_records,
        needed_enums,
        record_queue,
        enum_queue,
    );
}

fn rewrite_record_decl_for_imported_module(
    record: &mut RecordDecl,
    alias: &str,
    imported_module: &PathBuf,
    exports: &ExportIndex,
    needed_records: &mut HashMap<String, (String, PathBuf)>,
    needed_enums: &mut HashMap<String, (String, PathBuf)>,
    record_queue: &mut Vec<String>,
    enum_queue: &mut Vec<String>,
) {
    for generic in &mut record.generics {
        for bound in &mut generic.bounds {
            rewrite_type_ref_for_imported_module(
                bound,
                alias,
                imported_module,
                exports,
                needed_records,
                needed_enums,
                record_queue,
                enum_queue,
            );
        }
    }

    for field in &mut record.fields {
        rewrite_type_ref_for_imported_module(
            &mut field.ty,
            alias,
            imported_module,
            exports,
            needed_records,
            needed_enums,
            record_queue,
            enum_queue,
        );
    }
}

fn rewrite_enum_decl_for_imported_module(
    en: &mut EnumDecl,
    alias: &str,
    imported_module: &PathBuf,
    exports: &ExportIndex,
    needed_records: &mut HashMap<String, (String, PathBuf)>,
    needed_enums: &mut HashMap<String, (String, PathBuf)>,
    record_queue: &mut Vec<String>,
    enum_queue: &mut Vec<String>,
) {
    for generic in &mut en.generics {
        for bound in &mut generic.bounds {
            rewrite_type_ref_for_imported_module(
                bound,
                alias,
                imported_module,
                exports,
                needed_records,
                needed_enums,
                record_queue,
                enum_queue,
            );
        }
    }

    for variant in &mut en.variants {
        if let Some(payload) = &mut variant.payload {
            rewrite_type_ref_for_imported_module(
                payload,
                alias,
                imported_module,
                exports,
                needed_records,
                needed_enums,
                record_queue,
                enum_queue,
            );
        }
    }
}

fn rewrite_type_ref_for_imported_module(
    ty: &mut TypeRef,
    alias: &str,
    imported_module: &PathBuf,
    exports: &ExportIndex,
    needed_records: &mut HashMap<String, (String, PathBuf)>,
    needed_enums: &mut HashMap<String, (String, PathBuf)>,
    record_queue: &mut Vec<String>,
    enum_queue: &mut Vec<String>,
) {
    for arg in &mut ty.args {
        let TypeArg::Type(nested) = arg else {
            continue;
        };
        rewrite_type_ref_for_imported_module(
            nested,
            alias,
            imported_module,
            exports,
            needed_records,
            needed_enums,
            record_queue,
            enum_queue,
        );
    }

    // Only rewrite local type references (unqualified) from the imported module. If the signature
    // references other namespaces, we currently leave them as-is.
    if ty.name.contains("::") {
        return;
    }

    let original = ty.name.clone();
    if exports
        .records
        .get(imported_module)
        .and_then(|items| items.get(&original))
        .is_some()
    {
        let internal = format!("{alias}__{original}");
        ty.name = internal.clone();
        if needed_records
            .insert(internal.clone(), (original, imported_module.clone()))
            .is_none()
        {
            record_queue.push(internal);
        }
        return;
    }

    if exports
        .enums
        .get(imported_module)
        .and_then(|items| items.get(&original))
        .is_some()
    {
        let internal = format!("{alias}__{original}");
        ty.name = internal.clone();
        if needed_enums
            .insert(internal.clone(), (original, imported_module.clone()))
            .is_none()
        {
            enum_queue.push(internal);
        }
    }
}

fn rewrite_function_body_for_local_module_symbols(
    function: &mut FunctionDecl,
    alias: &str,
    imported_module: &PathBuf,
    exports: &ExportIndex,
    graph: &ModuleGraph,
    needed_functions: &mut HashMap<String, (String, PathBuf)>,
    needed_records: &mut HashMap<String, (String, PathBuf)>,
    needed_enums: &mut HashMap<String, (String, PathBuf)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let local_functions = exports
        .functions
        .get(imported_module)
        .map(|items| items.keys().cloned().collect::<HashSet<_>>())
        .unwrap_or_default();
    let local_records = exports
        .records
        .get(imported_module)
        .map(|items| items.keys().cloned().collect::<HashSet<_>>())
        .unwrap_or_default();
    let local_enums = exports
        .enums
        .get(imported_module)
        .map(|items| items.keys().cloned().collect::<HashSet<_>>())
        .unwrap_or_default();

    let variant_enum_map = build_local_variant_enum_map(imported_module, exports);

    if let Some(body) = &mut function.body {
        rewrite_local_block_symbols(
            body,
            alias,
            &local_functions,
            &local_records,
            &local_enums,
            &variant_enum_map,
        );
    }

    let alias_map = module_alias_map(imported_module, graph);
    normalize_function_body(
        function,
        &alias_map,
        exports,
        needed_functions,
        needed_records,
        needed_enums,
        diagnostics,
    );
}

fn build_local_variant_enum_map(
    imported_module: &PathBuf,
    exports: &ExportIndex,
) -> HashMap<String, Option<String>> {
    let mut variant_to_enum: HashMap<String, Option<String>> = HashMap::new();
    let Some(enums) = exports.enums.get(imported_module) else {
        return variant_to_enum;
    };

    for (enum_name, enum_decl) in enums {
        for variant in &enum_decl.variants {
            variant_to_enum
                .entry(variant.name.clone())
                .and_modify(|slot| *slot = None)
                .or_insert_with(|| Some(enum_name.clone()));
        }
    }
    variant_to_enum
}

fn rewrite_local_block_symbols(
    block: &mut Block,
    alias: &str,
    local_functions: &HashSet<String>,
    local_records: &HashSet<String>,
    local_enums: &HashSet<String>,
    variant_enum_map: &HashMap<String, Option<String>>,
) {
    for statement in &mut block.statements {
        match statement {
            Statement::Let(let_stmt) => {
                if let Some(ty) = &mut let_stmt.ty {
                    rewrite_local_type_ref(ty, alias, local_records, local_enums);
                }
                rewrite_local_expr_symbols(
                    &mut let_stmt.value,
                    alias,
                    local_functions,
                    local_records,
                    local_enums,
                    variant_enum_map,
                );
            }
            Statement::Assign(assign_stmt) => {
                rewrite_local_expr_symbols(
                    &mut assign_stmt.value,
                    alias,
                    local_functions,
                    local_records,
                    local_enums,
                    variant_enum_map,
                );
            }
            Statement::Return(ret) => {
                if let Some(expr) = &mut ret.value {
                    rewrite_local_expr_symbols(
                        expr,
                        alias,
                        local_functions,
                        local_records,
                        local_enums,
                        variant_enum_map,
                    );
                }
            }
            Statement::Expr(expr) => {
                rewrite_local_expr_symbols(
                    expr,
                    alias,
                    local_functions,
                    local_records,
                    local_enums,
                    variant_enum_map,
                );
            }
        }
    }

    if let Some(tail) = &mut block.tail {
        rewrite_local_expr_symbols(
            tail,
            alias,
            local_functions,
            local_records,
            local_enums,
            variant_enum_map,
        );
    }
}

fn rewrite_local_expr_symbols(
    expr: &mut Expr,
    alias: &str,
    local_functions: &HashSet<String>,
    local_records: &HashSet<String>,
    local_enums: &HashSet<String>,
    variant_enum_map: &HashMap<String, Option<String>>,
) {
    match expr {
        Expr::Path(path) => rewrite_local_variant_path(path, alias, local_enums, variant_enum_map),
        Expr::String(_) | Expr::Number(_) | Expr::Bool(_) => {}
        Expr::RecordLit { ty, fields } => {
            rewrite_local_type_ref(ty, alias, local_records, local_enums);
            for field in fields {
                rewrite_local_expr_symbols(
                    &mut field.value,
                    alias,
                    local_functions,
                    local_records,
                    local_enums,
                    variant_enum_map,
                );
            }
        }
        Expr::Call {
            target,
            type_args,
            args,
        } => {
            rewrite_local_call_target(
                target,
                alias,
                local_functions,
                local_enums,
                variant_enum_map,
            );
            for type_arg in type_args {
                if let TypeArg::Type(ty) = type_arg {
                    rewrite_local_type_ref(ty, alias, local_records, local_enums);
                }
            }
            for arg in args {
                rewrite_local_expr_symbols(
                    arg,
                    alias,
                    local_functions,
                    local_records,
                    local_enums,
                    variant_enum_map,
                );
            }
        }
        Expr::If {
            cond,
            then_block,
            else_block,
        } => {
            rewrite_local_expr_symbols(
                cond,
                alias,
                local_functions,
                local_records,
                local_enums,
                variant_enum_map,
            );
            rewrite_local_block_symbols(
                then_block,
                alias,
                local_functions,
                local_records,
                local_enums,
                variant_enum_map,
            );
            if let Some(block) = else_block {
                rewrite_local_block_symbols(
                    block,
                    alias,
                    local_functions,
                    local_records,
                    local_enums,
                    variant_enum_map,
                );
            }
        }
        Expr::While { cond, body } => {
            rewrite_local_expr_symbols(
                cond,
                alias,
                local_functions,
                local_records,
                local_enums,
                variant_enum_map,
            );
            rewrite_local_block_symbols(
                body,
                alias,
                local_functions,
                local_records,
                local_enums,
                variant_enum_map,
            );
        }
        Expr::Match { value, arms } => {
            rewrite_local_expr_symbols(
                value,
                alias,
                local_functions,
                local_records,
                local_enums,
                variant_enum_map,
            );
            for arm in arms {
                if let MatchPattern::Variant { path, .. } = &mut arm.pattern {
                    rewrite_local_variant_path(path, alias, local_enums, variant_enum_map);
                }
                match &mut arm.body {
                    MatchArmBody::Expr(body_expr) => rewrite_local_expr_symbols(
                        body_expr,
                        alias,
                        local_functions,
                        local_records,
                        local_enums,
                        variant_enum_map,
                    ),
                    MatchArmBody::Block(block) => rewrite_local_block_symbols(
                        block,
                        alias,
                        local_functions,
                        local_records,
                        local_enums,
                        variant_enum_map,
                    ),
                }
            }
        }
        Expr::Binary { left, right, .. } => {
            rewrite_local_expr_symbols(
                left,
                alias,
                local_functions,
                local_records,
                local_enums,
                variant_enum_map,
            );
            rewrite_local_expr_symbols(
                right,
                alias,
                local_functions,
                local_records,
                local_enums,
                variant_enum_map,
            );
        }
    }
}

fn rewrite_local_call_target(
    target: &mut Vec<String>,
    alias: &str,
    local_functions: &HashSet<String>,
    local_enums: &HashSet<String>,
    variant_enum_map: &HashMap<String, Option<String>>,
) {
    match target.as_slice() {
        [name] if local_functions.contains(name) => {
            *target = vec![format!("{alias}__{name}")];
        }
        [variant] => {
            let Some(Some(enum_name)) = variant_enum_map.get(variant) else {
                return;
            };
            *target = vec![format!("{alias}__{enum_name}"), variant.clone()];
        }
        [enum_name, _variant] if local_enums.contains(enum_name) => {
            target[0] = format!("{alias}__{enum_name}");
        }
        _ => {}
    }
}

fn rewrite_local_variant_path(
    path: &mut Vec<String>,
    alias: &str,
    local_enums: &HashSet<String>,
    variant_enum_map: &HashMap<String, Option<String>>,
) {
    match path.as_slice() {
        [variant] => {
            let Some(Some(enum_name)) = variant_enum_map.get(variant) else {
                return;
            };
            *path = vec![format!("{alias}__{enum_name}"), variant.clone()];
        }
        [enum_name, _variant] if local_enums.contains(enum_name) => {
            path[0] = format!("{alias}__{enum_name}");
        }
        _ => {}
    }
}

fn rewrite_local_type_ref(
    ty: &mut TypeRef,
    alias: &str,
    local_records: &HashSet<String>,
    local_enums: &HashSet<String>,
) {
    for arg in &mut ty.args {
        if let TypeArg::Type(inner) = arg {
            rewrite_local_type_ref(inner, alias, local_records, local_enums);
        }
    }

    if ty.name.contains("::") {
        return;
    }
    if local_records.contains(&ty.name) || local_enums.contains(&ty.name) {
        ty.name = format!("{alias}__{}", ty.name);
    }
}

fn executable_imported_function(template: &FunctionDecl, new_name: &str) -> FunctionDecl {
    let mut function = template.clone();
    function.name = new_name.to_string();
    function.span = Span::new(0, 0);
    function
}

fn stub_function(template: &FunctionDecl, new_name: &str) -> FunctionDecl {
    FunctionDecl {
        name: new_name.to_string(),
        generics: template.generics.clone(),
        params: template.params.clone(),
        return_type: template.return_type.clone(),
        intent: None,
        effects: Vec::new(),
        requires: Vec::new(),
        ensures: Vec::new(),
        failure: None,
        evidence: None,
        body: None,
        span: Span::new(0, 0),
    }
}

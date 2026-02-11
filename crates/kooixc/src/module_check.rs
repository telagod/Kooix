use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::ast::{
    Block, EnumDecl, Expr, FunctionDecl, Item, MatchArm, MatchArmBody, MatchPattern, Program,
    RecordDecl, Statement,
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

    let mut inserted = HashSet::new();
    for (internal, (original, imported_module)) in needed_functions {
        if !inserted.insert(internal.clone()) {
            continue;
        }
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
        program
            .items
            .push(Item::Function(stub_function(template, &internal)));
    }

    for (internal, (original, imported_module)) in needed_records {
        if !inserted.insert(internal.clone()) {
            continue;
        }
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
        let mut stub = template.clone();
        stub.name = internal.clone();
        stub.span = Span::new(0, 0);
        program.items.push(Item::Record(stub));
    }

    for (internal, (original, imported_module)) in needed_enums {
        if !inserted.insert(internal.clone()) {
            continue;
        }
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
        let mut stub = template.clone();
        stub.name = internal.clone();
        stub.span = Span::new(0, 0);
        program.items.push(Item::Enum(stub));
    }

    (program, diagnostics)
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
            Statement::Let(let_stmt) => normalize_expr(
                &mut let_stmt.value,
                alias_map,
                exports,
                needed_functions,
                needed_records,
                needed_enums,
                diagnostics,
            ),
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
        Expr::RecordLit { fields, .. } => {
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
    needed_records: &mut HashMap<String, (String, PathBuf)>,
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

            if exports
                .records
                .get(&imported_module)
                .and_then(|items| items.get(name))
                .is_some()
            {
                needed_records.insert(internal.clone(), (name.clone(), imported_module));
                *target = vec![internal];
                return;
            }

            if exports
                .enums
                .get(&imported_module)
                .and_then(|items| items.get(name))
                .is_some()
            {
                needed_enums.insert(internal.clone(), (name.clone(), imported_module));
                *target = vec![internal];
                return;
            }

            diagnostics.push(Diagnostic::error(
                format!(
                    "module check: unknown imported symbol '{alias}::{name}' (from '{}')",
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

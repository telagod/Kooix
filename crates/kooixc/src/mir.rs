use std::collections::{HashMap, HashSet, VecDeque};

use crate::ast::{BinaryOp, Block, Expr, Statement, TypeArg, TypeRef};
use crate::error::Diagnostic;
use crate::hir::{HirFunction, HirProgram};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirProgram {
    pub records: Vec<MirRecord>,
    pub enums: Vec<MirEnum>,
    pub functions: Vec<MirFunction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirRecord {
    pub name: String,
    pub generics: Vec<String>,
    pub fields: Vec<MirRecordField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirRecordField {
    pub name: String,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirEnum {
    pub name: String,
    pub generics: Vec<String>,
    pub variants: Vec<MirEnumVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirEnumVariant {
    pub name: String,
    pub tag: u8,
    pub payload: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirFunction {
    pub name: String,
    pub params: Vec<MirParam>,
    pub return_type: TypeRef,
    pub effects: Vec<String>,
    pub locals: Vec<MirLocal>,
    pub blocks: Vec<MirBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirParam {
    pub name: String,
    pub ty: TypeRef,
    pub local: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirLocal {
    pub name: String,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirBlock {
    pub label: String,
    pub statements: Vec<MirStatement>,
    pub terminator: MirTerminator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirStatement {
    Assign { dst: usize, rvalue: MirRvalue },
    Eval(MirRvalue),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirRvalue {
    Use(MirOperand),
    Binary {
        op: BinaryOp,
        left: MirOperand,
        right: MirOperand,
    },
    Call {
        callee: String,
        args: Vec<MirOperand>,
    },
    RecordLit {
        record: String,
        fields: Vec<MirOperand>,
        field_tys: Vec<TypeRef>,
    },
    ProjectField {
        base: MirOperand,
        record: String,
        index: usize,
        field_ty: TypeRef,
    },
    EnumLit {
        enum_name: String,
        tag: u8,
        payload: Option<MirOperand>,
        payload_ty: Option<TypeRef>,
    },
    EnumTag {
        base: MirOperand,
        enum_name: String,
    },
    EnumPayload {
        base: MirOperand,
        enum_name: String,
        payload_ty: TypeRef,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirOperand {
    ConstInt(i64),
    ConstBool(bool),
    ConstText(String),
    Local(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirTerminator {
    Return {
        value: Option<MirOperand>,
    },
    ReturnDefault(TypeRef),
    Goto {
        target: String,
    },
    If {
        cond: MirOperand,
        then_bb: String,
        else_bb: String,
    },
}

#[derive(Debug, Clone)]
struct FunctionSignature {
    return_type: TypeRef,
    effects: Vec<String>,
    has_generics: bool,
}

pub fn lower_hir(program: &HirProgram) -> Result<MirProgram, Vec<Diagnostic>> {
    let specialized_program = specialize_generic_functions(program)?;

    let signatures = build_signatures(&specialized_program);
    let records = build_native_records(&specialized_program);
    let record_map: HashMap<String, MirRecord> = records
        .iter()
        .cloned()
        .map(|record| (record.name.clone(), record))
        .collect();
    let enums = build_native_enums(&specialized_program);
    let enum_map: HashMap<String, MirEnum> = enums
        .iter()
        .cloned()
        .map(|enum_decl| (enum_decl.name.clone(), enum_decl))
        .collect();
    let mut diagnostics = Vec::new();
    let mut functions = Vec::new();

    for function in &specialized_program.functions {
        match lower_function(function, &signatures, &record_map, &enum_map) {
            Ok(mir_function) => functions.push(mir_function),
            Err(mut errors) => diagnostics.append(&mut errors),
        }
    }

    if diagnostics.is_empty() {
        Ok(MirProgram {
            records,
            enums,
            functions,
        })
    } else {
        Err(diagnostics)
    }
}

#[derive(Debug, Clone)]
struct GenericInstantiation {
    callee: String,
    args: Vec<TypeRef>,
}

fn specialize_generic_functions(program: &HirProgram) -> Result<HirProgram, Vec<Diagnostic>> {
    let generic_templates: HashMap<String, HirFunction> = program
        .functions
        .iter()
        .filter(|function| !function.generics.is_empty())
        .map(|function| (function.name.clone(), function.clone()))
        .collect();
    if generic_templates.is_empty() {
        return Ok(program.clone());
    }

    let enum_variant_index = build_enum_variant_index(program);
    let mut used_function_names: HashSet<String> = program
        .functions
        .iter()
        .map(|function| function.name.clone())
        .collect();

    let mut instance_name_by_key: HashMap<String, String> = HashMap::new();
    let mut queue: VecDeque<GenericInstantiation> = VecDeque::new();
    let mut instantiated: HashSet<String> = HashSet::new();
    let mut diagnostics = Vec::new();

    let mut out = program.clone();
    out.functions = program
        .functions
        .iter()
        .filter(|function| function.generics.is_empty())
        .cloned()
        .collect();

    for function in &mut out.functions {
        rewrite_function_calls_for_specialization(
            function,
            &generic_templates,
            &enum_variant_index,
            &mut queue,
            &mut instance_name_by_key,
            &mut used_function_names,
            &mut diagnostics,
        );
    }

    while let Some(instantiation) = queue.pop_front() {
        let key = generic_instantiation_key(&instantiation.callee, &instantiation.args);
        if !instantiated.insert(key.clone()) {
            continue;
        }

        let Some(template) = generic_templates.get(&instantiation.callee) else {
            diagnostics.push(Diagnostic::error(
                format!(
                    "native lowering: missing generic function template '{}'",
                    instantiation.callee
                ),
                crate::error::Span::new(0, 0),
            ));
            continue;
        };

        if instantiation.args.len() != template.generics.len() {
            diagnostics.push(Diagnostic::error(
                format!(
                    "native lowering: generic function '{}' expects {} type argument(s) but got {}",
                    template.name,
                    template.generics.len(),
                    instantiation.args.len()
                ),
                template.span,
            ));
            continue;
        }

        let Some(instance_name) = instance_name_by_key.get(&key).cloned() else {
            diagnostics.push(Diagnostic::error(
                format!(
                    "native lowering: missing generated instance name for generic function '{}'",
                    instantiation.callee
                ),
                template.span,
            ));
            continue;
        };

        let mut instance = template.clone();
        instance.name = instance_name;
        let generic_names = template
            .generics
            .iter()
            .map(|param| param.name.clone())
            .collect::<Vec<_>>();
        let subst = build_subst(&generic_names, &instantiation.args);
        instance.generics.clear();
        apply_subst_to_hir_function(&mut instance, &subst);

        rewrite_function_calls_for_specialization(
            &mut instance,
            &generic_templates,
            &enum_variant_index,
            &mut queue,
            &mut instance_name_by_key,
            &mut used_function_names,
            &mut diagnostics,
        );

        out.functions.push(instance);
    }

    if diagnostics.is_empty() {
        Ok(out)
    } else {
        Err(diagnostics)
    }
}

fn build_enum_variant_index(program: &HirProgram) -> HashMap<String, HashSet<String>> {
    let mut out: HashMap<String, HashSet<String>> = HashMap::new();
    for enum_decl in &program.enums {
        let variants = enum_decl
            .variants
            .iter()
            .map(|variant| variant.name.clone())
            .collect::<HashSet<_>>();
        out.insert(enum_decl.name.clone(), variants);
    }
    out
}

fn generic_instantiation_key(callee: &str, args: &[TypeRef]) -> String {
    let args = args
        .iter()
        .map(|ty| ty.to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!("{callee}<{args}>")
}

fn next_generic_instance_name(callee: &str, used_function_names: &mut HashSet<String>) -> String {
    let mut index = 0usize;
    loop {
        let candidate = format!("{callee}__inst_{index}");
        if used_function_names.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}

fn call_target_function_slot<'a>(
    target: &'a [String],
    enum_variant_index: &HashMap<String, HashSet<String>>,
) -> Option<(usize, &'a str)> {
    match target {
        [name] => Some((0, name.as_str())),
        [head, name] => {
            let looks_like_enum_variant = enum_variant_index
                .get(head.as_str())
                .map(|variants| variants.contains(name))
                .unwrap_or(false);
            if looks_like_enum_variant {
                None
            } else {
                Some((1, name.as_str()))
            }
        }
        _ => None,
    }
}

fn rewrite_function_calls_for_specialization(
    function: &mut HirFunction,
    generic_templates: &HashMap<String, HirFunction>,
    enum_variant_index: &HashMap<String, HashSet<String>>,
    queue: &mut VecDeque<GenericInstantiation>,
    instance_name_by_key: &mut HashMap<String, String>,
    used_function_names: &mut HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let function_name = function.name.clone();
    let function_span = function.span;
    let Some(body) = &mut function.body else {
        return;
    };
    rewrite_block_calls_for_specialization(
        body,
        &function_name,
        function_span,
        generic_templates,
        enum_variant_index,
        queue,
        instance_name_by_key,
        used_function_names,
        diagnostics,
    );
}

fn rewrite_block_calls_for_specialization(
    block: &mut Block,
    function_name: &str,
    function_span: crate::error::Span,
    generic_templates: &HashMap<String, HirFunction>,
    enum_variant_index: &HashMap<String, HashSet<String>>,
    queue: &mut VecDeque<GenericInstantiation>,
    instance_name_by_key: &mut HashMap<String, String>,
    used_function_names: &mut HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for statement in &mut block.statements {
        match statement {
            Statement::Let(stmt) => rewrite_expr_calls_for_specialization(
                &mut stmt.value,
                function_name,
                function_span,
                generic_templates,
                enum_variant_index,
                queue,
                instance_name_by_key,
                used_function_names,
                diagnostics,
            ),
            Statement::Assign(stmt) => rewrite_expr_calls_for_specialization(
                &mut stmt.value,
                function_name,
                function_span,
                generic_templates,
                enum_variant_index,
                queue,
                instance_name_by_key,
                used_function_names,
                diagnostics,
            ),
            Statement::Return(stmt) => {
                if let Some(value) = &mut stmt.value {
                    rewrite_expr_calls_for_specialization(
                        value,
                        function_name,
                        function_span,
                        generic_templates,
                        enum_variant_index,
                        queue,
                        instance_name_by_key,
                        used_function_names,
                        diagnostics,
                    );
                }
            }
            Statement::Expr(expr) => rewrite_expr_calls_for_specialization(
                expr,
                function_name,
                function_span,
                generic_templates,
                enum_variant_index,
                queue,
                instance_name_by_key,
                used_function_names,
                diagnostics,
            ),
        }
    }
    if let Some(tail) = &mut block.tail {
        rewrite_expr_calls_for_specialization(
            tail,
            function_name,
            function_span,
            generic_templates,
            enum_variant_index,
            queue,
            instance_name_by_key,
            used_function_names,
            diagnostics,
        );
    }
}

fn rewrite_expr_calls_for_specialization(
    expr: &mut Expr,
    function_name: &str,
    function_span: crate::error::Span,
    generic_templates: &HashMap<String, HirFunction>,
    enum_variant_index: &HashMap<String, HashSet<String>>,
    queue: &mut VecDeque<GenericInstantiation>,
    instance_name_by_key: &mut HashMap<String, String>,
    used_function_names: &mut HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match expr {
        Expr::Path(_) | Expr::String(_) | Expr::Number(_) | Expr::Bool(_) => {}
        Expr::RecordLit { fields, .. } => {
            for field in fields {
                rewrite_expr_calls_for_specialization(
                    &mut field.value,
                    function_name,
                    function_span,
                    generic_templates,
                    enum_variant_index,
                    queue,
                    instance_name_by_key,
                    used_function_names,
                    diagnostics,
                );
            }
        }
        Expr::Call {
            target,
            type_args,
            args,
        } => {
            for arg in args {
                rewrite_expr_calls_for_specialization(
                    arg,
                    function_name,
                    function_span,
                    generic_templates,
                    enum_variant_index,
                    queue,
                    instance_name_by_key,
                    used_function_names,
                    diagnostics,
                );
            }

            let Some((slot, callee_name_ref)) =
                call_target_function_slot(target, enum_variant_index)
            else {
                return;
            };
            let callee_name = callee_name_ref.to_string();
            let Some(template) = generic_templates.get(&callee_name) else {
                return;
            };

            if type_args.len() != template.generics.len() {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' call '{}' uses {} generic type argument(s), expected {}",
                        function_name,
                        target.join("."),
                        type_args.len(),
                        template.generics.len()
                    ),
                    function_span,
                ));
                return;
            }

            let mut concrete_args = Vec::new();
            for type_arg in type_args.iter() {
                match type_arg {
                    TypeArg::Type(ty) => concrete_args.push(ty.clone()),
                    TypeArg::String(_) | TypeArg::Number(_) => {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' call '{}' uses non-type generic argument, which native lowering does not support",
                                function_name,
                                target.join(".")
                            ),
                            function_span,
                        ));
                        return;
                    }
                }
            }

            let key = generic_instantiation_key(&callee_name, &concrete_args);
            let instance_name = if let Some(name) = instance_name_by_key.get(&key).cloned() {
                name
            } else {
                let name = next_generic_instance_name(&callee_name, used_function_names);
                instance_name_by_key.insert(key, name.clone());
                name
            };
            target[slot] = instance_name;
            type_args.clear();
            queue.push_back(GenericInstantiation {
                callee: callee_name,
                args: concrete_args,
            });
        }
        Expr::If {
            cond,
            then_block,
            else_block,
        } => {
            rewrite_expr_calls_for_specialization(
                cond,
                function_name,
                function_span,
                generic_templates,
                enum_variant_index,
                queue,
                instance_name_by_key,
                used_function_names,
                diagnostics,
            );
            rewrite_block_calls_for_specialization(
                then_block,
                function_name,
                function_span,
                generic_templates,
                enum_variant_index,
                queue,
                instance_name_by_key,
                used_function_names,
                diagnostics,
            );
            if let Some(block) = else_block {
                rewrite_block_calls_for_specialization(
                    block,
                    function_name,
                    function_span,
                    generic_templates,
                    enum_variant_index,
                    queue,
                    instance_name_by_key,
                    used_function_names,
                    diagnostics,
                );
            }
        }
        Expr::While { cond, body } => {
            rewrite_expr_calls_for_specialization(
                cond,
                function_name,
                function_span,
                generic_templates,
                enum_variant_index,
                queue,
                instance_name_by_key,
                used_function_names,
                diagnostics,
            );
            rewrite_block_calls_for_specialization(
                body,
                function_name,
                function_span,
                generic_templates,
                enum_variant_index,
                queue,
                instance_name_by_key,
                used_function_names,
                diagnostics,
            );
        }
        Expr::Match { value, arms } => {
            rewrite_expr_calls_for_specialization(
                value,
                function_name,
                function_span,
                generic_templates,
                enum_variant_index,
                queue,
                instance_name_by_key,
                used_function_names,
                diagnostics,
            );
            for arm in arms {
                match &mut arm.body {
                    crate::ast::MatchArmBody::Expr(body_expr) => {
                        rewrite_expr_calls_for_specialization(
                            body_expr,
                            function_name,
                            function_span,
                            generic_templates,
                            enum_variant_index,
                            queue,
                            instance_name_by_key,
                            used_function_names,
                            diagnostics,
                        );
                    }
                    crate::ast::MatchArmBody::Block(block) => {
                        rewrite_block_calls_for_specialization(
                            block,
                            function_name,
                            function_span,
                            generic_templates,
                            enum_variant_index,
                            queue,
                            instance_name_by_key,
                            used_function_names,
                            diagnostics,
                        );
                    }
                }
            }
        }
        Expr::Binary { left, right, .. } => {
            rewrite_expr_calls_for_specialization(
                left,
                function_name,
                function_span,
                generic_templates,
                enum_variant_index,
                queue,
                instance_name_by_key,
                used_function_names,
                diagnostics,
            );
            rewrite_expr_calls_for_specialization(
                right,
                function_name,
                function_span,
                generic_templates,
                enum_variant_index,
                queue,
                instance_name_by_key,
                used_function_names,
                diagnostics,
            );
        }
    }
}

fn apply_subst_to_hir_function(function: &mut HirFunction, subst: &HashMap<String, TypeRef>) {
    for param in &mut function.params {
        param.ty = apply_subst(&param.ty, subst);
    }
    function.return_type = apply_subst(&function.return_type, subst);
    for required in &mut function.requires {
        *required = apply_subst(required, subst);
    }
    if let Some(body) = &mut function.body {
        apply_subst_to_block(body, subst);
    }
}

fn apply_subst_to_block(block: &mut Block, subst: &HashMap<String, TypeRef>) {
    for statement in &mut block.statements {
        match statement {
            Statement::Let(stmt) => {
                if let Some(ty) = &mut stmt.ty {
                    *ty = apply_subst(ty, subst);
                }
                apply_subst_to_expr(&mut stmt.value, subst);
            }
            Statement::Assign(stmt) => apply_subst_to_expr(&mut stmt.value, subst),
            Statement::Return(stmt) => {
                if let Some(value) = &mut stmt.value {
                    apply_subst_to_expr(value, subst);
                }
            }
            Statement::Expr(expr) => apply_subst_to_expr(expr, subst),
        }
    }
    if let Some(tail) = &mut block.tail {
        apply_subst_to_expr(tail, subst);
    }
}

fn apply_subst_to_expr(expr: &mut Expr, subst: &HashMap<String, TypeRef>) {
    match expr {
        Expr::Path(_) | Expr::String(_) | Expr::Number(_) | Expr::Bool(_) => {}
        Expr::RecordLit { ty, fields } => {
            *ty = apply_subst(ty, subst);
            for field in fields {
                apply_subst_to_expr(&mut field.value, subst);
            }
        }
        Expr::Call {
            type_args, args, ..
        } => {
            for type_arg in type_args {
                if let TypeArg::Type(ty) = type_arg {
                    *ty = apply_subst(ty, subst);
                }
            }
            for arg in args {
                apply_subst_to_expr(arg, subst);
            }
        }
        Expr::If {
            cond,
            then_block,
            else_block,
        } => {
            apply_subst_to_expr(cond, subst);
            apply_subst_to_block(then_block, subst);
            if let Some(block) = else_block {
                apply_subst_to_block(block, subst);
            }
        }
        Expr::While { cond, body } => {
            apply_subst_to_expr(cond, subst);
            apply_subst_to_block(body, subst);
        }
        Expr::Match { value, arms } => {
            apply_subst_to_expr(value, subst);
            for arm in arms {
                match &mut arm.body {
                    crate::ast::MatchArmBody::Expr(body_expr) => {
                        apply_subst_to_expr(body_expr, subst)
                    }
                    crate::ast::MatchArmBody::Block(block) => apply_subst_to_block(block, subst),
                }
            }
        }
        Expr::Binary { left, right, .. } => {
            apply_subst_to_expr(left, subst);
            apply_subst_to_expr(right, subst);
        }
    }
}

fn build_native_records(program: &HirProgram) -> Vec<MirRecord> {
    program
        .records
        .iter()
        .map(|record| MirRecord {
            name: record.name.clone(),
            generics: record
                .generics
                .iter()
                .map(|param| param.name.clone())
                .collect(),
            fields: record
                .fields
                .iter()
                .map(|field| MirRecordField {
                    name: field.name.clone(),
                    ty: field.ty.clone(),
                })
                .collect(),
        })
        .collect()
}

fn build_native_enums(program: &HirProgram) -> Vec<MirEnum> {
    program
        .enums
        .iter()
        .filter(|enum_decl| enum_decl.variants.len() <= u8::MAX as usize)
        .map(|enum_decl| MirEnum {
            name: enum_decl.name.clone(),
            generics: enum_decl
                .generics
                .iter()
                .map(|param| param.name.clone())
                .collect(),
            variants: enum_decl
                .variants
                .iter()
                .enumerate()
                .map(|(index, variant)| MirEnumVariant {
                    name: variant.name.clone(),
                    tag: index as u8,
                    payload: variant.payload.clone(),
                })
                .collect(),
        })
        .collect()
}

fn build_signatures(program: &HirProgram) -> HashMap<String, FunctionSignature> {
    let mut signatures = HashMap::new();
    for function in &program.functions {
        let effects = function
            .effects
            .iter()
            .map(|effect| {
                if let Some(argument) = effect.argument.as_deref() {
                    format!("{}({argument})", effect.name)
                } else {
                    effect.name.clone()
                }
            })
            .collect::<Vec<_>>();

        signatures.insert(
            function.name.clone(),
            FunctionSignature {
                return_type: function.return_type.clone(),
                effects,
                has_generics: !function.generics.is_empty(),
            },
        );
    }
    signatures
}

fn lower_function(
    function: &HirFunction,
    signatures: &HashMap<String, FunctionSignature>,
    records: &HashMap<String, MirRecord>,
    enums: &HashMap<String, MirEnum>,
) -> Result<MirFunction, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();

    if !function.generics.is_empty() {
        diagnostics.push(Diagnostic::error(
            format!(
                "function '{}' is generic but native lowering does not support generics yet",
                function.name
            ),
            function.span,
        ));
    }

    if function.body.is_some() && !function.effects.is_empty() {
        diagnostics.push(Diagnostic::error(
            format!(
                "function '{}' has effects and cannot be lowered to native code yet",
                function.name
            ),
            function.span,
        ));
    }

    if function.body.is_some() {
        for param in &function.params {
            if !is_native_type(&param.ty, records, enums) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' parameter '{}' uses type '{}' which is not supported by native lowering yet",
                        function.name, param.name, param.ty
                    ),
                    function.span,
                ));
            }
        }

        if !is_native_type(&function.return_type, records, enums) {
            diagnostics.push(Diagnostic::error(
                format!(
                    "function '{}' return type '{}' is not supported by native lowering yet",
                    function.name, function.return_type
                ),
                function.span,
            ));
        }
    }

    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    let mut builder = MirBuilder::new(function, signatures, records, enums);
    match &function.body {
        None => {
            builder.emit_stub_body();
        }
        Some(body) => {
            if let Err(error) = builder.lower_function_body(body) {
                return Err(vec![error]);
            }
        }
    }

    Ok(builder.finish())
}

fn is_native_scalar_type(ty: &TypeRef) -> bool {
    matches!(ty.head(), "Int" | "Bool" | "Unit")
}

fn is_native_type(
    ty: &TypeRef,
    records: &HashMap<String, MirRecord>,
    enums: &HashMap<String, MirEnum>,
) -> bool {
    if is_native_scalar_type(ty) {
        return true;
    }

    if ty.head() == "Text" {
        return true;
    }

    if records.contains_key(ty.head()) {
        return true;
    }

    enums.contains_key(ty.head())
}

fn type_args_as_types(ty: &TypeRef) -> Vec<TypeRef> {
    ty.args
        .iter()
        .filter_map(|arg| match arg {
            TypeArg::Type(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

fn build_subst(generic_names: &[String], args: &[TypeRef]) -> HashMap<String, TypeRef> {
    let mut out = HashMap::new();
    for (name, arg) in generic_names.iter().zip(args.iter()) {
        out.insert(name.clone(), arg.clone());
    }
    out
}

fn apply_subst(ty: &TypeRef, subst: &HashMap<String, TypeRef>) -> TypeRef {
    if ty.args.is_empty() {
        if let Some(repl) = subst.get(&ty.name) {
            return repl.clone();
        }
        return ty.clone();
    }

    let mut out = ty.clone();
    out.args = ty
        .args
        .iter()
        .map(|arg| match arg {
            TypeArg::Type(t0) => TypeArg::Type(apply_subst(t0, subst)),
            other => other.clone(),
        })
        .collect();
    out
}

struct MirBuilder<'a> {
    function: &'a HirFunction,
    signatures: &'a HashMap<String, FunctionSignature>,
    records: &'a HashMap<String, MirRecord>,
    enums: &'a HashMap<String, MirEnum>,
    params: Vec<MirParam>,
    locals: Vec<MirLocal>,
    scopes: Vec<HashMap<String, usize>>,
    blocks: Vec<MirBlock>,
    next_block_id: usize,
    current_block: usize,
}

impl<'a> MirBuilder<'a> {
    fn new(
        function: &'a HirFunction,
        signatures: &'a HashMap<String, FunctionSignature>,
        records: &'a HashMap<String, MirRecord>,
        enums: &'a HashMap<String, MirEnum>,
    ) -> Self {
        let mut locals = Vec::new();
        let mut params = Vec::new();
        let mut scopes: Vec<HashMap<String, usize>> = vec![HashMap::new()];

        for param in &function.params {
            let local = locals.len();
            locals.push(MirLocal {
                name: param.name.clone(),
                ty: param.ty.clone(),
            });
            scopes[0].insert(param.name.clone(), local);
            params.push(MirParam {
                name: param.name.clone(),
                ty: param.ty.clone(),
                local,
            });
        }

        let entry_block = MirBlock {
            label: "bb0".to_string(),
            statements: Vec::new(),
            terminator: MirTerminator::ReturnDefault(TypeRef {
                name: "Unit".to_string(),
                args: Vec::new(),
            }),
        };

        Self {
            function,
            signatures,
            records,
            enums,
            params,
            locals,
            scopes,
            blocks: vec![entry_block],
            next_block_id: 1,
            current_block: 0,
        }
    }

    fn finish(self) -> MirFunction {
        let effects = self
            .function
            .effects
            .iter()
            .map(|effect| {
                if let Some(argument) = effect.argument.as_deref() {
                    format!("{}({argument})", effect.name)
                } else {
                    effect.name.clone()
                }
            })
            .collect();

        MirFunction {
            name: self.function.name.clone(),
            params: self.params,
            return_type: self.function.return_type.clone(),
            effects,
            locals: self.locals,
            blocks: self.blocks,
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        let popped = self.scopes.pop();
        if popped.is_none() {
            panic!("lowering bug: scope underflow");
        }
    }

    fn with_scope<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, Diagnostic>,
    ) -> Result<T, Diagnostic> {
        self.push_scope();
        let result = f(self);
        self.pop_scope();
        result
    }

    fn lookup_local(&self, name: &str) -> Option<usize> {
        for scope in self.scopes.iter().rev() {
            if let Some(local) = scope.get(name) {
                return Some(*local);
            }
        }
        None
    }

    fn declare_local(
        &mut self,
        name: &str,
        ty: TypeRef,
        allow_shadow: bool,
    ) -> Result<usize, Diagnostic> {
        if !allow_shadow && self.lookup_local(name).is_some() {
            return Err(Diagnostic::error(
                format!(
                    "function '{}' redefines local '{}' in lowering",
                    self.function.name, name
                ),
                self.function.span,
            ));
        }

        let local = self.locals.len();
        self.locals.push(MirLocal {
            name: name.to_string(),
            ty,
        });

        let scope = self
            .scopes
            .last_mut()
            .expect("lowering bug: no active scope");
        scope.insert(name.to_string(), local);
        Ok(local)
    }

    fn emit_stub_body(&mut self) {
        let block = &mut self.blocks[0];
        block.terminator = MirTerminator::ReturnDefault(self.function.return_type.clone());
    }

    fn lower_function_body(&mut self, body: &Block) -> Result<(), Diagnostic> {
        self.blocks[0].terminator = MirTerminator::ReturnDefault(TypeRef {
            name: "Unit".to_string(),
            args: Vec::new(),
        });

        self.lower_block_statements(body, BlockMode::FunctionBody)?;

        if self.block_is_terminated(self.current_block) {
            return Ok(());
        }

        let value = if let Some(expr) = &body.tail {
            self.lower_expr(expr)?
        } else {
            ExprValue::unit()
        };

        self.set_return_terminator(value);
        Ok(())
    }

    fn lower_block_statements(&mut self, block: &Block, mode: BlockMode) -> Result<(), Diagnostic> {
        for statement in &block.statements {
            if self.block_is_terminated(self.current_block) {
                break;
            }

            match statement {
                Statement::Let(stmt) => {
                    let value = self.lower_expr(&stmt.value)?;
                    let inferred_ty = value.ty.clone();
                    let ty = stmt.ty.as_ref().unwrap_or(&inferred_ty);

                    if !is_native_type(ty, self.records, self.enums) {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' uses let-binding '{}' with type '{}' which is not supported by native lowering yet",
                                self.function.name, stmt.name, ty
                            ),
                            self.function.span,
                        ));
                    }

                    let allow_shadow = stmt.name == "_";
                    let local = self.declare_local(&stmt.name, ty.clone(), allow_shadow)?;
                    self.emit_store(local, value);
                }
                Statement::Assign(stmt) => {
                    let Some(local) = self.lookup_local(&stmt.name) else {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' assigns to unknown local '{}'",
                                self.function.name, stmt.name
                            ),
                            self.function.span,
                        ));
                    };
                    let value = self.lower_expr(&stmt.value)?;
                    self.emit_store(local, value);
                }
                Statement::Return(stmt) => {
                    let value = if let Some(expr) = &stmt.value {
                        self.lower_expr(expr)?
                    } else {
                        ExprValue::unit()
                    };
                    self.set_return_terminator(value);
                }
                Statement::Expr(expr) => {
                    let _ = self.lower_expr(expr)?;
                }
            }
        }

        if self.block_is_terminated(self.current_block) {
            return Ok(());
        }

        match mode {
            BlockMode::FunctionBody => Ok(()),
            BlockMode::Value => Ok(()),
            BlockMode::LoopBody => {
                if let Some(expr) = &block.tail {
                    let _ = self.lower_expr(expr)?;
                }
                Ok(())
            }
        }
    }

    fn lower_block_value(&mut self, block: &Block) -> Result<ExprValue, Diagnostic> {
        self.lower_block_statements(block, BlockMode::Value)?;
        if self.block_is_terminated(self.current_block) {
            return Ok(ExprValue::unreachable());
        }

        if let Some(expr) = &block.tail {
            self.lower_expr(expr)
        } else {
            Ok(ExprValue::unit())
        }
    }

    fn lower_expr(&mut self, expr: &Expr) -> Result<ExprValue, Diagnostic> {
        match expr {
            Expr::Number(raw) => {
                let value = raw.parse::<i64>().map_err(|_| {
                    Diagnostic::error(
                        format!("invalid integer literal '{raw}'"),
                        self.function.span,
                    )
                })?;
                Ok(ExprValue {
                    ty: TypeRef {
                        name: "Int".to_string(),
                        args: Vec::new(),
                    },
                    operand: Some(MirOperand::ConstInt(value)),
                })
            }
            Expr::Bool(value) => Ok(ExprValue {
                ty: TypeRef {
                    name: "Bool".to_string(),
                    args: Vec::new(),
                },
                operand: Some(MirOperand::ConstBool(*value)),
            }),
            Expr::String(value) => Ok(ExprValue {
                ty: TypeRef {
                    name: "Text".to_string(),
                    args: Vec::new(),
                },
                operand: Some(MirOperand::ConstText(value.clone())),
            }),
            Expr::Path(segments) => {
                let Some((first, rest)) = segments.split_first() else {
                    return Err(Diagnostic::error(
                        "expected identifier path",
                        self.function.span,
                    ));
                };

                // Base: local variable?
                let mut cur_ty: TypeRef;
                let mut cur_op: MirOperand;

                if let Some(local) = self.lookup_local(first.as_str()) {
                    cur_ty = self.locals[local].ty.clone();
                    cur_op = MirOperand::Local(local);
                } else {
                    // Unit enum variant as value:
                    // - unqualified: `Nil`
                    // - qualified: `Option::None` (represented as two segments)
                    if rest.is_empty() {
                        let mut found: Option<(&MirEnum, &MirEnumVariant)> = None;
                        let mut ambiguous = false;
                        for enum_decl in self.enums.values() {
                            for variant in &enum_decl.variants {
                                if variant.name == *first {
                                    if found.is_some() {
                                        ambiguous = true;
                                        break;
                                    }
                                    found = Some((enum_decl, variant));
                                }
                            }
                            if ambiguous {
                                break;
                            }
                        }
                        if ambiguous {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' uses ambiguous enum variant '{}' (qualify it)",
                                    self.function.name, first
                                ),
                                self.function.span,
                            ));
                        }
                        let Some((enum_decl, variant)) = found else {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' uses unknown local '{}' in body",
                                    self.function.name, first
                                ),
                                self.function.span,
                            ));
                        };
                        if variant.payload.is_some() {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' uses enum variant '{}' without payload (expected call)",
                                    self.function.name, first
                                ),
                                self.function.span,
                            ));
                        }

                        cur_ty = TypeRef {
                            name: enum_decl.name.clone(),
                            args: Vec::new(),
                        };
                        let temp = self.new_temp_local(cur_ty.clone());
                        self.current_block_mut()
                            .statements
                            .push(MirStatement::Assign {
                                dst: temp,
                                rvalue: MirRvalue::EnumLit {
                                    enum_name: enum_decl.name.clone(),
                                    tag: variant.tag,
                                    payload: None,
                                    payload_ty: None,
                                },
                            });
                        cur_op = MirOperand::Local(temp);
                    } else if let [enum_name, variant_name] | [_, enum_name, variant_name] =
                        segments.as_slice()
                    {
                        // Qualified unit variant: Enum::Variant or Ns::Enum::Variant
                        let Some(enum_decl) = self.enums.get(enum_name.as_str()) else {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' uses unknown local '{}' in body",
                                    self.function.name, enum_name
                                ),
                                self.function.span,
                            ));
                        };
                        let Some(variant) = enum_decl
                            .variants
                            .iter()
                            .find(|variant| variant.name == *variant_name)
                        else {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' uses unknown enum variant '{}::{}'",
                                    self.function.name, enum_name, variant_name
                                ),
                                self.function.span,
                            ));
                        };
                        if variant.payload.is_some() {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' uses enum variant '{}::{}' without payload (expected call)",
                                    self.function.name, enum_name, variant_name
                                ),
                                self.function.span,
                            ));
                        }

                        cur_ty = TypeRef {
                            name: enum_decl.name.clone(),
                            args: Vec::new(),
                        };
                        let temp = self.new_temp_local(cur_ty.clone());
                        self.current_block_mut()
                            .statements
                            .push(MirStatement::Assign {
                                dst: temp,
                                rvalue: MirRvalue::EnumLit {
                                    enum_name: enum_decl.name.clone(),
                                    tag: variant.tag,
                                    payload: None,
                                    payload_ty: None,
                                },
                            });
                        cur_op = MirOperand::Local(temp);

                        // Fully consumed (do not treat as record projection).
                        return Ok(ExprValue {
                            ty: cur_ty,
                            operand: Some(cur_op),
                        });
                    } else {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' uses path '{}' which is not supported by native lowering yet",
                                self.function.name,
                                segments.join(".")
                            ),
                            self.function.span,
                        ));
                    }
                }

                // Record projection chain: x.a.b.c
                for member in rest {
                    let Some(record) = self.records.get(cur_ty.head()) else {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' uses path '{}' which is not supported by native lowering yet",
                                self.function.name,
                                segments.join(".")
                            ),
                            self.function.span,
                        ));
                    };

                    let args = type_args_as_types(&cur_ty);
                    let subst = build_subst(&record.generics, &args);

                    let Some((field_index, field_schema)) = record
                        .fields
                        .iter()
                        .enumerate()
                        .find(|(_, schema)| schema.name == *member)
                    else {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' uses unknown field '{}' on record '{}'",
                                self.function.name, member, cur_ty
                            ),
                            self.function.span,
                        ));
                    };

                    let field_ty = apply_subst(&field_schema.ty, &subst);
                    let temp = self.new_temp_local(field_ty.clone());
                    self.current_block_mut()
                        .statements
                        .push(MirStatement::Assign {
                            dst: temp,
                            rvalue: MirRvalue::ProjectField {
                                base: cur_op,
                                record: record.name.clone(),
                                index: field_index,
                                field_ty: field_ty.clone(),
                            },
                        });
                    cur_ty = field_ty;
                    cur_op = MirOperand::Local(temp);
                }

                Ok(ExprValue {
                    ty: cur_ty,
                    operand: Some(cur_op),
                })
            }
            Expr::Binary { op, left, right } => {
                let left_value = self.lower_expr(left)?;
                let right_value = self.lower_expr(right)?;

                let result_ty = match op {
                    BinaryOp::Add => TypeRef {
                        name: "Int".to_string(),
                        args: Vec::new(),
                    },
                    BinaryOp::Eq | BinaryOp::NotEq => TypeRef {
                        name: "Bool".to_string(),
                        args: Vec::new(),
                    },
                };

                match op {
                    BinaryOp::Add => {
                        if left_value.ty.head() != "Int" || right_value.ty.head() != "Int" {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' uses '+' on non-Int types '{}' and '{}'",
                                    self.function.name, left_value.ty, right_value.ty
                                ),
                                self.function.span,
                            ));
                        }
                    }
                    BinaryOp::Eq | BinaryOp::NotEq => {
                        if left_value.ty != right_value.ty
                            && !(left_value.ty.head() == right_value.ty.head()
                                && self.enums.contains_key(left_value.ty.head()))
                        {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' uses equality op on mismatched types '{}' and '{}'",
                                    self.function.name, left_value.ty, right_value.ty
                                ),
                                self.function.span,
                            ));
                        }

                        let head = left_value.ty.head();
                        let comparable = matches!(head, "Int" | "Bool" | "Text")
                            || self.enums.contains_key(head);
                        if !comparable {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' uses equality op on unsupported type '{}'",
                                    self.function.name, left_value.ty
                                ),
                                self.function.span,
                            ));
                        }
                    }
                }

                let left_op = left_value.into_operand_or_unit(self.function)?;
                let right_op = right_value.into_operand_or_unit(self.function)?;

                let temp = self.new_temp_local(result_ty.clone());
                self.current_block_mut()
                    .statements
                    .push(MirStatement::Assign {
                        dst: temp,
                        rvalue: MirRvalue::Binary {
                            op: *op,
                            left: left_op,
                            right: right_op,
                        },
                    });
                Ok(ExprValue {
                    ty: result_ty,
                    operand: Some(MirOperand::Local(temp)),
                })
            }
            Expr::Call {
                target,
                type_args,
                args,
            } => {
                if !type_args.is_empty() {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' call '{}' uses generic type arguments, which native lowering does not support yet",
                            self.function.name,
                            target.join(".")
                        ),
                        self.function.span,
                    ));
                }

                // Function call (unqualified or namespace-qualified alias).
                let function_target = match target.as_slice() {
                    [name] => Some(name.clone()),
                    [enum_or_ns, name] => {
                        let looks_like_enum_variant = self
                            .enums
                            .get(enum_or_ns)
                            .map(|en| en.variants.iter().any(|variant| variant.name == *name))
                            .unwrap_or(false);
                        if looks_like_enum_variant {
                            None
                        } else {
                            Some(name.clone())
                        }
                    }
                    _ => None,
                };

                if let Some(callee) = function_target {
                    if let Some(signature) = self.signatures.get(&callee) {
                        if signature.has_generics {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' calls generic function '{}' but native lowering does not support generics yet",
                                    self.function.name, callee
                                ),
                                self.function.span,
                            ));
                        }

                        if !signature.effects.is_empty() {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' calls effectful function '{}' which native lowering cannot execute",
                                    self.function.name, callee
                                ),
                                self.function.span,
                            ));
                        }

                        let return_ty = signature.return_type.clone();
                        if !is_native_type(&return_ty, self.records, self.enums) {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' calls '{}' returning '{}' which is not supported by native lowering yet",
                                    self.function.name, callee, return_ty
                                ),
                                self.function.span,
                            ));
                        }

                        let mut lowered_args = Vec::new();
                        for arg in args {
                            let value = self.lower_expr(arg)?;
                            lowered_args.push(value.into_operand_or_unit(self.function)?);
                        }

                        if return_ty.head() == "Unit" {
                            self.current_block_mut().statements.push(MirStatement::Eval(
                                MirRvalue::Call {
                                    callee,
                                    args: lowered_args,
                                },
                            ));
                            return Ok(ExprValue::unit());
                        }

                        let temp = self.new_temp_local(return_ty.clone());
                        self.current_block_mut()
                            .statements
                            .push(MirStatement::Assign {
                                dst: temp,
                                rvalue: MirRvalue::Call {
                                    callee,
                                    args: lowered_args,
                                },
                            });

                        return Ok(ExprValue {
                            ty: return_ty,
                            operand: Some(MirOperand::Local(temp)),
                        });
                    }
                }

                // Enum constructor call:
                // - unqualified: `Variant(...)` (must be unambiguous across enums)
                // - qualified: `Enum::Variant(...)`
                let (enum_name, variant_name) = match target.as_slice() {
                    [variant] => (None, variant.as_str()),
                    [enum_name, variant] => (Some(enum_name.as_str()), variant.as_str()),
                    [_, enum_name, variant] => (Some(enum_name.as_str()), variant.as_str()),
                    _ => {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' calls '{}' which is not supported by native lowering yet",
                                self.function.name,
                                target.join(".")
                            ),
                            self.function.span,
                        ));
                    }
                };

                let mut found: Option<(&MirEnum, &MirEnumVariant)> = None;
                let mut ambiguous = false;

                if let Some(en) = enum_name {
                    let Some(enum_decl) = self.enums.get(en) else {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' calls unknown enum '{}'",
                                self.function.name, en
                            ),
                            self.function.span,
                        ));
                    };
                    let Some(variant) = enum_decl
                        .variants
                        .iter()
                        .find(|variant| variant.name == variant_name)
                    else {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' calls unknown target '{}'",
                                self.function.name,
                                target.join(".")
                            ),
                            self.function.span,
                        ));
                    };
                    found = Some((enum_decl, variant));
                } else {
                    for enum_decl in self.enums.values() {
                        for variant in &enum_decl.variants {
                            if variant.name == variant_name {
                                if found.is_some() {
                                    ambiguous = true;
                                    break;
                                }
                                found = Some((enum_decl, variant));
                            }
                        }
                        if ambiguous {
                            break;
                        }
                    }
                }

                let Some((enum_decl, variant)) = found else {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' calls unknown target '{}'",
                            self.function.name,
                            target.join(".")
                        ),
                        self.function.span,
                    ));
                };
                if ambiguous {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' calls ambiguous enum constructor '{}' (qualify it)",
                            self.function.name,
                            target.join(".")
                        ),
                        self.function.span,
                    ));
                }

                let payload = if let Some(_pty) = &variant.payload {
                    if args.len() != 1 {
                        return Err(Diagnostic::error(
                            format!(
                                "enum variant '{}::{}' expects 1 argument but got {}",
                                enum_decl.name,
                                variant.name,
                                args.len()
                            ),
                            self.function.span,
                        ));
                    }
                    let value = self.lower_expr(&args[0])?;
                    let ty = value.ty.clone();
                    let op = value.into_operand_or_unit(self.function)?;
                    Some((op, ty))
                } else {
                    if !args.is_empty() {
                        return Err(Diagnostic::error(
                            format!(
                                "enum variant '{}::{}' expects 0 arguments but got {}",
                                enum_decl.name,
                                variant.name,
                                args.len()
                            ),
                            self.function.span,
                        ));
                    }
                    None
                };

                let enum_ty = TypeRef {
                    name: enum_decl.name.clone(),
                    args: Vec::new(),
                };
                let temp = self.new_temp_local(enum_ty.clone());
                self.current_block_mut()
                    .statements
                    .push(MirStatement::Assign {
                        dst: temp,
                        rvalue: MirRvalue::EnumLit {
                            enum_name: enum_decl.name.clone(),
                            tag: variant.tag,
                            payload: payload.as_ref().map(|(op, _)| op.clone()),
                            payload_ty: payload.as_ref().map(|(_, ty)| ty.clone()),
                        },
                    });
                Ok(ExprValue {
                    ty: enum_ty,
                    operand: Some(MirOperand::Local(temp)),
                })
            }
            Expr::If {
                cond,
                then_block,
                else_block,
            } => self.lower_if_expr(cond, then_block, else_block.as_deref()),
            Expr::While { cond, body } => self.lower_while_expr(cond, body),
            Expr::RecordLit { ty, fields } => {
                let Some(record) = self.records.get(ty.head()) else {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' uses record literal of unsupported type '{}'",
                            self.function.name,
                            ty.head()
                        ),
                        self.function.span,
                    ));
                };

                let mut lowered_fields: HashMap<&str, MirOperand> = HashMap::new();
                for field in fields {
                    let value = self.lower_expr(&field.value)?;
                    lowered_fields.insert(
                        field.name.as_str(),
                        value.into_operand_or_unit(self.function)?,
                    );
                }

                let mut ordered = Vec::new();
                for schema_field in &record.fields {
                    let Some(value) = lowered_fields.get(schema_field.name.as_str()) else {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' record literal missing field '{}' for type '{}'",
                                self.function.name, schema_field.name, record.name
                            ),
                            self.function.span,
                        ));
                    };
                    ordered.push(value.clone());
                }

                let args = type_args_as_types(ty);
                let subst = build_subst(&record.generics, &args);
                let field_tys: Vec<TypeRef> = record
                    .fields
                    .iter()
                    .map(|f| apply_subst(&f.ty, &subst))
                    .collect();

                let temp = self.new_temp_local(ty.clone());
                self.current_block_mut()
                    .statements
                    .push(MirStatement::Assign {
                        dst: temp,
                        rvalue: MirRvalue::RecordLit {
                            record: record.name.clone(),
                            fields: ordered,
                            field_tys,
                        },
                    });

                Ok(ExprValue {
                    ty: ty.clone(),
                    operand: Some(MirOperand::Local(temp)),
                })
            }
            Expr::Match { value, arms } => self.lower_match_expr(value, arms),
        }
    }

    fn lower_match_expr(
        &mut self,
        value: &Expr,
        arms: &[crate::ast::MatchArm],
    ) -> Result<ExprValue, Diagnostic> {
        let scrutinee = self.lower_expr(value)?;
        let scrut_ty = scrutinee.ty.clone();
        let Some(enum_decl) = self.enums.get(scrut_ty.head()) else {
            return Err(Diagnostic::error(
                format!(
                    "function '{}' uses match on unsupported type '{}'",
                    self.function.name, scrut_ty
                ),
                self.function.span,
            ));
        };

        let scrut_op = scrutinee.into_operand_or_unit(self.function)?;

        // Determine whether this match is exhaustive (to avoid requiring a wildcard arm).
        let mut has_wildcard = false;
        let mut covered: HashMap<u8, bool> = HashMap::new();
        for arm in arms {
            match &arm.pattern {
                crate::ast::MatchPattern::Wildcard => {
                    has_wildcard = true;
                }
                crate::ast::MatchPattern::Variant { path, .. } => {
                    let variant_name = match path.as_slice() {
                        [v] => v.as_str(),
                        [en, v] if en.as_str() == enum_decl.name => v.as_str(),
                        [_, en, v] if en.as_str() == enum_decl.name => v.as_str(),
                        _ => continue,
                    };
                    if let Some(variant) = enum_decl
                        .variants
                        .iter()
                        .find(|variant| variant.name == variant_name)
                    {
                        covered.insert(variant.tag, true);
                    }
                }
            }
        }
        let mut exhaustive = has_wildcard;
        if !exhaustive {
            exhaustive = enum_decl
                .variants
                .iter()
                .all(|variant| covered.get(&variant.tag).copied().unwrap_or(false));
        }

        let join_bb = self.new_block();
        let mut result_local: Option<usize> = None;
        let mut result_ty: Option<TypeRef> = None;

        // The current block is the first "test" block.
        let mut test_bb = self.blocks[self.current_block].label.clone();

        for (index, arm) in arms.iter().enumerate() {
            self.switch_to_block(&test_bb);
            if self.block_is_terminated(self.current_block) {
                break;
            }

            let is_last = index + 1 == arms.len();

            match &arm.pattern {
                crate::ast::MatchPattern::Wildcard => {
                    let arm_bb = self.new_block();
                    self.set_terminator(MirTerminator::Goto {
                        target: arm_bb.clone(),
                    });

                    self.switch_to_block(&arm_bb);
                    let value = self.with_scope(|builder| {
                        builder.lower_match_arm_body(&scrut_op, enum_decl, None, None, &arm.body)
                    })?;
                    self.record_match_arm_result(&mut result_local, &mut result_ty, value)?;

                    if !self.block_is_terminated(self.current_block) {
                        self.set_terminator(MirTerminator::Goto {
                            target: join_bb.clone(),
                        });
                    }
                    // Wildcard consumes the rest.
                    break;
                }
                crate::ast::MatchPattern::Variant { path, bind } => {
                    let variant_name = match path.as_slice() {
                        [v] => v.as_str(),
                        [en, v] => {
                            if en.as_str() != enum_decl.name {
                                return Err(Diagnostic::error(
                                    format!(
                                        "function '{}' match pattern targets different enum '{}'",
                                        self.function.name, en
                                    ),
                                    self.function.span,
                                ));
                            }
                            v.as_str()
                        }
                        [_, en, v] => {
                            if en.as_str() != enum_decl.name {
                                return Err(Diagnostic::error(
                                    format!(
                                        "function '{}' match pattern targets different enum '{}'",
                                        self.function.name, en
                                    ),
                                    self.function.span,
                                ));
                            }
                            v.as_str()
                        }
                        _ => {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' uses unsupported match pattern path '{}'",
                                    self.function.name,
                                    path.join(".")
                                ),
                                self.function.span,
                            ));
                        }
                    };

                    let Some(variant) = enum_decl
                        .variants
                        .iter()
                        .find(|variant| variant.name == variant_name)
                    else {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' match pattern uses unknown variant '{}::{}'",
                                self.function.name, enum_decl.name, variant_name
                            ),
                            self.function.span,
                        ));
                    };

                    let tag_local = self.new_temp_local(TypeRef {
                        name: "Int".to_string(),
                        args: Vec::new(),
                    });
                    self.current_block_mut()
                        .statements
                        .push(MirStatement::Assign {
                            dst: tag_local,
                            rvalue: MirRvalue::EnumTag {
                                base: scrut_op.clone(),
                                enum_name: enum_decl.name.clone(),
                            },
                        });
                    let cmp_local = self.new_temp_local(TypeRef {
                        name: "Bool".to_string(),
                        args: Vec::new(),
                    });
                    self.current_block_mut()
                        .statements
                        .push(MirStatement::Assign {
                            dst: cmp_local,
                            rvalue: MirRvalue::Binary {
                                op: BinaryOp::Eq,
                                left: MirOperand::Local(tag_local),
                                right: MirOperand::ConstInt(variant.tag as i64),
                            },
                        });

                    let arm_bb = self.new_block();
                    let else_bb = if is_last && exhaustive {
                        // Exhaustive match without wildcard: else branch is unreachable,
                        // but we still need a valid CFG edge.
                        join_bb.clone()
                    } else {
                        self.new_block()
                    };

                    self.set_terminator(MirTerminator::If {
                        cond: MirOperand::Local(cmp_local),
                        then_bb: arm_bb.clone(),
                        else_bb: else_bb.clone(),
                    });

                    self.switch_to_block(&arm_bb);
                    let scrut_args = type_args_as_types(&scrut_ty);
                    let subst = build_subst(&enum_decl.generics, &scrut_args);
                    let payload_ty = variant.payload.as_ref().map(|pty| apply_subst(pty, &subst));
                    let value = self.with_scope(|builder| {
                        builder.lower_match_arm_body(
                            &scrut_op,
                            enum_decl,
                            payload_ty,
                            bind.clone(),
                            &arm.body,
                        )
                    })?;
                    self.record_match_arm_result(&mut result_local, &mut result_ty, value)?;
                    if !self.block_is_terminated(self.current_block) {
                        self.set_terminator(MirTerminator::Goto {
                            target: join_bb.clone(),
                        });
                    }

                    test_bb = else_bb;

                    if is_last && !exhaustive {
                        self.switch_to_block(&test_bb);
                        return Err(Diagnostic::error(
                            "non-exhaustive match expression is not supported by native lowering yet",
                            self.function.span,
                        ));
                    }
                }
            }
        }

        self.switch_to_block(&join_bb);
        let ty = result_ty.unwrap_or(TypeRef {
            name: "Unit".to_string(),
            args: Vec::new(),
        });

        if ty.head() == "Unit" {
            Ok(ExprValue::unit())
        } else {
            let Some(local) = result_local else {
                return Ok(ExprValue::unit());
            };
            Ok(ExprValue {
                ty,
                operand: Some(MirOperand::Local(local)),
            })
        }
    }

    fn record_match_arm_result(
        &mut self,
        result_local: &mut Option<usize>,
        result_ty: &mut Option<TypeRef>,
        value: ExprValue,
    ) -> Result<(), Diagnostic> {
        if value.ty.head() == "Unit" {
            if result_ty.is_none() {
                *result_ty = Some(TypeRef {
                    name: "Unit".to_string(),
                    args: Vec::new(),
                });
            }
            return Ok(());
        }

        if result_local.is_none() {
            *result_ty = Some(value.ty.clone());
            *result_local = Some(self.new_temp_local(value.ty.clone()));
        }

        if let Some(dst) = *result_local {
            self.emit_store(dst, value);
        }
        Ok(())
    }

    fn lower_match_arm_body(
        &mut self,
        scrutinee: &MirOperand,
        enum_decl: &MirEnum,
        payload_ty: Option<TypeRef>,
        bind: Option<String>,
        body: &crate::ast::MatchArmBody,
    ) -> Result<ExprValue, Diagnostic> {
        if let Some(name) = bind {
            if name == "_" {
                // Discard binder.
                return match body {
                    crate::ast::MatchArmBody::Expr(expr) => self.lower_expr(expr),
                    crate::ast::MatchArmBody::Block(block) => self.lower_block_value(block),
                };
            }
            let Some(payload_ty2) = payload_ty else {
                return Err(Diagnostic::error(
                    format!(
                        "function '{}' uses payload binder for unit variant of enum '{}'",
                        self.function.name, enum_decl.name
                    ),
                    self.function.span,
                ));
            };

            let temp = self.new_temp_local(payload_ty2.clone());
            self.current_block_mut()
                .statements
                .push(MirStatement::Assign {
                    dst: temp,
                    rvalue: MirRvalue::EnumPayload {
                        base: scrutinee.clone(),
                        enum_name: enum_decl.name.clone(),
                        payload_ty: payload_ty2.clone(),
                    },
                });
            let local = self.declare_local(&name, payload_ty2.clone(), false)?;
            self.emit_store(
                local,
                ExprValue {
                    ty: payload_ty2,
                    operand: Some(MirOperand::Local(temp)),
                },
            );
        }

        match body {
            crate::ast::MatchArmBody::Expr(expr) => self.lower_expr(expr),
            crate::ast::MatchArmBody::Block(block) => self.lower_block_value(block),
        }
    }

    fn lower_if_expr(
        &mut self,
        cond: &Expr,
        then_block: &Block,
        else_block: Option<&Block>,
    ) -> Result<ExprValue, Diagnostic> {
        let cond_value = self.lower_expr(cond)?;
        if cond_value.ty.head() != "Bool" {
            return Err(Diagnostic::error(
                format!(
                    "function '{}' if condition must be Bool but got '{}'",
                    self.function.name, cond_value.ty
                ),
                self.function.span,
            ));
        }
        let cond_operand = cond_value.into_operand_or_unit(self.function)?;

        let then_bb = self.new_block();
        let else_bb = self.new_block();
        let join_bb = self.new_block();

        self.set_terminator(MirTerminator::If {
            cond: cond_operand,
            then_bb: then_bb.clone(),
            else_bb: else_bb.clone(),
        });

        self.switch_to_block(&then_bb);
        let then_value = self.with_scope(|builder| builder.lower_block_value(then_block))?;
        let then_end_bb = self.blocks[self.current_block].label.clone();
        let then_needs_join = !self.block_is_terminated(self.current_block);
        if then_needs_join {
            self.set_terminator(MirTerminator::Goto {
                target: join_bb.clone(),
            });
        }

        self.switch_to_block(&else_bb);
        let else_value = if let Some(block) = else_block {
            self.with_scope(|builder| builder.lower_block_value(block))?
        } else {
            ExprValue::unit()
        };
        let else_end_bb = self.blocks[self.current_block].label.clone();
        let else_needs_join = !self.block_is_terminated(self.current_block);
        if else_needs_join {
            self.set_terminator(MirTerminator::Goto {
                target: join_bb.clone(),
            });
        }

        let result_ty = if else_block.is_none() {
            if then_value.ty.head() != "Unit" {
                return Err(Diagnostic::error(
                    format!(
                        "function '{}' uses if expression without else returning '{}' but expected 'Unit' for native lowering",
                        self.function.name, then_value.ty
                    ),
                    self.function.span,
                ));
            }
            TypeRef {
                name: "Unit".to_string(),
                args: Vec::new(),
            }
        } else if then_value.ty != else_value.ty {
            return Err(Diagnostic::error(
                format!(
                    "function '{}' if expression branches return '{}' and '{}' (must match for native lowering)",
                    self.function.name, then_value.ty, else_value.ty
                ),
                self.function.span,
            ));
        } else {
            then_value.ty.clone()
        };

        if !is_native_type(&result_ty, self.records, self.enums) {
            return Err(Diagnostic::error(
                format!(
                    "function '{}' if expression returns '{}' which is not supported by native lowering yet",
                    self.function.name, result_ty
                ),
                self.function.span,
            ));
        }

        let result_temp = if result_ty.head() == "Unit" {
            None
        } else {
            Some(self.new_temp_local(result_ty.clone()))
        };

        if let Some(temp) = result_temp {
            if then_needs_join {
                self.switch_to_block(&then_end_bb);
                self.emit_store(temp, then_value.clone());
            }
            if else_needs_join {
                self.switch_to_block(&else_end_bb);
                self.emit_store(temp, else_value.clone());
            }
        }

        self.switch_to_block(&join_bb);
        Ok(ExprValue {
            ty: result_ty,
            operand: result_temp.map(MirOperand::Local),
        })
    }

    fn lower_while_expr(&mut self, cond: &Expr, body: &Block) -> Result<ExprValue, Diagnostic> {
        let cond_bb = self.new_block();
        let body_bb = self.new_block();
        let exit_bb = self.new_block();

        self.set_terminator(MirTerminator::Goto {
            target: cond_bb.clone(),
        });

        self.switch_to_block(&cond_bb);
        let cond_value = self.lower_expr(cond)?;
        if cond_value.ty.head() != "Bool" {
            return Err(Diagnostic::error(
                format!(
                    "function '{}' while condition must be Bool but got '{}'",
                    self.function.name, cond_value.ty
                ),
                self.function.span,
            ));
        }
        let cond_operand = cond_value.into_operand_or_unit(self.function)?;
        self.set_terminator(MirTerminator::If {
            cond: cond_operand,
            then_bb: body_bb.clone(),
            else_bb: exit_bb.clone(),
        });

        self.switch_to_block(&body_bb);
        self.with_scope(|builder| builder.lower_block_statements(body, BlockMode::LoopBody))?;
        if !self.block_is_terminated(self.current_block) {
            self.set_terminator(MirTerminator::Goto { target: cond_bb });
        }

        self.switch_to_block(&exit_bb);
        Ok(ExprValue::unit())
    }

    fn new_temp_local(&mut self, ty: TypeRef) -> usize {
        let local = self.locals.len();
        self.locals.push(MirLocal {
            name: format!("$t{local}"),
            ty,
        });
        local
    }

    fn new_block(&mut self) -> String {
        let label = format!("bb{}", self.next_block_id);
        self.next_block_id += 1;
        self.blocks.push(MirBlock {
            label: label.clone(),
            statements: Vec::new(),
            terminator: MirTerminator::ReturnDefault(TypeRef {
                name: "Unit".to_string(),
                args: Vec::new(),
            }),
        });
        label
    }

    fn block_index(&self, label: &str) -> Option<usize> {
        self.blocks.iter().position(|block| block.label == label)
    }

    fn switch_to_block(&mut self, label: &str) {
        if let Some(index) = self.block_index(label) {
            self.current_block = index;
            return;
        }
        panic!("lowering bug: missing block '{label}'");
    }

    fn current_block_mut(&mut self) -> &mut MirBlock {
        &mut self.blocks[self.current_block]
    }

    fn block_is_terminated(&self, block: usize) -> bool {
        !matches!(
            self.blocks[block].terminator,
            MirTerminator::ReturnDefault(_)
        )
    }

    fn set_terminator(&mut self, terminator: MirTerminator) {
        self.blocks[self.current_block].terminator = terminator;
    }

    fn emit_store(&mut self, local: usize, value: ExprValue) {
        if value.ty.head() == "Unit" {
            return;
        }

        let Some(operand) = value.operand else {
            return;
        };

        self.current_block_mut()
            .statements
            .push(MirStatement::Assign {
                dst: local,
                rvalue: MirRvalue::Use(operand),
            });
    }

    fn set_return_terminator(&mut self, value: ExprValue) {
        if self.function.return_type.head() == "Unit" {
            self.set_terminator(MirTerminator::Return { value: None });
            return;
        }

        let operand = value
            .operand
            .unwrap_or_else(|| match self.function.return_type.head() {
                "Bool" => MirOperand::ConstBool(false),
                _ => MirOperand::ConstInt(0),
            });
        self.set_terminator(MirTerminator::Return {
            value: Some(operand),
        });
    }
}

#[derive(Clone, Copy)]
enum BlockMode {
    FunctionBody,
    Value,
    LoopBody,
}

#[derive(Debug, Clone)]
struct ExprValue {
    ty: TypeRef,
    operand: Option<MirOperand>,
}

impl ExprValue {
    fn unit() -> Self {
        Self {
            ty: TypeRef {
                name: "Unit".to_string(),
                args: Vec::new(),
            },
            operand: None,
        }
    }

    fn unreachable() -> Self {
        Self::unit()
    }

    fn into_operand_or_unit(self, function: &HirFunction) -> Result<MirOperand, Diagnostic> {
        if self.ty.head() == "Unit" {
            return Err(Diagnostic::error(
                format!(
                    "function '{}' uses Unit value where a value is required in native lowering",
                    function.name
                ),
                function.span,
            ));
        }
        self.operand.ok_or_else(|| {
            Diagnostic::error(
                format!(
                    "function '{}' lowering lost operand for type '{}'",
                    function.name, self.ty
                ),
                function.span,
            )
        })
    }
}

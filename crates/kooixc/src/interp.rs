use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::ast::{BinaryOp, Block, Expr, MatchArmBody, MatchPattern, Program, Statement, TypeRef};
use crate::error::{Diagnostic, Span};
use crate::hir::{lower_program, HirFunction};
use crate::loader::load_source_map;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Unit,
    Int(i64),
    Bool(bool),
    Text(String),
    Record {
        name: String,
        fields: Vec<(String, Arc<Value>)>,
    },
    Enum {
        name: String,
        variant: String,
        payload: Option<Arc<Value>>,
    },
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Unit => f.write_str("()"),
            Value::Int(value) => write!(f, "{value}"),
            Value::Bool(value) => write!(f, "{value}"),
            Value::Text(value) => f.write_str(value),
            Value::Record { name, .. } => write!(f, "<{name}>"),
            Value::Enum { name, variant, .. } => write!(f, "<{name}::{variant}>"),
        }
    }
}

#[derive(Debug, Clone)]
struct EnumVariantInfo {
    enum_name: String,
    has_payload: bool,
}

#[derive(Debug, Clone)]
struct VariantRegistry {
    qualified: HashMap<String, EnumVariantInfo>,
    unqualified: HashMap<String, EnumVariantInfo>,
}

impl VariantRegistry {
    fn get_unqualified(&self, variant: &str) -> Option<&EnumVariantInfo> {
        self.unqualified.get(variant)
    }

    fn get_qualified(&self, enum_name: &str, variant: &str) -> Option<&EnumVariantInfo> {
        let key = format!("{enum_name}.{variant}");
        self.qualified.get(&key)
    }
}

#[derive(Debug, Clone)]
struct Env {
    scopes: Vec<HashMap<String, Value>>,
}

impl Env {
    fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    fn get(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Some(value.clone());
            }
        }
        None
    }

    fn insert(&mut self, name: String, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, value);
        }
    }

    fn assign(&mut self, name: &str, value: Value) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return true;
            }
        }
        false
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        let _ = self.scopes.pop();
        if self.scopes.is_empty() {
            self.scopes.push(HashMap::new());
        }
    }
}

pub fn run_program(program: &Program) -> Result<Value, Diagnostic> {
    let hir = lower_program(program);
    let mut functions: HashMap<String, HirFunction> = HashMap::new();
    for function in &hir.functions {
        functions.insert(function.name.clone(), function.clone());
    }

    let mut qualified_variants: HashMap<String, EnumVariantInfo> = HashMap::new();
    let mut unqualified_variants: HashMap<String, EnumVariantInfo> = HashMap::new();
    let mut duplicate_unqualified: HashSet<String> = HashSet::new();
    for enum_decl in &hir.enums {
        for variant in &enum_decl.variants {
            let info = EnumVariantInfo {
                enum_name: enum_decl.name.clone(),
                has_payload: variant.payload.is_some(),
            };

            qualified_variants.insert(format!("{}.{}", enum_decl.name, variant.name), info.clone());

            if duplicate_unqualified.contains(&variant.name) {
                continue;
            }
            if unqualified_variants
                .insert(variant.name.clone(), info)
                .is_some()
            {
                unqualified_variants.remove(&variant.name);
                duplicate_unqualified.insert(variant.name.clone());
            }
        }
    }
    let variants = VariantRegistry {
        qualified: qualified_variants,
        unqualified: unqualified_variants,
    };

    let Some(main) = functions.get("main") else {
        return Err(Diagnostic::error(
            "missing function 'main'",
            Span::new(0, 0),
        ));
    };

    if !main.params.is_empty() {
        return Err(Diagnostic::error(
            format!(
                "function 'main' expects {} parameters but interpreter only supports main()",
                main.params.len()
            ),
            main.span,
        ));
    }

    eval_function(main, &functions, &variants, &[], 0)
}

fn eval_function(
    function: &HirFunction,
    functions: &HashMap<String, HirFunction>,
    variants: &VariantRegistry,
    args: &[Value],
    depth: usize,
) -> Result<Value, Diagnostic> {
    const MAX_CALL_DEPTH: usize = 1024;
    if depth > MAX_CALL_DEPTH {
        return Err(Diagnostic::error(
            format!(
                "call stack overflow while executing function '{}'",
                function.name
            ),
            function.span,
        ));
    }

    if !function.effects.is_empty() {
        return Err(Diagnostic::error(
            format!(
                "function '{}' declares effects and cannot be executed by the interpreter",
                function.name
            ),
            function.span,
        ));
    }

    if function.params.len() != args.len() {
        return Err(Diagnostic::error(
            format!(
                "function '{}' called with {} arguments but expects {}",
                function.name,
                args.len(),
                function.params.len()
            ),
            function.span,
        ));
    }

    let Some(body) = &function.body else {
        if let Some(result) = eval_intrinsic_function(function, args) {
            return result;
        }
        return Err(Diagnostic::error(
            format!("function '{}' has no body to execute", function.name),
            function.span,
        ));
    };

    let mut env = Env::new();
    for (param, value) in function.params.iter().zip(args.iter()) {
        if !value_conforms_to_type_in_function(value, &param.ty, function) {
            return Err(Diagnostic::error(
                format!(
                    "function '{}' parameter '{}' expects type '{}' but got '{}'",
                    function.name,
                    param.name,
                    param.ty,
                    value_type_name(value)
                ),
                function.span,
            ));
        }
        env.insert(param.name.clone(), value.clone());
    }

    let mut returned: Option<Value> = None;

    for statement in &body.statements {
        match statement {
            Statement::Let(stmt) => {
                if env.get(&stmt.name).is_some() {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' redefines variable '{}' in interpreter",
                            function.name, stmt.name
                        ),
                        function.span,
                    ));
                }

                let value = eval_expr(&stmt.value, function, functions, variants, &mut env, depth)?;
                env.insert(stmt.name.clone(), value);
            }
            Statement::Assign(stmt) => {
                let value = eval_expr(&stmt.value, function, functions, variants, &mut env, depth)?;
                if !env.assign(&stmt.name, value) {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' assigns to unknown variable '{}' in interpreter",
                            function.name, stmt.name
                        ),
                        function.span,
                    ));
                }
            }
            Statement::Return(stmt) => {
                returned = Some(match &stmt.value {
                    Some(expr) => eval_expr(expr, function, functions, variants, &mut env, depth)?,
                    None => Value::Unit,
                });
                break;
            }
            Statement::Expr(expr) => {
                let _ = eval_expr(expr, function, functions, variants, &mut env, depth)?;
            }
        }
    }

    let value = if let Some(value) = returned {
        value
    } else if let Some(expr) = &body.tail {
        eval_expr(expr, function, functions, variants, &mut env, depth)?
    } else {
        Value::Unit
    };

    if function.return_type.head() == "Unit" {
        return Ok(Value::Unit);
    }

    if !value_conforms_to_type_in_function(&value, &function.return_type, function) {
        return Err(Diagnostic::error(
            format!(
                "function '{}' evaluated to '{}' but declared return type is '{}'",
                function.name,
                value_type_name(&value),
                function.return_type
            ),
            function.span,
        ));
    }

    Ok(value)
}

fn eval_intrinsic_function(
    function: &HirFunction,
    args: &[Value],
) -> Option<Result<Value, Diagnostic>> {
    let name = function.name.as_str();
    Some(match name {
        "text_len" => {
            let [Value::Text(s)] = args else {
                return Some(Err(Diagnostic::error(
                    "text_len expects (Text)",
                    function.span,
                )));
            };
            Ok(Value::Int(s.len() as i64))
        }
        "text_byte_at" => {
            let [Value::Text(s), Value::Int(index)] = args else {
                return Some(Err(Diagnostic::error(
                    "text_byte_at expects (Text, Int)",
                    function.span,
                )));
            };
            if *index < 0 {
                Ok(option_none())
            } else {
                let idx = *index as usize;
                match s.as_bytes().get(idx) {
                    Some(byte) => Ok(option_some(Value::Int(*byte as i64))),
                    None => Ok(option_none()),
                }
            }
        }
        "text_slice" => {
            let [Value::Text(s), Value::Int(start), Value::Int(end)] = args else {
                return Some(Err(Diagnostic::error(
                    "text_slice expects (Text, Int, Int)",
                    function.span,
                )));
            };
            if *start < 0 || *end < 0 {
                return Some(Ok(option_none()));
            }
            let start = *start as usize;
            let end = *end as usize;
            if start > end || end > s.len() {
                return Some(Ok(option_none()));
            }
            if !s.is_char_boundary(start) || !s.is_char_boundary(end) {
                return Some(Ok(option_none()));
            }
            Ok(option_some(Value::Text(s[start..end].to_string())))
        }
        "text_starts_with" => {
            let [Value::Text(s), Value::Text(prefix)] = args else {
                return Some(Err(Diagnostic::error(
                    "text_starts_with expects (Text, Text)",
                    function.span,
                )));
            };
            Ok(Value::Bool(s.starts_with(prefix)))
        }
        "text_concat" => {
            let [Value::Text(a), Value::Text(b)] = args else {
                return Some(Err(Diagnostic::error(
                    "text_concat expects (Text, Text)",
                    function.span,
                )));
            };
            Ok(Value::Text(format!("{a}{b}")))
        }
        "int_to_text" => {
            let [Value::Int(i)] = args else {
                return Some(Err(Diagnostic::error(
                    "int_to_text expects (Int)",
                    function.span,
                )));
            };
            Ok(Value::Text(i.to_string()))
        }
        "byte_is_ascii_whitespace" => {
            let [Value::Int(b)] = args else {
                return Some(Err(Diagnostic::error(
                    "byte_is_ascii_whitespace expects (Int)",
                    function.span,
                )));
            };
            Ok(Value::Bool(is_ascii_whitespace(*b)))
        }
        "byte_is_ascii_digit" => {
            let [Value::Int(b)] = args else {
                return Some(Err(Diagnostic::error(
                    "byte_is_ascii_digit expects (Int)",
                    function.span,
                )));
            };
            Ok(Value::Bool(is_ascii_digit(*b)))
        }
        "byte_is_ascii_alpha" => {
            let [Value::Int(b)] = args else {
                return Some(Err(Diagnostic::error(
                    "byte_is_ascii_alpha expects (Int)",
                    function.span,
                )));
            };
            Ok(Value::Bool(is_ascii_alpha(*b)))
        }
        "byte_is_ascii_alnum" => {
            let [Value::Int(b)] = args else {
                return Some(Err(Diagnostic::error(
                    "byte_is_ascii_alnum expects (Int)",
                    function.span,
                )));
            };
            Ok(Value::Bool(is_ascii_alnum(*b)))
        }
        "byte_is_ascii_ident_start" => {
            let [Value::Int(b)] = args else {
                return Some(Err(Diagnostic::error(
                    "byte_is_ascii_ident_start expects (Int)",
                    function.span,
                )));
            };
            Ok(Value::Bool(is_ascii_ident_start(*b)))
        }
        "byte_is_ascii_ident_continue" => {
            let [Value::Int(b)] = args else {
                return Some(Err(Diagnostic::error(
                    "byte_is_ascii_ident_continue expects (Int)",
                    function.span,
                )));
            };
            Ok(Value::Bool(is_ascii_ident_continue(*b)))
        }
        "host_load_source_map" => {
            let [Value::Text(path)] = args else {
                return Some(Err(Diagnostic::error(
                    "host_load_source_map expects (Text)",
                    function.span,
                )));
            };

            let mut entry = PathBuf::from(path);
            if entry.extension().is_none() {
                entry.set_extension("kooix");
            }

            if fs::metadata(&entry).is_err() {
                // Tests (and some tooling) execute with cwd = `crates/kooixc`, while most CLI usage
                // runs from repo root. Make this intrinsic resilient by searching parent dirs.
                let mut prefix = PathBuf::new();
                for _ in 0..8 {
                    prefix.push("..");
                    let candidate = prefix.join(&entry);
                    if fs::metadata(&candidate).is_ok() {
                        entry = candidate;
                        break;
                    }
                }
            }

            match load_source_map(&entry) {
                Ok(map) => Ok(result_ok(Value::Text(map.combined))),
                Err(errors) => {
                    let message = errors
                        .first()
                        .map(|error| error.message.clone())
                        .unwrap_or_else(|| "failed to load source map".to_string());
                    Ok(result_err(Value::Text(message)))
                }
            }
        }
        "host_eprintln" => {
            let [Value::Text(s)] = args else {
                return Some(Err(Diagnostic::error(
                    "host_eprintln expects (Text)",
                    function.span,
                )));
            };
            eprintln!("{s}");
            Ok(Value::Unit)
        }
        "host_write_file" => {
            let [Value::Text(path), Value::Text(content)] = args else {
                return Some(Err(Diagnostic::error(
                    "host_write_file expects (Text, Text)",
                    function.span,
                )));
            };

            match std::fs::write(path, content) {
                Ok(()) => Ok(result_ok(Value::Int(0))),
                Err(error) => Ok(result_err(Value::Text(format!(
                    "failed to write file '{path}': {error}"
                )))),
            }
        }
        "host_argc" => {
            if !args.is_empty() {
                return Some(Err(Diagnostic::error(
                    "host_argc expects ()",
                    function.span,
                )));
            }
            // Interpreter runs don't currently accept argv; keep this deterministic but align with
            // C argv semantics (argc includes argv[0]).
            Ok(Value::Int(1))
        }
        "host_argv" => {
            let [_index] = args else {
                return Some(Err(Diagnostic::error(
                    "host_argv expects (Int)",
                    function.span,
                )));
            };
            // Interpreter runs don't currently accept argv; keep this deterministic.
            Ok(Value::Text("".to_string()))
        }
        _ => return None,
    })
}

fn result_ok(value: Value) -> Value {
    Value::Enum {
        name: "Result".to_string(),
        variant: "Ok".to_string(),
        payload: Some(Arc::new(value)),
    }
}

fn result_err(value: Value) -> Value {
    Value::Enum {
        name: "Result".to_string(),
        variant: "Err".to_string(),
        payload: Some(Arc::new(value)),
    }
}

fn option_some(value: Value) -> Value {
    Value::Enum {
        name: "Option".to_string(),
        variant: "Some".to_string(),
        payload: Some(Arc::new(value)),
    }
}

fn option_none() -> Value {
    Value::Enum {
        name: "Option".to_string(),
        variant: "None".to_string(),
        payload: None,
    }
}

fn normalize_byte(b: i64) -> Option<u8> {
    if (0..=255).contains(&b) {
        Some(b as u8)
    } else {
        None
    }
}

fn is_ascii_whitespace(b: i64) -> bool {
    matches!(normalize_byte(b), Some(b' ' | b'\n' | b'\r' | b'\t'))
}

fn is_ascii_digit(b: i64) -> bool {
    matches!(normalize_byte(b), Some(b'0'..=b'9'))
}

fn is_ascii_alpha(b: i64) -> bool {
    matches!(normalize_byte(b), Some(b'a'..=b'z' | b'A'..=b'Z'))
}

fn is_ascii_alnum(b: i64) -> bool {
    is_ascii_alpha(b) || is_ascii_digit(b)
}

fn is_ascii_ident_start(b: i64) -> bool {
    is_ascii_alpha(b) || matches!(normalize_byte(b), Some(b'_'))
}

fn is_ascii_ident_continue(b: i64) -> bool {
    is_ascii_alnum(b) || matches!(normalize_byte(b), Some(b'_'))
}

fn value_conforms_to_type_in_function(value: &Value, ty: &TypeRef, function: &HirFunction) -> bool {
    if ty.args.is_empty()
        && function
            .generics
            .iter()
            .any(|generic| generic.name == ty.head())
    {
        return true;
    }

    value_conforms_to_type(value, ty)
}

fn eval_expr(
    expr: &Expr,
    function: &HirFunction,
    functions: &HashMap<String, HirFunction>,
    variants: &VariantRegistry,
    env: &mut Env,
    depth: usize,
) -> Result<Value, Diagnostic> {
    match expr {
        Expr::Number(raw) => {
            let value = raw.parse::<i64>().map_err(|_| {
                Diagnostic::error(format!("invalid integer literal '{raw}'"), function.span)
            })?;
            Ok(Value::Int(value))
        }
        Expr::String(value) => Ok(Value::Text(value.clone())),
        Expr::Bool(value) => Ok(Value::Bool(*value)),
        Expr::RecordLit { ty, fields } => {
            let mut values: Vec<(String, Arc<Value>)> = Vec::new();
            for field in fields {
                let value = eval_expr(&field.value, function, functions, variants, env, depth)?;
                if let Some((_, slot)) = values.iter_mut().find(|(name, _)| name == &field.name) {
                    *slot = Arc::new(value);
                } else {
                    values.push((field.name.clone(), Arc::new(value)));
                }
            }
            Ok(Value::Record {
                name: ty.name.clone(),
                fields: values,
            })
        }
        Expr::Path(segments) => {
            let Some(name) = segments.first() else {
                return Err(Diagnostic::error("expected identifier path", function.span));
            };

            if let Some(value) = env.get(name) {
                let mut value = value;
                for member in segments.iter().skip(1) {
                    match value {
                        Value::Record { fields, .. } => {
                            value = fields
                                .iter()
                                .rev()
                                .find(|(name, _)| name == member)
                                .map(|(_, value)| value.as_ref().clone())
                                .ok_or_else(|| {
                                    Diagnostic::error(
                                        format!("unknown member '{member}' on record value"),
                                        function.span,
                                    )
                                })?;
                        }
                        other => {
                            return Err(Diagnostic::error(
                                format!(
                                    "cannot access member '{}' on value of type '{}'",
                                    member,
                                    value_type_name(&other)
                                ),
                                function.span,
                            ));
                        }
                    }
                }

                return Ok(value);
            }

            match segments.as_slice() {
                [variant] => match variants.get_unqualified(variant) {
                    Some(info) if !info.has_payload => Ok(Value::Enum {
                        name: info.enum_name.clone(),
                        variant: variant.clone(),
                        payload: None,
                    }),
                    Some(_) => Err(Diagnostic::error(
                        format!(
                            "enum variant '{variant}' requires a payload (use '{variant}(...)')"
                        ),
                        function.span,
                    )),
                    None => Err(Diagnostic::error(
                        format!("unknown variable '{}'", segments.join(".")),
                        function.span,
                    )),
                },
                [enum_name, variant] => match variants.get_qualified(enum_name, variant) {
                    Some(info) if !info.has_payload => Ok(Value::Enum {
                        name: info.enum_name.clone(),
                        variant: variant.clone(),
                        payload: None,
                    }),
                    Some(_) => Err(Diagnostic::error(
                        format!(
                            "enum variant '{}.{}' requires a payload (use '{}.{}(...)')",
                            enum_name, variant, enum_name, variant
                        ),
                        function.span,
                    )),
                    None => Err(Diagnostic::error(
                        format!("unknown variable '{}'", segments.join(".")),
                        function.span,
                    )),
                },
                _ => Err(Diagnostic::error(
                    format!("unknown variable '{}'", segments.join(".")),
                    function.span,
                )),
            }
        }
        Expr::Call { target, args, .. } => {
            let target_display = target.join(".");

            if target.len() == 1 {
                let name = target[0].as_str();
                if let Some(callee) = functions.get(name) {
                    let mut values = Vec::new();
                    for arg in args {
                        values.push(eval_expr(arg, function, functions, variants, env, depth)?);
                    }

                    return eval_function(callee, functions, variants, &values, depth + 1);
                }
            }

            let (info, variant_name, variant_display) = match target.as_slice() {
                [variant] => {
                    let Some(info) = variants.get_unqualified(variant) else {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' calls unknown target '{}'",
                                function.name, target_display
                            ),
                            function.span,
                        ));
                    };
                    (info, variant, variant.clone())
                }
                [enum_name, variant] => {
                    let Some(info) = variants.get_qualified(enum_name, variant) else {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' calls unknown target '{}'",
                                function.name, target_display
                            ),
                            function.span,
                        ));
                    };
                    (info, variant, format!("{}.{}", enum_name, variant))
                }
                _ => {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' calls unknown target '{}'",
                            function.name, target_display
                        ),
                        function.span,
                    ));
                }
            };

            let payload = if info.has_payload {
                if args.len() != 1 {
                    return Err(Diagnostic::error(
                        format!(
                            "enum variant '{}' expects 1 payload argument but got {}",
                            variant_display,
                            args.len()
                        ),
                        function.span,
                    ));
                }

                Some(Arc::new(eval_expr(
                    &args[0], function, functions, variants, env, depth,
                )?))
            } else {
                if !args.is_empty() {
                    return Err(Diagnostic::error(
                        format!(
                            "enum variant '{}' expects 0 arguments but got {}",
                            variant_display,
                            args.len()
                        ),
                        function.span,
                    ));
                }
                None
            };

            Ok(Value::Enum {
                name: info.enum_name.clone(),
                variant: variant_name.clone(),
                payload,
            })
        }
        Expr::If {
            cond,
            then_block,
            else_block,
        } => {
            let cond_value = eval_expr(cond, function, functions, variants, env, depth)?;
            let Value::Bool(flag) = cond_value else {
                return Err(Diagnostic::error(
                    format!(
                        "if condition evaluated to '{}' but expected 'Bool'",
                        value_type_name(&cond_value)
                    ),
                    function.span,
                ));
            };

            if flag {
                eval_block_expr(
                    then_block.as_ref(),
                    function,
                    functions,
                    variants,
                    env,
                    depth,
                )
            } else if let Some(block) = else_block {
                eval_block_expr(block.as_ref(), function, functions, variants, env, depth)
            } else {
                Ok(Value::Unit)
            }
        }
        Expr::While { cond, body } => {
            const MAX_LOOP_ITERS: usize = 1_000_000;
            let mut iterations = 0usize;

            loop {
                let cond_value =
                    eval_expr(cond.as_ref(), function, functions, variants, env, depth)?;
                let Value::Bool(flag) = cond_value else {
                    return Err(Diagnostic::error(
                        format!(
                            "while condition evaluated to '{}' but expected 'Bool'",
                            value_type_name(&cond_value)
                        ),
                        function.span,
                    ));
                };

                if !flag {
                    break;
                }

                iterations += 1;
                if iterations > MAX_LOOP_ITERS {
                    return Err(Diagnostic::error(
                        format!(
                            "while loop exceeded {MAX_LOOP_ITERS} iterations in function '{}' (possible non-termination)",
                            function.name
                        ),
                        function.span,
                    ));
                }

                let _ = eval_block_expr(body.as_ref(), function, functions, variants, env, depth)?;
            }

            Ok(Value::Unit)
        }
        Expr::Match { value, arms } => {
            let scrutinee = eval_expr(value.as_ref(), function, functions, variants, env, depth)?;

            for arm in arms {
                let is_match = match &arm.pattern {
                    MatchPattern::Wildcard => true,
                    MatchPattern::Variant { path, .. } => match &scrutinee {
                        Value::Enum { name, variant, .. } => match path.as_slice() {
                            [pat_variant] => variant == pat_variant,
                            [pat_enum, pat_variant] => name == pat_enum && variant == pat_variant,
                            _ => {
                                return Err(Diagnostic::error(
                                    format!("match arm uses invalid pattern '{}'", path.join(".")),
                                    function.span,
                                ));
                            }
                        },
                        other => {
                            return Err(Diagnostic::error(
                                format!(
                                    "match scrutinee evaluated to '{}' but expected an enum value",
                                    value_type_name(other)
                                ),
                                function.span,
                            ));
                        }
                    },
                };

                if !is_match {
                    continue;
                }

                env.push_scope();
                if let MatchPattern::Variant { path, bind } = &arm.pattern {
                    if let Some(bind_name) = bind {
                        let pattern_display = path.join(".");
                        match &scrutinee {
                            Value::Enum {
                                name,
                                variant,
                                payload: Some(payload),
                                ..
                            } => {
                                let matches = match path.as_slice() {
                                    [pat_variant] => variant == pat_variant,
                                    [pat_enum, pat_variant] => {
                                        name == pat_enum && variant == pat_variant
                                    }
                                    _ => {
                                        env.pop_scope();
                                        return Err(Diagnostic::error(
                                            format!(
                                                "match arm uses invalid pattern '{}'",
                                                pattern_display
                                            ),
                                            function.span,
                                        ));
                                    }
                                };

                                if !matches {
                                    env.pop_scope();
                                    return Err(Diagnostic::error(
                                        format!(
                                            "match scrutinee evaluated to '<{}::{}>' but expected enum variant '{}'",
                                            name, variant, pattern_display
                                        ),
                                        function.span,
                                    ));
                                }
                                env.insert(bind_name.clone(), payload.as_ref().clone());
                            }
                            Value::Enum {
                                name,
                                variant,
                                payload: None,
                                ..
                            } => {
                                let matches = match path.as_slice() {
                                    [pat_variant] => variant == pat_variant,
                                    [pat_enum, pat_variant] => {
                                        name == pat_enum && variant == pat_variant
                                    }
                                    _ => {
                                        env.pop_scope();
                                        return Err(Diagnostic::error(
                                            format!(
                                                "match arm uses invalid pattern '{}'",
                                                pattern_display
                                            ),
                                            function.span,
                                        ));
                                    }
                                };

                                if !matches {
                                    env.pop_scope();
                                    return Err(Diagnostic::error(
                                        format!(
                                            "match scrutinee evaluated to '<{}::{}>' but expected enum variant '{}'",
                                            name, variant, pattern_display
                                        ),
                                        function.span,
                                    ));
                                }
                                env.pop_scope();
                                return Err(Diagnostic::error(
                                    format!(
                                        "match arm '{}' binds '{}' but variant has no payload",
                                        pattern_display, bind_name
                                    ),
                                    function.span,
                                ));
                            }
                            other => {
                                env.pop_scope();
                                return Err(Diagnostic::error(
                                    format!(
                                        "match scrutinee evaluated to '{}' but expected enum variant '{}'",
                                        value_type_name(other),
                                        pattern_display
                                    ),
                                    function.span,
                                ));
                            }
                        }
                    }
                }

                let result = match &arm.body {
                    MatchArmBody::Expr(expr) => {
                        eval_expr(expr, function, functions, variants, env, depth)
                    }
                    MatchArmBody::Block(block) => {
                        eval_block_expr(block, function, functions, variants, env, depth)
                    }
                };

                env.pop_scope();
                return result;
            }

            Err(Diagnostic::error(
                "non-exhaustive match expression",
                function.span,
            ))
        }
        Expr::Binary { op, left, right } => {
            let left_value = eval_expr(left, function, functions, variants, env, depth)?;
            let right_value = eval_expr(right, function, functions, variants, env, depth)?;

            match op {
                BinaryOp::Add => match (left_value, right_value) {
                    (Value::Int(left), Value::Int(right)) => {
                        left.checked_add(right).map(Value::Int).ok_or_else(|| {
                            Diagnostic::error(
                                format!(
                                    "integer overflow while executing '{}' in function '{}'",
                                    "+", function.name
                                ),
                                function.span,
                            )
                        })
                    }
                    (left, right) => Err(Diagnostic::error(
                        format!(
                            "cannot apply '+' to '{}' and '{}'",
                            value_type_name(&left),
                            value_type_name(&right)
                        ),
                        function.span,
                    )),
                },
                BinaryOp::Eq => Ok(Value::Bool(left_value == right_value)),
                BinaryOp::NotEq => Ok(Value::Bool(left_value != right_value)),
            }
        }
    }
}

fn eval_block_expr(
    block: &Block,
    function: &HirFunction,
    functions: &HashMap<String, HirFunction>,
    variants: &VariantRegistry,
    env: &mut Env,
    depth: usize,
) -> Result<Value, Diagnostic> {
    env.push_scope();

    let result = (|| {
        for statement in &block.statements {
            match statement {
                Statement::Let(stmt) => {
                    if env.get(&stmt.name).is_some() {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' redefines variable '{}' in interpreter block",
                                function.name, stmt.name
                            ),
                            function.span,
                        ));
                    }

                    let value = eval_expr(&stmt.value, function, functions, variants, env, depth)?;
                    env.insert(stmt.name.clone(), value);
                }
                Statement::Assign(stmt) => {
                    let value = eval_expr(&stmt.value, function, functions, variants, env, depth)?;
                    if !env.assign(&stmt.name, value) {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' assigns to unknown variable '{}' in interpreter block",
                                function.name, stmt.name
                            ),
                            function.span,
                        ));
                    }
                }
                Statement::Return(_) => {
                    return Err(Diagnostic::error(
                        "return is not supported inside a block expression",
                        function.span,
                    ));
                }
                Statement::Expr(expr) => {
                    let _ = eval_expr(expr, function, functions, variants, env, depth)?;
                }
            }
        }

        if let Some(expr) = &block.tail {
            eval_expr(expr, function, functions, variants, env, depth)
        } else {
            Ok(Value::Unit)
        }
    })();

    env.pop_scope();
    result
}

fn value_type_name(value: &Value) -> String {
    match value {
        Value::Unit => "Unit".to_string(),
        Value::Int(_) => "Int".to_string(),
        Value::Bool(_) => "Bool".to_string(),
        Value::Text(_) => "Text".to_string(),
        Value::Record { name, .. } => name.clone(),
        Value::Enum { name, .. } => name.clone(),
    }
}

fn value_conforms_to_type(value: &Value, ty: &TypeRef) -> bool {
    match ty.head() {
        "Unit" => matches!(value, Value::Unit),
        "Int" => matches!(value, Value::Int(_)),
        "Bool" => matches!(value, Value::Bool(_)),
        "Text" | "String" => matches!(value, Value::Text(_)),
        named => {
            matches!(value, Value::Record { name, .. } if name == named)
                || matches!(value, Value::Enum { name, .. } if name == named)
        }
    }
}

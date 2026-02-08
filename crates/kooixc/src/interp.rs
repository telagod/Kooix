use std::collections::HashMap;

use crate::ast::{BinaryOp, Expr, Program, Statement, TypeRef};
use crate::error::{Diagnostic, Span};
use crate::hir::{lower_program, HirFunction};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Unit,
    Int(i64),
    Bool(bool),
    Text(String),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Unit => f.write_str("()"),
            Value::Int(value) => write!(f, "{value}"),
            Value::Bool(value) => write!(f, "{value}"),
            Value::Text(value) => f.write_str(value),
        }
    }
}

pub fn run_program(program: &Program) -> Result<Value, Diagnostic> {
    let hir = lower_program(program);
    let mut functions: HashMap<String, HirFunction> = HashMap::new();
    for function in &hir.functions {
        functions.insert(function.name.clone(), function.clone());
    }

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

    eval_function(main, &functions, &[], 0)
}

fn eval_function(
    function: &HirFunction,
    functions: &HashMap<String, HirFunction>,
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
        return Err(Diagnostic::error(
            format!("function '{}' has no body to execute", function.name),
            function.span,
        ));
    };

    let mut env: HashMap<String, Value> = HashMap::new();
    for (param, value) in function.params.iter().zip(args.iter()) {
        if !value_conforms_to_type(value, &param.ty) {
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
                let value = eval_expr(&stmt.value, function, functions, &env, depth)?;
                env.insert(stmt.name.clone(), value);
            }
            Statement::Return(stmt) => {
                returned = Some(match &stmt.value {
                    Some(expr) => eval_expr(expr, function, functions, &env, depth)?,
                    None => Value::Unit,
                });
                break;
            }
            Statement::Expr(expr) => {
                let _ = eval_expr(expr, function, functions, &env, depth)?;
            }
        }
    }

    let value = if let Some(value) = returned {
        value
    } else if let Some(expr) = &body.tail {
        eval_expr(expr, function, functions, &env, depth)?
    } else {
        Value::Unit
    };

    if function.return_type.head() == "Unit" {
        return Ok(Value::Unit);
    }

    if !value_conforms_to_type(&value, &function.return_type) {
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

fn eval_expr(
    expr: &Expr,
    function: &HirFunction,
    functions: &HashMap<String, HirFunction>,
    env: &HashMap<String, Value>,
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
        Expr::Path(segments) => {
            if segments.len() != 1 {
                return Err(Diagnostic::error(
                    format!(
                        "unsupported member path '{}' in interpreter (only identifiers are supported)",
                        segments.join(".")
                    ),
                    function.span,
                ));
            }

            let name = &segments[0];
            env.get(name).cloned().ok_or_else(|| {
                Diagnostic::error(format!("unknown variable '{name}'"), function.span)
            })
        }
        Expr::Call { target, args } => {
            let Some(callee) = functions.get(target) else {
                return Err(Diagnostic::error(
                    format!(
                        "function '{}' calls unknown target '{}'",
                        function.name, target
                    ),
                    function.span,
                ));
            };

            let mut values = Vec::new();
            for arg in args {
                values.push(eval_expr(arg, function, functions, env, depth)?);
            }

            eval_function(callee, functions, &values, depth + 1)
        }
        Expr::Binary { op, left, right } => {
            let left_value = eval_expr(left, function, functions, env, depth)?;
            let right_value = eval_expr(right, function, functions, env, depth)?;

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

fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::Unit => "Unit",
        Value::Int(_) => "Int",
        Value::Bool(_) => "Bool",
        Value::Text(_) => "Text",
    }
}

fn value_conforms_to_type(value: &Value, ty: &TypeRef) -> bool {
    match ty.head() {
        "Unit" => matches!(value, Value::Unit),
        "Int" => matches!(value, Value::Int(_)),
        "Bool" => matches!(value, Value::Bool(_)),
        "Text" | "String" => matches!(value, Value::Text(_)),
        _ => false,
    }
}

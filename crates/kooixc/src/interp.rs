use std::collections::HashMap;

use crate::ast::{BinaryOp, Block, Expr, Program, Statement, TypeRef};
use crate::error::{Diagnostic, Span};
use crate::hir::{lower_program, HirFunction};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Unit,
    Int(i64),
    Bool(bool),
    Text(String),
    Record {
        name: String,
        fields: HashMap<String, Value>,
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
        }
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

    let mut env = Env::new();
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
                if env.get(&stmt.name).is_some() {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' redefines variable '{}' in interpreter",
                            function.name, stmt.name
                        ),
                        function.span,
                    ));
                }

                let value = eval_expr(&stmt.value, function, functions, &mut env, depth)?;
                env.insert(stmt.name.clone(), value);
            }
            Statement::Assign(stmt) => {
                let value = eval_expr(&stmt.value, function, functions, &mut env, depth)?;
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
                    Some(expr) => eval_expr(expr, function, functions, &mut env, depth)?,
                    None => Value::Unit,
                });
                break;
            }
            Statement::Expr(expr) => {
                let _ = eval_expr(expr, function, functions, &mut env, depth)?;
            }
        }
    }

    let value = if let Some(value) = returned {
        value
    } else if let Some(expr) = &body.tail {
        eval_expr(expr, function, functions, &mut env, depth)?
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
            let mut values: HashMap<String, Value> = HashMap::new();
            for field in fields {
                let value = eval_expr(&field.value, function, functions, env, depth)?;
                values.insert(field.name.clone(), value);
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

            let mut value = env
                .get(name)
                .ok_or_else(|| Diagnostic::error(format!("unknown variable '{name}'"), function.span))?;

            for member in segments.iter().skip(1) {
                match value {
                    Value::Record { fields, .. } => {
                        value = fields.get(member).cloned().ok_or_else(|| {
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

            Ok(value)
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
        Expr::If {
            cond,
            then_block,
            else_block,
        } => {
            let cond_value = eval_expr(cond, function, functions, env, depth)?;
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
                eval_block_expr(then_block.as_ref(), function, functions, env, depth)
            } else if let Some(block) = else_block {
                eval_block_expr(block.as_ref(), function, functions, env, depth)
            } else {
                Ok(Value::Unit)
            }
        }
        Expr::While { cond, body } => {
            const MAX_LOOP_ITERS: usize = 1_000_000;
            let mut iterations = 0usize;

            loop {
                let cond_value = eval_expr(cond.as_ref(), function, functions, env, depth)?;
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

                let _ = eval_block_expr(body.as_ref(), function, functions, env, depth)?;
            }

            Ok(Value::Unit)
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

fn eval_block_expr(
    block: &Block,
    function: &HirFunction,
    functions: &HashMap<String, HirFunction>,
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

                    let value = eval_expr(&stmt.value, function, functions, env, depth)?;
                    env.insert(stmt.name.clone(), value);
                }
                Statement::Assign(stmt) => {
                    let value = eval_expr(&stmt.value, function, functions, env, depth)?;
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
                    let _ = eval_expr(expr, function, functions, env, depth)?;
                }
            }
        }

        if let Some(expr) = &block.tail {
            eval_expr(expr, function, functions, env, depth)
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
    }
}

fn value_conforms_to_type(value: &Value, ty: &TypeRef) -> bool {
    match ty.head() {
        "Unit" => matches!(value, Value::Unit),
        "Int" => matches!(value, Value::Int(_)),
        "Bool" => matches!(value, Value::Bool(_)),
        "Text" | "String" => matches!(value, Value::Text(_)),
        record_name => matches!(value, Value::Record { name, .. } if name == record_name),
    }
}

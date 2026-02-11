use std::collections::HashSet;

use crate::ast::{
    AgentDecl, EnsureClause, Expr, FailureAction, FailureActionArg, FailurePolicy, FailureRule,
    FunctionDecl, ImportDecl, Item, MatchArm, MatchArmBody, MatchPattern, PredicateValue, Program,
    RecordLitField, Statement, TypeArg, TypeRef, WorkflowCall, WorkflowCallArg, WorkflowDecl,
    WorkflowStep,
};

pub fn normalize_program(program: &Program) -> Program {
    let import_namespaces = collect_import_namespaces(program);
    if import_namespaces.is_empty() {
        return program.clone();
    }

    let mut items = Vec::new();
    for item in &program.items {
        items.push(normalize_item(item, &import_namespaces));
    }
    Program { items }
}

fn collect_import_namespaces(program: &Program) -> HashSet<String> {
    let mut out = HashSet::new();
    for item in &program.items {
        if let Item::Import(ImportDecl { ns: Some(ns), .. }) = item {
            out.insert(ns.clone());
        }
    }
    out
}

fn normalize_item(item: &Item, import_namespaces: &HashSet<String>) -> Item {
    match item {
        Item::Capability(cap) => Item::Capability(cap.clone()),
        Item::Import(import) => Item::Import(import.clone()),
        Item::Record(record) => Item::Record(record.clone()),
        Item::Enum(en) => Item::Enum(en.clone()),
        Item::Function(function) => Item::Function(normalize_function(function, import_namespaces)),
        Item::Workflow(workflow) => Item::Workflow(normalize_workflow(workflow, import_namespaces)),
        Item::Agent(agent) => Item::Agent(normalize_agent(agent, import_namespaces)),
    }
}

fn normalize_function(
    function: &FunctionDecl,
    import_namespaces: &HashSet<String>,
) -> FunctionDecl {
    let mut out = function.clone();
    normalize_type_ref(&mut out.return_type, import_namespaces);
    for param in &mut out.params {
        normalize_type_ref(&mut param.ty, import_namespaces);
    }
    for required in &mut out.requires {
        normalize_type_ref(required, import_namespaces);
    }
    for ensure in &mut out.ensures {
        normalize_ensure_clause(ensure, import_namespaces);
    }
    if let Some(failure) = &mut out.failure {
        normalize_failure_policy(failure, import_namespaces);
    }
    if let Some(body) = &mut out.body {
        for statement in &mut body.statements {
            normalize_statement(statement, import_namespaces);
        }
        if let Some(tail) = &mut body.tail {
            normalize_expr(tail, import_namespaces);
        }
    }
    out
}

fn normalize_workflow(
    workflow: &WorkflowDecl,
    import_namespaces: &HashSet<String>,
) -> WorkflowDecl {
    let mut out = workflow.clone();
    normalize_type_ref(&mut out.return_type, import_namespaces);
    for param in &mut out.params {
        normalize_type_ref(&mut param.ty, import_namespaces);
    }
    for required in &mut out.requires {
        normalize_type_ref(required, import_namespaces);
    }
    for step in &mut out.steps {
        normalize_workflow_step(step, import_namespaces);
    }
    for field in &mut out.output {
        normalize_type_ref(&mut field.ty, import_namespaces);
        if let Some(source) = &mut field.source {
            normalize_segments(source, import_namespaces);
        }
    }
    out
}

fn normalize_workflow_step(step: &mut WorkflowStep, import_namespaces: &HashSet<String>) {
    normalize_workflow_call(&mut step.call, import_namespaces);
    for ensure in &mut step.ensures {
        normalize_ensure_clause(ensure, import_namespaces);
    }
    if let Some(action) = &mut step.on_fail {
        normalize_failure_action(action, import_namespaces);
    }
}

fn normalize_workflow_call(call: &mut WorkflowCall, import_namespaces: &HashSet<String>) {
    for arg in &mut call.args {
        normalize_workflow_call_arg(arg, import_namespaces);
    }
}

fn normalize_workflow_call_arg(arg: &mut WorkflowCallArg, import_namespaces: &HashSet<String>) {
    match arg {
        WorkflowCallArg::Path(segments) => normalize_segments(segments, import_namespaces),
        WorkflowCallArg::String(_) | WorkflowCallArg::Number(_) => {}
    }
}

fn normalize_agent(agent: &AgentDecl, import_namespaces: &HashSet<String>) -> AgentDecl {
    let mut out = agent.clone();
    normalize_type_ref(&mut out.return_type, import_namespaces);
    for param in &mut out.params {
        normalize_type_ref(&mut param.ty, import_namespaces);
    }
    for ensure in &mut out.ensures {
        normalize_ensure_clause(ensure, import_namespaces);
    }
    for required in &mut out.requires {
        normalize_type_ref(required, import_namespaces);
    }
    normalize_ensure_clause(&mut out.loop_spec.stop_when, import_namespaces);
    if let Some(when) = &mut out.policy.human_in_loop_when {
        normalize_ensure_clause(when, import_namespaces);
    }
    out
}

fn normalize_failure_policy(policy: &mut FailurePolicy, import_namespaces: &HashSet<String>) {
    for rule in &mut policy.rules {
        normalize_failure_rule(rule, import_namespaces);
    }
}

fn normalize_failure_rule(rule: &mut FailureRule, import_namespaces: &HashSet<String>) {
    normalize_failure_action(&mut rule.action, import_namespaces);
}

fn normalize_failure_action(action: &mut FailureAction, import_namespaces: &HashSet<String>) {
    for arg in &mut action.args {
        normalize_failure_action_arg(arg, import_namespaces);
    }
}

fn normalize_failure_action_arg(arg: &mut FailureActionArg, import_namespaces: &HashSet<String>) {
    let _ = import_namespaces;
    let _ = arg;
}

fn normalize_statement(statement: &mut Statement, import_namespaces: &HashSet<String>) {
    match statement {
        Statement::Let(let_stmt) => {
            if let Some(ty) = &mut let_stmt.ty {
                normalize_type_ref(ty, import_namespaces);
            }
            normalize_expr(&mut let_stmt.value, import_namespaces);
        }
        Statement::Assign(assign_stmt) => {
            normalize_expr(&mut assign_stmt.value, import_namespaces);
        }
        Statement::Return(ret) => {
            if let Some(expr) = &mut ret.value {
                normalize_expr(expr, import_namespaces);
            }
        }
        Statement::Expr(expr) => normalize_expr(expr, import_namespaces),
    }
}

fn normalize_expr(expr: &mut Expr, import_namespaces: &HashSet<String>) {
    match expr {
        Expr::Path(segments) => normalize_segments(segments, import_namespaces),
        Expr::String(_) | Expr::Number(_) | Expr::Bool(_) => {}
        Expr::RecordLit { ty, fields } => {
            normalize_type_ref(ty, import_namespaces);
            for field in fields {
                normalize_record_lit_field(field, import_namespaces);
            }
        }
        Expr::Call {
            target,
            type_args,
            args,
        } => {
            normalize_segments(target, import_namespaces);
            for arg in type_args {
                normalize_type_arg(arg, import_namespaces);
            }
            for arg in args {
                normalize_expr(arg, import_namespaces);
            }
        }
        Expr::If {
            cond,
            then_block,
            else_block,
        } => {
            normalize_expr(cond, import_namespaces);
            for statement in &mut then_block.statements {
                normalize_statement(statement, import_namespaces);
            }
            if let Some(tail) = &mut then_block.tail {
                normalize_expr(tail, import_namespaces);
            }
            if let Some(block) = else_block {
                for statement in &mut block.statements {
                    normalize_statement(statement, import_namespaces);
                }
                if let Some(tail) = &mut block.tail {
                    normalize_expr(tail, import_namespaces);
                }
            }
        }
        Expr::While { cond, body } => {
            normalize_expr(cond, import_namespaces);
            for statement in &mut body.statements {
                normalize_statement(statement, import_namespaces);
            }
            if let Some(tail) = &mut body.tail {
                normalize_expr(tail, import_namespaces);
            }
        }
        Expr::Match { value, arms } => {
            normalize_expr(value, import_namespaces);
            for arm in arms {
                normalize_match_arm(arm, import_namespaces);
            }
        }
        Expr::Binary { left, right, .. } => {
            normalize_expr(left, import_namespaces);
            normalize_expr(right, import_namespaces);
        }
    }
}

fn normalize_record_lit_field(field: &mut RecordLitField, import_namespaces: &HashSet<String>) {
    normalize_expr(&mut field.value, import_namespaces);
}

fn normalize_match_arm(arm: &mut MatchArm, import_namespaces: &HashSet<String>) {
    match &mut arm.pattern {
        MatchPattern::Wildcard => {}
        MatchPattern::Variant { path, .. } => normalize_segments(path, import_namespaces),
    }
    match &mut arm.body {
        MatchArmBody::Expr(expr) => normalize_expr(expr, import_namespaces),
        MatchArmBody::Block(block) => {
            for statement in &mut block.statements {
                normalize_statement(statement, import_namespaces);
            }
            if let Some(tail) = &mut block.tail {
                normalize_expr(tail, import_namespaces);
            }
        }
    }
}

fn normalize_ensure_clause(clause: &mut EnsureClause, import_namespaces: &HashSet<String>) {
    normalize_predicate_value(&mut clause.left, import_namespaces);
    normalize_predicate_value(&mut clause.right, import_namespaces);
}

fn normalize_predicate_value(value: &mut PredicateValue, import_namespaces: &HashSet<String>) {
    match value {
        PredicateValue::Path(segments) => normalize_segments(segments, import_namespaces),
        PredicateValue::String(_) | PredicateValue::Number(_) => {}
    }
}

fn normalize_type_arg(arg: &mut TypeArg, import_namespaces: &HashSet<String>) {
    match arg {
        TypeArg::Type(ty) => normalize_type_ref(ty, import_namespaces),
        TypeArg::String(_) | TypeArg::Number(_) => {}
    }
}

fn normalize_type_ref(ty: &mut TypeRef, import_namespaces: &HashSet<String>) {
    if let Some(stripped) = strip_namespace_prefix(&ty.name, import_namespaces) {
        ty.name = stripped;
    }
    for arg in &mut ty.args {
        normalize_type_arg(arg, import_namespaces);
    }
}

fn strip_namespace_prefix(name: &str, import_namespaces: &HashSet<String>) -> Option<String> {
    let mut parts = name.splitn(2, "::");
    let head = parts.next()?;
    let rest = parts.next()?;
    if import_namespaces.contains(head) {
        Some(rest.to_string())
    } else {
        None
    }
}

fn normalize_segments(segments: &mut Vec<String>, import_namespaces: &HashSet<String>) {
    if segments.len() >= 2 && import_namespaces.contains(&segments[0]) {
        segments.remove(0);
    }
}

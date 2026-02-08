use kooixc::ast::{Expr, FailureValue, Item, PredicateOp, PredicateValue, Statement};
use kooixc::error::Severity;
use kooixc::interp::Value;
use kooixc::native::{
    compile_llvm_ir_to_executable, compile_llvm_ir_to_executable_with_tools,
    run_executable_with_args_and_stdin, run_executable_with_args_and_stdin_and_timeout,
    NativeError,
};
use kooixc::{
    check_source, compile_and_run_native_source, compile_and_run_native_source_with_args,
    compile_and_run_native_source_with_args_and_stdin,
    compile_and_run_native_source_with_args_stdin_and_timeout, emit_llvm_ir_source, lower_source,
    lower_to_mir_source, parse_source, run_source,
};

#[test]
fn parses_program_with_effects_and_capabilities() {
    let source = r#"
cap Net<"api.openai.com">;
cap Model<"openai", "gpt-4o-mini", 1000>;

fn summarize(doc: Text) -> Summary !{model(openai), net} requires [Model<"openai", "gpt-4o-mini", 1000>, Net<"api.openai.com">];
"#;

    let program = parse_source(source).expect("program should parse");
    assert_eq!(program.items.len(), 3);
}

#[test]
fn parses_function_with_intent_and_ensures() {
    let source = r#"
fn score(doc: Text) -> Score intent "produce confidence score" ensures [output.confidence >= 0, output.confidence <= 1];
"#;

    let program = parse_source(source).expect("program should parse");
    assert_eq!(program.items.len(), 1);

    let function = match &program.items[0] {
        Item::Function(function) => function,
        _ => panic!("expected first item to be function"),
    };

    assert_eq!(function.intent.as_deref(), Some("produce confidence score"));
    assert_eq!(function.ensures.len(), 2);
    assert_eq!(function.ensures[0].op, PredicateOp::Gte);

    let PredicateValue::Path(path) = &function.ensures[0].left else {
        panic!("expected left side to be path");
    };
    assert_eq!(path, &vec!["output".to_string(), "confidence".to_string()]);
}

#[test]
fn parses_function_with_failure_policy() {
    let source = r#"
fn summarize(doc: Text) -> Summary failure { timeout -> retry(exp_backoff, max=3); invalid_output -> fallback("template"); };
"#;

    let program = parse_source(source).expect("program should parse");
    let function = match &program.items[0] {
        Item::Function(function) => function,
        _ => panic!("expected first item to be function"),
    };

    let failure = function
        .failure
        .as_ref()
        .expect("failure policy should exist");
    assert_eq!(failure.rules.len(), 2);
    assert_eq!(failure.rules[0].condition, "timeout");
    assert_eq!(failure.rules[0].action.name, "retry");
    assert_eq!(failure.rules[0].action.args.len(), 2);
    assert_eq!(failure.rules[0].action.args[1].key.as_deref(), Some("max"));
    assert!(matches!(
        failure.rules[0].action.args[1].value,
        FailureValue::Number(_)
    ));
}

#[test]
fn parses_function_with_evidence_block() {
    let source = r#"
fn summarize(doc: Text) -> Summary evidence { trace "summarize.v1"; metrics [latency_ms, token_in, token_out]; };
"#;

    let program = parse_source(source).expect("program should parse");
    let function = match &program.items[0] {
        Item::Function(function) => function,
        _ => panic!("expected first item to be function"),
    };

    let evidence = function.evidence.as_ref().expect("evidence should exist");
    assert_eq!(evidence.trace.as_deref(), Some("summarize.v1"));
    assert_eq!(evidence.metrics.len(), 3);
}

#[test]
fn parses_function_with_body_block() {
    let source = r#"
fn add(a: Int, b: Int) -> Int {
  let c: Int = a + b;
  c
};
"#;

    let program = parse_source(source).expect("program should parse");
    let function = match &program.items[0] {
        Item::Function(function) => function,
        _ => panic!("expected first item to be function"),
    };

    let body = function.body.as_ref().expect("body should exist");
    assert_eq!(body.statements.len(), 1);
    assert!(matches!(body.statements[0], Statement::Let(_)));
    assert!(matches!(body.tail, Some(Expr::Path(_))));
}

#[test]
fn fails_when_function_body_return_type_mismatches() {
    let source = r#"
fn main() -> Int { true };
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("body evaluates to 'Bool' but expected 'Int'")
    }));
}

#[test]
fn fails_when_function_body_let_type_mismatches() {
    let source = r#"
fn main() -> Int {
  let x: Bool = 1;
  0
};
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("declares type 'Bool' but value is 'Int'")
    }));
}

#[test]
fn rejects_llvm_emission_for_function_body_until_lowering_is_implemented() {
    let source = r#"
fn main() -> Int { 0 };
"#;

    let result = emit_llvm_ir_source(source);
    assert!(
        matches!(result, Err(diagnostics) if diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic.message.contains("MIR/LLVM lowering is not implemented yet")
        }))
    );
}

#[test]
fn runs_interpreter_for_simple_main() {
    let source = r#"
fn add(a: Int, b: Int) -> Int { a + b };
fn main() -> Int { add(20, 22) };
"#;

    let result = run_source(source).expect("run should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.value, Value::Int(42));
}

#[test]
fn runs_interpreter_with_if_expression() {
    let source = r#"
fn main() -> Int { if true { 1 } else { 2 } };
"#;

    let result = run_source(source).expect("run should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.value, Value::Int(1));
}

#[test]
fn runs_interpreter_with_while_and_assignment() {
    let source = r#"
fn main() -> Int {
  let i: Int = 0;
  while i != 10 {
    i = i + 1;
  };
  i
};
"#;

    let result = run_source(source).expect("run should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.value, Value::Int(10));
}

#[test]
fn runs_interpreter_with_record_literal_and_member_access() {
    let source = r#"
record Pair { a: Int; b: Int; };

fn main() -> Int {
  let p: Pair = Pair { a: 1; b: 2; };
  p.a + p.b
};
"#;

    let result = run_source(source).expect("run should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.value, Value::Int(3));
}

#[test]
fn runs_interpreter_with_enum_and_match_expression() {
    let source = r#"
enum Option<T> { Some(T); None; };

fn main() -> Int {
  let x: Option<Int> = Some(42);
  match x {
    Some(v) => v;
    None => 0;
  }
};
"#;

    let result = run_source(source).expect("run should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.value, Value::Int(42));
}

#[test]
fn fails_when_if_expression_branch_types_differ() {
    let source = r#"
fn main() -> Int { if true { 1 } else { false } };
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("if expression branches return 'Int' and 'Bool'")
    }));
}

#[test]
fn fails_when_match_expression_is_non_exhaustive() {
    let source = r#"
enum Flag { On; Off; };

fn main() -> Int {
  let f: Flag = On;
  match f { On => 1; }
};
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("match expression on 'Flag' is non-exhaustive")
    }));
}

#[test]
fn fails_when_while_condition_is_not_bool() {
    let source = r#"
fn main() -> Int {
  while 1 { 0 };
  0
};
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("uses while condition of type 'Int' but expected 'Bool'")
    }));
}

#[test]
fn fails_when_assignment_type_mismatches() {
    let source = r#"
fn main() -> Int {
  let x: Bool = true;
  x = 1;
  0
};
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("assigns 'x' as 'Int' but variable is 'Bool'")
    }));
}

#[test]
fn fails_when_assignment_target_is_unknown() {
    let source = r#"
fn main() -> Int {
  x = 1;
  0
};
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("assigns to unknown variable 'x' in body")
    }));
}

#[test]
fn fails_when_record_literal_is_missing_field() {
    let source = r#"
record Pair { a: Int; b: Int; };

fn main() -> Int {
  let p: Pair = Pair { a: 1; };
  0
};
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("record literal for 'Pair' is missing field 'b'")
    }));
}

#[test]
fn fails_when_record_literal_uses_unknown_field() {
    let source = r#"
record Pair { a: Int; b: Int; };

fn main() -> Int {
  let p: Pair = Pair { a: 1; c: 2; b: 3; };
  0
};
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("record literal uses unknown field 'c' on type 'Pair'")
    }));
}

#[test]
fn fails_when_member_access_is_unknown() {
    let source = r#"
record Pair { a: Int; b: Int; };

fn main() -> Int {
  let p: Pair = Pair { a: 1; b: 2; };
  p.c
};
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("cannot infer member 'c' on type 'Pair'")
    }));
}

#[test]
fn interpreter_rejects_effectful_functions() {
    let source = r#"
cap Net<"example.com">;
fn main() -> Int !{net} requires [Net<"example.com">] { 0 };
"#;

    let diagnostics = run_source(source).expect_err("run should fail");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("declares effects and cannot be executed by the interpreter")
    }));
}

#[test]
fn fails_when_effect_has_no_requires_clause() {
    let source = r#"
cap Net<"api.openai.com">;
fn ping() -> Unit !{net};
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("declares effects but no required capabilities")
    }));
}

#[test]
fn fails_when_effect_misses_required_capability() {
    let source = r#"
cap Net<"api.openai.com">;
cap Model<"openai", "gpt-4o-mini", 1000>;
fn summarize(doc: Text) -> Summary !{model(openai), net} requires [Net<"api.openai.com">];
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("uses effect 'model' but does not require 'Model' capability")
    }));
}

#[test]
fn fails_when_required_capability_is_not_declared() {
    let source = r#"
fn call_external() -> Unit !{net} requires [Net<"example.com">];
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("requires capability 'Net' but it is not declared")
    }));
}

#[test]
fn warns_on_unknown_effect() {
    let source = r#"
cap Model<"openai", "gpt-4o-mini", 1000>;
fn score(doc: Text) -> Int !{custom_effect} requires [Model<"openai", "gpt-4o-mini", 1000>];
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("unknown effect 'custom_effect'")
    }));
}

#[test]
fn fails_on_duplicate_functions() {
    let source = r#"
fn foo() -> Unit;
fn foo() -> Unit;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("duplicate function declaration 'foo'")
    }));
}

#[test]
fn lowers_program_to_hir() {
    let source = r#"
cap Net<"api.openai.com">;
fn ping() -> Unit !{net} requires [Net<"api.openai.com">];
"#;

    let hir = lower_source(source).expect("source should lower");
    assert_eq!(hir.capabilities.len(), 1);
    assert_eq!(hir.functions.len(), 1);
    assert_eq!(hir.functions[0].effects[0].name, "net");
    assert!(hir.functions[0].intent.is_none());
    assert!(hir.functions[0].ensures.is_empty());
}

#[test]
fn lowers_intent_and_ensures_to_hir() {
    let source = r#"
fn score(x: Int) -> Int intent "rank output" ensures [output.value == x];
"#;

    let hir = lower_source(source).expect("source should lower");
    assert_eq!(hir.functions.len(), 1);
    assert_eq!(hir.functions[0].intent.as_deref(), Some("rank output"));
    assert_eq!(hir.functions[0].ensures.len(), 1);
    assert_eq!(hir.functions[0].ensures[0].op, PredicateOp::Eq);
}

#[test]
fn warns_on_empty_intent_clause() {
    let source = r#"
fn score(x: Int) -> Int intent "";
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning && diagnostic.message.contains("empty intent")
    }));
}

#[test]
fn warns_on_unknown_symbol_in_ensures() {
    let source = r#"
fn score(x: Int) -> Int ensures [ghost.value > 0];
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("ensures references unknown symbol 'ghost'")
    }));
}

#[test]
fn rejects_unknown_failure_action() {
    let source = r#"
fn summarize(doc: Text) -> Summary failure { timeout -> teleport("nowhere"); };
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("uses unknown failure action 'teleport'")
    }));
}

#[test]
fn rejects_retry_without_strategy_argument() {
    let source = r#"
fn summarize(doc: Text) -> Summary failure { timeout -> retry(); };
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("retry' without strategy argument")
    }));
}

#[test]
fn rejects_retry_max_with_non_number() {
    let source = r#"
fn summarize(doc: Text) -> Summary failure { timeout -> retry(exp_backoff, max="three"); };
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("retry argument 'max' with non-number value")
    }));
}

#[test]
fn rejects_evidence_without_trace() {
    let source = r#"
fn summarize(doc: Text) -> Summary evidence { metrics [latency_ms]; };
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message.contains("evidence block requires trace")
    }));
}

#[test]
fn warns_on_empty_evidence_metrics() {
    let source = r#"
fn summarize(doc: Text) -> Summary evidence { trace "summarize.v1"; metrics []; };
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("evidence block has empty metrics")
    }));
}

#[test]
fn warns_on_duplicate_evidence_metrics() {
    let source = r#"
fn summarize(doc: Text) -> Summary evidence { trace "summarize.v1"; metrics [latency_ms, latency_ms]; };
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("evidence block repeats metric 'latency_ms'")
    }));
}

#[test]
fn parses_record_declaration() {
    let source = r#"
record Answer {
  text: Text;
  confidence: Float;
}
;
"#;

    let program = parse_source(source).expect("record should parse");
    assert_eq!(program.items.len(), 1);
    assert!(matches!(program.items[0], Item::Record(_)));
}

#[test]
fn parses_generic_record_declaration() {
    let source = r#"
record Box<T> {
  value: T;
}
;
"#;

    let program = parse_source(source).expect("generic record should parse");
    assert_eq!(program.items.len(), 1);

    let record = match &program.items[0] {
        Item::Record(record) => record,
        _ => panic!("expected first item to be record"),
    };

    assert_eq!(record.name, "Box");
    assert_eq!(record.generics.len(), 1);
    assert_eq!(record.generics[0].name, "T");
    assert!(record.generics[0].bounds.is_empty());
    assert_eq!(record.fields.len(), 1);
    assert_eq!(record.fields[0].name, "value");
    assert_eq!(record.fields[0].ty.to_string(), "T");
}

#[test]
fn parses_bounded_generic_record_declaration() {
    let source = r#"
record Box<T: Answer> {
  value: T;
}
;
"#;

    let program = parse_source(source).expect("bounded generic record should parse");
    let record = match &program.items[0] {
        Item::Record(record) => record,
        _ => panic!("expected first item to be record"),
    };

    assert_eq!(record.generics.len(), 1);
    assert_eq!(record.generics[0].name, "T");
    assert_eq!(record.generics[0].bounds.len(), 1);
    assert_eq!(record.generics[0].bounds[0].to_string(), "Answer");
}

#[test]
fn parses_multi_bound_generic_record_declaration() {
    let source = r#"
record Box<T: Answer + Summary> {
  value: T;
}
;
"#;

    let program = parse_source(source).expect("multi-bound generic record should parse");
    let record = match &program.items[0] {
        Item::Record(record) => record,
        _ => panic!("expected first item to be record"),
    };

    assert_eq!(record.generics.len(), 1);
    assert_eq!(record.generics[0].name, "T");
    assert_eq!(record.generics[0].bounds.len(), 2);
    assert_eq!(record.generics[0].bounds[0].to_string(), "Answer");
    assert_eq!(record.generics[0].bounds[1].to_string(), "Summary");
}

#[test]
fn parses_where_clause_record_generic_bounds() {
    let source = r#"
record Box<T> where T: Answer + Summary {
  value: T;
}
;
"#;

    let program = parse_source(source).expect("where-clause record should parse");
    let record = match &program.items[0] {
        Item::Record(record) => record,
        _ => panic!("expected first item to be record"),
    };

    assert_eq!(record.generics.len(), 1);
    assert_eq!(record.generics[0].name, "T");
    assert_eq!(record.generics[0].bounds.len(), 2);
    assert_eq!(record.generics[0].bounds[0].to_string(), "Answer");
    assert_eq!(record.generics[0].bounds[1].to_string(), "Summary");
}

#[test]
fn rejects_duplicate_record_declarations() {
    let source = r#"
record Answer { text: Text; };
record Answer { text: Text; };
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("duplicate record declaration 'Answer'")
    }));
}

#[test]
fn rejects_duplicate_record_fields() {
    let source = r#"
record Answer {
  text: Text;
  text: Text;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("record 'Answer' repeats field 'text'")
    }));
}

#[test]
fn rejects_duplicate_record_generic_parameters() {
    let source = r#"
record Box<T, T> {
  value: T;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("record 'Box' repeats generic parameter 'T'")
    }));
}

#[test]
fn rejects_record_generic_parameter_with_type_arguments() {
    let source = r#"
record Box<T> {
  value: T<Int>;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("record 'Box' uses generic parameter 'T' with type arguments")
    }));
}

#[test]
fn rejects_record_field_type_with_record_generic_arity_mismatch() {
    let source = r#"
record Box<T> {
  value: T;
}
;
record Envelope {
  payload: Box;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("record 'Envelope' field 'payload' uses record type 'Box' with 0 generic argument(s), expected 1")
    }));
}

#[test]
fn warns_on_record_without_fields() {
    let source = r#"
record Empty {}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("record 'Empty' declares no fields")
    }));
}

#[test]
fn parses_workflow_with_steps_output_and_evidence() {
    let source = r#"
cap Tool<"vector_search", "read-only">;
workflow ingest_and_answer(input: Query) -> Answer
intent "retrieve context and generate grounded answer"
requires [Tool<"vector_search", "read-only">]
steps {
  s1: normalize(input) ensures [output.score >= 0];
  s2: retrieve(input) on_fail -> retry(exp_backoff, max=2);
}
output {
  answer: Answer;
}
evidence {
  trace "workflow.ingest_and_answer.v1";
  metrics [step_latency_ms, token_total];
}
;
"#;

    let program = parse_source(source).expect("workflow program should parse");
    assert_eq!(program.items.len(), 2);
}

#[test]
fn lowers_workflow_to_hir() {
    let source = r#"
cap Tool<"vector_search", "read-only">;
workflow flow(input: Query) -> Answer
requires [Tool<"vector_search", "read-only">]
steps {
  s1: normalize(input);
}
;
"#;

    let hir = lower_source(source).expect("source should lower");
    assert_eq!(hir.workflows.len(), 1);
    assert_eq!(hir.workflows[0].steps.len(), 1);
    assert_eq!(hir.workflows[0].steps[0].id, "s1");
}

#[test]
fn rejects_duplicate_workflow_declarations() {
    let source = r#"
workflow flow() -> Unit steps { s1: noop(); };
workflow flow() -> Unit steps { s1: noop(); };
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("duplicate workflow declaration 'flow'")
    }));
}

#[test]
fn rejects_duplicate_workflow_step_ids() {
    let source = r#"
workflow flow() -> Unit
steps {
  s1: noop();
  s1: noop();
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message.contains("repeats step id 's1'")
    }));
}

#[test]
fn rejects_unknown_workflow_on_fail_action() {
    let source = r#"
workflow flow() -> Unit
steps {
  s1: noop() on_fail -> teleport("x");
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("uses unknown on_fail action 'teleport'")
    }));
}

#[test]
fn warns_on_unknown_workflow_step_call_target() {
    let source = r#"
workflow flow() -> Unit
steps {
  s1: ghost_call();
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("workflow 'flow' step 's1' calls unknown target 'ghost_call'")
    }));
}

#[test]
fn accepts_declared_workflow_step_call_targets() {
    let source = r#"
fn normalize(input: Query) -> Unit;
workflow helper() -> Unit steps { s1: normalize(input); };
agent resolver(input: Ticket) -> Resolution
state { INIT -> DONE; }
policy { allow_tools ["kb"]; deny_tools ["payment"]; max_iterations = 2; }
loop { perceive -> act; stop when state == DONE; }
;
workflow flow(input: Query) -> Unit
steps {
  s1: normalize(input);
  s2: helper();
  s3: resolver(input);
}
;
"#;

    let diagnostics = check_source(source);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic.message.contains("calls unknown target")
    }));
}

#[test]
fn rejects_workflow_step_call_with_argument_count_mismatch() {
    let source = r#"
fn normalize(input: Query) -> Unit;
workflow flow(input: Query) -> Unit
steps {
  s1: normalize();
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("calls 'normalize' with 0 argument(s), expected 1")
    }));
}

#[test]
fn rejects_workflow_step_call_with_argument_type_mismatch() {
    let source = r#"
fn fetch(id: Int) -> Unit;
workflow flow() -> Unit
steps {
  s1: fetch("abc");
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("passes argument 1 to 'fetch' as 'Text' but expected 'Int'")
    }));
}

#[test]
fn rejects_function_return_type_with_record_generic_arity_mismatch() {
    let source = r#"
record Box<T> {
  value: T;
}
;
fn answer(input: Query) -> Box;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("function 'answer' return type uses record type 'Box' with 0 generic argument(s), expected 1")
    }));
}

#[test]
fn rejects_function_return_type_with_record_generic_bound_mismatch() {
    let source = r#"
record Box<T: Answer> {
  value: T;
}
;
fn answer(input: Query) -> Box<Text>;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message.contains(
                "function 'answer' return type uses record type 'Box' with generic argument 'T' as 'Text' but it must satisfy bound 'Answer'"
            )
    }));
}

#[test]
fn rejects_function_return_type_with_record_generic_multi_bound_mismatch() {
    let source = r#"
record Box<T: Answer + Summary> {
  value: T;
}
;
fn answer(input: Query) -> Box<Answer>;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message.contains(
                "function 'answer' return type uses record type 'Box' with generic argument 'T' as 'Answer' but it must satisfy bound 'Summary'"
            )
    }));
}

#[test]
fn rejects_function_return_type_with_record_generic_where_bound_mismatch() {
    let source = r#"
record Box<T> where T: Answer {
  value: T;
}
;
fn answer(input: Query) -> Box<Text>;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message.contains(
                "function 'answer' return type uses record type 'Box' with generic argument 'T' as 'Text' but it must satisfy bound 'Answer'"
            )
    }));
}

#[test]
fn accepts_function_return_type_with_record_generic_bound_match() {
    let source = r#"
record Box<T: Answer> {
  value: T;
}
;
fn answer(input: Query) -> Box<Answer>;
"#;

    let diagnostics = check_source(source);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message.contains("function 'answer' return type")
    }));
}

#[test]
fn accepts_record_bound_structural_satisfaction() {
    let source = r#"
record HasText { text: Text; };
record Answer { text: String; score: Int; };
record Box<T: HasText> { value: T; };
fn answer(input: Query) -> Box<Answer>;
"#;

    let diagnostics = check_source(source);
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error));
}

#[test]
fn rejects_record_bound_structural_mismatch() {
    let source = r#"
record HasText { text: Text; };
record Wrong { score: Int; };
record Box<T: HasText> { value: T; };
fn answer(input: Query) -> Box<Wrong>;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message.contains(
                "function 'answer' return type uses record type 'Box' with generic argument 'T' as 'Wrong' but it must satisfy bound 'HasText'"
            )
    }));
}

#[test]
fn rejects_record_bound_structural_multi_mismatch_aggregated() {
    let source = r#"
record HasText { text: Text; };
record HasScore { score: Int; };
record Wrong { other: Int; };
record Box<T: HasText + HasScore> { value: T; };
fn answer(input: Query) -> Box<Wrong>;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message.contains(
                "function 'answer' return type uses record type 'Box' with generic argument 'T' as 'Wrong' but it must satisfy bounds 'HasText + HasScore'"
            )
    }));
}

#[test]
fn warns_when_workflow_step_argument_symbol_is_unbound() {
    let source = r#"
fn normalize(input: Query) -> Unit;
workflow flow() -> Unit
steps {
  s1: normalize(input);
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("'input' is not available in workflow scope")
    }));
}

#[test]
fn accepts_workflow_step_call_with_matching_signature() {
    let source = r#"
fn normalize(input: Query) -> Unit;
workflow helper(input: Query) -> Unit
steps {
  s1: normalize(input);
}
;
workflow flow(input: Query) -> Unit
steps {
  s1: helper(input);
}
;
"#;

    let diagnostics = check_source(source);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message.contains("workflow 'flow' step 's1'")
    }));
}

#[test]
fn accepts_workflow_step_call_with_previous_step_binding() {
    let source = r#"
fn retrieve(input: Query) -> Docs;
fn rank(docs: Docs) -> Unit;
workflow flow(input: Query) -> Unit
steps {
  s1: retrieve(input);
  s2: rank(s1);
}
;
"#;

    let diagnostics = check_source(source);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message.contains("workflow 'flow' step 's2'")
    }));
}

#[test]
fn rejects_workflow_step_call_with_previous_step_type_mismatch() {
    let source = r#"
fn retrieve(input: Query) -> Docs;
fn rank(score: Int) -> Unit;
workflow flow(input: Query) -> Unit
steps {
  s1: retrieve(input);
  s2: rank(s1);
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("passes argument 1 to 'rank' as 'Docs' but expected 'Int'")
    }));
}

#[test]
fn warns_when_workflow_step_uses_future_step_symbol() {
    let source = r#"
fn retrieve(input: Query) -> Docs;
fn rank(docs: Docs) -> Unit;
workflow flow(input: Query) -> Unit
steps {
  s1: rank(s2);
  s2: retrieve(input);
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("'s2' is not available in workflow scope")
    }));
}

#[test]
fn accepts_workflow_step_call_with_option_member_projection() {
    let source = r#"
fn retrieve(input: Query) -> Option<Answer>;
fn deliver(answer: Answer) -> Unit;
workflow flow(input: Query) -> Unit
steps {
  s1: retrieve(input);
  s2: deliver(s1.some);
}
;
"#;

    let diagnostics = check_source(source);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message.contains("workflow 'flow' step 's2'")
    }));
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic.message.contains("cannot infer member 'some'")
    }));
}

#[test]
fn accepts_workflow_step_call_with_record_member_projection() {
    let source = r#"
record Answer {
  text: Text;
}
;
fn retrieve(input: Query) -> Answer;
fn deliver(text: Text) -> Unit;
workflow flow(input: Query) -> Unit
steps {
  s1: retrieve(input);
  s2: deliver(s1.text);
}
;
"#;

    let diagnostics = check_source(source);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("cannot infer member 'text' on type 'Answer'")
    }));
}

#[test]
fn accepts_workflow_step_call_with_generic_record_member_projection() {
    let source = r#"
record Box<T> {
  value: T;
}
;
fn retrieve(input: Query) -> Box<Answer>;
fn deliver(answer: Answer) -> Unit;
workflow flow(input: Query) -> Unit
steps {
  s1: retrieve(input);
  s2: deliver(s1.value);
}
;
"#;

    let diagnostics = check_source(source);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message.contains("workflow 'flow' step 's2'")
    }));
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("cannot infer member 'value' on type 'Box<Answer>'")
    }));
}

#[test]
fn warns_on_workflow_step_call_with_unsupported_member_projection() {
    let source = r#"
fn retrieve(input: Query) -> Answer;
fn deliver(answer: Answer) -> Unit;
workflow flow(input: Query) -> Unit
steps {
  s1: retrieve(input);
  s2: deliver(s1.value);
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("cannot infer member 'value' on type 'Answer'")
    }));
}

#[test]
fn warns_on_workflow_step_call_with_record_generic_arity_mismatch() {
    let source = r#"
record Box<T> {
  value: T;
}
;
fn retrieve(input: Query) -> Box;
fn deliver(answer: Answer) -> Unit;
workflow flow(input: Query) -> Unit
steps {
  s1: retrieve(input);
  s2: deliver(s1.value);
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("cannot infer member 'value' on type 'Box'")
    }));
}

#[test]
fn rejects_duplicate_workflow_output_fields() {
    let source = r#"
fn answer(input: Query) -> Answer;
workflow flow(input: Query) -> Answer
steps {
  s1: answer(input);
}
output {
  result: Answer;
  result: Answer;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("output block repeats field 'result'")
    }));
}

#[test]
fn warns_when_workflow_output_field_has_no_matching_source_symbol() {
    let source = r#"
fn noop() -> Unit;
workflow flow(input: Query) -> Answer
steps {
  s1: noop();
}
output {
  answer: Answer;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("output field 'answer' has type 'Answer' but no matching source symbol")
    }));
}

#[test]
fn warns_when_workflow_output_field_implicit_binding_is_ambiguous() {
    let source = r#"
fn answer_a(input: Query) -> Answer;
fn answer_b(input: Query) -> Answer;
workflow flow(input: Query) -> Answer
steps {
  s1: answer_a(input);
  s2: answer_b(input);
}
output {
  result: Answer;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("implicitly matches multiple source symbols: s1, s2")
    }));
}

#[test]
fn prefers_name_based_implicit_workflow_output_binding() {
    let source = r#"
fn answer_a(input: Query) -> Answer;
fn answer_b(input: Query) -> Answer;
workflow flow(input: Query) -> Answer
steps {
  answer: answer_a(input);
  s2: answer_b(input);
}
output {
  answer: Answer;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("implicitly matches multiple source symbols")
    }));
}

#[test]
fn warns_when_name_based_implicit_output_binding_type_mismatches() {
    let source = r#"
fn summarize(input: Query) -> Summary;
fn answer_b(input: Query) -> Answer;
workflow flow(input: Query) -> Answer
steps {
  answer: summarize(input);
  s2: answer_b(input);
}
output {
  answer: Answer;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic.message.contains(
                "matches symbol 'answer' by name but type is 'Summary', expected 'Answer'",
            )
    }));
}

#[test]
fn explicit_workflow_output_binding_resolves_implicit_ambiguity() {
    let source = r#"
fn answer_a(input: Query) -> Answer;
fn answer_b(input: Query) -> Answer;
workflow flow(input: Query) -> Answer
steps {
  s1: answer_a(input);
  s2: answer_b(input);
}
output {
  result: Answer = s2;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("implicitly matches multiple source symbols")
    }));
}

#[test]
fn warns_when_workflow_output_contract_does_not_expose_return_type() {
    let source = r#"
fn summarize(input: Query) -> Summary;
workflow flow(input: Query) -> Answer
steps {
  s1: summarize(input);
}
output {
  summary: Summary;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("output contract does not expose return type 'Answer'")
    }));
}

#[test]
fn accepts_workflow_output_contract_with_step_bound_return_type() {
    let source = r#"
fn answer(input: Query) -> Answer;
workflow flow(input: Query) -> Answer
steps {
  s1: answer(input);
}
output {
  result: Answer;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("output contract does not expose return type")
    }));
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic.message.contains("output field 'result'")
    }));
}

#[test]
fn accepts_workflow_output_contract_with_explicit_source_binding() {
    let source = r#"
fn answer(input: Query) -> Answer;
workflow flow(input: Query) -> Answer
steps {
  s1: answer(input);
}
output {
  result: Answer = s1;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message.contains("output field 'result'")
    }));
}

#[test]
fn rejects_workflow_output_binding_to_unknown_symbol() {
    let source = r#"
fn answer(input: Query) -> Answer;
workflow flow(input: Query) -> Answer
steps {
  s1: answer(input);
}
output {
  result: Answer = ghost;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("binds to 'ghost' but symbol is not available")
    }));
}

#[test]
fn rejects_workflow_output_binding_type_mismatch() {
    let source = r#"
fn summarize(input: Query) -> Summary;
workflow flow(input: Query) -> Answer
steps {
  s1: summarize(input);
}
output {
  result: Answer = s1;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("binds 's1' as 'Summary' but declared type is 'Answer'")
    }));
}

#[test]
fn rejects_workflow_output_field_type_with_record_generic_arity_mismatch() {
    let source = r#"
record Pair<T, U> {
  left: T;
  right: U;
}
;
fn build(input: Query) -> Pair<Answer, Text>;
workflow flow(input: Query) -> Unit
steps {
  s1: build(input);
}
output {
  result: Pair<Answer> = s1;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("workflow 'flow' output field 'result' uses record type 'Pair' with 1 generic argument(s), expected 2")
    }));
}

#[test]
fn accepts_workflow_output_binding_with_option_member_projection() {
    let source = r#"
fn answer(input: Query) -> Option<Answer>;
workflow flow(input: Query) -> Answer
steps {
  s1: answer(input);
}
output {
  result: Answer = s1.some;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message.contains("output field 'result'")
    }));
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic.message.contains("cannot infer member 'some'")
    }));
}

#[test]
fn accepts_workflow_output_binding_with_record_member_projection() {
    let source = r#"
record Answer {
  text: Text;
}
;
fn answer(input: Query) -> Answer;
workflow flow(input: Query) -> Text
steps {
  s1: answer(input);
}
output {
  result: Text = s1.text;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message.contains("output field 'result'")
    }));
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("cannot infer member 'text' on type 'Answer'")
    }));
}

#[test]
fn accepts_workflow_output_binding_with_generic_record_member_projection() {
    let source = r#"
record Envelope<T> {
  payload: Option<T>;
}
;
fn answer(input: Query) -> Envelope<Answer>;
workflow flow(input: Query) -> Answer
steps {
  s1: answer(input);
}
output {
  result: Answer = s1.payload.some;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message.contains("output field 'result'")
    }));
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("cannot infer member 'payload' on type 'Envelope<Answer>'")
    }));
}

#[test]
fn warns_on_workflow_output_binding_with_unsupported_member_projection() {
    let source = r#"
fn answer(input: Query) -> Answer;
workflow flow(input: Query) -> Answer
steps {
  s1: answer(input);
}
output {
  result: Answer = s1.value;
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("cannot infer member 'value' on type 'Answer'")
    }));
}

#[test]
fn rejects_workflow_requires_without_top_level_capability() {
    let source = r#"
workflow flow() -> Unit
requires [Tool<"vector_search", "read-only">]
steps {
  s1: noop();
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("workflow 'flow' requires capability 'Tool' but it is not declared")
    }));
}

#[test]
fn rejects_workflow_evidence_without_trace() {
    let source = r#"
workflow flow() -> Unit
steps {
  s1: noop();
}
evidence {
  metrics [latency_ms];
}
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("workflow 'flow' evidence block requires trace")
    }));
}

#[test]
fn parses_agent_with_state_policy_loop_and_evidence() {
    let source = r#"
cap Tool<"kb_search", "read-only">;
agent support_agent(input: Ticket) -> Resolution
intent "resolve support tickets within policy boundaries"
state {
  INIT -> CLASSIFIED;
  CLASSIFIED -> DONE, ESCALATED;
  any -> ESCALATED;
}
policy {
  allow_tools ["kb_search", "order_lookup"];
  deny_tools ["payment_refund"];
  max_iterations = 8;
  human_in_loop when input.risk_score > 7;
}
requires [Tool<"kb_search", "read-only">]
loop { perceive -> reason -> act -> observe; stop when state == DONE; }
ensures [state == DONE]
evidence {
  trace "agent.support.v1";
  metrics [iteration_count, policy_block_count];
}
;
"#;

    let program = parse_source(source).expect("agent program should parse");
    assert_eq!(program.items.len(), 2);
}

#[test]
fn lowers_agent_to_hir() {
    let source = r#"
agent a(input: Ticket) -> Resolution
state { INIT -> DONE; }
policy { allow_tools ["kb"]; deny_tools ["payment"]; max_iterations = 3; }
loop { perceive -> act; stop when state == DONE; }
;
"#;

    let hir = lower_source(source).expect("source should lower");
    assert_eq!(hir.agents.len(), 1);
    assert_eq!(hir.agents[0].name, "a");
    assert_eq!(hir.agents[0].loop_spec.stages.len(), 2);
}

#[test]
fn rejects_duplicate_agent_declarations() {
    let source = r#"
agent a() -> Unit state { INIT -> DONE; } policy { allow_tools ["kb"]; deny_tools ["x"]; } loop { perceive -> act; stop when state == DONE; };
agent a() -> Unit state { INIT -> DONE; } policy { allow_tools ["kb"]; deny_tools ["x"]; } loop { perceive -> act; stop when state == DONE; };
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("duplicate agent declaration 'a'")
    }));
}

#[test]
fn rejects_agent_policy_allow_deny_conflict() {
    let source = r#"
agent a() -> Unit
state { INIT -> DONE; }
policy {
  allow_tools ["foo"];
  deny_tools ["foo"];
}
loop { perceive -> act; stop when state == DONE; }
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("policy conflicts on tool 'foo'")
    }));
}

#[test]
fn warns_agent_policy_deny_precedence_when_overlap_exists() {
    let source = r#"
agent a() -> Unit
state { INIT -> DONE; }
policy {
  allow_tools ["foo", "bar"];
  deny_tools ["foo"];
}
loop { perceive -> act; stop when state == DONE; }
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("policy deny takes precedence over allow")
    }));
}

#[test]
fn warns_on_agent_unreachable_states() {
    let source = r#"
agent a() -> Unit
state {
  INIT -> DONE;
  STUCK -> LOST;
}
policy { allow_tools ["kb"]; deny_tools ["payment"]; }
loop { perceive -> act; stop when state == DONE; }
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("has unreachable states: LOST, STUCK")
    }));
}

#[test]
fn warns_on_agent_unknown_stop_state_without_guard() {
    let source = r#"
agent a() -> Unit
state {
  INIT -> RUNNING;
  RUNNING -> RUNNING;
}
policy { allow_tools ["kb"]; deny_tools ["payment"]; }
loop { perceive -> act; stop when state == DONE; }
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("stop condition targets unknown state 'DONE'")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("closed state cycle without exit: RUNNING")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning && diagnostic.message.contains("may not terminate")
    }));
}

#[test]
fn suppresses_agent_non_termination_warning_with_max_iterations_guard() {
    let source = r#"
agent a() -> Unit
state {
  INIT -> RUNNING;
  RUNNING -> RUNNING;
}
policy {
  allow_tools ["kb"];
  deny_tools ["payment"];
  max_iterations = 5;
}
loop { perceive -> act; stop when state == DONE; }
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("stop condition targets unknown state 'DONE'")
    }));
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning && diagnostic.message.contains("may not terminate")
    }));
}

#[test]
fn avoids_agent_cycle_warning_when_stop_state_is_inside_cycle() {
    let source = r#"
agent a() -> Unit
state {
  INIT -> RUNNING;
  RUNNING -> RUNNING;
}
policy { allow_tools ["kb"]; deny_tools ["payment"]; }
loop { perceive -> act; stop when state == RUNNING; }
;
"#;

    let diagnostics = check_source(source);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic
                .message
                .contains("closed state cycle without exit")
    }));
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning && diagnostic.message.contains("may not terminate")
    }));
}

#[test]
fn rejects_agent_requires_without_top_level_capability() {
    let source = r#"
agent a(input: Ticket) -> Resolution
state { INIT -> DONE; }
policy { allow_tools ["kb"]; deny_tools ["payment"]; }
requires [Tool<"kb_search", "read-only">]
loop { perceive -> act; stop when state == DONE; }
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("agent 'a' requires capability 'Tool' but it is not declared")
    }));
}

#[test]
fn rejects_agent_evidence_without_trace() {
    let source = r#"
agent a() -> Unit
state { INIT -> DONE; }
policy { allow_tools ["kb"]; deny_tools ["payment"]; }
loop { perceive -> act; stop when state == DONE; }
evidence { metrics [iteration_count]; }
;
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("agent 'a' evidence block requires trace")
    }));
}
#[test]
fn rejects_invalid_model_capability_shape() {
    let source = r#"
cap Model<"openai", "gpt-4o-mini">;
fn summarize(doc: Text) -> Summary !{model(openai)} requires [Model<"openai", "gpt-4o-mini">];
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("capability 'Model' expects 3 type arguments")
    }));
}

#[test]
fn rejects_model_effect_without_provider_argument() {
    let source = r#"
cap Model<"openai", "gpt-4o-mini", 1000>;
fn summarize(doc: Text) -> Summary !{model} requires [Model<"openai", "gpt-4o-mini", 1000>];
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("uses effect 'model' without provider argument")
    }));
}

#[test]
fn rejects_mismatched_model_provider_requirement() {
    let source = r#"
cap Model<"openai", "gpt-4o-mini", 1000>;
cap Model<"anthropic", "claude-3-5-sonnet", 500>;
fn summarize(doc: Text) -> Summary !{model(openai)} requires [Model<"anthropic", "claude-3-5-sonnet", 500>];
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("model(openai)' but no matching Model capability is required")
    }));
}

#[test]
fn warns_on_duplicate_effects() {
    let source = r#"
cap Net<"api.openai.com">;
fn ping() -> Unit !{net, net} requires [Net<"api.openai.com">];
"#;

    let diagnostics = check_source(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Warning
            && diagnostic.message.contains("repeats effect 'net()'")
    }));
}

#[test]
fn lowers_hir_to_mir() {
    let source = r#"
cap Net<"api.openai.com">;
fn ping() -> Unit !{net} requires [Net<"api.openai.com">];
"#;

    let mir = lower_to_mir_source(source).expect("source should lower to mir");
    assert_eq!(mir.functions.len(), 1);
    assert_eq!(mir.functions[0].name, "ping");
    assert_eq!(mir.functions[0].entry.label, "entry");
}

#[test]
fn emits_llvm_ir_for_simple_functions() {
    let source = r#"
fn answer() -> Int;
fn noop() -> Unit;
"#;

    let ir = emit_llvm_ir_source(source).expect("source should emit llvm ir");
    assert!(ir.contains("define i64 @answer()"));
    assert!(ir.contains("ret i64 0"));
    assert!(ir.contains("define void @noop()"));
    assert!(ir.contains("ret void"));
}

#[test]
fn fails_native_compile_when_tool_missing() {
    let ir = "define i64 @answer() {\nentry:\n  ret i64 0\n}\n";
    let output = std::env::temp_dir().join("kooixc-native-missing-tool.bin");
    let result = compile_llvm_ir_to_executable_with_tools(
        ir,
        &output,
        "definitely-missing-llc",
        "definitely-missing-clang",
    );

    assert!(matches!(result, Err(NativeError::ToolNotFound(_))));
}

#[test]
fn rejects_native_compile_when_semantic_errors_exist() {
    let source = r#"
cap Model<"openai", "gpt-4o-mini">;
fn summarize(doc: Text) -> Summary !{model(openai)} requires [Model<"openai", "gpt-4o-mini">];
"#;

    let result = kooixc::compile_native_source(source, std::path::Path::new("/tmp/kooixc-bad"));
    assert!(matches!(result, Err(NativeError::Diagnostics(_))));
}

#[test]
fn compiles_native_binary_when_toolchain_available() {
    if !tool_exists("llc") || !tool_exists("clang") {
        return;
    }

    let ir = "define i32 @main() {\nentry:\n  ret i32 0\n}\n";
    let output = std::env::temp_dir().join("kooixc-native-smoke");
    let _ = std::fs::remove_file(&output);

    compile_llvm_ir_to_executable(ir, &output).expect("native compile should succeed");
    assert!(output.exists());

    let _ = std::fs::remove_file(&output);
}

#[test]
fn compiles_and_runs_native_binary() {
    if !tool_exists("llc") || !tool_exists("clang") {
        return;
    }

    let source = r#"
fn main() -> Int;
"#;

    let output = std::env::temp_dir().join("kooixc-native-run-smoke");
    let _ = std::fs::remove_file(&output);

    let run_output =
        compile_and_run_native_source(source, &output).expect("compile+run should work");
    assert_eq!(run_output.status_code, Some(0));

    let _ = std::fs::remove_file(&output);
}

#[test]
fn compiles_and_runs_native_binary_with_args() {
    if !tool_exists("llc") || !tool_exists("clang") {
        return;
    }

    let source = r#"
fn main() -> Int;
"#;

    let output = std::env::temp_dir().join("kooixc-native-run-args-smoke");
    let _ = std::fs::remove_file(&output);

    let args = vec!["demo".to_string(), "123".to_string()];
    let run_output = compile_and_run_native_source_with_args(source, &output, &args)
        .expect("compile+run with args should work");
    assert_eq!(run_output.status_code, Some(0));

    let _ = std::fs::remove_file(&output);
}

#[test]
fn compiles_and_runs_native_binary_with_stdin() {
    if !tool_exists("llc") || !tool_exists("clang") {
        return;
    }

    let source = r#"
fn main() -> Int;
"#;

    let output = std::env::temp_dir().join("kooixc-native-run-stdin-smoke");
    let _ = std::fs::remove_file(&output);

    let run_output = compile_and_run_native_source_with_args_and_stdin(
        source,
        &output,
        &[],
        Some(b"hello from stdin\n"),
    )
    .expect("compile+run with stdin should work");
    assert_eq!(run_output.status_code, Some(0));

    let _ = std::fs::remove_file(&output);
}

#[test]
fn runs_existing_executable_with_stdin() {
    if !tool_exists("llc") || !tool_exists("clang") {
        return;
    }

    let ir = "define i32 @main() {\nentry:\n  ret i32 0\n}\n";
    let output = std::env::temp_dir().join("kooixc-native-stdin-direct");
    let _ = std::fs::remove_file(&output);

    compile_llvm_ir_to_executable(ir, &output).expect("native compile should succeed");
    let run_output =
        run_executable_with_args_and_stdin(&output, &["arg1".to_string()], Some(b"stdin payload"))
            .expect("run with stdin should succeed");
    assert_eq!(run_output.status_code, Some(0));

    let _ = std::fs::remove_file(&output);
}

#[test]
fn compiles_and_runs_native_binary_with_timeout() {
    if !tool_exists("llc") || !tool_exists("clang") {
        return;
    }

    let source = r#"
fn main() -> Int;
"#;

    let output = std::env::temp_dir().join("kooixc-native-run-timeout-smoke");
    let _ = std::fs::remove_file(&output);

    let run_output = compile_and_run_native_source_with_args_stdin_and_timeout(
        source,
        &output,
        &[],
        None,
        Some(500),
    )
    .expect("compile+run with timeout should work");
    assert_eq!(run_output.status_code, Some(0));

    let _ = std::fs::remove_file(&output);
}

#[cfg(unix)]
#[test]
fn run_executable_times_out() {
    let shell_path = resolve_unix_test_shell();

    let args = vec!["-c".to_string(), "sleep 2".to_string()];
    let result = run_executable_with_args_and_stdin_and_timeout(&shell_path, &args, None, Some(50));

    assert!(matches!(
        result,
        Err(NativeError::TimedOut { timeout_ms: 50 })
    ));
}

#[cfg(unix)]
#[test]
fn run_executable_finishes_before_timeout() {
    let shell_path = resolve_unix_test_shell();

    let args = vec!["-c".to_string(), "exit 0".to_string()];
    let result =
        run_executable_with_args_and_stdin_and_timeout(&shell_path, &args, None, Some(500))
            .expect("fast process should finish before timeout");

    assert_eq!(result.status_code, Some(0));
}

#[cfg(unix)]
#[test]
fn run_executable_timeout_path_is_stable_under_repetition() {
    let shell_path = resolve_unix_test_shell();
    let args = vec!["-c".to_string(), "sleep 1".to_string()];

    for _ in 0..20 {
        let result =
            run_executable_with_args_and_stdin_and_timeout(&shell_path, &args, None, Some(20));
        assert!(matches!(
            result,
            Err(NativeError::TimedOut { timeout_ms: 20 })
        ));
    }
}

#[cfg(unix)]
#[test]
fn run_executable_fast_path_is_stable_under_repetition() {
    let shell_path = resolve_unix_test_shell();
    let args = vec!["-c".to_string(), "exit 0".to_string()];

    for _ in 0..20 {
        let result =
            run_executable_with_args_and_stdin_and_timeout(&shell_path, &args, None, Some(200))
                .expect("fast process should not time out");
        assert_eq!(result.status_code, Some(0));
    }
}

#[cfg(unix)]
fn resolve_unix_test_shell() -> std::path::PathBuf {
    for candidate in ["/bin/sh", "/usr/bin/sh"] {
        let path = std::path::Path::new(candidate);
        if path.exists() {
            return path.to_path_buf();
        }
    }

    panic!("no usable shell found for timeout tests (expected /bin/sh or /usr/bin/sh)");
}

#[cfg(windows)]
#[test]
fn run_executable_times_out_on_windows() {
    let shell_path = resolve_windows_test_shell();

    let args = vec!["/C".to_string(), "ping 127.0.0.1 -n 4 >NUL".to_string()];
    let result = run_executable_with_args_and_stdin_and_timeout(&shell_path, &args, None, Some(50));

    assert!(matches!(
        result,
        Err(NativeError::TimedOut { timeout_ms: 50 })
    ));
}

#[cfg(windows)]
#[test]
fn run_executable_finishes_before_timeout_on_windows() {
    let shell_path = resolve_windows_test_shell();

    let args = vec!["/C".to_string(), "exit 0".to_string()];
    let result =
        run_executable_with_args_and_stdin_and_timeout(&shell_path, &args, None, Some(500))
            .expect("fast process should finish before timeout");

    assert_eq!(result.status_code, Some(0));
}

#[cfg(windows)]
#[test]
fn run_executable_timeout_path_is_stable_under_repetition_on_windows() {
    let shell_path = resolve_windows_test_shell();
    let args = vec!["/C".to_string(), "ping 127.0.0.1 -n 4 >NUL".to_string()];

    for _ in 0..10 {
        let result =
            run_executable_with_args_and_stdin_and_timeout(&shell_path, &args, None, Some(50));
        assert!(matches!(
            result,
            Err(NativeError::TimedOut { timeout_ms: 50 })
        ));
    }
}

#[cfg(windows)]
#[test]
fn run_executable_fast_path_is_stable_under_repetition_on_windows() {
    let shell_path = resolve_windows_test_shell();
    let args = vec!["/C".to_string(), "exit 0".to_string()];

    for _ in 0..20 {
        let result =
            run_executable_with_args_and_stdin_and_timeout(&shell_path, &args, None, Some(200))
                .expect("fast process should not time out");
        assert_eq!(result.status_code, Some(0));
    }
}

#[cfg(windows)]
fn resolve_windows_test_shell() -> std::path::PathBuf {
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        let candidate = std::path::PathBuf::from(system_root)
            .join("System32")
            .join("cmd.exe");
        if candidate.exists() {
            return candidate;
        }
    }

    std::path::PathBuf::from("cmd.exe")
}

fn tool_exists(tool: &str) -> bool {
    std::process::Command::new(tool)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

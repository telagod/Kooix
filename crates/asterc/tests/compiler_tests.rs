use asterc::ast::{FailureValue, Item, PredicateOp, PredicateValue};
use asterc::error::Severity;
use asterc::native::{
    compile_llvm_ir_to_executable, compile_llvm_ir_to_executable_with_tools,
    run_executable_with_args_and_stdin, run_executable_with_args_and_stdin_and_timeout,
    NativeError,
};
use asterc::{
    check_source, compile_and_run_native_source, compile_and_run_native_source_with_args,
    compile_and_run_native_source_with_args_and_stdin,
    compile_and_run_native_source_with_args_stdin_and_timeout, emit_llvm_ir_source, lower_source,
    lower_to_mir_source, parse_source,
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
    let output = std::env::temp_dir().join("asterc-native-missing-tool.bin");
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

    let result = asterc::compile_native_source(source, std::path::Path::new("/tmp/asterc-bad"));
    assert!(matches!(result, Err(NativeError::Diagnostics(_))));
}

#[test]
fn compiles_native_binary_when_toolchain_available() {
    if !tool_exists("llc") || !tool_exists("clang") {
        return;
    }

    let ir = "define i32 @main() {\nentry:\n  ret i32 0\n}\n";
    let output = std::env::temp_dir().join("asterc-native-smoke");
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

    let output = std::env::temp_dir().join("asterc-native-run-smoke");
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

    let output = std::env::temp_dir().join("asterc-native-run-args-smoke");
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

    let output = std::env::temp_dir().join("asterc-native-run-stdin-smoke");
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
    let output = std::env::temp_dir().join("asterc-native-stdin-direct");
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

    let output = std::env::temp_dir().join("asterc-native-run-timeout-smoke");
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
    use std::os::unix::fs::PermissionsExt;

    let script_path = std::env::temp_dir().join("asterc-timeout-smoke.sh");
    let _ = std::fs::remove_file(&script_path);

    std::fs::write(&script_path, "#!/bin/sh\nsleep 1\n")
        .expect("should write timeout smoke script");
    let mut permissions = std::fs::metadata(&script_path)
        .expect("should stat script")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script_path, permissions).expect("should chmod script");

    let result = run_executable_with_args_and_stdin_and_timeout(&script_path, &[], None, Some(10));
    assert!(matches!(result, Err(NativeError::TimedOut { .. })));

    let _ = std::fs::remove_file(&script_path);
}

fn tool_exists(tool: &str) -> bool {
    std::process::Command::new(tool)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

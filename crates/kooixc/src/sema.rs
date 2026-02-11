use std::collections::{HashMap, HashSet, VecDeque};

use crate::ast::{
    AssignStmt, BinaryOp, Block, EnsureClause, Expr, FailureAction, FailureValue, LetStmt,
    MatchArmBody, MatchPattern, PredicateValue, Program, RecordGenericParam, ReturnStmt, Statement,
    TypeArg, TypeRef, WorkflowCallArg,
};
use crate::error::{Diagnostic, Span};
use crate::hir::{
    lower_program, HirAgent, HirEffect, HirEnum, HirFunction, HirProgram, HirRecord, HirWorkflow,
};

#[derive(Debug, Clone)]
struct InvocableSignature {
    generics: Vec<RecordGenericParam>,
    params: Vec<TypeRef>,
    return_type: TypeRef,
}

pub fn check_program(program: &Program) -> Vec<Diagnostic> {
    let hir = lower_program(program);
    let mut diagnostics = Vec::new();

    let declared_invocable_targets: HashSet<String> = hir
        .functions
        .iter()
        .map(|function| function.name.clone())
        .chain(hir.workflows.iter().map(|workflow| workflow.name.clone()))
        .chain(hir.agents.iter().map(|agent| agent.name.clone()))
        .collect();

    let mut declared_invocable_signatures: HashMap<String, InvocableSignature> = HashMap::new();
    for function in &hir.functions {
        declared_invocable_signatures
            .entry(function.name.clone())
            .or_insert_with(|| InvocableSignature {
                generics: function.generics.clone(),
                params: function
                    .params
                    .iter()
                    .map(|param| param.ty.clone())
                    .collect(),
                return_type: function.return_type.clone(),
            });
    }
    for workflow in &hir.workflows {
        declared_invocable_signatures
            .entry(workflow.name.clone())
            .or_insert_with(|| InvocableSignature {
                generics: Vec::new(),
                params: workflow
                    .params
                    .iter()
                    .map(|param| param.ty.clone())
                    .collect(),
                return_type: workflow.return_type.clone(),
            });
    }
    for agent in &hir.agents {
        declared_invocable_signatures
            .entry(agent.name.clone())
            .or_insert_with(|| InvocableSignature {
                generics: Vec::new(),
                params: agent.params.iter().map(|param| param.ty.clone()).collect(),
                return_type: agent.return_type.clone(),
            });
    }

    let declared_record_types = validate_record_declarations(&hir.records, &mut diagnostics);
    let declared_enum_types =
        validate_enum_declarations(&hir.enums, &declared_invocable_targets, &mut diagnostics);
    validate_type_ref_arity_usage(
        &hir,
        &declared_record_types,
        &declared_enum_types,
        &mut diagnostics,
    );

    let mut declared_capability_instances = HashSet::new();
    let mut declared_capability_heads = HashSet::new();

    for capability in &hir.capabilities {
        let capability_name = capability.ty.to_string();
        if !declared_capability_instances.insert(capability_name.clone()) {
            diagnostics.push(Diagnostic::error(
                format!("duplicate capability declaration '{capability_name}'"),
                capability.span,
            ));
        }
        declared_capability_heads.insert(capability.ty.head().to_string());
        validate_capability_shape(
            &capability.ty,
            "top-level capability",
            capability.span,
            &mut diagnostics,
        );
    }

    let mut declared_functions = HashSet::new();

    for function in &hir.functions {
        if !declared_functions.insert(function.name.clone()) {
            diagnostics.push(Diagnostic::error(
                format!("duplicate function declaration '{}'", function.name),
                function.span,
            ));
        }

        validate_intent(function, &mut diagnostics);

        if !function.effects.is_empty() && function.requires.is_empty() {
            diagnostics.push(Diagnostic::error(
                format!(
                    "function '{}' declares effects but no required capabilities",
                    function.name
                ),
                function.span,
            ));
        }

        let mut seen_requires = HashSet::new();
        for required in &function.requires {
            if !seen_requires.insert(required.to_string()) {
                diagnostics.push(Diagnostic::warning(
                    format!(
                        "function '{}' repeats required capability '{}'",
                        function.name, required
                    ),
                    function.span,
                ));
            }
            validate_required_capability(
                required,
                function,
                &declared_capability_heads,
                &declared_capability_instances,
                &mut diagnostics,
            );
        }

        let mut seen_effects = HashSet::new();
        for effect in &function.effects {
            let effect_key = format!(
                "{}:{}",
                effect.name,
                effect.argument.as_deref().unwrap_or("")
            );
            if !seen_effects.insert(effect_key) {
                diagnostics.push(Diagnostic::warning(
                    format!(
                        "function '{}' repeats effect '{}({})'",
                        function.name,
                        effect.name,
                        effect.argument.as_deref().unwrap_or("")
                    ),
                    function.span,
                ));
            }

            validate_effect_contract(effect, function, &mut diagnostics);
        }

        if function.effects.is_empty() && !function.requires.is_empty() {
            diagnostics.push(Diagnostic::warning(
                format!(
                    "function '{}' declares capabilities but has no effects",
                    function.name
                ),
                function.span,
            ));
        }

        validate_ensures(function, &mut diagnostics);
        validate_failure(function, &mut diagnostics);
        validate_evidence(function, &mut diagnostics);
        validate_function_body(
            function,
            &declared_invocable_signatures,
            &declared_record_types,
            &declared_enum_types,
            &mut diagnostics,
        );
    }

    let mut declared_workflows = HashSet::new();
    for workflow in &hir.workflows {
        if !declared_workflows.insert(workflow.name.clone()) {
            diagnostics.push(Diagnostic::error(
                format!("duplicate workflow declaration '{}'", workflow.name),
                workflow.span,
            ));
        }

        validate_workflow(
            workflow,
            &declared_capability_heads,
            &declared_capability_instances,
            &declared_invocable_targets,
            &declared_invocable_signatures,
            &declared_record_types,
            &mut diagnostics,
        );
    }

    let mut declared_agents = HashSet::new();
    for agent in &hir.agents {
        if !declared_agents.insert(agent.name.clone()) {
            diagnostics.push(Diagnostic::error(
                format!("duplicate agent declaration '{}'", agent.name),
                agent.span,
            ));
        }

        validate_agent(
            agent,
            &declared_capability_heads,
            &declared_capability_instances,
            &mut diagnostics,
        );
    }

    diagnostics
}

#[derive(Debug, Clone)]
struct RecordSchema {
    generics: Vec<RecordGenericParam>,
    fields: HashMap<String, TypeRef>,
}

#[derive(Debug, Clone)]
struct EnumSchema {
    generics: Vec<RecordGenericParam>,
    variants: HashMap<String, Option<TypeRef>>,
}

fn validate_record_declarations(
    records: &[HirRecord],
    diagnostics: &mut Vec<Diagnostic>,
) -> HashMap<String, RecordSchema> {
    let mut declared_record_types: HashMap<String, RecordSchema> = HashMap::new();

    for record in records {
        if declared_record_types.contains_key(&record.name) {
            diagnostics.push(Diagnostic::error(
                format!("duplicate record declaration '{}'", record.name),
                record.span,
            ));
            continue;
        }

        if record.fields.is_empty() {
            diagnostics.push(Diagnostic::warning(
                format!("record '{}' declares no fields", record.name),
                record.span,
            ));
        }

        let mut seen_generics = HashSet::new();
        for generic in &record.generics {
            if !seen_generics.insert(generic.name.clone()) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "record '{}' repeats generic parameter '{}'",
                        record.name, generic.name
                    ),
                    record.span,
                ));
            }
        }

        let generic_set: HashSet<&str> = record
            .generics
            .iter()
            .map(|param| param.name.as_str())
            .collect();
        let mut field_types: HashMap<String, TypeRef> = HashMap::new();
        for field in &record.fields {
            if field_types
                .insert(field.name.clone(), field.ty.clone())
                .is_some()
            {
                diagnostics.push(Diagnostic::error(
                    format!("record '{}' repeats field '{}'", record.name, field.name),
                    record.span,
                ));
            }

            validate_record_field_type_ref(&field.ty, &generic_set, record, diagnostics);
        }

        let mut normalized_generics = record.generics.clone();
        for generic in &mut normalized_generics {
            let mut seen_bounds = HashSet::new();
            generic.bounds.retain(|bound| {
                let key = bound.to_string();
                if seen_bounds.insert(key.clone()) {
                    true
                } else {
                    diagnostics.push(Diagnostic::warning(
                        format!(
                            "record '{}' repeats bound '{}' for generic parameter '{}'",
                            record.name, key, generic.name
                        ),
                        record.span,
                    ));
                    false
                }
            });
        }

        declared_record_types.insert(
            record.name.clone(),
            RecordSchema {
                generics: normalized_generics,
                fields: field_types,
            },
        );
    }

    declared_record_types
}

fn validate_record_field_type_ref(
    ty: &TypeRef,
    generic_set: &HashSet<&str>,
    record: &HirRecord,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if generic_set.contains(ty.head()) {
        if !ty.args.is_empty() {
            diagnostics.push(Diagnostic::error(
                format!(
                    "record '{}' uses generic parameter '{}' with type arguments",
                    record.name,
                    ty.head(),
                ),
                record.span,
            ));
        }
        return;
    }

    for arg in &ty.args {
        if let TypeArg::Type(inner) = arg {
            validate_record_field_type_ref(inner, generic_set, record, diagnostics);
        }
    }
}

fn validate_enum_declarations(
    enums: &[HirEnum],
    declared_invocable_targets: &HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> HashMap<String, EnumSchema> {
    let mut declared_enum_types: HashMap<String, EnumSchema> = HashMap::new();

    for enum_decl in enums {
        if declared_enum_types.contains_key(&enum_decl.name) {
            diagnostics.push(Diagnostic::error(
                format!("duplicate enum declaration '{}'", enum_decl.name),
                enum_decl.span,
            ));
            continue;
        }

        if enum_decl.variants.is_empty() {
            diagnostics.push(Diagnostic::warning(
                format!("enum '{}' declares no variants", enum_decl.name),
                enum_decl.span,
            ));
        }

        let mut seen_generics = HashSet::new();
        for generic in &enum_decl.generics {
            if !seen_generics.insert(generic.name.clone()) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "enum '{}' repeats generic parameter '{}'",
                        enum_decl.name, generic.name
                    ),
                    enum_decl.span,
                ));
            }
        }

        let mut normalized_generics = enum_decl.generics.clone();
        for generic in &mut normalized_generics {
            let mut seen_bounds = HashSet::new();
            generic.bounds.retain(|bound| {
                let key = bound.to_string();
                if seen_bounds.insert(key.clone()) {
                    true
                } else {
                    diagnostics.push(Diagnostic::warning(
                        format!(
                            "enum '{}' repeats bound '{}' for generic parameter '{}'",
                            enum_decl.name, key, generic.name
                        ),
                        enum_decl.span,
                    ));
                    false
                }
            });
        }

        let generic_set: HashSet<&str> = normalized_generics
            .iter()
            .map(|param| param.name.as_str())
            .collect();

        let mut variants: HashMap<String, Option<TypeRef>> = HashMap::new();
        for variant in &enum_decl.variants {
            if variant.name == "_" {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "enum '{}' declares variant '_' (reserved for match wildcard)",
                        enum_decl.name
                    ),
                    enum_decl.span,
                ));
                continue;
            }

            if declared_invocable_targets.contains(&variant.name) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "enum '{}' variant '{}' conflicts with invocable target name",
                        enum_decl.name, variant.name
                    ),
                    enum_decl.span,
                ));
            }

            if variants
                .insert(variant.name.clone(), variant.payload.clone())
                .is_some()
            {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "enum '{}' repeats variant '{}'",
                        enum_decl.name, variant.name
                    ),
                    enum_decl.span,
                ));
            }

            if let Some(payload) = &variant.payload {
                validate_enum_variant_payload_type_ref(
                    payload,
                    &generic_set,
                    enum_decl,
                    diagnostics,
                );
            }
        }

        declared_enum_types.insert(
            enum_decl.name.clone(),
            EnumSchema {
                generics: normalized_generics,
                variants,
            },
        );
    }

    declared_enum_types
}

fn validate_enum_variant_payload_type_ref(
    ty: &TypeRef,
    generic_set: &HashSet<&str>,
    enum_decl: &HirEnum,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if generic_set.contains(ty.head()) {
        if !ty.args.is_empty() {
            diagnostics.push(Diagnostic::error(
                format!(
                    "enum '{}' uses generic parameter '{}' with type arguments",
                    enum_decl.name,
                    ty.head(),
                ),
                enum_decl.span,
            ));
        }
        return;
    }

    for arg in &ty.args {
        if let TypeArg::Type(inner) = arg {
            validate_enum_variant_payload_type_ref(inner, generic_set, enum_decl, diagnostics);
        }
    }
}

fn validate_type_ref_arity_usage(
    program: &HirProgram,
    declared_record_types: &HashMap<String, RecordSchema>,
    declared_enum_types: &HashMap<String, EnumSchema>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for record in &program.records {
        for field in &record.fields {
            validate_declared_type_ref_arity(
                &field.ty,
                declared_record_types,
                declared_enum_types,
                &format!("record '{}' field '{}'", record.name, field.name),
                record.span,
                diagnostics,
            );
        }
    }

    for enum_decl in &program.enums {
        for variant in &enum_decl.variants {
            if let Some(payload) = &variant.payload {
                validate_declared_type_ref_arity(
                    payload,
                    declared_record_types,
                    declared_enum_types,
                    &format!("enum '{}' variant '{}'", enum_decl.name, variant.name),
                    enum_decl.span,
                    diagnostics,
                );
            }
        }
    }

    for function in &program.functions {
        for param in &function.params {
            validate_declared_type_ref_arity(
                &param.ty,
                declared_record_types,
                declared_enum_types,
                &format!("function '{}' parameter '{}'", function.name, param.name),
                function.span,
                diagnostics,
            );
        }

        validate_declared_type_ref_arity(
            &function.return_type,
            declared_record_types,
            declared_enum_types,
            &format!("function '{}' return type", function.name),
            function.span,
            diagnostics,
        );
    }

    for workflow in &program.workflows {
        for param in &workflow.params {
            validate_declared_type_ref_arity(
                &param.ty,
                declared_record_types,
                declared_enum_types,
                &format!("workflow '{}' parameter '{}'", workflow.name, param.name),
                workflow.span,
                diagnostics,
            );
        }

        validate_declared_type_ref_arity(
            &workflow.return_type,
            declared_record_types,
            declared_enum_types,
            &format!("workflow '{}' return type", workflow.name),
            workflow.span,
            diagnostics,
        );

        for output in &workflow.output {
            validate_declared_type_ref_arity(
                &output.ty,
                declared_record_types,
                declared_enum_types,
                &format!(
                    "workflow '{}' output field '{}'",
                    workflow.name, output.name
                ),
                workflow.span,
                diagnostics,
            );
        }
    }

    for agent in &program.agents {
        for param in &agent.params {
            validate_declared_type_ref_arity(
                &param.ty,
                declared_record_types,
                declared_enum_types,
                &format!("agent '{}' parameter '{}'", agent.name, param.name),
                agent.span,
                diagnostics,
            );
        }

        validate_declared_type_ref_arity(
            &agent.return_type,
            declared_record_types,
            declared_enum_types,
            &format!("agent '{}' return type", agent.name),
            agent.span,
            diagnostics,
        );
    }
}

fn validate_declared_type_ref_arity(
    ty: &TypeRef,
    declared_record_types: &HashMap<String, RecordSchema>,
    declared_enum_types: &HashMap<String, EnumSchema>,
    context: &str,
    span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(record_schema) = declared_record_types.get(ty.head()) {
        let expected_arity = record_schema.generics.len();
        let actual_arity = ty.args.len();
        if expected_arity != actual_arity {
            diagnostics.push(Diagnostic::error(
                format!(
                    "{} uses record type '{}' with {} generic argument(s), expected {}",
                    context,
                    ty.head(),
                    actual_arity,
                    expected_arity,
                ),
                span,
            ));
        } else {
            for (index, generic) in record_schema.generics.iter().enumerate() {
                let Some(actual_arg) = ty.args.get(index) else {
                    continue;
                };

                match actual_arg {
                    TypeArg::Type(actual_ty) => {
                        if generic.bounds.is_empty() {
                            continue;
                        }

                        let mut failed_bounds = Vec::new();
                        for bound in &generic.bounds {
                            if !type_satisfies_bound(actual_ty, bound, declared_record_types) {
                                failed_bounds.push(bound.to_string());
                            }
                        }

                        if failed_bounds.is_empty() {
                            continue;
                        }

                        if failed_bounds.len() == 1 {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "{} uses record type '{}' with generic argument '{}' as '{}' but it must satisfy bound '{}'",
                                    context,
                                    ty.head(),
                                    generic.name,
                                    actual_ty,
                                    failed_bounds[0],
                                ),
                                span,
                            ));
                            continue;
                        }

                        diagnostics.push(Diagnostic::error(
                            format!(
                                "{} uses record type '{}' with generic argument '{}' as '{}' but it must satisfy bounds '{}'",
                                context,
                                ty.head(),
                                generic.name,
                                actual_ty,
                                failed_bounds.join(" + "),
                            ),
                            span,
                        ));
                    }
                    TypeArg::String(actual) => {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "{} uses record type '{}' with generic argument '{}' as string '{}' but expected a type",
                                context,
                                ty.head(),
                                generic.name,
                                actual,
                            ),
                            span,
                        ));
                    }
                    TypeArg::Number(actual) => {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "{} uses record type '{}' with generic argument '{}' as number '{}' but expected a type",
                                context,
                                ty.head(),
                                generic.name,
                                actual,
                            ),
                            span,
                        ));
                    }
                }
            }
        }
    }

    if let Some(enum_schema) = declared_enum_types.get(ty.head()) {
        let expected_arity = enum_schema.generics.len();
        let actual_arity = ty.args.len();
        if expected_arity != actual_arity {
            diagnostics.push(Diagnostic::error(
                format!(
                    "{} uses enum type '{}' with {} generic argument(s), expected {}",
                    context,
                    ty.head(),
                    actual_arity,
                    expected_arity,
                ),
                span,
            ));
        } else {
            for (index, generic) in enum_schema.generics.iter().enumerate() {
                let Some(actual_arg) = ty.args.get(index) else {
                    continue;
                };

                match actual_arg {
                    TypeArg::Type(actual_ty) => {
                        if generic.bounds.is_empty() {
                            continue;
                        }

                        let mut failed_bounds = Vec::new();
                        for bound in &generic.bounds {
                            if !type_satisfies_bound(actual_ty, bound, declared_record_types) {
                                failed_bounds.push(bound.to_string());
                            }
                        }

                        if failed_bounds.is_empty() {
                            continue;
                        }

                        if failed_bounds.len() == 1 {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "{} uses enum type '{}' with generic argument '{}' as '{}' but it must satisfy bound '{}'",
                                    context,
                                    ty.head(),
                                    generic.name,
                                    actual_ty,
                                    failed_bounds[0],
                                ),
                                span,
                            ));
                            continue;
                        }

                        diagnostics.push(Diagnostic::error(
                            format!(
                                "{} uses enum type '{}' with generic argument '{}' as '{}' but it must satisfy bounds '{}'",
                                context,
                                ty.head(),
                                generic.name,
                                actual_ty,
                                failed_bounds.join(" + "),
                            ),
                            span,
                        ));
                    }
                    TypeArg::String(actual) => {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "{} uses enum type '{}' with generic argument '{}' as string '{}' but expected a type",
                                context,
                                ty.head(),
                                generic.name,
                                actual,
                            ),
                            span,
                        ));
                    }
                    TypeArg::Number(actual) => {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "{} uses enum type '{}' with generic argument '{}' as number '{}' but expected a type",
                                context,
                                ty.head(),
                                generic.name,
                                actual,
                            ),
                            span,
                        ));
                    }
                }
            }
        }
    }

    for arg in &ty.args {
        if let TypeArg::Type(inner) = arg {
            validate_declared_type_ref_arity(
                inner,
                declared_record_types,
                declared_enum_types,
                context,
                span,
                diagnostics,
            );
        }
    }
}

fn validate_agent(
    agent: &HirAgent,
    declared_capability_heads: &HashSet<String>,
    declared_capability_instances: &HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(intent) = &agent.intent {
        if intent.trim().is_empty() {
            diagnostics.push(Diagnostic::warning(
                format!("agent '{}' declares an empty intent", agent.name),
                agent.span,
            ));
        }
    }

    if agent.state_rules.is_empty() {
        diagnostics.push(Diagnostic::error(
            format!("agent '{}' declares no state transitions", agent.name),
            agent.span,
        ));
    }

    let mut seen_state_edges = HashSet::new();
    for rule in &agent.state_rules {
        if rule.to.is_empty() {
            diagnostics.push(Diagnostic::error(
                format!(
                    "agent '{}' has state rule '{}' with no target state",
                    agent.name, rule.from
                ),
                agent.span,
            ));
        }

        for target in &rule.to {
            let edge = format!("{}->{}", rule.from, target);
            if !seen_state_edges.insert(edge.clone()) {
                diagnostics.push(Diagnostic::warning(
                    format!("agent '{}' repeats state transition '{}'", agent.name, edge),
                    agent.span,
                ));
            }
        }
    }

    let state_analysis = validate_agent_state_reachability(agent, diagnostics);
    let known_state_symbols = collect_agent_state_symbols(agent);

    let allow_set: HashSet<String> = agent.policy.allow_tools.iter().cloned().collect();
    let deny_set: HashSet<String> = agent.policy.deny_tools.iter().cloned().collect();

    let mut overlap_tools: Vec<String> = allow_set.intersection(&deny_set).cloned().collect();
    overlap_tools.sort();

    for tool in &overlap_tools {
        diagnostics.push(Diagnostic::error(
            format!(
                "agent '{}' policy conflicts on tool '{}': both allow and deny",
                agent.name, tool
            ),
            agent.span,
        ));
    }

    if !overlap_tools.is_empty() {
        diagnostics.push(Diagnostic::warning(
            format!(
                "agent '{}' policy deny takes precedence over allow for tools: {}",
                agent.name,
                overlap_tools.join(", ")
            ),
            agent.span,
        ));
    }

    if let Some(max_iterations) = &agent.policy.max_iterations {
        if max_iterations == "0" {
            diagnostics.push(Diagnostic::error(
                format!("agent '{}' sets max_iterations to 0", agent.name),
                agent.span,
            ));
        }
    }

    if agent.loop_spec.stages.is_empty() {
        diagnostics.push(Diagnostic::error(
            format!("agent '{}' loop has no stages", agent.name),
            agent.span,
        ));
    }

    validate_agent_termination(
        agent,
        state_analysis.as_ref(),
        &known_state_symbols,
        diagnostics,
    );

    let mut seen_loop_stages = HashSet::new();
    for stage in &agent.loop_spec.stages {
        if !seen_loop_stages.insert(stage.clone()) {
            diagnostics.push(Diagnostic::warning(
                format!("agent '{}' loop repeats stage '{}'", agent.name, stage),
                agent.span,
            ));
        }
    }

    let mut seen_requires = HashSet::new();
    for required in &agent.requires {
        if !seen_requires.insert(required.to_string()) {
            diagnostics.push(Diagnostic::warning(
                format!(
                    "agent '{}' repeats required capability '{}'",
                    agent.name, required
                ),
                agent.span,
            ));
        }
        validate_required_capability_for_agent(
            required,
            agent,
            declared_capability_heads,
            declared_capability_instances,
            diagnostics,
        );
    }

    validate_agent_predicate_value(
        &agent.loop_spec.stop_when.left,
        agent,
        &known_state_symbols,
        "agent loop stop condition",
        diagnostics,
    );
    validate_agent_predicate_value(
        &agent.loop_spec.stop_when.right,
        agent,
        &known_state_symbols,
        "agent loop stop condition",
        diagnostics,
    );

    if let Some(predicate) = &agent.policy.human_in_loop_when {
        validate_agent_predicate_value(
            &predicate.left,
            agent,
            &known_state_symbols,
            "agent policy human_in_loop condition",
            diagnostics,
        );
        validate_agent_predicate_value(
            &predicate.right,
            agent,
            &known_state_symbols,
            "agent policy human_in_loop condition",
            diagnostics,
        );
    }

    for ensure in &agent.ensures {
        validate_agent_predicate_value(
            &ensure.left,
            agent,
            &known_state_symbols,
            "agent ensures",
            diagnostics,
        );
        validate_agent_predicate_value(
            &ensure.right,
            agent,
            &known_state_symbols,
            "agent ensures",
            diagnostics,
        );
    }

    validate_agent_evidence(agent, diagnostics);
}

fn validate_agent_predicate_value(
    value: &PredicateValue,
    agent: &HirAgent,
    known_state_symbols: &HashSet<String>,
    context: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let PredicateValue::Path(segments) = value else {
        return;
    };

    let Some(root) = segments.first() else {
        return;
    };

    let mut allowed_roots: HashSet<String> = HashSet::new();
    allowed_roots.insert("state".to_string());
    allowed_roots.insert("output".to_string());
    allowed_roots.extend(known_state_symbols.iter().cloned());
    for param in &agent.params {
        allowed_roots.insert(param.name.clone());
    }

    if !allowed_roots.contains(root) {
        diagnostics.push(Diagnostic::warning(
            format!(
                "{} in agent '{}' references unknown symbol '{}'",
                context, agent.name, root
            ),
            agent.span,
        ));
    }
}

#[derive(Debug, Clone)]
struct AgentStateAnalysis {
    reachable_states: HashSet<String>,
    reachable_terminal_states: HashSet<String>,
    reachable_adjacency: HashMap<String, HashSet<String>>,
}

fn validate_agent_state_reachability(
    agent: &HirAgent,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<AgentStateAnalysis> {
    if agent.state_rules.is_empty() {
        return None;
    }

    let mut explicit_states = HashSet::new();
    let mut direct_adjacency: HashMap<String, Vec<String>> = HashMap::new();
    let mut any_targets = HashSet::new();

    for rule in &agent.state_rules {
        if rule.from != "any" {
            explicit_states.insert(rule.from.clone());
            direct_adjacency
                .entry(rule.from.clone())
                .or_default()
                .extend(rule.to.iter().cloned());
        }

        for target in &rule.to {
            explicit_states.insert(target.clone());
            if rule.from == "any" {
                any_targets.insert(target.clone());
            }
        }
    }

    let mut full_adjacency: HashMap<String, HashSet<String>> = HashMap::new();
    for state in &explicit_states {
        let mut targets: HashSet<String> = HashSet::new();
        if let Some(direct_targets) = direct_adjacency.get(state) {
            targets.extend(direct_targets.iter().cloned());
        }
        targets.extend(any_targets.iter().cloned());
        full_adjacency.insert(state.clone(), targets);
    }

    let mut initial_states = Vec::new();
    if explicit_states.contains("INIT") {
        initial_states.push("INIT".to_string());
    } else if let Some(rule) = agent.state_rules.iter().find(|rule| rule.from != "any") {
        initial_states.push(rule.from.clone());
    }

    if initial_states.is_empty() {
        diagnostics.push(Diagnostic::warning(
            format!(
                "agent '{}' has no concrete initial state for reachability analysis",
                agent.name
            ),
            agent.span,
        ));
        return None;
    }

    let mut reachable = HashSet::new();
    let mut queue = VecDeque::new();

    for initial in initial_states {
        if reachable.insert(initial.clone()) {
            queue.push_back(initial);
        }
    }

    while let Some(state) = queue.pop_front() {
        if let Some(targets) = full_adjacency.get(&state) {
            for target in targets {
                if reachable.insert(target.clone()) {
                    queue.push_back(target.clone());
                }
            }
        }
    }

    let mut unreachable: Vec<String> = explicit_states
        .iter()
        .filter(|state| !reachable.contains(*state))
        .cloned()
        .collect();
    unreachable.sort();

    if !unreachable.is_empty() {
        diagnostics.push(Diagnostic::warning(
            format!(
                "agent '{}' has unreachable states: {}",
                agent.name,
                unreachable.join(", ")
            ),
            agent.span,
        ));
    }

    let reachable_terminal_states = explicit_states
        .iter()
        .filter(|state| {
            if !reachable.contains(*state) {
                return false;
            }

            full_adjacency
                .get(*state)
                .map(|targets| targets.is_empty())
                .unwrap_or(true)
        })
        .cloned()
        .collect();

    let reachable_adjacency = full_adjacency
        .iter()
        .filter(|(state, _)| reachable.contains(*state))
        .map(|(state, targets)| {
            let filtered_targets = targets
                .iter()
                .filter(|target| reachable.contains(*target))
                .cloned()
                .collect();
            (state.clone(), filtered_targets)
        })
        .collect();

    Some(AgentStateAnalysis {
        reachable_states: reachable,
        reachable_terminal_states,
        reachable_adjacency,
    })
}

fn collect_agent_state_symbols(agent: &HirAgent) -> HashSet<String> {
    let mut symbols = HashSet::new();
    for rule in &agent.state_rules {
        if rule.from != "any" {
            symbols.insert(rule.from.clone());
        }
        symbols.extend(rule.to.iter().cloned());
    }
    symbols
}

fn extract_state_equality_target(predicate: &EnsureClause) -> Option<String> {
    if predicate.op != crate::ast::PredicateOp::Eq {
        return None;
    }

    match (&predicate.left, &predicate.right) {
        (PredicateValue::Path(left), PredicateValue::Path(right))
            if left.len() == 1 && left[0] == "state" && right.len() == 1 =>
        {
            Some(right[0].clone())
        }
        (PredicateValue::Path(left), PredicateValue::String(right))
            if left.len() == 1 && left[0] == "state" =>
        {
            Some(right.clone())
        }
        (PredicateValue::Path(left), PredicateValue::Path(right))
            if right.len() == 1 && right[0] == "state" && left.len() == 1 =>
        {
            Some(left[0].clone())
        }
        (PredicateValue::String(left), PredicateValue::Path(right))
            if right.len() == 1 && right[0] == "state" =>
        {
            Some(left.clone())
        }
        _ => None,
    }
}

fn strong_connect_state_node(
    node: String,
    adjacency: &HashMap<String, HashSet<String>>,
    index: &mut usize,
    indices: &mut HashMap<String, usize>,
    lowlinks: &mut HashMap<String, usize>,
    stack: &mut Vec<String>,
    on_stack: &mut HashSet<String>,
    components: &mut Vec<Vec<String>>,
) {
    let node_index = *index;
    indices.insert(node.clone(), node_index);
    lowlinks.insert(node.clone(), node_index);
    *index += 1;

    stack.push(node.clone());
    on_stack.insert(node.clone());

    let mut neighbors: Vec<String> = adjacency
        .get(&node)
        .map(|targets| targets.iter().cloned().collect())
        .unwrap_or_default();
    neighbors.sort();

    for neighbor in neighbors {
        if !indices.contains_key(&neighbor) {
            strong_connect_state_node(
                neighbor.clone(),
                adjacency,
                index,
                indices,
                lowlinks,
                stack,
                on_stack,
                components,
            );

            let neighbor_lowlink = lowlinks.get(&neighbor).copied().unwrap_or(node_index);
            if let Some(lowlink) = lowlinks.get_mut(&node) {
                *lowlink = (*lowlink).min(neighbor_lowlink);
            }
        } else if on_stack.contains(&neighbor) {
            let neighbor_index = indices.get(&neighbor).copied().unwrap_or(node_index);
            if let Some(lowlink) = lowlinks.get_mut(&node) {
                *lowlink = (*lowlink).min(neighbor_index);
            }
        }
    }

    let node_lowlink = lowlinks.get(&node).copied().unwrap_or(node_index);
    let node_discovery_index = indices.get(&node).copied().unwrap_or(node_index);

    if node_lowlink == node_discovery_index {
        let mut component = Vec::new();
        while let Some(stack_node) = stack.pop() {
            on_stack.remove(&stack_node);
            let is_root = stack_node == node;
            component.push(stack_node);
            if is_root {
                break;
            }
        }
        component.sort();
        components.push(component);
    }
}

fn find_reachable_closed_cycles(state_analysis: &AgentStateAnalysis) -> Vec<Vec<String>> {
    let mut index = 0usize;
    let mut indices = HashMap::new();
    let mut lowlinks = HashMap::new();
    let mut stack = Vec::new();
    let mut on_stack = HashSet::new();
    let mut components = Vec::new();

    let mut nodes: Vec<String> = state_analysis.reachable_adjacency.keys().cloned().collect();
    nodes.sort();

    for node in nodes {
        if !indices.contains_key(&node) {
            strong_connect_state_node(
                node,
                &state_analysis.reachable_adjacency,
                &mut index,
                &mut indices,
                &mut lowlinks,
                &mut stack,
                &mut on_stack,
                &mut components,
            );
        }
    }

    let mut closed_cycles = Vec::new();

    for component in components {
        let has_cycle = component.len() > 1
            || state_analysis
                .reachable_adjacency
                .get(&component[0])
                .map(|targets| targets.contains(&component[0]))
                .unwrap_or(false);

        if !has_cycle {
            continue;
        }

        let component_set: HashSet<&String> = component.iter().collect();
        let has_exit_edge = component.iter().any(|state| {
            state_analysis
                .reachable_adjacency
                .get(state)
                .map(|targets| targets.iter().any(|target| !component_set.contains(target)))
                .unwrap_or(false)
        });

        if !has_exit_edge {
            closed_cycles.push(component);
        }
    }

    closed_cycles.sort();
    closed_cycles
}

fn validate_agent_termination(
    agent: &HirAgent,
    state_analysis: Option<&AgentStateAnalysis>,
    known_state_symbols: &HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let stop_state = extract_state_equality_target(&agent.loop_spec.stop_when);

    let reachable_stop_state = match (&stop_state, state_analysis) {
        (Some(state), Some(analysis)) => {
            if !known_state_symbols.contains(state) {
                diagnostics.push(Diagnostic::warning(
                    format!(
                        "agent '{}' stop condition targets unknown state '{}'",
                        agent.name, state
                    ),
                    agent.span,
                ));
                false
            } else if !analysis.reachable_states.contains(state) {
                diagnostics.push(Diagnostic::warning(
                    format!(
                        "agent '{}' stop condition targets unreachable state '{}'",
                        agent.name, state
                    ),
                    agent.span,
                ));
                false
            } else {
                true
            }
        }
        _ => false,
    };

    if agent.policy.max_iterations.is_some() {
        return;
    }

    let has_reachable_terminal_state = state_analysis
        .map(|analysis| !analysis.reachable_terminal_states.is_empty())
        .unwrap_or(false);

    let mut uncovered_closed_cycles = state_analysis
        .map(find_reachable_closed_cycles)
        .unwrap_or_default();

    if reachable_stop_state {
        if let Some(state) = &stop_state {
            uncovered_closed_cycles.retain(|cycle| !cycle.iter().any(|entry| entry == state));
        }
    }

    if let Some(cycle) = uncovered_closed_cycles.first() {
        diagnostics.push(Diagnostic::warning(
            format!(
                "agent '{}' has reachable closed state cycle without exit: {}",
                agent.name,
                cycle.join(", ")
            ),
            agent.span,
        ));
    }

    if !reachable_stop_state && !has_reachable_terminal_state {
        let qualifier = if stop_state.is_some() {
            "stop condition does not reach a reachable terminal state"
        } else {
            "stop condition is not a direct state equality"
        };

        diagnostics.push(Diagnostic::warning(
            format!(
                "agent '{}' may not terminate: {} and no max_iterations guard",
                agent.name, qualifier
            ),
            agent.span,
        ));
    }
}

fn validate_agent_evidence(agent: &HirAgent, diagnostics: &mut Vec<Diagnostic>) {
    let Some(evidence) = &agent.evidence else {
        return;
    };

    match &evidence.trace {
        Some(trace) if trace.trim().is_empty() => {
            diagnostics.push(Diagnostic::error(
                format!("agent '{}' declares empty evidence trace", agent.name),
                agent.span,
            ));
        }
        Some(_) => {}
        None => {
            diagnostics.push(Diagnostic::error(
                format!("agent '{}' evidence block requires trace", agent.name),
                agent.span,
            ));
        }
    }

    if evidence.metrics.is_empty() {
        diagnostics.push(Diagnostic::warning(
            format!("agent '{}' evidence block has empty metrics", agent.name),
            agent.span,
        ));
        return;
    }

    let mut seen = HashSet::new();
    for metric in &evidence.metrics {
        if !seen.insert(metric.clone()) {
            diagnostics.push(Diagnostic::warning(
                format!(
                    "agent '{}' evidence block repeats metric '{}'",
                    agent.name, metric
                ),
                agent.span,
            ));
        }
    }
}

fn validate_workflow(
    workflow: &HirWorkflow,
    declared_capability_heads: &HashSet<String>,
    declared_capability_instances: &HashSet<String>,
    declared_invocable_targets: &HashSet<String>,
    declared_invocable_signatures: &HashMap<String, InvocableSignature>,
    declared_record_types: &HashMap<String, RecordSchema>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(intent) = &workflow.intent {
        if intent.trim().is_empty() {
            diagnostics.push(Diagnostic::warning(
                format!("workflow '{}' declares an empty intent", workflow.name),
                workflow.span,
            ));
        }
    }

    let mut seen_requires = HashSet::new();
    for required in &workflow.requires {
        if !seen_requires.insert(required.to_string()) {
            diagnostics.push(Diagnostic::warning(
                format!(
                    "workflow '{}' repeats required capability '{}'",
                    workflow.name, required
                ),
                workflow.span,
            ));
        }
        validate_required_capability_for_workflow(
            required,
            workflow,
            declared_capability_heads,
            declared_capability_instances,
            diagnostics,
        );
    }

    if workflow.steps.is_empty() {
        diagnostics.push(Diagnostic::warning(
            format!("workflow '{}' declares no steps", workflow.name),
            workflow.span,
        ));
    }

    let mut available_symbols: HashMap<String, TypeRef> = workflow
        .params
        .iter()
        .map(|param| (param.name.clone(), param.ty.clone()))
        .collect();

    let mut seen_step_ids = HashSet::new();
    for step in &workflow.steps {
        if !seen_step_ids.insert(step.id.clone()) {
            diagnostics.push(Diagnostic::error(
                format!("workflow '{}' repeats step id '{}'", workflow.name, step.id),
                workflow.span,
            ));
        }

        if validate_workflow_step_call_target(
            workflow,
            &step.id,
            &step.call.target,
            declared_invocable_targets,
            diagnostics,
        ) {
            if let Some(signature) = declared_invocable_signatures.get(&step.call.target) {
                validate_workflow_step_call_signature(
                    workflow,
                    &step.id,
                    &step.call.target,
                    &step.call.args,
                    signature,
                    &available_symbols,
                    declared_record_types,
                    diagnostics,
                );

                available_symbols.insert(step.id.clone(), signature.return_type.clone());
            }
        }

        if let Some(action) = &step.on_fail {
            validate_workflow_step_action(workflow, &step.id, action, diagnostics);
        }
    }

    validate_workflow_output_contract(
        workflow,
        &available_symbols,
        declared_record_types,
        diagnostics,
    );
    validate_workflow_evidence(workflow, diagnostics);
}

fn validate_workflow_output_contract(
    workflow: &HirWorkflow,
    available_symbols: &HashMap<String, TypeRef>,
    declared_record_types: &HashMap<String, RecordSchema>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if workflow.output.is_empty() {
        return;
    }

    let mut seen_fields = HashSet::new();
    for field in &workflow.output {
        if !seen_fields.insert(field.name.clone()) {
            diagnostics.push(Diagnostic::error(
                format!(
                    "workflow '{}' output block repeats field '{}'",
                    workflow.name, field.name
                ),
                workflow.span,
            ));
        }

        match &field.source {
            Some(source) => {
                let Some(root) = source.first() else {
                    continue;
                };

                let Some(source_type) = available_symbols.get(root) else {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "workflow '{}' output field '{}' binds to '{}' but symbol is not available in workflow scope (params + previous step ids)",
                            workflow.name,
                            field.name,
                            format_workflow_call_path(source)
                        ),
                        workflow.span,
                    ));
                    continue;
                };

                let resolved_type = match infer_member_projection_type(
                    source_type,
                    &source[1..],
                    declared_record_types,
                ) {
                    Ok(ty) => ty,
                    Err(projection) => {
                        diagnostics.push(Diagnostic::warning(
                            format!(
                                "workflow '{}' output field '{}' binds '{}' but cannot infer member '{}' on type '{}'",
                                workflow.name,
                                field.name,
                                format_workflow_call_path(source),
                                projection.member,
                                projection.base_type,
                            ),
                            workflow.span,
                        ));
                        continue;
                    }
                };

                if !types_compatible_for_workflow_call(&field.ty, &resolved_type) {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "workflow '{}' output field '{}' binds '{}' as '{}' but declared type is '{}'",
                            workflow.name,
                            field.name,
                            format_workflow_call_path(source),
                            resolved_type,
                            field.ty
                        ),
                        workflow.span,
                    ));
                }
            }
            None => {
                if let Some(named_type) = available_symbols.get(&field.name) {
                    if types_compatible_for_workflow_call(&field.ty, named_type) {
                        // Prefer implicit name-based binding before type-only matching.
                        continue;
                    }

                    diagnostics.push(Diagnostic::warning(
                        format!(
                            "workflow '{}' output field '{}' matches symbol '{}' by name but type is '{}', expected '{}'; use explicit '= symbol' binding",
                            workflow.name,
                            field.name,
                            field.name,
                            named_type,
                            field.ty
                        ),
                        workflow.span,
                    ));
                }

                let mut matching_sources: Vec<String> = available_symbols
                    .iter()
                    .filter(|(_, symbol_type)| {
                        types_compatible_for_workflow_call(&field.ty, symbol_type)
                    })
                    .map(|(name, _)| name.clone())
                    .collect();
                matching_sources.sort();

                if matching_sources.is_empty() {
                    diagnostics.push(Diagnostic::warning(
                        format!(
                            "workflow '{}' output field '{}' has type '{}' but no matching source symbol exists in workflow scope",
                            workflow.name, field.name, field.ty
                        ),
                        workflow.span,
                    ));
                    continue;
                }

                if matching_sources.len() > 1 {
                    diagnostics.push(Diagnostic::warning(
                        format!(
                            "workflow '{}' output field '{}' implicitly matches multiple source symbols: {}; use explicit '= symbol' binding",
                            workflow.name,
                            field.name,
                            matching_sources.join(", ")
                        ),
                        workflow.span,
                    ));
                }
            }
        }
    }

    let exposes_return_type = workflow
        .output
        .iter()
        .any(|field| types_compatible_for_workflow_call(&workflow.return_type, &field.ty));

    if !exposes_return_type {
        diagnostics.push(Diagnostic::warning(
            format!(
                "workflow '{}' output contract does not expose return type '{}'",
                workflow.name, workflow.return_type
            ),
            workflow.span,
        ));
    }
}

fn validate_failure(function: &HirFunction, diagnostics: &mut Vec<Diagnostic>) {
    let Some(policy) = &function.failure else {
        return;
    };

    for rule in &policy.rules {
        if rule.condition.trim().is_empty() {
            diagnostics.push(Diagnostic::warning(
                format!(
                    "function '{}' has failure rule with empty condition",
                    function.name
                ),
                function.span,
            ));
        }
        validate_failure_action(function, &rule.action, diagnostics);
    }
}

fn validate_failure_action(
    function: &HirFunction,
    action: &FailureAction,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match action.name.as_str() {
        "retry" => {
            if action.args.is_empty() {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' uses failure action 'retry' without strategy argument",
                        function.name
                    ),
                    function.span,
                ));
                return;
            }

            if action.args[0].key.is_some() {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' uses failure action 'retry' with invalid first argument",
                        function.name
                    ),
                    function.span,
                ));
            }

            let mut seen_keys = HashSet::new();
            for arg in action.args.iter().skip(1) {
                let Some(key) = &arg.key else {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' uses failure action 'retry' with positional argument after strategy",
                            function.name
                        ),
                        function.span,
                    ));
                    continue;
                };

                if !seen_keys.insert(key.clone()) {
                    diagnostics.push(Diagnostic::warning(
                        format!(
                            "function '{}' repeats retry argument '{}'",
                            function.name, key
                        ),
                        function.span,
                    ));
                }

                if key == "max" && !matches!(arg.value, FailureValue::Number(_)) {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' uses retry argument 'max' with non-number value",
                            function.name
                        ),
                        function.span,
                    ));
                }
            }
        }
        "fallback" | "abort" => {
            if action.args.len() != 1 {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' uses failure action '{}' with invalid argument count",
                        function.name, action.name
                    ),
                    function.span,
                ));
                return;
            }

            let arg = &action.args[0];
            if arg.key.is_some() || !matches!(arg.value, FailureValue::String(_)) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' uses failure action '{}' with invalid argument type",
                        function.name, action.name
                    ),
                    function.span,
                ));
            }
        }
        "compensate" => {
            if !action.args.is_empty() {
                diagnostics.push(Diagnostic::warning(
                    format!(
                        "function '{}' uses failure action 'compensate' with arguments; arguments are ignored",
                        function.name
                    ),
                    function.span,
                ));
            }
        }
        _ => {
            diagnostics.push(Diagnostic::error(
                format!(
                    "function '{}' uses unknown failure action '{}'",
                    function.name, action.name
                ),
                function.span,
            ));
        }
    }
}

fn validate_evidence(function: &HirFunction, diagnostics: &mut Vec<Diagnostic>) {
    let Some(evidence) = &function.evidence else {
        return;
    };

    match &evidence.trace {
        Some(trace) if trace.trim().is_empty() => {
            diagnostics.push(Diagnostic::error(
                format!("function '{}' declares empty evidence trace", function.name),
                function.span,
            ));
        }
        Some(_) => {}
        None => {
            diagnostics.push(Diagnostic::error(
                format!("function '{}' evidence block requires trace", function.name),
                function.span,
            ));
        }
    }

    if evidence.metrics.is_empty() {
        diagnostics.push(Diagnostic::warning(
            format!(
                "function '{}' evidence block has empty metrics",
                function.name
            ),
            function.span,
        ));
        return;
    }

    let mut seen = HashSet::new();
    for metric in &evidence.metrics {
        if !seen.insert(metric.clone()) {
            diagnostics.push(Diagnostic::warning(
                format!(
                    "function '{}' evidence block repeats metric '{}'",
                    function.name, metric
                ),
                function.span,
            ));
        }
    }
}

fn validate_function_body(
    function: &HirFunction,
    signatures: &HashMap<String, InvocableSignature>,
    declared_record_types: &HashMap<String, RecordSchema>,
    declared_enum_types: &HashMap<String, EnumSchema>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(body) = &function.body else {
        return;
    };

    let mut env: HashMap<String, TypeRef> = HashMap::new();
    for param in &function.params {
        env.insert(param.name.clone(), param.ty.clone());
    }

    let mut ends_with_return = false;

    for statement in &body.statements {
        ends_with_return = false;
        match statement {
            Statement::Let(LetStmt { name, ty, value }) => {
                if env.contains_key(name) {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' redefines variable '{}' in body",
                            function.name, name
                        ),
                        function.span,
                    ));
                    continue;
                }

                if let Some(explicit) = ty {
                    let Some(value_type) = infer_expr_type_with_expected(
                        function,
                        value,
                        &env,
                        signatures,
                        declared_record_types,
                        declared_enum_types,
                        Some(explicit),
                        diagnostics,
                    ) else {
                        continue;
                    };

                    if *explicit != value_type {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' let '{}' declares type '{}' but value is '{}'",
                                function.name, name, explicit, value_type
                            ),
                            function.span,
                        ));
                        continue;
                    }
                    env.insert(name.clone(), explicit.clone());
                } else {
                    let Some(value_type) = infer_expr_type(
                        function,
                        value,
                        &env,
                        signatures,
                        declared_record_types,
                        declared_enum_types,
                        diagnostics,
                    ) else {
                        continue;
                    };
                    env.insert(name.clone(), value_type);
                }
            }
            Statement::Assign(AssignStmt { name, value }) => {
                let Some(existing_ty) = env.get(name).cloned() else {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' assigns to unknown variable '{}' in body",
                            function.name, name
                        ),
                        function.span,
                    ));
                    continue;
                };

                let Some(actual_ty) = infer_expr_type_with_expected(
                    function,
                    value,
                    &env,
                    signatures,
                    declared_record_types,
                    declared_enum_types,
                    Some(&existing_ty),
                    diagnostics,
                ) else {
                    continue;
                };

                if actual_ty != existing_ty {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' assigns '{}' as '{}' but variable is '{}'",
                            function.name, name, actual_ty, existing_ty
                        ),
                        function.span,
                    ));
                }
            }
            Statement::Return(ReturnStmt { value }) => {
                ends_with_return = true;

                let expected = &function.return_type;
                match value {
                    None => {
                        if expected.head() != "Unit" {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "function '{}' returns nothing but expected '{}'",
                                    function.name, expected
                                ),
                                function.span,
                            ));
                        }
                    }
                    Some(expr) => {
                        let Some(actual) = infer_expr_type_with_expected(
                            function,
                            expr,
                            &env,
                            signatures,
                            declared_record_types,
                            declared_enum_types,
                            Some(expected),
                            diagnostics,
                        ) else {
                            continue;
                        };
                        if actual != *expected {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "function '{}' returns '{}' but expected '{}'",
                                    function.name, actual, expected
                                ),
                                function.span,
                            ));
                        }
                    }
                }
            }
            Statement::Expr(expr) => {
                let _ = infer_expr_type(
                    function,
                    expr,
                    &env,
                    signatures,
                    declared_record_types,
                    declared_enum_types,
                    diagnostics,
                );
            }
        }
    }

    let expected = &function.return_type;
    if expected.head() == "Unit" {
        return;
    }

    if ends_with_return {
        return;
    }

    let tail_type = match &body.tail {
        Some(expr) => infer_expr_type_with_expected(
            function,
            expr,
            &env,
            signatures,
            declared_record_types,
            declared_enum_types,
            Some(expected),
            diagnostics,
        ),
        None => None,
    };

    if let Some(tail_ty) = tail_type {
        if tail_ty != *expected {
            diagnostics.push(Diagnostic::error(
                format!(
                    "function '{}' body evaluates to '{}' but expected '{}'",
                    function.name, tail_ty, expected
                ),
                function.span,
            ));
        }
        return;
    }

    diagnostics.push(Diagnostic::error(
        format!(
            "function '{}' body does not return a value of type '{}'",
            function.name, expected
        ),
        function.span,
    ));
}

fn infer_expr_type(
    function: &HirFunction,
    expr: &Expr,
    env: &HashMap<String, TypeRef>,
    signatures: &HashMap<String, InvocableSignature>,
    declared_record_types: &HashMap<String, RecordSchema>,
    declared_enum_types: &HashMap<String, EnumSchema>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<TypeRef> {
    infer_expr_type_with_expected(
        function,
        expr,
        env,
        signatures,
        declared_record_types,
        declared_enum_types,
        None,
        diagnostics,
    )
}

enum EnumVariantResolution<'a> {
    Missing,
    Unique {
        enum_name: &'a str,
        schema: &'a EnumSchema,
        payload: Option<&'a TypeRef>,
    },
    Ambiguous {
        enum_names: Vec<&'a str>,
    },
}

fn resolve_enum_variant_unqualified<'a>(
    variant: &str,
    declared_enum_types: &'a HashMap<String, EnumSchema>,
) -> EnumVariantResolution<'a> {
    let mut matches = Vec::new();
    for (enum_name, schema) in declared_enum_types {
        if let Some(payload) = schema.variants.get(variant) {
            matches.push((enum_name.as_str(), schema, payload.as_ref()));
        }
    }

    match matches.as_slice() {
        [] => EnumVariantResolution::Missing,
        [(enum_name, schema, payload)] => EnumVariantResolution::Unique {
            enum_name,
            schema,
            payload: *payload,
        },
        _ => {
            let mut enum_names: Vec<&'a str> = matches.iter().map(|(name, _, _)| *name).collect();
            enum_names.sort();
            enum_names.dedup();
            EnumVariantResolution::Ambiguous { enum_names }
        }
    }
}

fn resolve_enum_variant_qualified<'a>(
    enum_name: &str,
    variant: &str,
    declared_enum_types: &'a HashMap<String, EnumSchema>,
) -> Option<(&'a EnumSchema, Option<&'a TypeRef>)> {
    let schema = declared_enum_types.get(enum_name)?;
    let payload = schema.variants.get(variant)?;
    Some((schema, payload.as_ref()))
}

fn infer_expr_type_with_expected(
    function: &HirFunction,
    expr: &Expr,
    env: &HashMap<String, TypeRef>,
    signatures: &HashMap<String, InvocableSignature>,
    declared_record_types: &HashMap<String, RecordSchema>,
    declared_enum_types: &HashMap<String, EnumSchema>,
    expected: Option<&TypeRef>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<TypeRef> {
    match expr {
        Expr::Number(_) => Some(TypeRef {
            name: "Int".to_string(),
            args: Vec::new(),
        }),
        Expr::String(_) => Some(TypeRef {
            name: "Text".to_string(),
            args: Vec::new(),
        }),
        Expr::Bool(_) => Some(TypeRef {
            name: "Bool".to_string(),
            args: Vec::new(),
        }),
        Expr::RecordLit { ty, fields } => {
            let Some(record_schema) = declared_record_types.get(ty.head()) else {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' uses record literal of unknown type '{}'",
                        function.name,
                        ty.head()
                    ),
                    function.span,
                ));
                return None;
            };

            let expected_arity = record_schema.generics.len();
            let actual_arity = ty.args.len();
            if expected_arity != actual_arity {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' record literal uses record type '{}' with {} generic argument(s), expected {}",
                        function.name,
                        ty.head(),
                        actual_arity,
                        expected_arity
                    ),
                    function.span,
                ));
                return None;
            }

            for (index, generic) in record_schema.generics.iter().enumerate() {
                let Some(actual_arg) = ty.args.get(index) else {
                    continue;
                };

                match actual_arg {
                    TypeArg::Type(actual_ty) => {
                        if generic.bounds.is_empty() {
                            continue;
                        }

                        let mut failed_bounds = Vec::new();
                        for bound in &generic.bounds {
                            if !type_satisfies_bound(actual_ty, bound, declared_record_types) {
                                failed_bounds.push(bound.to_string());
                            }
                        }

                        if failed_bounds.is_empty() {
                            continue;
                        }

                        if failed_bounds.len() == 1 {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "function '{}' record literal uses record type '{}' with generic argument '{}' as '{}' but it must satisfy bound '{}'",
                                    function.name,
                                    ty.head(),
                                    generic.name,
                                    actual_ty,
                                    failed_bounds[0],
                                ),
                                function.span,
                            ));
                        } else {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "function '{}' record literal uses record type '{}' with generic argument '{}' as '{}' but it must satisfy bounds '{}'",
                                    function.name,
                                    ty.head(),
                                    generic.name,
                                    actual_ty,
                                    failed_bounds.join(" + "),
                                ),
                                function.span,
                            ));
                        }
                    }
                    TypeArg::String(actual) => {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' record literal uses record type '{}' with generic argument '{}' as string '{}' but expected a type",
                                function.name,
                                ty.head(),
                                generic.name,
                                actual,
                            ),
                            function.span,
                        ));
                    }
                    TypeArg::Number(actual) => {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' record literal uses record type '{}' with generic argument '{}' as number '{}' but expected a type",
                                function.name,
                                ty.head(),
                                generic.name,
                                actual,
                            ),
                            function.span,
                        ));
                    }
                }
            }

            let mut seen_fields: HashSet<String> = HashSet::new();
            for field in fields {
                if !seen_fields.insert(field.name.clone()) {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' record literal repeats field '{}'",
                            function.name, field.name
                        ),
                        function.span,
                    ));
                    continue;
                }

                let Some(template_type) = record_schema.fields.get(&field.name) else {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' record literal uses unknown field '{}' on type '{}'",
                            function.name,
                            field.name,
                            ty.head()
                        ),
                        function.span,
                    ));
                    continue;
                };

                let expected_type = substitute_record_generic_type(
                    template_type,
                    &record_schema.generics,
                    &ty.args,
                );
                let Some(actual_type) = infer_expr_type(
                    function,
                    &field.value,
                    env,
                    signatures,
                    declared_record_types,
                    declared_enum_types,
                    diagnostics,
                ) else {
                    continue;
                };

                if actual_type != expected_type {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' record literal field '{}' is '{}' but expected '{}'",
                            function.name, field.name, actual_type, expected_type
                        ),
                        function.span,
                    ));
                }
            }

            for field_name in record_schema.fields.keys() {
                if !seen_fields.contains(field_name) {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' record literal for '{}' is missing field '{}'",
                            function.name,
                            ty.head(),
                            field_name
                        ),
                        function.span,
                    ));
                }
            }

            Some(ty.clone())
        }
        Expr::Path(segments) => {
            let Some(name) = segments.first() else {
                return None;
            };

            if let Some(root_ty) = env.get(name) {
                if segments.len() == 1 {
                    return Some(root_ty.clone());
                }

                return match infer_member_projection_type(
                    root_ty,
                    &segments[1..],
                    declared_record_types,
                ) {
                    Ok(ty) => Some(ty),
                    Err(projection) => {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' uses member path '{}' but cannot infer member '{}' on type '{}'",
                                function.name,
                                segments.join("."),
                                projection.member,
                                projection.base_type,
                            ),
                            function.span,
                        ));
                        None
                    }
                };
            }

            if segments.len() == 1 {
                match resolve_enum_variant_unqualified(name, declared_enum_types) {
                    EnumVariantResolution::Missing => {}
                    EnumVariantResolution::Ambiguous { enum_names } => {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' uses ambiguous enum variant '{}'; qualify it as '<Enum>.{}' (candidates: {})",
                                function.name,
                                name,
                                name,
                                enum_names.join(", ")
                            ),
                            function.span,
                        ));
                        return None;
                    }
                    EnumVariantResolution::Unique {
                        enum_name,
                        schema,
                        payload,
                    } => {
                        if payload.is_some() {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "function '{}' uses enum variant '{}' without a payload (expected '{}(<expr>)')",
                                    function.name, name, name
                                ),
                                function.span,
                            ));
                            return None;
                        }

                        if schema.generics.is_empty() {
                            return Some(TypeRef {
                                name: enum_name.to_string(),
                                args: Vec::new(),
                            });
                        }

                        if let Some(expected) = expected {
                            if expected.head() == enum_name {
                                return Some(expected.clone());
                            }
                        }

                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' cannot infer generic arguments for enum '{}' variant '{}' (add a type annotation)",
                                function.name, enum_name, name
                            ),
                            function.span,
                        ));
                        return None;
                    }
                }
            } else if segments.len() == 2 {
                let enum_name = segments[0].as_str();
                let variant = segments[1].as_str();
                let Some((schema, payload)) =
                    resolve_enum_variant_qualified(enum_name, variant, declared_enum_types)
                else {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' uses unknown variable '{}' in body",
                            function.name,
                            segments.join(".")
                        ),
                        function.span,
                    ));
                    return None;
                };

                if payload.is_some() {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' uses enum variant '{}.{}' without a payload (expected '{}.{}(<expr>)')",
                            function.name, enum_name, variant, enum_name, variant
                        ),
                        function.span,
                    ));
                    return None;
                }

                if schema.generics.is_empty() {
                    return Some(TypeRef {
                        name: enum_name.to_string(),
                        args: Vec::new(),
                    });
                }

                if let Some(expected) = expected {
                    if expected.head() == enum_name {
                        return Some(expected.clone());
                    }
                }

                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' cannot infer generic arguments for enum '{}' variant '{}' (add a type annotation)",
                        function.name, enum_name, variant
                    ),
                    function.span,
                ));
                return None;
            }

            diagnostics.push(Diagnostic::error(
                format!(
                    "function '{}' uses unknown variable '{}' in body",
                    function.name,
                    segments.join(".")
                ),
                function.span,
            ));
            None
        }
        Expr::Call {
            target,
            type_args,
            args,
        } => {
            let target_display = target.join(".");

            if target.len() == 1 {
                let name = target[0].as_str();
                if let Some(signature) = signatures.get(name) {
                    let mut expected_params: Vec<TypeRef> = signature.params.clone();
                    let mut expected_return: TypeRef = signature.return_type.clone();

                    if signature.generics.is_empty() {
                        if !type_args.is_empty() {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "function '{}' is not generic but call provides {} type argument(s)",
                                    name,
                                    type_args.len()
                                ),
                                function.span,
                            ));
                            return None;
                        }
                    } else {
                        if type_args.is_empty() {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "function '{}' is generic and requires explicit type arguments (use '{}<...>(...)')",
                                    name, name
                                ),
                                function.span,
                            ));
                            return None;
                        }

                        if type_args.len() != signature.generics.len() {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "function '{}' expects {} type argument(s) but call provides {}",
                                    name,
                                    signature.generics.len(),
                                    type_args.len()
                                ),
                                function.span,
                            ));
                            return None;
                        }

                        for arg in type_args {
                            match arg {
                                TypeArg::Type(_) => {}
                                TypeArg::String(_) | TypeArg::Number(_) => {
                                    diagnostics.push(Diagnostic::error(
                                        format!(
                                            "function '{}' call uses non-type generic argument '{}'; only type arguments are supported for functions",
                                            name,
                                            arg
                                        ),
                                        function.span,
                                    ));
                                    return None;
                                }
                            }
                        }

                        for (index, generic) in signature.generics.iter().enumerate() {
                            let Some(TypeArg::Type(actual_ty)) = type_args.get(index) else {
                                continue;
                            };

                            if generic.bounds.is_empty() {
                                continue;
                            }

                            let mut failed_bounds = Vec::new();
                            for bound in &generic.bounds {
                                if !type_satisfies_bound(actual_ty, bound, declared_record_types) {
                                    failed_bounds.push(bound.to_string());
                                }
                            }

                            if failed_bounds.is_empty() {
                                continue;
                            }

                            if failed_bounds.len() == 1 {
                                diagnostics.push(Diagnostic::error(
                                    format!(
                                        "function '{}' call provides generic argument '{}' as '{}' but it must satisfy bound '{}'",
                                        name,
                                        generic.name,
                                        actual_ty,
                                        failed_bounds[0]
                                    ),
                                    function.span,
                                ));
                            } else {
                                diagnostics.push(Diagnostic::error(
                                    format!(
                                        "function '{}' call provides generic argument '{}' as '{}' but it must satisfy bounds '{}'",
                                        name,
                                        generic.name,
                                        actual_ty,
                                        failed_bounds.join(" + ")
                                    ),
                                    function.span,
                                ));
                            }
                        }

                        expected_params = expected_params
                            .iter()
                            .map(|param| {
                                substitute_record_generic_type(
                                    param,
                                    &signature.generics,
                                    type_args,
                                )
                            })
                            .collect();
                        expected_return = substitute_record_generic_type(
                            &expected_return,
                            &signature.generics,
                            type_args,
                        );
                    }

                    if expected_params.len() != args.len() {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' calls '{}' with {} args but expected {}",
                                function.name,
                                name,
                                args.len(),
                                expected_params.len()
                            ),
                            function.span,
                        ));
                        return None;
                    }

                    for (index, (arg, expected_ty)) in
                        args.iter().zip(expected_params.iter()).enumerate()
                    {
                        let Some(actual_ty) = infer_expr_type_with_expected(
                            function,
                            arg,
                            env,
                            signatures,
                            declared_record_types,
                            declared_enum_types,
                            Some(expected_ty),
                            diagnostics,
                        ) else {
                            continue;
                        };
                        if actual_ty != *expected_ty {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "function '{}' calls '{}' arg {} as '{}' but expected '{}'",
                                    function.name,
                                    name,
                                    index + 1,
                                    actual_ty,
                                    expected_ty
                                ),
                                function.span,
                            ));
                        }
                    }

                    return Some(expected_return);
                }
            }

            if !type_args.is_empty() {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "call '{}' provides type arguments, but only function calls support '<...>' currently",
                        target_display
                    ),
                    function.span,
                ));
                return None;
            }

            let (enum_name, enum_schema, payload_template, variant_display) = match target
                .as_slice()
            {
                [variant] => match resolve_enum_variant_unqualified(variant, declared_enum_types) {
                    EnumVariantResolution::Missing => {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' calls unknown target '{}' in body",
                                function.name, variant
                            ),
                            function.span,
                        ));
                        return None;
                    }
                    EnumVariantResolution::Ambiguous { enum_names } => {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' calls ambiguous enum variant '{}'; qualify it as '<Enum>.{}' (candidates: {})",
                                function.name,
                                variant,
                                variant,
                                enum_names.join(", ")
                            ),
                            function.span,
                        ));
                        return None;
                    }
                    EnumVariantResolution::Unique {
                        enum_name,
                        schema,
                        payload,
                    } => (enum_name, schema, payload, variant.as_str()),
                },
                [enum_name, variant] => {
                    let Some((schema, payload)) =
                        resolve_enum_variant_qualified(enum_name, variant, declared_enum_types)
                    else {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' calls unknown target '{}' in body",
                                function.name, target_display
                            ),
                            function.span,
                        ));
                        return None;
                    };
                    (enum_name.as_str(), schema, payload, target_display.as_str())
                }
                _ => {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' calls unknown target '{}' in body",
                            function.name, target_display
                        ),
                        function.span,
                    ));
                    return None;
                }
            };

            let return_type = if enum_schema.generics.is_empty() {
                TypeRef {
                    name: enum_name.to_string(),
                    args: Vec::new(),
                }
            } else if let Some(expected) = expected {
                if expected.head() != enum_name {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' uses enum variant '{}' but expected type '{}' does not match enum '{}'",
                            function.name, variant_display, expected, enum_name
                        ),
                        function.span,
                    ));
                    return None;
                }
                expected.clone()
            } else {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' cannot infer generic arguments for enum '{}' variant '{}' (add a type annotation)",
                        function.name, enum_name, variant_display
                    ),
                    function.span,
                ));
                return None;
            };

            match payload_template {
                Some(payload_template) => {
                    if args.len() != 1 {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' calls enum variant '{}' with {} args but expected 1",
                                function.name,
                                variant_display,
                                args.len()
                            ),
                            function.span,
                        ));
                        return None;
                    }

                    let payload_ty = substitute_record_generic_type(
                        payload_template,
                        &enum_schema.generics,
                        &return_type.args,
                    );

                    let Some(actual_payload_ty) = infer_expr_type_with_expected(
                        function,
                        &args[0],
                        env,
                        signatures,
                        declared_record_types,
                        declared_enum_types,
                        Some(&payload_ty),
                        diagnostics,
                    ) else {
                        return None;
                    };

                    if actual_payload_ty != payload_ty {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' calls enum variant '{}' with payload '{}' but expected '{}'",
                                function.name, variant_display, actual_payload_ty, payload_ty
                            ),
                            function.span,
                        ));
                        return None;
                    }
                }
                None => {
                    if !args.is_empty() {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' calls enum variant '{}' with {} args but expected 0",
                                function.name,
                                variant_display,
                                args.len()
                            ),
                            function.span,
                        ));
                        return None;
                    }
                }
            }

            Some(return_type)
        }
        Expr::If {
            cond,
            then_block,
            else_block,
        } => {
            let Some(cond_ty) = infer_expr_type(
                function,
                cond,
                env,
                signatures,
                declared_record_types,
                declared_enum_types,
                diagnostics,
            ) else {
                return None;
            };
            if cond_ty.head() != "Bool" {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' uses if condition of type '{}' but expected 'Bool'",
                        function.name, cond_ty
                    ),
                    function.span,
                ));
                return None;
            }

            let then_ty = infer_block_expr_type(
                function,
                then_block.as_ref(),
                env,
                signatures,
                declared_record_types,
                declared_enum_types,
                diagnostics,
            )?;
            let else_ty = match else_block {
                Some(block) => infer_block_expr_type(
                    function,
                    block.as_ref(),
                    env,
                    signatures,
                    declared_record_types,
                    declared_enum_types,
                    diagnostics,
                )?,
                None => unit_type(),
            };

            if else_block.is_none() {
                if then_ty.head() != "Unit" {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' uses if expression without else returning '{}' but expected 'Unit'",
                            function.name, then_ty
                        ),
                        function.span,
                    ));
                    return None;
                }
                return Some(unit_type());
            }

            if then_ty != else_ty {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' if expression branches return '{}' and '{}' (types differ)",
                        function.name, then_ty, else_ty
                    ),
                    function.span,
                ));
                return None;
            }

            Some(then_ty)
        }
        Expr::While { cond, body } => {
            let Some(cond_ty) = infer_expr_type(
                function,
                cond.as_ref(),
                env,
                signatures,
                declared_record_types,
                declared_enum_types,
                diagnostics,
            ) else {
                return None;
            };
            if cond_ty.head() != "Bool" {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' uses while condition of type '{}' but expected 'Bool'",
                        function.name, cond_ty
                    ),
                    function.span,
                ));
                return None;
            }

            let _ = infer_block_expr_type(
                function,
                body.as_ref(),
                env,
                signatures,
                declared_record_types,
                declared_enum_types,
                diagnostics,
            )?;
            Some(unit_type())
        }
        Expr::Match { value, arms } => {
            if arms.is_empty() {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' uses match expression with no arms",
                        function.name
                    ),
                    function.span,
                ));
                return None;
            }

            let Some(value_ty) = infer_expr_type(
                function,
                value.as_ref(),
                env,
                signatures,
                declared_record_types,
                declared_enum_types,
                diagnostics,
            ) else {
                return None;
            };

            let Some(enum_schema) = declared_enum_types.get(value_ty.head()) else {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' matches on value of type '{}' but expected an enum type",
                        function.name, value_ty
                    ),
                    function.span,
                ));
                return None;
            };

            let mut saw_wildcard = false;
            let mut seen_variants: HashSet<String> = HashSet::new();

            let mut result_ty: Option<TypeRef> = None;
            let mut type_mismatch = false;

            for arm in arms {
                let mut arm_env = env.clone();
                match &arm.pattern {
                    MatchPattern::Wildcard => {
                        if saw_wildcard {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "function '{}' match expression repeats wildcard arm '_'",
                                    function.name
                                ),
                                function.span,
                            ));
                        }
                        saw_wildcard = true;
                    }
                    MatchPattern::Variant { path, bind } => {
                        let (enum_qual, variant_name) = match path.as_slice() {
                            [variant] => (None, variant),
                            [enum_name, variant] => (Some(enum_name.as_str()), variant),
                            _ => {
                                diagnostics.push(Diagnostic::error(
                                    format!(
                                        "function '{}' match expression uses invalid pattern '{}'",
                                        function.name,
                                        path.join(".")
                                    ),
                                    function.span,
                                ));
                                continue;
                            }
                        };

                        if let Some(enum_qual) = enum_qual {
                            if enum_qual != value_ty.head() {
                                diagnostics.push(Diagnostic::error(
                                    format!(
                                        "function '{}' match expression pattern '{}' targets enum '{}' but scrutinee type is '{}'",
                                        function.name,
                                        path.join("."),
                                        enum_qual,
                                        value_ty.head()
                                    ),
                                    function.span,
                                ));
                                continue;
                            }
                        }

                        if !seen_variants.insert(variant_name.clone()) {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "function '{}' match expression repeats variant arm '{}'",
                                    function.name, variant_name
                                ),
                                function.span,
                            ));
                        }

                        let Some(payload_template) = enum_schema.variants.get(variant_name) else {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "function '{}' match expression uses unknown variant '{}' for enum '{}'",
                                    function.name,
                                    variant_name,
                                    value_ty.head()
                                ),
                                function.span,
                            ));
                            continue;
                        };

                        if let Some(bind_name) = bind {
                            match payload_template {
                                Some(payload_template) => {
                                    let payload_ty = substitute_record_generic_type(
                                        payload_template,
                                        &enum_schema.generics,
                                        &value_ty.args,
                                    );
                                    arm_env.insert(bind_name.clone(), payload_ty);
                                }
                                None => {
                                    diagnostics.push(Diagnostic::error(
                                        format!(
                                            "function '{}' match arm '{}' binds '{}' but variant has no payload",
                                            function.name, variant_name, bind_name
                                        ),
                                        function.span,
                                    ));
                                }
                            }
                        }
                    }
                }

                let Some(arm_ty) = (match &arm.body {
                    MatchArmBody::Expr(expr) => infer_expr_type_with_expected(
                        function,
                        expr,
                        &arm_env,
                        signatures,
                        declared_record_types,
                        declared_enum_types,
                        expected,
                        diagnostics,
                    ),
                    MatchArmBody::Block(block) => infer_block_expr_type_with_expected(
                        function,
                        block,
                        &arm_env,
                        signatures,
                        declared_record_types,
                        declared_enum_types,
                        expected,
                        diagnostics,
                    ),
                }) else {
                    continue;
                };

                if let Some(existing) = &result_ty {
                    if *existing != arm_ty {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' match expression arms return '{}' and '{}' (types differ)",
                                function.name, existing, arm_ty
                            ),
                            function.span,
                        ));
                        type_mismatch = true;
                    }
                } else {
                    result_ty = Some(arm_ty);
                }
            }

            if !saw_wildcard {
                let mut missing: Vec<String> = enum_schema
                    .variants
                    .keys()
                    .filter(|variant| !seen_variants.contains(*variant))
                    .cloned()
                    .collect();
                missing.sort();
                if !missing.is_empty() {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' match expression on '{}' is non-exhaustive; missing variants: {}",
                            function.name,
                            value_ty.head(),
                            missing.join(", ")
                        ),
                        function.span,
                    ));
                }
            }

            if type_mismatch {
                None
            } else {
                result_ty
            }
        }
        Expr::Binary { op, left, right } => {
            let left_ty = infer_expr_type(
                function,
                left,
                env,
                signatures,
                declared_record_types,
                declared_enum_types,
                diagnostics,
            )?;
            let right_ty = infer_expr_type(
                function,
                right,
                env,
                signatures,
                declared_record_types,
                declared_enum_types,
                diagnostics,
            )?;
            match op {
                BinaryOp::Add => {
                    if left_ty.head() != "Int" || right_ty.head() != "Int" {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' uses '+' with '{}' and '{}'; expected Int + Int",
                                function.name, left_ty, right_ty
                            ),
                            function.span,
                        ));
                        return None;
                    }
                    Some(TypeRef {
                        name: "Int".to_string(),
                        args: Vec::new(),
                    })
                }
                BinaryOp::Eq | BinaryOp::NotEq => {
                    if left_ty != right_ty {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' compares '{}' and '{}' but types differ",
                                function.name, left_ty, right_ty
                            ),
                            function.span,
                        ));
                        return None;
                    }
                    Some(TypeRef {
                        name: "Bool".to_string(),
                        args: Vec::new(),
                    })
                }
            }
        }
    }
}

fn infer_block_expr_type(
    function: &HirFunction,
    block: &Block,
    env: &HashMap<String, TypeRef>,
    signatures: &HashMap<String, InvocableSignature>,
    declared_record_types: &HashMap<String, RecordSchema>,
    declared_enum_types: &HashMap<String, EnumSchema>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<TypeRef> {
    infer_block_expr_type_with_expected(
        function,
        block,
        env,
        signatures,
        declared_record_types,
        declared_enum_types,
        None,
        diagnostics,
    )
}

fn infer_block_expr_type_with_expected(
    function: &HirFunction,
    block: &Block,
    env: &HashMap<String, TypeRef>,
    signatures: &HashMap<String, InvocableSignature>,
    declared_record_types: &HashMap<String, RecordSchema>,
    declared_enum_types: &HashMap<String, EnumSchema>,
    expected: Option<&TypeRef>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<TypeRef> {
    let mut local_env = env.clone();

    for statement in &block.statements {
        match statement {
            Statement::Let(LetStmt { name, ty, value }) => {
                if local_env.contains_key(name) {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' redefines variable '{}' in block expression",
                            function.name, name
                        ),
                        function.span,
                    ));
                    return None;
                }

                if let Some(explicit) = ty {
                    let value_ty = infer_expr_type_with_expected(
                        function,
                        value,
                        &local_env,
                        signatures,
                        declared_record_types,
                        declared_enum_types,
                        Some(explicit),
                        diagnostics,
                    )?;
                    if *explicit != value_ty {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' let '{}' declares type '{}' but value is '{}'",
                                function.name, name, explicit, value_ty
                            ),
                            function.span,
                        ));
                        return None;
                    }
                    local_env.insert(name.clone(), explicit.clone());
                } else {
                    let value_ty = infer_expr_type(
                        function,
                        value,
                        &local_env,
                        signatures,
                        declared_record_types,
                        declared_enum_types,
                        diagnostics,
                    )?;
                    local_env.insert(name.clone(), value_ty);
                }
            }
            Statement::Assign(AssignStmt { name, value }) => {
                let Some(existing_ty) = local_env.get(name).cloned() else {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' assigns to unknown variable '{}' in block expression",
                            function.name, name
                        ),
                        function.span,
                    ));
                    return None;
                };

                let actual_ty = infer_expr_type_with_expected(
                    function,
                    value,
                    &local_env,
                    signatures,
                    declared_record_types,
                    declared_enum_types,
                    Some(&existing_ty),
                    diagnostics,
                )?;
                if actual_ty != existing_ty {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' assigns '{}' as '{}' but variable is '{}'",
                            function.name, name, actual_ty, existing_ty
                        ),
                        function.span,
                    ));
                    return None;
                }
            }
            Statement::Return(ReturnStmt { .. }) => {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' uses 'return' inside a block expression (unsupported)",
                        function.name
                    ),
                    function.span,
                ));
                return None;
            }
            Statement::Expr(expr) => {
                let _ = infer_expr_type(
                    function,
                    expr,
                    &local_env,
                    signatures,
                    declared_record_types,
                    declared_enum_types,
                    diagnostics,
                );
            }
        }
    }

    match &block.tail {
        Some(expr) => infer_expr_type_with_expected(
            function,
            expr,
            &local_env,
            signatures,
            declared_record_types,
            declared_enum_types,
            expected,
            diagnostics,
        ),
        None => Some(unit_type()),
    }
}

fn unit_type() -> TypeRef {
    TypeRef {
        name: "Unit".to_string(),
        args: Vec::new(),
    }
}

fn validate_workflow_evidence(workflow: &HirWorkflow, diagnostics: &mut Vec<Diagnostic>) {
    let Some(evidence) = &workflow.evidence else {
        return;
    };

    match &evidence.trace {
        Some(trace) if trace.trim().is_empty() => {
            diagnostics.push(Diagnostic::error(
                format!("workflow '{}' declares empty evidence trace", workflow.name),
                workflow.span,
            ));
        }
        Some(_) => {}
        None => {
            diagnostics.push(Diagnostic::error(
                format!("workflow '{}' evidence block requires trace", workflow.name),
                workflow.span,
            ));
        }
    }

    if evidence.metrics.is_empty() {
        diagnostics.push(Diagnostic::warning(
            format!(
                "workflow '{}' evidence block has empty metrics",
                workflow.name
            ),
            workflow.span,
        ));
        return;
    }

    let mut seen = HashSet::new();
    for metric in &evidence.metrics {
        if !seen.insert(metric.clone()) {
            diagnostics.push(Diagnostic::warning(
                format!(
                    "workflow '{}' evidence block repeats metric '{}'",
                    workflow.name, metric
                ),
                workflow.span,
            ));
        }
    }
}

fn validate_workflow_step_call_target(
    workflow: &HirWorkflow,
    step_id: &str,
    call_target: &str,
    declared_invocable_targets: &HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    if !declared_invocable_targets.contains(call_target) {
        diagnostics.push(Diagnostic::warning(
            format!(
                "workflow '{}' step '{}' calls unknown target '{}'",
                workflow.name, step_id, call_target
            ),
            workflow.span,
        ));
        return false;
    }

    true
}

fn validate_workflow_step_call_signature(
    workflow: &HirWorkflow,
    step_id: &str,
    call_target: &str,
    call_args: &[WorkflowCallArg],
    signature: &InvocableSignature,
    available_symbols: &HashMap<String, TypeRef>,
    declared_record_types: &HashMap<String, RecordSchema>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if call_args.len() != signature.params.len() {
        diagnostics.push(Diagnostic::error(
            format!(
                "workflow '{}' step '{}' calls '{}' with {} argument(s), expected {}",
                workflow.name,
                step_id,
                call_target,
                call_args.len(),
                signature.params.len()
            ),
            workflow.span,
        ));
    }

    for (index, (arg, expected_type)) in call_args.iter().zip(signature.params.iter()).enumerate() {
        let Some(actual_type) = infer_workflow_call_arg_type(
            workflow,
            step_id,
            call_target,
            arg,
            available_symbols,
            declared_record_types,
            diagnostics,
        ) else {
            continue;
        };

        if !types_compatible_for_workflow_call(expected_type, &actual_type) {
            diagnostics.push(Diagnostic::error(
                format!(
                    "workflow '{}' step '{}' passes argument {} to '{}' as '{}' but expected '{}'",
                    workflow.name,
                    step_id,
                    index + 1,
                    call_target,
                    actual_type,
                    expected_type
                ),
                workflow.span,
            ));
        }
    }
}

fn infer_workflow_call_arg_type(
    workflow: &HirWorkflow,
    step_id: &str,
    call_target: &str,
    arg: &WorkflowCallArg,
    available_symbols: &HashMap<String, TypeRef>,
    declared_record_types: &HashMap<String, RecordSchema>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<TypeRef> {
    match arg {
        WorkflowCallArg::String(_) => Some(TypeRef {
            name: "Text".to_string(),
            args: Vec::new(),
        }),
        WorkflowCallArg::Number(_) => Some(TypeRef {
            name: "Int".to_string(),
            args: Vec::new(),
        }),
        WorkflowCallArg::Path(segments) => {
            let Some(root) = segments.first() else {
                return None;
            };

            let Some(root_type) = available_symbols.get(root) else {
                diagnostics.push(Diagnostic::warning(
                    format!(
                        "workflow '{}' step '{}' passes '{}' to '{}' but '{}' is not available in workflow scope (params + previous step ids)",
                        workflow.name,
                        step_id,
                        format_workflow_call_path(segments),
                        call_target,
                        root
                    ),
                    workflow.span,
                ));
                return None;
            };

            match infer_member_projection_type(root_type, &segments[1..], declared_record_types) {
                Ok(ty) => Some(ty),
                Err(projection) => {
                    diagnostics.push(Diagnostic::warning(
                        format!(
                            "workflow '{}' step '{}' passes '{}' to '{}' but cannot infer member '{}' on type '{}'",
                            workflow.name,
                            step_id,
                            format_workflow_call_path(segments),
                            call_target,
                            projection.member,
                            projection.base_type,
                        ),
                        workflow.span,
                    ));
                    None
                }
            }
        }
    }
}

fn format_workflow_call_path(segments: &[String]) -> String {
    segments.join(".")
}

#[derive(Debug)]
struct MemberProjectionFailure {
    member: String,
    base_type: TypeRef,
}

fn infer_member_projection_type(
    root_type: &TypeRef,
    members: &[String],
    declared_record_types: &HashMap<String, RecordSchema>,
) -> Result<TypeRef, MemberProjectionFailure> {
    let mut current = root_type.clone();
    for member in members {
        let Some(next) = project_member_type(&current, member, declared_record_types) else {
            return Err(MemberProjectionFailure {
                member: member.clone(),
                base_type: current,
            });
        };
        current = next;
    }

    Ok(current)
}

fn project_member_type(
    base: &TypeRef,
    member: &str,
    declared_record_types: &HashMap<String, RecordSchema>,
) -> Option<TypeRef> {
    if let Some(record_schema) = declared_record_types.get(base.head()) {
        if record_schema.generics.len() != base.args.len() {
            return None;
        }

        if !record_type_args_satisfy_bounds(base, record_schema, declared_record_types) {
            return None;
        }

        if let Some(field_type) = record_schema.fields.get(member) {
            return Some(substitute_record_generic_type(
                field_type,
                &record_schema.generics,
                &base.args,
            ));
        }
    }

    match base.head() {
        "Option" => {
            if matches!(member, "some" | "value") {
                first_type_arg(base)
            } else {
                None
            }
        }
        "Result" => match member {
            "ok" | "value" => type_arg_at(base, 0),
            "err" | "error" => type_arg_at(base, 1),
            _ => None,
        },
        "List" | "Vec" | "Array" => {
            if matches!(member, "item" | "first") {
                first_type_arg(base)
            } else {
                None
            }
        }
        "Map" => match member {
            "key" => type_arg_at(base, 0),
            "value" => type_arg_at(base, 1),
            _ => None,
        },
        _ => None,
    }
}

fn type_satisfies_bound(
    actual: &TypeRef,
    bound: &TypeRef,
    declared_record_types: &HashMap<String, RecordSchema>,
) -> bool {
    let mut seen = HashSet::new();
    type_satisfies_bound_inner(actual, bound, declared_record_types, &mut seen)
}

fn type_satisfies_bound_inner(
    actual: &TypeRef,
    bound: &TypeRef,
    declared_record_types: &HashMap<String, RecordSchema>,
    seen: &mut HashSet<(String, String)>,
) -> bool {
    if types_compatible_for_workflow_call(bound, actual) {
        return true;
    }

    if bound.head() == actual.head() {
        if bound.args.len() != actual.args.len() {
            return false;
        }

        for (expected_arg, actual_arg) in bound.args.iter().zip(actual.args.iter()) {
            match (expected_arg, actual_arg) {
                (TypeArg::Type(expected_ty), TypeArg::Type(actual_ty)) => {
                    if !type_satisfies_bound_inner(
                        actual_ty,
                        expected_ty,
                        declared_record_types,
                        seen,
                    ) {
                        return false;
                    }
                }
                (TypeArg::String(expected), TypeArg::String(actual)) if expected == actual => {}
                (TypeArg::Number(expected), TypeArg::Number(actual)) if expected == actual => {}
                _ => return false,
            }
        }

        return true;
    }

    let key = (actual.to_string(), bound.to_string());
    if !seen.insert(key) {
        return true;
    }

    let Some(actual_schema) = declared_record_types.get(actual.head()) else {
        return false;
    };
    let Some(bound_schema) = declared_record_types.get(bound.head()) else {
        return false;
    };

    if actual_schema.generics.len() != actual.args.len() {
        return false;
    }
    if bound_schema.generics.len() != bound.args.len() {
        return false;
    }

    for (field_name, bound_field_type) in &bound_schema.fields {
        let Some(actual_field_type) = actual_schema.fields.get(field_name) else {
            return false;
        };

        let expected_type =
            substitute_record_generic_type(bound_field_type, &bound_schema.generics, &bound.args);
        let actual_type = substitute_record_generic_type(
            actual_field_type,
            &actual_schema.generics,
            &actual.args,
        );

        if !type_satisfies_bound_inner(&actual_type, &expected_type, declared_record_types, seen) {
            return false;
        }
    }

    true
}

fn record_type_args_satisfy_bounds(
    base: &TypeRef,
    record_schema: &RecordSchema,
    declared_record_types: &HashMap<String, RecordSchema>,
) -> bool {
    record_schema.generics.iter().zip(base.args.iter()).all(
        |(generic, actual_arg)| match actual_arg {
            _ if generic.bounds.is_empty() => true,
            TypeArg::Type(actual_ty) => generic
                .bounds
                .iter()
                .all(|bound| type_satisfies_bound(actual_ty, bound, declared_record_types)),
            TypeArg::String(_) | TypeArg::Number(_) => false,
        },
    )
}

fn substitute_record_generic_type(
    ty: &TypeRef,
    generics: &[RecordGenericParam],
    actual_args: &[TypeArg],
) -> TypeRef {
    if ty.args.is_empty() {
        if let Some(index) = generics.iter().position(|param| param.name == ty.head()) {
            if let Some(TypeArg::Type(actual_ty)) = actual_args.get(index) {
                return actual_ty.clone();
            }
        }
    }

    TypeRef {
        name: ty.name.clone(),
        args: ty
            .args
            .iter()
            .map(|arg| match arg {
                TypeArg::Type(inner) => {
                    TypeArg::Type(substitute_record_generic_type(inner, generics, actual_args))
                }
                TypeArg::String(value) => TypeArg::String(value.clone()),
                TypeArg::Number(value) => TypeArg::Number(value.clone()),
            })
            .collect(),
    }
}

fn first_type_arg(base: &TypeRef) -> Option<TypeRef> {
    type_arg_at(base, 0)
}

fn type_arg_at(base: &TypeRef, index: usize) -> Option<TypeRef> {
    match base.args.get(index) {
        Some(TypeArg::Type(ty)) => Some(ty.clone()),
        _ => None,
    }
}

fn types_compatible_for_workflow_call(expected: &TypeRef, actual: &TypeRef) -> bool {
    if expected == actual {
        return true;
    }

    let expected_head = expected.head();
    let actual_head = actual.head();

    matches!(
        (expected_head, actual_head),
        ("Text", "String")
            | ("String", "Text")
            | ("Num", "Int")
            | ("Float", "Int")
            | ("Number", "Int")
    )
}

fn validate_workflow_step_action(
    workflow: &HirWorkflow,
    step_id: &str,
    action: &FailureAction,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match action.name.as_str() {
        "retry" => {
            if action.args.is_empty() {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "workflow '{}' step '{}' uses on_fail 'retry' without strategy argument",
                        workflow.name, step_id
                    ),
                    workflow.span,
                ));
                return;
            }

            if action.args[0].key.is_some() {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "workflow '{}' step '{}' uses on_fail 'retry' with invalid first argument",
                        workflow.name, step_id
                    ),
                    workflow.span,
                ));
            }

            let mut seen_keys = HashSet::new();
            for arg in action.args.iter().skip(1) {
                let Some(key) = &arg.key else {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "workflow '{}' step '{}' uses on_fail 'retry' with positional argument after strategy",
                            workflow.name, step_id
                        ),
                        workflow.span,
                    ));
                    continue;
                };

                if !seen_keys.insert(key.clone()) {
                    diagnostics.push(Diagnostic::warning(
                        format!(
                            "workflow '{}' step '{}' repeats retry argument '{}'",
                            workflow.name, step_id, key
                        ),
                        workflow.span,
                    ));
                }

                if key == "max" && !matches!(arg.value, FailureValue::Number(_)) {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "workflow '{}' step '{}' uses retry argument 'max' with non-number value",
                            workflow.name, step_id
                        ),
                        workflow.span,
                    ));
                }
            }
        }
        "fallback" | "abort" => {
            if action.args.len() != 1 {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "workflow '{}' step '{}' uses on_fail '{}' with invalid argument count",
                        workflow.name, step_id, action.name
                    ),
                    workflow.span,
                ));
                return;
            }

            let arg = &action.args[0];
            if arg.key.is_some() || !matches!(arg.value, FailureValue::String(_)) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "workflow '{}' step '{}' uses on_fail '{}' with invalid argument type",
                        workflow.name, step_id, action.name
                    ),
                    workflow.span,
                ));
            }
        }
        "compensate" => {
            if !action.args.is_empty() {
                diagnostics.push(Diagnostic::warning(
                    format!(
                        "workflow '{}' step '{}' uses on_fail 'compensate' with arguments; arguments are ignored",
                        workflow.name, step_id
                    ),
                    workflow.span,
                ));
            }
        }
        _ => {
            diagnostics.push(Diagnostic::error(
                format!(
                    "workflow '{}' step '{}' uses unknown on_fail action '{}'",
                    workflow.name, step_id, action.name
                ),
                workflow.span,
            ));
        }
    }
}

fn validate_intent(function: &HirFunction, diagnostics: &mut Vec<Diagnostic>) {
    if let Some(intent) = &function.intent {
        if intent.trim().is_empty() {
            diagnostics.push(Diagnostic::warning(
                format!("function '{}' declares an empty intent", function.name),
                function.span,
            ));
        }
    }
}

fn validate_ensures(function: &HirFunction, diagnostics: &mut Vec<Diagnostic>) {
    if function.ensures.is_empty() {
        return;
    }

    let mut allowed_roots = HashSet::new();
    allowed_roots.insert("output".to_string());
    for param in &function.params {
        allowed_roots.insert(param.name.clone());
    }

    for ensure in &function.ensures {
        validate_predicate_value(ensure, &ensure.left, function, &allowed_roots, diagnostics);
        validate_predicate_value(ensure, &ensure.right, function, &allowed_roots, diagnostics);
    }
}

fn validate_predicate_value(
    _ensure: &EnsureClause,
    value: &PredicateValue,
    function: &HirFunction,
    allowed_roots: &HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let PredicateValue::Path(segments) = value else {
        return;
    };

    let Some(root) = segments.first() else {
        return;
    };

    if !allowed_roots.contains(root) {
        diagnostics.push(Diagnostic::warning(
            format!(
                "function '{}' ensures references unknown symbol '{}'",
                function.name, root
            ),
            function.span,
        ));
    }
}

fn validate_capability_shape(
    capability: &TypeRef,
    context: &str,
    span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match capability.head() {
        "Model" => {
            require_arity(capability, 3, context, span, diagnostics);
            require_arg_kind(capability, 0, ArgKind::String, context, span, diagnostics);
            require_arg_kind(capability, 1, ArgKind::String, context, span, diagnostics);
            require_arg_kind(capability, 2, ArgKind::Number, context, span, diagnostics);
        }
        "Net" => {
            require_arity(capability, 1, context, span, diagnostics);
            require_arg_kind(capability, 0, ArgKind::String, context, span, diagnostics);
        }
        "Tool" => {
            require_arity(capability, 2, context, span, diagnostics);
            require_arg_kind(capability, 0, ArgKind::String, context, span, diagnostics);
            require_arg_kind(capability, 1, ArgKind::String, context, span, diagnostics);
        }
        "Io" => {
            require_arity(capability, 0, context, span, diagnostics);
        }
        _ => {
            diagnostics.push(Diagnostic::warning(
                format!(
                    "{} uses unknown capability '{}'; no schema rule applied",
                    context,
                    capability.head()
                ),
                span,
            ));
        }
    }
}

fn validate_required_capability(
    required: &TypeRef,
    function: &HirFunction,
    declared_capability_heads: &HashSet<String>,
    declared_capability_instances: &HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    validate_capability_shape(
        required,
        &format!("function '{}' requires", function.name),
        function.span,
        diagnostics,
    );

    if !declared_capability_heads.contains(required.head()) {
        diagnostics.push(Diagnostic::error(
            format!(
                "function '{}' requires capability '{}' but it is not declared at top level",
                function.name,
                required.head()
            ),
            function.span,
        ));
    }

    let required_instance = required.to_string();
    if !declared_capability_instances.contains(&required_instance) {
        diagnostics.push(Diagnostic::error(
            format!(
                "function '{}' requires capability instance '{}' but it is not declared at top level",
                function.name, required_instance
            ),
            function.span,
        ));
    }
}

fn validate_required_capability_for_workflow(
    required: &TypeRef,
    workflow: &HirWorkflow,
    declared_capability_heads: &HashSet<String>,
    declared_capability_instances: &HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    validate_capability_shape(
        required,
        &format!("workflow '{}' requires", workflow.name),
        workflow.span,
        diagnostics,
    );

    if !declared_capability_heads.contains(required.head()) {
        diagnostics.push(Diagnostic::error(
            format!(
                "workflow '{}' requires capability '{}' but it is not declared at top level",
                workflow.name,
                required.head()
            ),
            workflow.span,
        ));
    }

    let required_instance = required.to_string();
    if !declared_capability_instances.contains(&required_instance) {
        diagnostics.push(Diagnostic::error(
            format!(
                "workflow '{}' requires capability instance '{}' but it is not declared at top level",
                workflow.name, required_instance
            ),
            workflow.span,
        ));
    }
}

fn validate_required_capability_for_agent(
    required: &TypeRef,
    agent: &HirAgent,
    declared_capability_heads: &HashSet<String>,
    declared_capability_instances: &HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    validate_capability_shape(
        required,
        &format!("agent '{}' requires", agent.name),
        agent.span,
        diagnostics,
    );

    if !declared_capability_heads.contains(required.head()) {
        diagnostics.push(Diagnostic::error(
            format!(
                "agent '{}' requires capability '{}' but it is not declared at top level",
                agent.name,
                required.head()
            ),
            agent.span,
        ));
    }

    let required_instance = required.to_string();
    if !declared_capability_instances.contains(&required_instance) {
        diagnostics.push(Diagnostic::error(
            format!(
                "agent '{}' requires capability instance '{}' but it is not declared at top level",
                agent.name, required_instance
            ),
            agent.span,
        ));
    }
}

fn validate_effect_contract(
    effect: &HirEffect,
    function: &HirFunction,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(required_capability) = effect_required_capability_name(effect.name.as_str()) {
        let has_required_capability = function
            .requires
            .iter()
            .any(|required| required.head() == required_capability);
        if !has_required_capability {
            diagnostics.push(Diagnostic::error(
                format!(
                    "function '{}' uses effect '{}' but does not require '{}' capability",
                    function.name, effect.name, required_capability
                ),
                function.span,
            ));
            return;
        }

        match effect.name.as_str() {
            "model" => {
                let Some(provider) = effect.argument.as_deref() else {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' uses effect 'model' without provider argument",
                            function.name
                        ),
                        function.span,
                    ));
                    return;
                };

                let matched = function.requires.iter().any(|required| {
                    required.head() == "Model" && first_string_arg(required) == Some(provider)
                });

                if !matched {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' uses effect 'model({provider})' but no matching Model capability is required",
                            function.name
                        ),
                        function.span,
                    ));
                }
            }
            "tool" => {
                let Some(tool_name) = effect.argument.as_deref() else {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' uses effect 'tool' without tool name argument",
                            function.name
                        ),
                        function.span,
                    ));
                    return;
                };

                let matched = function.requires.iter().any(|required| {
                    required.head() == "Tool" && first_string_arg(required) == Some(tool_name)
                });

                if !matched {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "function '{}' uses effect 'tool({tool_name})' but no matching Tool capability is required",
                            function.name
                        ),
                        function.span,
                    ));
                }
            }
            "net" => {
                if let Some(domain) = effect.argument.as_deref() {
                    let matched = function.requires.iter().any(|required| {
                        required.head() == "Net" && first_string_arg(required) == Some(domain)
                    });

                    if !matched {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "function '{}' uses effect 'net({domain})' but no matching Net capability is required",
                                function.name
                            ),
                            function.span,
                        ));
                    }
                }
            }
            "io" => {
                if effect.argument.is_some() {
                    diagnostics.push(Diagnostic::warning(
                        format!(
                            "function '{}' uses effect 'io' with an argument; argument is ignored",
                            function.name
                        ),
                        function.span,
                    ));
                }
            }
            _ => {}
        }
    } else {
        diagnostics.push(Diagnostic::warning(
            format!(
                "function '{}' uses unknown effect '{}'; no capability rule applied",
                function.name, effect.name
            ),
            function.span,
        ));
    }
}

fn effect_required_capability_name(effect_name: &str) -> Option<&'static str> {
    match effect_name {
        "net" => Some("Net"),
        "model" => Some("Model"),
        "tool" => Some("Tool"),
        "io" => Some("Io"),
        _ => None,
    }
}

fn first_string_arg(capability: &TypeRef) -> Option<&str> {
    capability.args.first().and_then(|arg| {
        if let TypeArg::String(value) = arg {
            Some(value.as_str())
        } else {
            None
        }
    })
}

#[derive(Clone, Copy)]
enum ArgKind {
    String,
    Number,
}

impl ArgKind {
    fn name(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
        }
    }
}

fn require_arity(
    capability: &TypeRef,
    expected: usize,
    context: &str,
    span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if capability.args.len() != expected {
        diagnostics.push(Diagnostic::error(
            format!(
                "{} capability '{}' expects {} type arguments, found {}",
                context,
                capability.head(),
                expected,
                capability.args.len()
            ),
            span,
        ));
    }
}

fn require_arg_kind(
    capability: &TypeRef,
    index: usize,
    expected: ArgKind,
    context: &str,
    span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(arg) = capability.args.get(index) else {
        return;
    };

    let is_valid = match (expected, arg) {
        (ArgKind::String, TypeArg::String(_)) => true,
        (ArgKind::Number, TypeArg::Number(_)) => true,
        _ => false,
    };

    if !is_valid {
        diagnostics.push(Diagnostic::error(
            format!(
                "{} capability '{}' argument {} expects {}, found {}",
                context,
                capability.head(),
                index,
                expected.name(),
                arg_kind_name(arg)
            ),
            span,
        ));
    }
}

fn arg_kind_name(arg: &TypeArg) -> &'static str {
    match arg {
        TypeArg::Type(_) => "type",
        TypeArg::String(_) => "string",
        TypeArg::Number(_) => "number",
    }
}

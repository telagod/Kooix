use crate::ast::{
    AgentPolicy, Block, EnsureClause, EvidenceSpec, FailureAction, FailurePolicy, Item, LoopSpec,
    OutputField, Program, RecordField, RecordGenericParam, StateRule, TypeRef, WorkflowCall,
};
use crate::error::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirProgram {
    pub capabilities: Vec<HirCapability>,
    pub functions: Vec<HirFunction>,
    pub workflows: Vec<HirWorkflow>,
    pub agents: Vec<HirAgent>,
    pub records: Vec<HirRecord>,
    pub enums: Vec<HirEnum>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirCapability {
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirFunction {
    pub name: String,
    pub params: Vec<HirParam>,
    pub return_type: TypeRef,
    pub intent: Option<String>,
    pub effects: Vec<HirEffect>,
    pub requires: Vec<TypeRef>,
    pub ensures: Vec<EnsureClause>,
    pub failure: Option<FailurePolicy>,
    pub evidence: Option<EvidenceSpec>,
    pub body: Option<Block>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirParam {
    pub name: String,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirEffect {
    pub name: String,
    pub argument: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirWorkflow {
    pub name: String,
    pub params: Vec<HirParam>,
    pub return_type: TypeRef,
    pub intent: Option<String>,
    pub requires: Vec<TypeRef>,
    pub steps: Vec<HirWorkflowStep>,
    pub output: Vec<OutputField>,
    pub evidence: Option<EvidenceSpec>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirWorkflowStep {
    pub id: String,
    pub call: WorkflowCall,
    pub ensures: Vec<EnsureClause>,
    pub on_fail: Option<FailureAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirRecord {
    pub name: String,
    pub generics: Vec<RecordGenericParam>,
    pub fields: Vec<RecordField>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirEnum {
    pub name: String,
    pub generics: Vec<RecordGenericParam>,
    pub variants: Vec<HirEnumVariant>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirEnumVariant {
    pub name: String,
    pub payload: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirAgent {
    pub name: String,
    pub params: Vec<HirParam>,
    pub return_type: TypeRef,
    pub intent: Option<String>,
    pub state_rules: Vec<StateRule>,
    pub policy: AgentPolicy,
    pub requires: Vec<TypeRef>,
    pub loop_spec: LoopSpec,
    pub ensures: Vec<EnsureClause>,
    pub evidence: Option<EvidenceSpec>,
    pub span: Span,
}

pub fn lower_program(program: &Program) -> HirProgram {
    let mut capabilities = Vec::new();
    let mut functions = Vec::new();
    let mut workflows = Vec::new();
    let mut agents = Vec::new();
    let mut records = Vec::new();
    let mut enums = Vec::new();

    for item in &program.items {
        match item {
            Item::Capability(capability_decl) => {
                capabilities.push(HirCapability {
                    ty: capability_decl.capability.clone(),
                    span: capability_decl.span,
                });
            }
            Item::Import(_) => {}
            Item::Function(function_decl) => {
                functions.push(HirFunction {
                    name: function_decl.name.clone(),
                    params: function_decl
                        .params
                        .iter()
                        .map(|param| HirParam {
                            name: param.name.clone(),
                            ty: param.ty.clone(),
                        })
                        .collect(),
                    return_type: function_decl.return_type.clone(),
                    intent: function_decl.intent.clone(),
                    effects: function_decl
                        .effects
                        .iter()
                        .map(|effect| HirEffect {
                            name: effect.name.clone(),
                            argument: effect.argument.clone(),
                        })
                        .collect(),
                    requires: function_decl.requires.clone(),
                    ensures: function_decl.ensures.clone(),
                    failure: function_decl.failure.clone(),
                    evidence: function_decl.evidence.clone(),
                    body: function_decl.body.clone(),
                    span: function_decl.span,
                });
            }
            Item::Workflow(workflow_decl) => {
                workflows.push(HirWorkflow {
                    name: workflow_decl.name.clone(),
                    params: workflow_decl
                        .params
                        .iter()
                        .map(|param| HirParam {
                            name: param.name.clone(),
                            ty: param.ty.clone(),
                        })
                        .collect(),
                    return_type: workflow_decl.return_type.clone(),
                    intent: workflow_decl.intent.clone(),
                    requires: workflow_decl.requires.clone(),
                    steps: workflow_decl
                        .steps
                        .iter()
                        .map(|step| HirWorkflowStep {
                            id: step.id.clone(),
                            call: step.call.clone(),
                            ensures: step.ensures.clone(),
                            on_fail: step.on_fail.clone(),
                        })
                        .collect(),
                    output: workflow_decl.output.clone(),
                    evidence: workflow_decl.evidence.clone(),
                    span: workflow_decl.span,
                });
            }
            Item::Agent(agent_decl) => {
                agents.push(HirAgent {
                    name: agent_decl.name.clone(),
                    params: agent_decl
                        .params
                        .iter()
                        .map(|param| HirParam {
                            name: param.name.clone(),
                            ty: param.ty.clone(),
                        })
                        .collect(),
                    return_type: agent_decl.return_type.clone(),
                    intent: agent_decl.intent.clone(),
                    state_rules: agent_decl.state_rules.clone(),
                    policy: agent_decl.policy.clone(),
                    requires: agent_decl.requires.clone(),
                    loop_spec: agent_decl.loop_spec.clone(),
                    ensures: agent_decl.ensures.clone(),
                    evidence: agent_decl.evidence.clone(),
                    span: agent_decl.span,
                });
            }
            Item::Record(record_decl) => {
                records.push(HirRecord {
                    name: record_decl.name.clone(),
                    generics: record_decl.generics.clone(),
                    fields: record_decl.fields.clone(),
                    span: record_decl.span,
                });
            }
            Item::Enum(enum_decl) => {
                enums.push(HirEnum {
                    name: enum_decl.name.clone(),
                    generics: enum_decl.generics.clone(),
                    variants: enum_decl
                        .variants
                        .iter()
                        .map(|variant| HirEnumVariant {
                            name: variant.name.clone(),
                            payload: variant.payload.clone(),
                        })
                        .collect(),
                    span: enum_decl.span,
                });
            }
        }
    }

    HirProgram {
        capabilities,
        functions,
        workflows,
        agents,
        records,
        enums,
    }
}

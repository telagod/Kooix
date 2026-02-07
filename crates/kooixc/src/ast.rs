use std::fmt;

use crate::error::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Capability(CapabilityDecl),
    Function(FunctionDecl),
    Workflow(WorkflowDecl),
    Agent(AgentDecl),
    Record(RecordDecl),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityDecl {
    pub capability: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordDecl {
    pub name: String,
    pub fields: Vec<RecordField>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordField {
    pub name: String,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: TypeRef,
    pub intent: Option<String>,
    pub effects: Vec<EffectSpec>,
    pub requires: Vec<TypeRef>,
    pub ensures: Vec<EnsureClause>,
    pub failure: Option<FailurePolicy>,
    pub evidence: Option<EvidenceSpec>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSpec {
    pub name: String,
    pub argument: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnsureClause {
    pub left: PredicateValue,
    pub op: PredicateOp,
    pub right: PredicateValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PredicateValue {
    Path(Vec<String>),
    String(String),
    Number(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredicateOp {
    Eq,
    NotEq,
    Lt,
    Lte,
    Gt,
    Gte,
    In,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailurePolicy {
    pub rules: Vec<FailureRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureRule {
    pub condition: String,
    pub action: FailureAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureAction {
    pub name: String,
    pub args: Vec<FailureActionArg>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureActionArg {
    pub key: Option<String>,
    pub value: FailureValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureValue {
    Ident(String),
    String(String),
    Number(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceSpec {
    pub trace: Option<String>,
    pub metrics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: TypeRef,
    pub intent: Option<String>,
    pub requires: Vec<TypeRef>,
    pub steps: Vec<WorkflowStep>,
    pub output: Vec<OutputField>,
    pub evidence: Option<EvidenceSpec>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowStep {
    pub id: String,
    pub call: WorkflowCall,
    pub ensures: Vec<EnsureClause>,
    pub on_fail: Option<FailureAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowCall {
    pub target: String,
    pub args: Vec<WorkflowCallArg>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowCallArg {
    Path(Vec<String>),
    String(String),
    Number(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputField {
    pub name: String,
    pub ty: TypeRef,
    pub source: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDecl {
    pub name: String,
    pub params: Vec<Param>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateRule {
    pub from: String,
    pub to: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPolicy {
    pub allow_tools: Vec<String>,
    pub deny_tools: Vec<String>,
    pub max_iterations: Option<String>,
    pub human_in_loop_when: Option<EnsureClause>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopSpec {
    pub stages: Vec<String>,
    pub stop_when: EnsureClause,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef {
    pub name: String,
    pub args: Vec<TypeArg>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeArg {
    Type(TypeRef),
    String(String),
    Number(String),
}

impl TypeRef {
    pub fn head(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.args.is_empty() {
            return f.write_str(&self.name);
        }

        let args = self
            .args
            .iter()
            .map(|arg| arg.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "{}<{}>", self.name, args)
    }
}

impl fmt::Display for TypeArg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeArg::Type(ty) => write!(f, "{ty}"),
            TypeArg::String(value) => write!(f, "\"{value}\""),
            TypeArg::Number(value) => f.write_str(value),
        }
    }
}

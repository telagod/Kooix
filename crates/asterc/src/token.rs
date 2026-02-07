use crate::error::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    KwCap,
    KwFn,
    KwWorkflow,
    KwAgent,
    KwSteps,
    KwOnFail,
    KwOutput,
    KwState,
    KwPolicy,
    KwLoop,
    KwAllowTools,
    KwDenyTools,
    KwMaxIterations,
    KwHumanInLoop,
    KwStop,
    KwWhen,
    KwAny,
    KwIntent,
    KwEnsures,
    KwFailure,
    KwEvidence,
    KwTrace,
    KwMetrics,
    KwIn,
    KwRequires,
    Ident(String),
    StringLiteral(String),
    Number(String),
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LAngle,
    RAngle,
    Comma,
    Dot,
    Colon,
    Semicolon,
    Bang,
    Eq,
    EqEq,
    NotEq,
    Lte,
    Gte,
    Arrow,
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

use crate::error::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    KwCap,
    KwImport,
    KwFn,
    KwWorkflow,
    KwAgent,
    KwRecord,
    KwEnum,
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
    KwWhere,
    KwLet,
    KwReturn,
    KwTrue,
    KwFalse,
    KwIf,
    KwElse,
    KwWhile,
    KwMatch,
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
    Plus,
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
    FatArrow,
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

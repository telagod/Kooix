use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::ast::Program;
use crate::error::{Diagnostic, Span};
use crate::lexer;
use crate::parser;
use crate::token::{Token, TokenKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub path: PathBuf,
    pub source: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportEdge {
    pub raw: String,
    pub resolved: PathBuf,
    pub ns: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleNode {
    pub path: PathBuf,
    pub imports: Vec<ImportEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleGraph {
    pub entry: PathBuf,
    pub modules: Vec<ModuleNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedModule {
    pub path: PathBuf,
    pub program: Program,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMap {
    pub combined: String,
    pub files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn locate(&self, byte_index: usize) -> Option<&SourceFile> {
        self.files
            .iter()
            .find(|file| byte_index >= file.start && byte_index < file.end)
    }
}

pub fn load_source_map(entry: &Path) -> Result<SourceMap, Vec<Diagnostic>> {
    let (map, _) = load_source_map_with_module_graph(entry)?;
    Ok(map)
}

pub fn load_source_map_with_module_graph(
    entry: &Path,
) -> Result<(SourceMap, ModuleGraph), Vec<Diagnostic>> {
    let mut loader = Loader {
        combined: String::new(),
        files: Vec::new(),
        modules: Vec::new(),
        visited: HashSet::new(),
    };

    loader.load_file(entry)?;
    Ok((
        SourceMap {
            combined: loader.combined,
            files: loader.files,
        },
        ModuleGraph {
            entry: entry.to_path_buf(),
            modules: loader.modules,
        },
    ))
}

pub fn load_module_programs(
    entry: &Path,
) -> Result<(ModuleGraph, Vec<LoadedModule>), Vec<Diagnostic>> {
    let (map, graph) = load_source_map_with_module_graph(entry)?;

    let mut modules = Vec::new();
    for file in &map.files {
        let tokens = lexer::lex(&file.source)
            .map_err(|error| vec![qualify_diagnostic(&file.path, &file.source, error)])?;
        let program = parser::parse(&tokens)
            .map_err(|error| vec![qualify_diagnostic(&file.path, &file.source, error)])?;
        modules.push(LoadedModule {
            path: file.path.clone(),
            program,
        });
    }

    Ok((graph, modules))
}

struct Loader {
    combined: String,
    files: Vec<SourceFile>,
    modules: Vec<ModuleNode>,
    visited: HashSet<PathBuf>,
}

impl Loader {
    fn load_file(&mut self, path: &Path) -> Result<(), Vec<Diagnostic>> {
        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if self.visited.contains(&canonical) {
            return Ok(());
        }
        self.visited.insert(canonical.clone());

        let source = fs::read_to_string(path).map_err(|error| {
            vec![Diagnostic::error(
                format!("failed to read file '{}': {error}", path.display()),
                Span::new(0, 0),
            )]
        })?;

        let tokens =
            lexer::lex(&source).map_err(|error| vec![qualify_diagnostic(path, &source, error)])?;
        let imports = collect_import_specs(&tokens)
            .map_err(|error| vec![qualify_diagnostic(path, &source, error)])?;

        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        let mut edges = Vec::new();
        for import in imports {
            let import_path = resolve_import_path(base_dir, &import.path);
            edges.push(ImportEdge {
                raw: import.path,
                resolved: import_path.clone(),
                ns: import.ns,
            });
            self.load_file(&import_path)?;
        }

        self.modules.push(ModuleNode {
            path: canonical,
            imports: edges,
        });

        self.append_file(path, source);
        Ok(())
    }

    fn append_file(&mut self, path: &Path, mut source: String) {
        if !source.ends_with('\n') {
            source.push('\n');
        }
        source.push('\n');

        self.combined
            .push_str(&format!("// --- file: {} ---\n", path.display()));
        let start = self.combined.len();
        self.combined.push_str(&source);
        let end = self.combined.len();

        self.files.push(SourceFile {
            path: path.to_path_buf(),
            source,
            start,
            end,
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportSpec {
    path: String,
    ns: Option<String>,
}

fn collect_import_specs(tokens: &[Token]) -> Result<Vec<ImportSpec>, Diagnostic> {
    let mut imports: Vec<ImportSpec> = Vec::new();
    let mut depth: i32 = 0;
    let mut idx = 0usize;

    while let Some(token) = tokens.get(idx) {
        match &token.kind {
            TokenKind::LBrace | TokenKind::LParen | TokenKind::LBracket => {
                depth += 1;
            }
            TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            TokenKind::KwImport if depth == 0 => {
                let span = token.span;
                let Some(path_token) = tokens.get(idx + 1) else {
                    return Err(Diagnostic::error(
                        "import declaration is missing a string literal path",
                        span,
                    ));
                };
                let TokenKind::StringLiteral(path) = &path_token.kind else {
                    return Err(Diagnostic::error(
                        format!(
                            "import expects string literal path, found {}",
                            token_kind_name(&path_token.kind)
                        ),
                        path_token.span,
                    ));
                };
                let mut ns = None;
                let next = tokens.get(idx + 2).map(|token| &token.kind);
                let (end_idx, ok) = match next {
                    Some(TokenKind::Semicolon) => (idx + 3, true),
                    Some(TokenKind::KwAs) => {
                        let Some(ns_token) = tokens.get(idx + 3) else {
                            return Err(Diagnostic::error(
                                "import declaration is missing a namespace after 'as'",
                                span,
                            ));
                        };
                        if !matches!(&ns_token.kind, TokenKind::Ident(_)) {
                            return Err(Diagnostic::error(
                                format!(
                                    "import expects identifier after 'as', found {}",
                                    token_kind_name(&ns_token.kind)
                                ),
                                ns_token.span,
                            ));
                        }
                        if let TokenKind::Ident(name) = &ns_token.kind {
                            ns = Some(name.clone());
                        }
                        if !matches!(
                            tokens.get(idx + 4).map(|token| &token.kind),
                            Some(TokenKind::Semicolon)
                        ) {
                            return Err(Diagnostic::error(
                                "import declaration must end with ';'",
                                span,
                            ));
                        }
                        (idx + 5, true)
                    }
                    _ => (idx + 1, false),
                };

                if !ok {
                    return Err(Diagnostic::error(
                        "import declaration must end with ';'",
                        span,
                    ));
                }

                imports.push(ImportSpec {
                    path: path.clone(),
                    ns,
                });
                idx = end_idx;
                continue;
            }
            _ => {}
        }

        idx += 1;
    }

    Ok(imports)
}

fn resolve_import_path(base_dir: &Path, raw: &str) -> PathBuf {
    let candidate = Path::new(raw);
    let mut resolved = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base_dir.join(candidate)
    };

    if resolved.extension().is_none() {
        resolved.set_extension("kooix");
    }

    resolved
}

fn token_kind_name(kind: &TokenKind) -> &'static str {
    match kind {
        TokenKind::KwCap => "'cap'",
        TokenKind::KwImport => "'import'",
        TokenKind::KwAs => "'as'",
        TokenKind::KwFn => "'fn'",
        TokenKind::KwWorkflow => "'workflow'",
        TokenKind::KwAgent => "'agent'",
        TokenKind::KwRecord => "'record'",
        TokenKind::KwEnum => "'enum'",
        TokenKind::KwSteps => "'steps'",
        TokenKind::KwOnFail => "'on_fail'",
        TokenKind::KwOutput => "'output'",
        TokenKind::KwState => "'state'",
        TokenKind::KwPolicy => "'policy'",
        TokenKind::KwLoop => "'loop'",
        TokenKind::KwAllowTools => "'allow_tools'",
        TokenKind::KwDenyTools => "'deny_tools'",
        TokenKind::KwMaxIterations => "'max_iterations'",
        TokenKind::KwHumanInLoop => "'human_in_loop'",
        TokenKind::KwStop => "'stop'",
        TokenKind::KwWhen => "'when'",
        TokenKind::KwAny => "'any'",
        TokenKind::KwIntent => "'intent'",
        TokenKind::KwEnsures => "'ensures'",
        TokenKind::KwFailure => "'failure'",
        TokenKind::KwEvidence => "'evidence'",
        TokenKind::KwTrace => "'trace'",
        TokenKind::KwMetrics => "'metrics'",
        TokenKind::KwIn => "'in'",
        TokenKind::KwRequires => "'requires'",
        TokenKind::KwWhere => "'where'",
        TokenKind::KwLet => "'let'",
        TokenKind::KwReturn => "'return'",
        TokenKind::KwTrue => "'true'",
        TokenKind::KwFalse => "'false'",
        TokenKind::KwIf => "'if'",
        TokenKind::KwElse => "'else'",
        TokenKind::KwWhile => "'while'",
        TokenKind::KwMatch => "'match'",
        TokenKind::Ident(_) => "identifier",
        TokenKind::StringLiteral(_) => "string literal",
        TokenKind::Number(_) => "number",
        TokenKind::LParen => "'('",
        TokenKind::RParen => "')'",
        TokenKind::LBrace => "'{'",
        TokenKind::RBrace => "'}'",
        TokenKind::LBracket => "'['",
        TokenKind::RBracket => "']'",
        TokenKind::LAngle => "'<'",
        TokenKind::RAngle => "'>'",
        TokenKind::Comma => "','",
        TokenKind::Plus => "'+'",
        TokenKind::Dot => "'.'",
        TokenKind::Colon => "':'",
        TokenKind::ColonColon => "'::'",
        TokenKind::Semicolon => "';'",
        TokenKind::Bang => "'!'",
        TokenKind::Eq => "'='",
        TokenKind::EqEq => "'=='",
        TokenKind::NotEq => "'!='",
        TokenKind::Lte => "'<='",
        TokenKind::Gte => "'>='",
        TokenKind::Arrow => "'->'",
        TokenKind::FatArrow => "'=>'",
        TokenKind::Eof => "end of file",
    }
}

fn qualify_diagnostic(path: &Path, source: &str, diagnostic: Diagnostic) -> Diagnostic {
    let (line, col) = byte_to_line_col(source, diagnostic.span.start);
    let message = format!(
        "{}:{}:{}: {}",
        path.display(),
        line,
        col,
        diagnostic.message
    );
    Diagnostic {
        severity: diagnostic.severity,
        message,
        span: Span::new(0, 0),
    }
}

fn byte_to_line_col(source: &str, byte_index: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (index, ch) in source.char_indices() {
        if index >= byte_index {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

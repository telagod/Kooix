pub mod ast;
pub mod error;
pub mod hir;
pub mod interp;
pub mod lexer;
pub mod llvm;
pub mod loader;
pub mod mir;
pub mod module_check;
pub mod native;
pub mod normalize;
pub mod parser;
pub mod sema;
pub mod token;

use crate::error::Severity;
use ast::Program;
use error::Diagnostic;
use hir::HirProgram;
use mir::MirProgram;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleCheckResult {
    pub path: std::path::PathBuf,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn parse_source(source: &str) -> Result<Program, Vec<Diagnostic>> {
    let tokens = lexer::lex(source).map_err(|error| vec![error])?;
    parser::parse(&tokens).map_err(|error| vec![error])
}

pub fn check_source(source: &str) -> Vec<Diagnostic> {
    match parse_source(source) {
        Ok(program) => sema::check_program(&program),
        Err(parse_errors) => parse_errors,
    }
}

pub fn check_entry_modules(entry: &Path) -> Result<Vec<ModuleCheckResult>, Vec<Diagnostic>> {
    let (_graph, modules) = loader::load_module_programs(entry)?;
    let exports = module_check::build_export_index(&modules);

    let mut out = Vec::new();
    for module in modules {
        let (program, mut diagnostics) =
            module_check::prepare_program_for_module_check(&module, &_graph, &exports);
        diagnostics.extend(sema::check_program(&program));
        out.push(ModuleCheckResult {
            path: module.path,
            diagnostics,
        });
    }
    Ok(out)
}

pub fn lower_source(source: &str) -> Result<HirProgram, Vec<Diagnostic>> {
    let program = parse_source(source)?;
    Ok(hir::lower_program(&program))
}

pub fn lower_to_mir_source(source: &str) -> Result<MirProgram, Vec<Diagnostic>> {
    let program = parse_source(source)?;
    let mut diagnostics = sema::check_program(&program);
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(diagnostics);
    }

    let hir_program = hir::lower_program(&program);
    match mir::lower_hir(&hir_program) {
        Ok(mir_program) => Ok(mir_program),
        Err(mut lowering_errors) => {
            diagnostics.append(&mut lowering_errors);
            Err(diagnostics)
        }
    }
}

pub fn emit_llvm_ir_source(source: &str) -> Result<String, Vec<Diagnostic>> {
    let mir_program = lower_to_mir_source(source)?;
    Ok(llvm::emit_program(&mir_program))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
    pub value: interp::Value,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn run_source(source: &str) -> Result<RunResult, Vec<Diagnostic>> {
    let program = parse_source(source)?;
    let diagnostics = sema::check_program(&program);
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(diagnostics);
    }

    // Stage1 compiler (and future self-hosted tooling) can be deeply recursive when executed under
    // the Stage0 interpreter. Run it on a larger stack to avoid host-side stack overflows.
    let value = std::thread::Builder::new()
        .name("kooix-interp".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || interp::run_program(&program))
        .map_err(|error| {
            vec![Diagnostic::error(
                format!("failed to spawn interpreter thread: {error}"),
                crate::error::Span::new(0, 0),
            )]
        })?
        .join()
        .map_err(|_| {
            vec![Diagnostic::error(
                "interpreter thread panicked",
                crate::error::Span::new(0, 0),
            )]
        })?
        .map_err(|error| vec![error])?;
    Ok(RunResult { value, diagnostics })
}

pub fn compile_native_source(source: &str, output_path: &Path) -> Result<(), native::NativeError> {
    let llvm_ir = emit_llvm_ir_source(source).map_err(native::NativeError::Diagnostics)?;
    native::compile_llvm_ir_to_executable(&llvm_ir, output_path)
}

pub fn compile_and_run_native_source(
    source: &str,
    output_path: &Path,
) -> Result<native::RunOutput, native::NativeError> {
    compile_and_run_native_source_with_args(source, output_path, &[])
}

pub fn compile_and_run_native_source_with_args(
    source: &str,
    output_path: &Path,
    args: &[String],
) -> Result<native::RunOutput, native::NativeError> {
    compile_and_run_native_source_with_args_and_stdin(source, output_path, args, None)
}

pub fn compile_and_run_native_source_with_args_and_stdin(
    source: &str,
    output_path: &Path,
    args: &[String],
    stdin_data: Option<&[u8]>,
) -> Result<native::RunOutput, native::NativeError> {
    compile_and_run_native_source_with_args_stdin_and_timeout(
        source,
        output_path,
        args,
        stdin_data,
        None,
    )
}

pub fn compile_and_run_native_source_with_args_stdin_and_timeout(
    source: &str,
    output_path: &Path,
    args: &[String],
    stdin_data: Option<&[u8]>,
    timeout_ms: Option<u64>,
) -> Result<native::RunOutput, native::NativeError> {
    compile_native_source(source, output_path)?;
    native::run_executable_with_args_and_stdin_and_timeout(
        output_path,
        args,
        stdin_data,
        timeout_ms,
    )
}

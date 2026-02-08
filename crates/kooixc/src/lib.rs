pub mod ast;
pub mod error;
pub mod hir;
pub mod lexer;
pub mod llvm;
pub mod mir;
pub mod native;
pub mod parser;
pub mod sema;
pub mod token;

use crate::error::Severity;
use ast::{Item, Program};
use error::Diagnostic;
use hir::HirProgram;
use mir::MirProgram;
use std::path::Path;

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

    let mut unsupported = false;
    for item in &program.items {
        if let Item::Function(function) = item {
            if function.body.is_some() {
                unsupported = true;
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' has a body but MIR/LLVM lowering is not implemented yet",
                        function.name
                    ),
                    function.span,
                ));
            }
        }
    }
    if unsupported {
        return Err(diagnostics);
    }

    let hir_program = hir::lower_program(&program);
    Ok(mir::lower_hir(&hir_program))
}

pub fn emit_llvm_ir_source(source: &str) -> Result<String, Vec<Diagnostic>> {
    let mir_program = lower_to_mir_source(source)?;
    Ok(llvm::emit_program(&mir_program))
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

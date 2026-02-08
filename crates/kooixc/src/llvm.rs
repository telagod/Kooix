use std::fmt::Write;

use crate::ast::TypeRef;
use crate::mir::{MirFunction, MirProgram, MirTerminator};

pub fn emit_program(program: &MirProgram) -> String {
    let mut output = String::new();
    output.push_str("; ModuleID = 'kooix_mvp'\n");
    output.push_str("source_filename = \"kooix\"\n\n");

    for function in &program.functions {
        emit_function(function, &mut output);
        output.push('\n');
    }

    output
}

fn emit_function(function: &MirFunction, output: &mut String) {
    let return_type = llvm_type(&function.return_type);
    let params = function
        .params
        .iter()
        .map(|param| format!("{} %{}", llvm_type(&param.ty), sanitize_symbol(&param.name)))
        .collect::<Vec<_>>()
        .join(", ");

    let fn_name = sanitize_symbol(&function.name);
    let _ = writeln!(output, "define {return_type} @{fn_name}({params}) {{");

    if !function.effects.is_empty() {
        let _ = writeln!(output, "entry:");
        let _ = writeln!(output, "  ; effects: {}", function.effects.join(", "));
    } else {
        let _ = writeln!(output, "entry:");
    }

    match &function.entry.terminator {
        MirTerminator::ReturnDefault(ty) => {
            let _ = writeln!(output, "  {}", return_default_instruction(ty));
        }
    }

    let _ = writeln!(output, "}}");
}

fn llvm_type(ty: &TypeRef) -> &'static str {
    match ty.head() {
        "Unit" => "void",
        "Int" => "i64",
        "Bool" => "i1",
        "Float" => "double",
        "String" => "i8*",
        _ => "i8*",
    }
}

fn return_default_instruction(ty: &TypeRef) -> String {
    match llvm_type(ty) {
        "void" => "ret void".to_string(),
        "i64" => "ret i64 0".to_string(),
        "i1" => "ret i1 0".to_string(),
        "double" => "ret double 0.0".to_string(),
        _ => "ret i8* null".to_string(),
    }
}

fn sanitize_symbol(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

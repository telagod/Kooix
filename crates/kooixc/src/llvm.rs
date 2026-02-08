use std::collections::HashMap;
use std::fmt::Write;

use crate::ast::{BinaryOp, TypeRef};
use crate::mir::{
    MirBlock, MirFunction, MirOperand, MirProgram, MirRvalue, MirStatement, MirTerminator,
};

pub fn emit_program(program: &MirProgram) -> String {
    let mut output = String::new();
    output.push_str("; ModuleID = 'kooix_mvp'\n");
    output.push_str("source_filename = \"kooix\"\n\n");

    let signatures: HashMap<&str, (&TypeRef, Vec<&TypeRef>)> = program
        .functions
        .iter()
        .map(|function| {
            (
                function.name.as_str(),
                (
                    &function.return_type,
                    function.params.iter().map(|param| &param.ty).collect(),
                ),
            )
        })
        .collect();

    for function in &program.functions {
        emit_function(function, &signatures, &mut output);
        output.push('\n');
    }

    output
}

fn emit_function(
    function: &MirFunction,
    signatures: &HashMap<&str, (&TypeRef, Vec<&TypeRef>)>,
    output: &mut String,
) {
    let return_type = llvm_type(&function.return_type);
    let params = function
        .params
        .iter()
        .map(|param| format!("{} %{}", llvm_type(&param.ty), sanitize_symbol(&param.name)))
        .collect::<Vec<_>>()
        .join(", ");

    let fn_name = sanitize_symbol(&function.name);
    let _ = writeln!(output, "define {return_type} @{fn_name}({params}) {{");

    if function.blocks.is_empty() {
        let _ = writeln!(output, "entry:");
        let _ = writeln!(output, "  {}", return_default_instruction(&function.return_type));
        let _ = writeln!(output, "}}");
        return;
    }

    let local_ptrs: Vec<Option<String>> = function
        .locals
        .iter()
        .enumerate()
        .map(|(index, local)| {
            if llvm_type(&local.ty) == "void" {
                None
            } else {
                Some(format!("%l{index}"))
            }
        })
        .collect();

    let mut emitter = FunctionEmitter {
        function,
        signatures,
        local_ptrs,
        next_tmp: 0,
    };

    for (index, block) in function.blocks.iter().enumerate() {
        emitter.emit_block(block, index == 0, output);
    }

    let _ = writeln!(output, "}}");
}

struct FunctionEmitter<'a> {
    function: &'a MirFunction,
    signatures: &'a HashMap<&'a str, (&'a TypeRef, Vec<&'a TypeRef>)>,
    local_ptrs: Vec<Option<String>>,
    next_tmp: usize,
}

impl<'a> FunctionEmitter<'a> {
    fn emit_block(&mut self, block: &MirBlock, is_entry: bool, output: &mut String) {
        let _ = writeln!(output, "{}:", sanitize_label(&block.label));

        if is_entry {
            self.emit_allocas(output);
            if !self.function.effects.is_empty() {
                let _ = writeln!(output, "  ; effects: {}", self.function.effects.join(", "));
            }
        }

        for statement in &block.statements {
            self.emit_statement(statement, output);
        }

        self.emit_terminator(&block.terminator, output);
    }

    fn emit_allocas(&mut self, output: &mut String) {
        for (index, local) in self.function.locals.iter().enumerate() {
            let Some(ptr_name) = &self.local_ptrs[index] else {
                continue;
            };
            let ty = llvm_type(&local.ty);
            let _ = writeln!(output, "  {ptr_name} = alloca {ty}");
        }

        for param in &self.function.params {
            let Some(ptr_name) = &self.local_ptrs[param.local] else {
                continue;
            };
            let ty = llvm_type(&param.ty);
            let param_name = format!("%{}", sanitize_symbol(&param.name));
            let _ = writeln!(output, "  store {ty} {param_name}, {ty}* {ptr_name}");
        }
    }

    fn emit_statement(&mut self, statement: &MirStatement, output: &mut String) {
        match statement {
            MirStatement::Assign { dst, rvalue } => {
                let dst_ty = &self.function.locals[*dst].ty;
                if llvm_type(dst_ty) == "void" {
                    self.emit_rvalue(rvalue, output);
                    return;
                }

                let ptr_name = self.local_ptrs.get(*dst).and_then(|value| value.clone());
                let Some(ptr_name) = ptr_name else {
                    self.emit_rvalue(rvalue, output);
                    return;
                };
                let dst_llvm_ty = llvm_type(dst_ty);
                let value = self.emit_rvalue_value(rvalue, output);
                let _ = writeln!(
                    output,
                    "  store {dst_llvm_ty} {value}, {dst_llvm_ty}* {ptr_name}"
                );
            }
            MirStatement::Eval(rvalue) => {
                self.emit_rvalue(rvalue, output);
            }
        }
    }

    fn emit_terminator(&mut self, terminator: &MirTerminator, output: &mut String) {
        match terminator {
            MirTerminator::Return { value } => match value {
                None => {
                    let _ = writeln!(output, "  ret void");
                }
                Some(operand) => {
                    let ty = llvm_type(&self.function.return_type);
                    let value = self.emit_operand_value(operand, &self.function.return_type, output);
                    let _ = writeln!(output, "  ret {ty} {value}");
                }
            },
            MirTerminator::ReturnDefault(ty) => {
                let _ = writeln!(output, "  {}", return_default_instruction(ty));
            }
            MirTerminator::Goto { target } => {
                let _ = writeln!(output, "  br label %{}", sanitize_label(target));
            }
            MirTerminator::If {
                cond,
                then_bb,
                else_bb,
            } => {
                let cond_value = self.emit_operand_value(
                    cond,
                    &TypeRef {
                        name: "Bool".to_string(),
                        args: Vec::new(),
                    },
                    output,
                );
                let _ = writeln!(
                    output,
                    "  br i1 {cond_value}, label %{}, label %{}",
                    sanitize_label(then_bb),
                    sanitize_label(else_bb)
                );
            }
        }
    }

    fn emit_rvalue(&mut self, rvalue: &MirRvalue, output: &mut String) {
        let _ = self.emit_rvalue_value(rvalue, output);
    }

    fn emit_rvalue_value(&mut self, rvalue: &MirRvalue, output: &mut String) -> String {
        match rvalue {
            MirRvalue::Use(operand) => self.emit_operand_value(
                operand,
                &self.operand_type(operand),
                output,
            ),
            MirRvalue::Binary { op, left, right } => {
                let left_ty = self.operand_type(left);
                let right_ty = self.operand_type(right);

                let left_value = self.emit_operand_value(left, &left_ty, output);
                let right_value = self.emit_operand_value(right, &right_ty, output);

                let tmp = self.fresh_tmp();
                match op {
                    BinaryOp::Add => {
                        let _ = writeln!(output, "  {tmp} = add i64 {left_value}, {right_value}");
                    }
                    BinaryOp::Eq => {
                        let ty = llvm_type(&left_ty);
                        let _ = writeln!(
                            output,
                            "  {tmp} = icmp eq {ty} {left_value}, {right_value}"
                        );
                    }
                    BinaryOp::NotEq => {
                        let ty = llvm_type(&left_ty);
                        let _ = writeln!(
                            output,
                            "  {tmp} = icmp ne {ty} {left_value}, {right_value}"
                        );
                    }
                }
                tmp
            }
            MirRvalue::Call { callee, args } => {
                let Some((return_ty, param_tys)) = self
                    .signatures
                    .get(callee.as_str())
                    .map(|(ret, params)| (*ret, params.clone()))
                else {
                    // Should not happen after sema/mir validation.
                    return "0".to_string();
                };

                let default_param_ty = TypeRef {
                    name: "Int".to_string(),
                    args: Vec::new(),
                };
                let call_args = args
                    .iter()
                    .enumerate()
                    .map(|(index, arg)| {
                        let param_ty =
                            param_tys.get(index).copied().unwrap_or(&default_param_ty);
                        let value = self.emit_operand_value(arg, param_ty, output);
                        format!("{} {value}", llvm_type(param_ty))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");

                let fn_name = sanitize_symbol(callee);
                let ret_llvm_ty = llvm_type(return_ty);
                if ret_llvm_ty == "void" {
                    let _ = writeln!(output, "  call void @{fn_name}({call_args})");
                    "0".to_string()
                } else {
                    let tmp = self.fresh_tmp();
                    let _ =
                        writeln!(output, "  {tmp} = call {ret_llvm_ty} @{fn_name}({call_args})");
                    tmp
                }
            }
        }
    }

    fn operand_type(&self, operand: &MirOperand) -> TypeRef {
        match operand {
            MirOperand::ConstInt(_) => TypeRef {
                name: "Int".to_string(),
                args: Vec::new(),
            },
            MirOperand::ConstBool(_) => TypeRef {
                name: "Bool".to_string(),
                args: Vec::new(),
            },
            MirOperand::Local(index) => self
                .function
                .locals
                .get(*index)
                .map(|local| local.ty.clone())
                .unwrap_or(TypeRef {
                    name: "Int".to_string(),
                    args: Vec::new(),
                }),
        }
    }

    fn emit_operand_value(&mut self, operand: &MirOperand, ty: &TypeRef, output: &mut String) -> String {
        match operand {
            MirOperand::ConstInt(value) => value.to_string(),
            MirOperand::ConstBool(value) => {
                if *value {
                    "1".to_string()
                } else {
                    "0".to_string()
                }
            }
            MirOperand::Local(index) => {
                let Some(ptr_name) = self.local_ptrs.get(*index).and_then(|v| v.clone()) else {
                    return "0".to_string();
                };
                let llvm_ty = llvm_type(ty);
                let tmp = self.fresh_tmp();
                let _ = writeln!(output, "  {tmp} = load {llvm_ty}, {llvm_ty}* {ptr_name}");
                tmp
            }
        }
    }

    fn fresh_tmp(&mut self) -> String {
        let id = self.next_tmp;
        self.next_tmp += 1;
        format!("%t{id}")
    }
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

fn sanitize_label(raw: &str) -> String {
    sanitize_symbol(raw)
}

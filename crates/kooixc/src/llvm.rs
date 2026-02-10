use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use std::path::PathBuf;

use crate::ast::{BinaryOp, TypeRef};
use crate::loader::load_source_map;
use crate::mir::{
    MirBlock, MirEnum, MirFunction, MirOperand, MirProgram, MirRecord, MirRvalue,
    MirStatement, MirTerminator,
};

pub fn emit_program(program: &MirProgram) -> String {
    let mut output = String::new();
    output.push_str("; ModuleID = 'kooix_mvp'\n");
    output.push_str("source_filename = \"kooix\"\n\n");

    let records: HashMap<&str, &MirRecord> = program
        .records
        .iter()
        .map(|record| (record.name.as_str(), record))
        .collect();
    let enums: HashMap<&str, &MirEnum> = program
        .enums
        .iter()
        .map(|enum_decl| (enum_decl.name.as_str(), enum_decl))
        .collect();

    // Minimal runtime dependencies.
    output.push_str("declare i8* @malloc(i64)\n");
    output.push_str("declare i64 @strlen(i8*)\n");
    output.push_str("declare i32 @memcmp(i8*, i8*, i64)\n");
    output.push_str("declare i8* @memcpy(i8*, i8*, i64)\n");
    output.push_str("declare i32 @strcmp(i8*, i8*)\n\n");
    // Native host intrinsics (provided by crates/kooixc/native_runtime/runtime.c).
    output.push_str("declare i8* @kx_host_load_source_map(i8*)\n");
    output.push_str("declare void @kx_host_eprintln(i8*)\n\n");
    output.push_str("declare i8* @kx_host_write_file(i8*, i8*)\n\n");
    output.push_str("declare i8* @kx_text_concat(i8*, i8*)\n");
    output.push_str("declare i8* @kx_int_to_text(i64)\n\n");

    // String constants.
    let text_consts = collect_text_constants(program);
    for (name, bytes) in &text_consts {
        emit_text_constant(name, bytes, &mut output);
    }
    if !text_consts.is_empty() {
        output.push('\n');
    }

    for record in &program.records {
        emit_record_decl(record, &mut output);
    }
    for enum_decl in &program.enums {
        emit_enum_decl(enum_decl, &mut output);
    }
    if !program.records.is_empty() || !program.enums.is_empty() {
        output.push('\n');
    }

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

    let text_const_ptrs: HashMap<String, TextConstRef> = text_consts
        .iter()
        .map(|(name, bytes)| {
            (
                bytes_to_key(bytes),
                TextConstRef {
                    global: name.clone(),
                    len: bytes.len(),
                },
            )
        })
        .collect();

    for function in &program.functions {
        emit_function(
            function,
            &records,
            &enums,
            &signatures,
            &text_const_ptrs,
            &mut output,
        );
        output.push('\n');
    }

    output
}

#[derive(Debug, Clone)]
struct TextConstRef {
    global: String,
    len: usize,
}

fn emit_function(
    function: &MirFunction,
    records: &HashMap<&str, &MirRecord>,
    enums: &HashMap<&str, &MirEnum>,
    signatures: &HashMap<&str, (&TypeRef, Vec<&TypeRef>)>,
    text_consts: &HashMap<String, TextConstRef>,
    output: &mut String,
) {
    let return_type = llvm_type(&function.return_type, records, enums);
    let params = function
        .params
        .iter()
        .map(|param| {
            format!(
                "{} %{}",
                llvm_type(&param.ty, records, enums),
                sanitize_symbol(&param.name)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    let fn_name = sanitize_symbol(&function.name);
    let _ = writeln!(output, "define {return_type} @{fn_name}({params}) {{");

    if function.blocks.is_empty() {
        let _ = writeln!(output, "entry:");
        let _ = writeln!(
            output,
            "  {}",
            return_default_instruction(&function.return_type, records, enums)
        );
        let _ = writeln!(output, "}}");
        return;
    }

    let local_ptrs: Vec<Option<String>> = function
        .locals
        .iter()
        .enumerate()
        .map(|(index, local)| {
            if llvm_type(&local.ty, records, enums) == "void" {
                None
            } else {
                Some(format!("%l{index}"))
            }
        })
        .collect();

    let mut emitter = FunctionEmitter {
        function,
        records,
        enums,
        signatures,
        text_consts,
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
    records: &'a HashMap<&'a str, &'a MirRecord>,
    enums: &'a HashMap<&'a str, &'a MirEnum>,
    signatures: &'a HashMap<&'a str, (&'a TypeRef, Vec<&'a TypeRef>)>,
    text_consts: &'a HashMap<String, TextConstRef>,
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
            let ty = llvm_type(&local.ty, self.records, self.enums);
            let _ = writeln!(output, "  {ptr_name} = alloca {ty}");
        }

        for param in &self.function.params {
            let Some(ptr_name) = &self.local_ptrs[param.local] else {
                continue;
            };
            let ty = llvm_type(&param.ty, self.records, self.enums);
            let param_name = format!("%{}", sanitize_symbol(&param.name));
            let _ = writeln!(output, "  store {ty} {param_name}, {ty}* {ptr_name}");
        }
    }

    fn emit_statement(&mut self, statement: &MirStatement, output: &mut String) {
        match statement {
            MirStatement::Assign { dst, rvalue } => {
                let dst_ty = &self.function.locals[*dst].ty;
                if llvm_type(dst_ty, self.records, self.enums) == "void" {
                    self.emit_rvalue(rvalue, output);
                    return;
                }

                let ptr_name = self.local_ptrs.get(*dst).and_then(|value| value.clone());
                let Some(ptr_name) = ptr_name else {
                    self.emit_rvalue(rvalue, output);
                    return;
                };
                let dst_llvm_ty = llvm_type(dst_ty, self.records, self.enums);
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
                    let ty = llvm_type(&self.function.return_type, self.records, self.enums);
                    let value =
                        self.emit_operand_value(operand, &self.function.return_type, output);
                    let _ = writeln!(output, "  ret {ty} {value}");
                }
            },
            MirTerminator::ReturnDefault(ty) => {
                let _ = writeln!(
                    output,
                    "  {}",
                    return_default_instruction(ty, self.records, self.enums)
                );
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
            MirRvalue::Use(operand) => {
                self.emit_operand_value(operand, &self.operand_type(operand), output)
            }
            MirRvalue::Binary { op, left, right } => {
                let left_ty = self.operand_type(left);
                let right_ty = self.operand_type(right);

                let left_value = self.emit_operand_value(left, &left_ty, output);
                let right_value = self.emit_operand_value(right, &right_ty, output);

                let tmp = self.fresh_tmp();
                match op {
                    BinaryOp::Add => {
                        let _ = writeln!(output, "  {tmp} = add i64 {left_value}, {right_value}");
                        tmp
                    }
                    BinaryOp::Eq | BinaryOp::NotEq => {
                        let head = left_ty.head();
                        if head == "Text" {
                            let cmp = self.fresh_tmp();
                            let _ = writeln!(
                                output,
                                "  {cmp} = call i32 @strcmp(i8* {left_value}, i8* {right_value})"
                            );
                            let pred = if matches!(op, BinaryOp::Eq) { "eq" } else { "ne" };
                            let _ = writeln!(output, "  {tmp} = icmp {pred} i32 {cmp}, 0");
                            tmp
                        } else if self.enums.contains_key(head) {
                            let eqv = self.emit_enum_eq(head, &left_value, &right_value, output);
                            if matches!(op, BinaryOp::Eq) {
                                eqv
                            } else {
                                let inv = self.fresh_tmp();
                                let _ = writeln!(output, "  {inv} = xor i1 {eqv}, 1");
                                inv
                            }
                        } else {
                            let ty = llvm_type(&left_ty, self.records, self.enums);
                            let pred = if matches!(op, BinaryOp::Eq) { "eq" } else { "ne" };
                            let _ = writeln!(
                                output,
                                "  {tmp} = icmp {pred} {ty} {left_value}, {right_value}"
                            );
                            tmp
                        }
                    }
                }
            }
            MirRvalue::Call { callee, args } => self.emit_call_value(callee, args, output),
            MirRvalue::RecordLit {
                record,
                fields,
                field_tys,
            } => self.emit_record_lit(record, fields, field_tys, output),
            MirRvalue::ProjectField {
                base,
                record,
                index,
                field_ty,
            } => self.emit_project_field(base, record, *index, field_ty, output),
            MirRvalue::EnumLit {
                enum_name,
                tag,
                payload,
                payload_ty,
            } => self.emit_enum_lit(enum_name, *tag, payload.as_ref(), payload_ty.as_ref(), output),
            MirRvalue::EnumTag { base, enum_name } => self.emit_enum_tag(base, enum_name, output),
            MirRvalue::EnumPayload {
                base,
                enum_name,
                payload_ty,
            } => self.emit_enum_payload(base, enum_name, payload_ty, output),
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
            MirOperand::ConstText(value) => {
                // Text constants are tracked in the module with a trailing NUL byte (C string).
                // Keep the lookup key consistent with `collect_text_bytes_in_operand`.
                let mut bytes = value.as_bytes().to_vec();
                bytes.push(0);
                let key = bytes_to_key(&bytes);
                let Some(info) = self.text_consts.get(&key) else {
                    return "null".to_string();
                };
                let n = info.len;
                let tmp = self.fresh_tmp();
                let _ = writeln!(
                    output,
                    "  {tmp} = getelementptr inbounds [{n} x i8], [{n} x i8]* {}, i64 0, i64 0",
                    info.global
                );
                tmp
            }
            MirOperand::Local(index) => {
                let Some(ptr_name) = self.local_ptrs.get(*index).and_then(|v| v.clone()) else {
                    return "0".to_string();
                };
                let llvm_ty = llvm_type(ty, self.records, self.enums);
                let tmp = self.fresh_tmp();
                let _ = writeln!(output, "  {tmp} = load {llvm_ty}, {llvm_ty}* {ptr_name}");
                tmp
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
            MirOperand::ConstText(_) => TypeRef {
                name: "Text".to_string(),
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

    fn emit_call_value(&mut self, callee: &str, args: &[MirOperand], output: &mut String) -> String {
        // Intrinsics (native runtime).
        if let Some(value) = self.emit_intrinsic_call(callee, args, output) {
            return value;
        }

        let Some((return_ty, param_tys)) = self
            .signatures
            .get(callee)
            .map(|(ret, params)| (*ret, params.clone()))
        else {
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
                let param_ty = param_tys.get(index).copied().unwrap_or(&default_param_ty);
                let value = self.emit_operand_value(arg, param_ty, output);
                format!("{} {value}", llvm_type(param_ty, self.records, self.enums))
            })
            .collect::<Vec<_>>()
            .join(", ");

        let fn_name = sanitize_symbol(callee);
        let ret_llvm_ty = llvm_type(return_ty, self.records, self.enums);
        if ret_llvm_ty == "void" {
            let _ = writeln!(output, "  call void @{fn_name}({call_args})");
            "0".to_string()
        } else {
            let tmp = self.fresh_tmp();
            let _ = writeln!(output, "  {tmp} = call {ret_llvm_ty} @{fn_name}({call_args})");
            tmp
        }
    }

    fn emit_intrinsic_call(
        &mut self,
        callee: &str,
        args: &[MirOperand],
        output: &mut String,
    ) -> Option<String> {
        match callee {
            "text_len" => {
                let [s] = args else { return Some("0".to_string()) };
                let sv = self.emit_operand_value(s, &TypeRef { name: "Text".to_string(), args: Vec::new() }, output);
                let tmp = self.fresh_tmp();
                let _ = writeln!(output, "  {tmp} = call i64 @strlen(i8* {sv})");
                Some(tmp)
            }
            "byte_is_ascii_whitespace" => {
                let [b] = args else { return Some("0".to_string()) };
                let bv = self.emit_operand_value(b, &TypeRef { name: "Int".to_string(), args: Vec::new() }, output);
                Some(self.emit_ascii_whitespace(&bv, output))
            }
            "byte_is_ascii_digit" => {
                let [b] = args else { return Some("0".to_string()) };
                let bv = self.emit_operand_value(b, &TypeRef { name: "Int".to_string(), args: Vec::new() }, output);
                Some(self.emit_ascii_digit(&bv, output))
            }
            "byte_is_ascii_alpha" => {
                let [b] = args else { return Some("0".to_string()) };
                let bv = self.emit_operand_value(b, &TypeRef { name: "Int".to_string(), args: Vec::new() }, output);
                Some(self.emit_ascii_alpha(&bv, output))
            }
            "byte_is_ascii_alnum" => {
                let [b] = args else { return Some("0".to_string()) };
                let bv = self.emit_operand_value(b, &TypeRef { name: "Int".to_string(), args: Vec::new() }, output);
                let a = self.emit_ascii_alpha(&bv, output);
                let d = self.emit_ascii_digit(&bv, output);
                Some(self.emit_or_i1(&a, &d, output))
            }
            "byte_is_ascii_ident_start" => {
                let [b] = args else { return Some("0".to_string()) };
                let bv = self.emit_operand_value(b, &TypeRef { name: "Int".to_string(), args: Vec::new() }, output);
                let a = self.emit_ascii_alpha(&bv, output);
                let u = self.emit_eq_i64(&bv, 95, output);
                Some(self.emit_or_i1(&a, &u, output))
            }
            "byte_is_ascii_ident_continue" => {
                let [b] = args else { return Some("0".to_string()) };
                let bv = self.emit_operand_value(b, &TypeRef { name: "Int".to_string(), args: Vec::new() }, output);
                let a = self.emit_ascii_alpha(&bv, output);
                let d = self.emit_ascii_digit(&bv, output);
                let ad = self.emit_or_i1(&a, &d, output);
                let u = self.emit_eq_i64(&bv, 95, output);
                Some(self.emit_or_i1(&ad, &u, output))
            }
            "text_starts_with" => {
                let [s, prefix] = args else { return Some("0".to_string()) };
                let s_val = self.emit_operand_value(s, &TypeRef { name: "Text".to_string(), args: Vec::new() }, output);
                let p_val = self.emit_operand_value(prefix, &TypeRef { name: "Text".to_string(), args: Vec::new() }, output);
                let plen = self.fresh_tmp();
                let _ = writeln!(output, "  {plen} = call i64 @strlen(i8* {p_val})");
                let slen = self.fresh_tmp();
                let _ = writeln!(output, "  {slen} = call i64 @strlen(i8* {s_val})");
                let ge = self.fresh_tmp();
                let _ = writeln!(output, "  {ge} = icmp uge i64 {slen}, {plen}");

                // Important: only call memcmp when slen >= plen. Otherwise we'd read past the end
                // of `s` (UB), and Stage1's lexer relies on deterministic `starts_with` checks.
                let ok_bb = self.fresh_tmp_label("bb_ok");
                let join_bb = self.fresh_tmp_label("bb_join");
                let res_slot = self.fresh_tmp();
                let _ = writeln!(output, "  {res_slot} = alloca i1");
                let _ = writeln!(output, "  store i1 0, i1* {res_slot}");
                let _ = writeln!(output, "  br i1 {ge}, label %{ok_bb}, label %{join_bb}");

                let _ = writeln!(output, "{ok_bb}:");
                let cmp = self.fresh_tmp();
                let _ = writeln!(
                    output,
                    "  {cmp} = call i32 @memcmp(i8* {s_val}, i8* {p_val}, i64 {plen})"
                );
                let eq0 = self.fresh_tmp();
                let _ = writeln!(output, "  {eq0} = icmp eq i32 {cmp}, 0");
                let _ = writeln!(output, "  store i1 {eq0}, i1* {res_slot}");
                let _ = writeln!(output, "  br label %{join_bb}");

                let _ = writeln!(output, "{join_bb}:");
                let outv = self.fresh_tmp();
                let _ = writeln!(output, "  {outv} = load i1, i1* {res_slot}");
                Some(outv)
            }
            "text_byte_at" => {
                // Returns Option<Int>
                let [s, index] = args else { return Some("null".to_string()) };
                let s_val = self.emit_operand_value(s, &TypeRef { name: "Text".to_string(), args: Vec::new() }, output);
                let idx_val = self.emit_operand_value(index, &TypeRef { name: "Int".to_string(), args: Vec::new() }, output);

                let idx_neg = self.fresh_tmp();
                let _ = writeln!(output, "  {idx_neg} = icmp slt i64 {idx_val}, 0");

                let len = self.fresh_tmp();
                let _ = writeln!(output, "  {len} = call i64 @strlen(i8* {s_val})");
                let idx_uge = self.fresh_tmp();
                let _ = writeln!(output, "  {idx_uge} = icmp uge i64 {idx_val}, {len}");
                let oob = self.fresh_tmp();
                let _ = writeln!(output, "  {oob} = or i1 {idx_neg}, {idx_uge}");

                let ok_bb = self.fresh_tmp_label("bb_ok");
                let none_bb = self.fresh_tmp_label("bb_none");
                let join_bb = self.fresh_tmp_label("bb_join");
                let res_ptr = self.fresh_tmp();

                // Allocate a stack slot for the result pointer (Option*).
                let opt_ty = TypeRef { name: "Option".to_string(), args: Vec::new() };
                let opt_ptr_ty = llvm_type(&opt_ty, self.records, self.enums);
                let res_slot = self.fresh_tmp();
                let _ = writeln!(output, "  {res_slot} = alloca {opt_ptr_ty}");

                let _ = writeln!(
                    output,
                    "  br i1 {oob}, label %{none_bb}, label %{ok_bb}"
                );

                // ok_bb: Some(byte)
                let _ = writeln!(output, "{ok_bb}:");
                let gep = self.fresh_tmp();
                let _ = writeln!(output, "  {gep} = getelementptr inbounds i8, i8* {s_val}, i64 {idx_val}");
                let b = self.fresh_tmp();
                let _ = writeln!(output, "  {b} = load i8, i8* {gep}");
                let bz = self.fresh_tmp();
                let _ = writeln!(output, "  {bz} = zext i8 {b} to i64");
                let some_tag = self.lookup_enum_tag("Option", "Some").unwrap_or(0);
                let some_ptr = self.emit_enum_alloc("Option", some_tag, &bz, output);
                let _ = writeln!(output, "  store {opt_ptr_ty} {some_ptr}, {opt_ptr_ty}* {res_slot}");
                let _ = writeln!(output, "  br label %{join_bb}");

                // none_bb: None
                let _ = writeln!(output, "{none_bb}:");
                let none_tag = self.lookup_enum_tag("Option", "None").unwrap_or(1);
                let zero = "0".to_string();
                let none_ptr = self.emit_enum_alloc("Option", none_tag, &zero, output);
                let _ = writeln!(output, "  store {opt_ptr_ty} {none_ptr}, {opt_ptr_ty}* {res_slot}");
                let _ = writeln!(output, "  br label %{join_bb}");

                // join
                let _ = writeln!(output, "{join_bb}:");
                let _ = writeln!(output, "  {res_ptr} = load {opt_ptr_ty}, {opt_ptr_ty}* {res_slot}");
                Some(res_ptr)
            }
            "text_slice" => {
                // Returns Option<Text>
                let [s, start, end] = args else { return Some("null".to_string()) };
                let s_val = self.emit_operand_value(s, &TypeRef { name: "Text".to_string(), args: Vec::new() }, output);
                let start_val = self.emit_operand_value(start, &TypeRef { name: "Int".to_string(), args: Vec::new() }, output);
                let end_val = self.emit_operand_value(end, &TypeRef { name: "Int".to_string(), args: Vec::new() }, output);

                let start_neg = self.fresh_tmp();
                let _ = writeln!(output, "  {start_neg} = icmp slt i64 {start_val}, 0");
                let end_neg = self.fresh_tmp();
                let _ = writeln!(output, "  {end_neg} = icmp slt i64 {end_val}, 0");
                let neg = self.fresh_tmp();
                let _ = writeln!(output, "  {neg} = or i1 {start_neg}, {end_neg}");
                let gt = self.fresh_tmp();
                let _ = writeln!(output, "  {gt} = icmp sgt i64 {start_val}, {end_val}");
                let bad1 = self.fresh_tmp();
                let _ = writeln!(output, "  {bad1} = or i1 {neg}, {gt}");

                let len = self.fresh_tmp();
                let _ = writeln!(output, "  {len} = call i64 @strlen(i8* {s_val})");
                let end_oob = self.fresh_tmp();
                let _ = writeln!(output, "  {end_oob} = icmp ugt i64 {end_val}, {len}");
                let bad = self.fresh_tmp();
                let _ = writeln!(output, "  {bad} = or i1 {bad1}, {end_oob}");

                let ok_bb = self.fresh_tmp_label("bb_ok");
                let none_bb = self.fresh_tmp_label("bb_none");
                let join_bb = self.fresh_tmp_label("bb_join");

                let opt_ty = TypeRef { name: "Option".to_string(), args: Vec::new() };
                let opt_ptr_ty = llvm_type(&opt_ty, self.records, self.enums);
                let res_slot = self.fresh_tmp();
                let _ = writeln!(output, "  {res_slot} = alloca {opt_ptr_ty}");
                let _ = writeln!(output, "  br i1 {bad}, label %{none_bb}, label %{ok_bb}");

                let _ = writeln!(output, "{ok_bb}:");
                let size = self.fresh_tmp();
                let _ = writeln!(output, "  {size} = sub i64 {end_val}, {start_val}");
                let size1 = self.fresh_tmp();
                let _ = writeln!(output, "  {size1} = add i64 {size}, 1");
                let buf = self.fresh_tmp();
                let _ = writeln!(output, "  {buf} = call i8* @malloc(i64 {size1})");
                let src = self.fresh_tmp();
                let _ = writeln!(output, "  {src} = getelementptr inbounds i8, i8* {s_val}, i64 {start_val}");
                let _ = writeln!(output, "  call i8* @memcpy(i8* {buf}, i8* {src}, i64 {size})");
                let nul_ptr = self.fresh_tmp();
                let _ = writeln!(output, "  {nul_ptr} = getelementptr inbounds i8, i8* {buf}, i64 {size}");
                let _ = writeln!(output, "  store i8 0, i8* {nul_ptr}");
                let some_tag = self.lookup_enum_tag("Option", "Some").unwrap_or(0);
                let buf_word = self.ptr_to_word("i8*", &buf, output);
                let some_ptr = self.emit_enum_alloc("Option", some_tag, &buf_word, output);
                let _ = writeln!(output, "  store {opt_ptr_ty} {some_ptr}, {opt_ptr_ty}* {res_slot}");
                let _ = writeln!(output, "  br label %{join_bb}");

                let _ = writeln!(output, "{none_bb}:");
                let none_tag = self.lookup_enum_tag("Option", "None").unwrap_or(1);
                let zero = "0".to_string();
                let none_ptr = self.emit_enum_alloc("Option", none_tag, &zero, output);
                let _ = writeln!(output, "  store {opt_ptr_ty} {none_ptr}, {opt_ptr_ty}* {res_slot}");
                let _ = writeln!(output, "  br label %{join_bb}");

                let _ = writeln!(output, "{join_bb}:");
                let res = self.fresh_tmp();
                let _ = writeln!(output, "  {res} = load {opt_ptr_ty}, {opt_ptr_ty}* {res_slot}");
                Some(res)
            }
            "host_load_source_map" => {
                // Host-only helper. For the native backend, we evaluate this at compile time when
                // the path is a string literal. Otherwise, it calls into the linked native runtime.
                let [path_op] = args else { return Some("null".to_string()) };

                if let MirOperand::ConstText(path) = path_op {
                    // Compile-time fold for deterministic bootstrap.
                    let (tag, payload_text) = match native_load_source_map(path) {
                        Ok(source) => (self.lookup_enum_tag("Result", "Ok").unwrap_or(0), source),
                        Err(message) => (self.lookup_enum_tag("Result", "Err").unwrap_or(1), message),
                    };
                    let payload_ptr = self.emit_text_ptr_for_value(&payload_text, output);
                    let payload_word = self.ptr_to_word("i8*", &payload_ptr, output);
                    let res_ptr = self.emit_enum_alloc("Result", tag, &payload_word, output);
                    return Some(res_ptr);
                }

                // Runtime path.
                let path_val = self.emit_operand_value(
                    path_op,
                    &TypeRef {
                        name: "Text".to_string(),
                        args: Vec::new(),
                    },
                    output,
                );
                let raw = self.fresh_tmp();
                let _ = writeln!(output, "  {raw} = call i8* @kx_host_load_source_map(i8* {path_val})");
                let res_ty = llvm_type(
                    &TypeRef {
                        name: "Result".to_string(),
                        args: Vec::new(),
                    },
                    self.records,
                    self.enums,
                );
                let cast = self.fresh_tmp();
                let _ = writeln!(output, "  {cast} = bitcast i8* {raw} to {res_ty}");
                Some(cast)
            }
            "host_eprintln" => {
                let [s] = args else { return Some("0".to_string()) };
                let sv = self.emit_operand_value(
                    s,
                    &TypeRef {
                        name: "Text".to_string(),
                        args: Vec::new(),
                    },
                    output,
                );
                let _ = writeln!(output, "  call void @kx_host_eprintln(i8* {sv})");
                Some("0".to_string())
            }
            "host_write_file" => {
                // Runtime file write, returns Result<Int, Text>.
                let [path, content] = args else { return Some("null".to_string()) };
                let pv = self.emit_operand_value(
                    path,
                    &TypeRef {
                        name: "Text".to_string(),
                        args: Vec::new(),
                    },
                    output,
                );
                let cv = self.emit_operand_value(
                    content,
                    &TypeRef {
                        name: "Text".to_string(),
                        args: Vec::new(),
                    },
                    output,
                );
                let raw = self.fresh_tmp();
                let _ = writeln!(
                    output,
                    "  {raw} = call i8* @kx_host_write_file(i8* {pv}, i8* {cv})"
                );
                let res_ty = llvm_type(
                    &TypeRef {
                        name: "Result".to_string(),
                        args: Vec::new(),
                    },
                    self.records,
                    self.enums,
                );
                let cast = self.fresh_tmp();
                let _ = writeln!(output, "  {cast} = bitcast i8* {raw} to {res_ty}");
                Some(cast)
            }
            "text_concat" => {
                let [a, b] = args else { return Some("null".to_string()) };
                let av = self.emit_operand_value(
                    a,
                    &TypeRef {
                        name: "Text".to_string(),
                        args: Vec::new(),
                    },
                    output,
                );
                let bv = self.emit_operand_value(
                    b,
                    &TypeRef {
                        name: "Text".to_string(),
                        args: Vec::new(),
                    },
                    output,
                );
                let tmp = self.fresh_tmp();
                let _ = writeln!(output, "  {tmp} = call i8* @kx_text_concat(i8* {av}, i8* {bv})");
                Some(tmp)
            }
            "int_to_text" => {
                let [i] = args else { return Some("null".to_string()) };
                let iv = self.emit_operand_value(
                    i,
                    &TypeRef {
                        name: "Int".to_string(),
                        args: Vec::new(),
                    },
                    output,
                );
                let tmp = self.fresh_tmp();
                let _ = writeln!(output, "  {tmp} = call i8* @kx_int_to_text(i64 {iv})");
                Some(tmp)
            }
            _ => None,
        }
    }

    fn emit_text_ptr_for_value(&mut self, value: &str, output: &mut String) -> String {
        let mut bytes = value.as_bytes().to_vec();
        bytes.push(0);
        let key = bytes_to_key(&bytes);
        let Some(info) = self.text_consts.get(&key) else {
            return "null".to_string();
        };
        let n = info.len;
        let tmp = self.fresh_tmp();
        let _ = writeln!(
            output,
            "  {tmp} = getelementptr inbounds [{n} x i8], [{n} x i8]* {}, i64 0, i64 0",
            info.global
        );
        tmp
    }

    fn emit_eq_i64(&mut self, lhs: &str, rhs: i64, output: &mut String) -> String {
        let tmp = self.fresh_tmp();
        let _ = writeln!(output, "  {tmp} = icmp eq i64 {lhs}, {rhs}");
        tmp
    }

    fn emit_or_i1(&mut self, a: &str, b: &str, output: &mut String) -> String {
        let tmp = self.fresh_tmp();
        let _ = writeln!(output, "  {tmp} = or i1 {a}, {b}");
        tmp
    }

    fn emit_and_i1(&mut self, a: &str, b: &str, output: &mut String) -> String {
        let tmp = self.fresh_tmp();
        let _ = writeln!(output, "  {tmp} = and i1 {a}, {b}");
        tmp
    }

    fn emit_range_i64(&mut self, v: &str, lo: i64, hi: i64, output: &mut String) -> String {
        let ge = self.fresh_tmp();
        let _ = writeln!(output, "  {ge} = icmp sge i64 {v}, {lo}");
        let le = self.fresh_tmp();
        let _ = writeln!(output, "  {le} = icmp sle i64 {v}, {hi}");
        self.emit_and_i1(&ge, &le, output)
    }

    fn emit_ascii_whitespace(&mut self, v: &str, output: &mut String) -> String {
        let t = self.emit_eq_i64(v, 9, output);
        let n = self.emit_eq_i64(v, 10, output);
        let r = self.emit_eq_i64(v, 13, output);
        let s = self.emit_eq_i64(v, 32, output);
        let tn = self.emit_or_i1(&t, &n, output);
        let rr = self.emit_or_i1(&r, &s, output);
        self.emit_or_i1(&tn, &rr, output)
    }

    fn emit_ascii_digit(&mut self, v: &str, output: &mut String) -> String {
        self.emit_range_i64(v, 48, 57, output)
    }

    fn emit_ascii_alpha(&mut self, v: &str, output: &mut String) -> String {
        let az = self.emit_range_i64(v, 65, 90, output);
        let zz = self.emit_range_i64(v, 97, 122, output);
        self.emit_or_i1(&az, &zz, output)
    }

    fn emit_record_lit(
        &mut self,
        record: &str,
        fields: &[MirOperand],
        field_tys: &[TypeRef],
        output: &mut String,
    ) -> String {
        let Some(schema) = self.records.get(record) else {
            return "null".to_string();
        };

        let rec_ty = llvm_named_record_type(record);
        let rec_ptr_ty = format!("{rec_ty}*");

        let size = self.emit_sizeof(&rec_ty, output);
        let raw = self.fresh_tmp();
        let _ = writeln!(output, "  {raw} = call i8* @malloc(i64 {size})");
        let ptr = self.fresh_tmp();
        let _ = writeln!(output, "  {ptr} = bitcast i8* {raw} to {rec_ptr_ty}");

        let default_field_ty = TypeRef {
            name: "Int".to_string(),
            args: Vec::new(),
        };

        for (index, operand) in fields.iter().enumerate() {
            let field_ty = field_tys
                .get(index)
                .or_else(|| schema.fields.get(index).map(|f| &f.ty))
                .unwrap_or(&default_field_ty);
            let value = self.emit_operand_value(operand, field_ty, output);
            let word = self.emit_value_to_word(&value, field_ty, output);
            let gep = self.fresh_tmp();
            let _ = writeln!(
                output,
                "  {gep} = getelementptr inbounds {rec_ty}, {rec_ptr_ty} {ptr}, i32 0, i32 {index}"
            );
            let _ = writeln!(output, "  store i64 {word}, i64* {gep}");
        }

        ptr
    }

    fn emit_project_field(
        &mut self,
        base: &MirOperand,
        record: &str,
        index: usize,
        field_ty: &TypeRef,
        output: &mut String,
    ) -> String {
        let rec_ty = llvm_named_record_type(record);
        let rec_ptr_ty = format!("{rec_ty}*");
        let base_ty = TypeRef {
            name: record.to_string(),
            args: Vec::new(),
        };
        let base_value = self.emit_operand_value(base, &base_ty, output);

        let gep = self.fresh_tmp();
        let _ = writeln!(
            output,
            "  {gep} = getelementptr inbounds {rec_ty}, {rec_ptr_ty} {base_value}, i32 0, i32 {index}"
        );
        let word = self.fresh_tmp();
        let _ = writeln!(output, "  {word} = load i64, i64* {gep}");

        self.emit_word_to_value(&word, field_ty, output)
    }

    fn emit_enum_lit(
        &mut self,
        enum_name: &str,
        tag: u8,
        payload: Option<&MirOperand>,
        payload_ty: Option<&TypeRef>,
        output: &mut String,
    ) -> String {
        let default_payload_ty = TypeRef {
            name: "Int".to_string(),
            args: Vec::new(),
        };

        let payload_word = if let Some(op) = payload {
            let pty = payload_ty.unwrap_or(&default_payload_ty);
            let v = self.emit_operand_value(op, pty, output);
            self.emit_value_to_word(&v, pty, output)
        } else {
            "0".to_string()
        };
        self.emit_enum_alloc(enum_name, tag, &payload_word, output)
    }

    fn emit_enum_alloc(&mut self, enum_name: &str, tag: u8, payload_word: &str, output: &mut String) -> String {
        let en_ty = llvm_named_enum_type(enum_name);
        let en_ptr_ty = format!("{en_ty}*");

        let size = self.emit_sizeof(&en_ty, output);
        let raw = self.fresh_tmp();
        let _ = writeln!(output, "  {raw} = call i8* @malloc(i64 {size})");
        let ptr = self.fresh_tmp();
        let _ = writeln!(output, "  {ptr} = bitcast i8* {raw} to {en_ptr_ty}");

        let tag_ptr = self.fresh_tmp();
        let _ = writeln!(
            output,
            "  {tag_ptr} = getelementptr inbounds {en_ty}, {en_ptr_ty} {ptr}, i32 0, i32 0"
        );
        let _ = writeln!(output, "  store i8 {tag}, i8* {tag_ptr}");

        let payload_ptr = self.fresh_tmp();
        let _ = writeln!(
            output,
            "  {payload_ptr} = getelementptr inbounds {en_ty}, {en_ptr_ty} {ptr}, i32 0, i32 1"
        );
        let _ = writeln!(output, "  store i64 {payload_word}, i64* {payload_ptr}");

        ptr
    }

    fn emit_enum_tag(&mut self, base: &MirOperand, enum_name: &str, output: &mut String) -> String {
        let en_ty = llvm_named_enum_type(enum_name);
        let en_ptr_ty = format!("{en_ty}*");
        let base_ty = TypeRef {
            name: enum_name.to_string(),
            args: Vec::new(),
        };
        let base_value = self.emit_operand_value(base, &base_ty, output);

        let tag_ptr = self.fresh_tmp();
        let _ = writeln!(
            output,
            "  {tag_ptr} = getelementptr inbounds {en_ty}, {en_ptr_ty} {base_value}, i32 0, i32 0"
        );
        let tag8 = self.fresh_tmp();
        let _ = writeln!(output, "  {tag8} = load i8, i8* {tag_ptr}");
        let tag64 = self.fresh_tmp();
        let _ = writeln!(output, "  {tag64} = zext i8 {tag8} to i64");
        tag64
    }

    fn emit_enum_payload(&mut self, base: &MirOperand, enum_name: &str, payload_ty: &TypeRef, output: &mut String) -> String {
        let en_ty = llvm_named_enum_type(enum_name);
        let en_ptr_ty = format!("{en_ty}*");
        let base_ty = TypeRef {
            name: enum_name.to_string(),
            args: Vec::new(),
        };
        let base_value = self.emit_operand_value(base, &base_ty, output);

        let payload_ptr = self.fresh_tmp();
        let _ = writeln!(
            output,
            "  {payload_ptr} = getelementptr inbounds {en_ty}, {en_ptr_ty} {base_value}, i32 0, i32 1"
        );
        let word = self.fresh_tmp();
        let _ = writeln!(output, "  {word} = load i64, i64* {payload_ptr}");
        self.emit_word_to_value(&word, payload_ty, output)
    }

    fn emit_enum_eq(&mut self, enum_name: &str, left: &str, right: &str, output: &mut String) -> String {
        let en_ty = llvm_named_enum_type(enum_name);
        let en_ptr_ty = format!("{en_ty}*");
        let ltag_ptr = self.fresh_tmp();
        let _ = writeln!(
            output,
            "  {ltag_ptr} = getelementptr inbounds {en_ty}, {en_ptr_ty} {left}, i32 0, i32 0"
        );
        let rtag_ptr = self.fresh_tmp();
        let _ = writeln!(
            output,
            "  {rtag_ptr} = getelementptr inbounds {en_ty}, {en_ptr_ty} {right}, i32 0, i32 0"
        );
        let ltag = self.fresh_tmp();
        let _ = writeln!(output, "  {ltag} = load i8, i8* {ltag_ptr}");
        let rtag = self.fresh_tmp();
        let _ = writeln!(output, "  {rtag} = load i8, i8* {rtag_ptr}");
        let tag_eq = self.fresh_tmp();
        let _ = writeln!(output, "  {tag_eq} = icmp eq i8 {ltag}, {rtag}");

        let lp_ptr = self.fresh_tmp();
        let _ = writeln!(
            output,
            "  {lp_ptr} = getelementptr inbounds {en_ty}, {en_ptr_ty} {left}, i32 0, i32 1"
        );
        let rp_ptr = self.fresh_tmp();
        let _ = writeln!(
            output,
            "  {rp_ptr} = getelementptr inbounds {en_ty}, {en_ptr_ty} {right}, i32 0, i32 1"
        );
        let lp = self.fresh_tmp();
        let _ = writeln!(output, "  {lp} = load i64, i64* {lp_ptr}");
        let rp = self.fresh_tmp();
        let _ = writeln!(output, "  {rp} = load i64, i64* {rp_ptr}");
        let payload_eq = self.fresh_tmp();
        let _ = writeln!(output, "  {payload_eq} = icmp eq i64 {lp}, {rp}");
        let both = self.fresh_tmp();
        let _ = writeln!(output, "  {both} = and i1 {tag_eq}, {payload_eq}");
        both
    }

    fn emit_sizeof(&mut self, llvm_named_ty: &str, output: &mut String) -> String {
        let ptr_ty = format!("{llvm_named_ty}*");
        let gep = self.fresh_tmp();
        let _ = writeln!(
            output,
            "  {gep} = getelementptr inbounds {llvm_named_ty}, {ptr_ty} null, i32 1"
        );
        let size = self.fresh_tmp();
        let _ = writeln!(output, "  {size} = ptrtoint {ptr_ty} {gep} to i64");
        size
    }

    fn emit_value_to_word(&mut self, value: &str, ty: &TypeRef, output: &mut String) -> String {
        match ty.head() {
            "Int" => value.to_string(),
            "Bool" => {
                let tmp = self.fresh_tmp();
                let _ = writeln!(output, "  {tmp} = zext i1 {value} to i64");
                tmp
            }
            head if is_generic_param_name(head) => value.to_string(),
            "Text" => self.ptr_to_word("i8*", value, output),
            head if self.records.contains_key(head) => {
                let pty = format!("{}*", llvm_named_record_type(head));
                self.ptr_to_word(&pty, value, output)
            }
            head if self.enums.contains_key(head) => {
                let pty = format!("{}*", llvm_named_enum_type(head));
                self.ptr_to_word(&pty, value, output)
            }
            _ => {
                // Treat as pointer-like.
                self.ptr_to_word("i8*", value, output)
            }
        }
    }

    fn emit_word_to_value(&mut self, word: &str, ty: &TypeRef, output: &mut String) -> String {
        match ty.head() {
            "Int" => word.to_string(),
            "Bool" => {
                let tmp = self.fresh_tmp();
                let _ = writeln!(output, "  {tmp} = trunc i64 {word} to i1");
                tmp
            }
            head if is_generic_param_name(head) => word.to_string(),
            "Text" => self.word_to_ptr("i8*", word, output),
            head if self.records.contains_key(head) => {
                let pty = format!("{}*", llvm_named_record_type(head));
                self.word_to_ptr(&pty, word, output)
            }
            head if self.enums.contains_key(head) => {
                let pty = format!("{}*", llvm_named_enum_type(head));
                self.word_to_ptr(&pty, word, output)
            }
            _ => self.word_to_ptr("i8*", word, output),
        }
    }

    fn ptr_to_word(&mut self, ptr_ty: &str, value: &str, output: &mut String) -> String {
        let tmp = self.fresh_tmp();
        let _ = writeln!(output, "  {tmp} = ptrtoint {ptr_ty} {value} to i64");
        tmp
    }

    fn word_to_ptr(&mut self, ptr_ty: &str, word: &str, output: &mut String) -> String {
        let tmp = self.fresh_tmp();
        let _ = writeln!(output, "  {tmp} = inttoptr i64 {word} to {ptr_ty}");
        tmp
    }

    fn lookup_enum_tag(&self, enum_name: &str, variant_name: &str) -> Option<u8> {
        let enum_decl = self.enums.get(enum_name)?;
        enum_decl
            .variants
            .iter()
            .find(|variant| variant.name == variant_name)
            .map(|variant| variant.tag)
    }

    fn fresh_tmp(&mut self) -> String {
        let id = self.next_tmp;
        self.next_tmp += 1;
        format!("%t{id}")
    }

    fn fresh_tmp_label(&mut self, prefix: &str) -> String {
        let id = self.next_tmp;
        self.next_tmp += 1;
        format!("{prefix}{id}")
    }
}

fn emit_record_decl(record: &MirRecord, output: &mut String) {
    let name = llvm_named_record_type(&record.name);
    let fields = (0..record.fields.len())
        .map(|_| "i64")
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(output, "{name} = type {{ {fields} }}");
}

fn emit_enum_decl(enum_decl: &MirEnum, output: &mut String) {
    let name = llvm_named_enum_type(&enum_decl.name);
    let _ = writeln!(output, "{name} = type {{ i8, i64 }}");
}

fn llvm_named_record_type(raw: &str) -> String {
    format!("%{}", sanitize_symbol(raw))
}

fn llvm_named_enum_type(raw: &str) -> String {
    format!("%{}", sanitize_symbol(raw))
}

fn llvm_type(ty: &TypeRef, records: &HashMap<&str, &MirRecord>, enums: &HashMap<&str, &MirEnum>) -> String {
    match ty.head() {
        "Unit" => "void".to_string(),
        "Int" => "i64".to_string(),
        "Bool" => "i1".to_string(),
        "Float" => "double".to_string(),
        "String" | "Text" => "i8*".to_string(),
        other => {
            if records.contains_key(other) {
                format!("{}*", llvm_named_record_type(other))
            } else if enums.contains_key(other) {
                format!("{}*", llvm_named_enum_type(other))
            } else if is_generic_param_name(other) && ty.args.is_empty() {
                "i64".to_string()
            } else {
                "i8*".to_string()
            }
        }
    }
}

fn return_default_instruction(ty: &TypeRef, records: &HashMap<&str, &MirRecord>, enums: &HashMap<&str, &MirEnum>) -> String {
    let llvm_ty = llvm_type(ty, records, enums);
    if llvm_ty == "void" {
        return "ret void".to_string();
    }
    if llvm_ty.ends_with('*') {
        return format!("ret {llvm_ty} null");
    }
    match llvm_ty.as_str() {
        "i64" => "ret i64 0".to_string(),
        "i1" => "ret i1 0".to_string(),
        "double" => "ret double 0.0".to_string(),
        "i8*" => "ret i8* null".to_string(),
        other => format!("ret {other} zeroinitializer"),
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

fn is_generic_param_name(name: &str) -> bool {
    matches!(name, "T" | "E" | "A" | "B" | "K" | "V" | "U")
}

fn bytes_to_key(bytes: &[u8]) -> String {
    // Stable, lossless key.
    let mut out = String::new();
    for b in bytes {
        let _ = write!(&mut out, "{:02x}", b);
    }
    out
}

fn collect_text_constants(program: &MirProgram) -> BTreeMap<String, Vec<u8>> {
    let mut seen: HashMap<String, String> = HashMap::new(); // key -> global name
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new(); // global name -> bytes (including NUL)

    let mut next_id = 0usize;
    for bytes in iter_text_bytes(program) {
        let key = bytes_to_key(&bytes);
        if seen.contains_key(&key) {
            continue;
        }
        let name = format!("@.str.{next_id}");
        next_id += 1;
        seen.insert(key, name.clone());
        out.insert(name, bytes);
    }

    // Native backend internal messages (ensure they're always available).
    for msg in ["native: host_load_source_map requires string literal path", "host_write_file: path is null"] {
        let mut bytes = msg.as_bytes().to_vec();
        bytes.push(0);
        let key = bytes_to_key(&bytes);
        if seen.contains_key(&key) {
            continue;
        }
        let name = format!("@.str.{next_id}");
        next_id += 1;
        seen.insert(key, name.clone());
        out.insert(name, bytes);
    }

    out
}

fn iter_text_bytes(program: &MirProgram) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for function in &program.functions {
        for block in &function.blocks {
            for stmt in &block.statements {
                match stmt {
                    MirStatement::Assign { rvalue, .. } | MirStatement::Eval(rvalue) => {
                        collect_text_bytes_in_rvalue(rvalue, &mut out);
                    }
                }
            }
            collect_text_bytes_in_terminator(&block.terminator, &mut out);
        }
    }
    out
}

fn collect_text_bytes_in_terminator(term: &MirTerminator, out: &mut Vec<Vec<u8>>) {
    match term {
        MirTerminator::Return { value } => {
            if let Some(op) = value {
                collect_text_bytes_in_operand(op, out);
            }
        }
        MirTerminator::If { cond, .. } => collect_text_bytes_in_operand(cond, out),
        _ => {}
    }
}

fn collect_text_bytes_in_rvalue(rv: &MirRvalue, out: &mut Vec<Vec<u8>>) {
    match rv {
        MirRvalue::Use(op) => collect_text_bytes_in_operand(op, out),
        MirRvalue::Binary { left, right, .. } => {
            collect_text_bytes_in_operand(left, out);
            collect_text_bytes_in_operand(right, out);
        }
        MirRvalue::Call { callee, args } => {
            for op in args {
                collect_text_bytes_in_operand(op, out);
            }

            if callee == "host_load_source_map" {
                if let [MirOperand::ConstText(path)] = args.as_slice() {
                    let bytes = match native_load_source_map(path) {
                        Ok(source) => {
                            let mut bytes = source.as_bytes().to_vec();
                            bytes.push(0);
                            bytes
                        }
                        Err(message) => {
                            let mut bytes = message.as_bytes().to_vec();
                            bytes.push(0);
                            bytes
                        }
                    };
                    out.push(bytes);
                }
            }
        }
        MirRvalue::RecordLit { fields, .. } => {
            for op in fields {
                collect_text_bytes_in_operand(op, out);
            }
        }
        MirRvalue::ProjectField { base, .. } => collect_text_bytes_in_operand(base, out),
        MirRvalue::EnumLit { payload, .. } => {
            if let Some(op) = payload {
                collect_text_bytes_in_operand(op, out);
            }
        }
        MirRvalue::EnumTag { base, .. } => collect_text_bytes_in_operand(base, out),
        MirRvalue::EnumPayload { base, .. } => collect_text_bytes_in_operand(base, out),
    }
}

fn collect_text_bytes_in_operand(op: &MirOperand, out: &mut Vec<Vec<u8>>) {
    if let MirOperand::ConstText(s) = op {
        let mut bytes = s.as_bytes().to_vec();
        bytes.push(0);
        out.push(bytes);
    }
}

fn native_load_source_map(raw: &str) -> Result<String, String> {
    let mut entry = PathBuf::from(raw);
    if entry.extension().is_none() {
        entry.set_extension("kooix");
    }

    if std::fs::metadata(&entry).is_err() {
        // Mirror the interpreter's resilience: tests may run with cwd = crates/kooixc.
        let mut prefix = PathBuf::new();
        for _ in 0..8 {
            prefix.push("..");
            let candidate = prefix.join(&entry);
            if std::fs::metadata(&candidate).is_ok() {
                entry = candidate;
                break;
            }
        }
    }

    match load_source_map(&entry) {
        Ok(map) => Ok(map.combined),
        Err(errors) => Err(errors
            .first()
            .map(|error| error.message.clone())
            .unwrap_or_else(|| "failed to load source map".to_string())),
    }
}

fn emit_text_constant(name: &str, bytes: &[u8], output: &mut String) {
    let n = bytes.len();
    let escaped = llvm_escape_bytes(bytes);
    let _ = writeln!(
        output,
        "{name} = private unnamed_addr constant [{n} x i8] c\"{escaped}\", align 1"
    );
}

fn llvm_escape_bytes(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        match b {
            b'\n' => out.push_str("\\0A"),
            b'\r' => out.push_str("\\0D"),
            b'\t' => out.push_str("\\09"),
            b'\"' => out.push_str("\\22"),
            b'\\' => out.push_str("\\5C"),
            0 => out.push_str("\\00"),
            0x20..=0x7e => out.push(b as char),
            _ => {
                let _ = write!(&mut out, "\\{:02X}", b);
            }
        }
    }
    out
}

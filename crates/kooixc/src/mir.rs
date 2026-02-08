use std::collections::HashMap;

use crate::ast::{BinaryOp, Block, Expr, Statement, TypeRef};
use crate::error::Diagnostic;
use crate::hir::{HirFunction, HirProgram};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirProgram {
    pub records: Vec<MirRecord>,
    pub enums: Vec<MirEnum>,
    pub functions: Vec<MirFunction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirRecord {
    pub name: String,
    pub fields: Vec<MirRecordField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirRecordField {
    pub name: String,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirEnum {
    pub name: String,
    pub variants: Vec<MirEnumVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirEnumVariant {
    pub name: String,
    pub tag: u8,
    pub payload: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirFunction {
    pub name: String,
    pub params: Vec<MirParam>,
    pub return_type: TypeRef,
    pub effects: Vec<String>,
    pub locals: Vec<MirLocal>,
    pub blocks: Vec<MirBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirParam {
    pub name: String,
    pub ty: TypeRef,
    pub local: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirLocal {
    pub name: String,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirBlock {
    pub label: String,
    pub statements: Vec<MirStatement>,
    pub terminator: MirTerminator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirStatement {
    Assign { dst: usize, rvalue: MirRvalue },
    Eval(MirRvalue),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirRvalue {
    Use(MirOperand),
    Binary {
        op: BinaryOp,
        left: MirOperand,
        right: MirOperand,
    },
    Call { callee: String, args: Vec<MirOperand> },
    RecordLit { record: String, fields: Vec<MirOperand> },
    ProjectField {
        base: MirOperand,
        record: String,
        index: usize,
    },
    EnumLit {
        enum_name: String,
        tag: u8,
        payload: Option<MirOperand>,
        payload_ty: Option<TypeRef>,
    },
    EnumTag { base: MirOperand, enum_name: String },
    EnumPayload {
        base: MirOperand,
        enum_name: String,
        payload_ty: TypeRef,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirOperand {
    ConstInt(i64),
    ConstBool(bool),
    Local(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirTerminator {
    Return { value: Option<MirOperand> },
    ReturnDefault(TypeRef),
    Goto { target: String },
    If {
        cond: MirOperand,
        then_bb: String,
        else_bb: String,
    },
}

#[derive(Debug, Clone)]
struct FunctionSignature {
    return_type: TypeRef,
    effects: Vec<String>,
    has_generics: bool,
}

pub fn lower_hir(program: &HirProgram) -> Result<MirProgram, Vec<Diagnostic>> {
    let signatures = build_signatures(program);
    let records = build_native_records(program);
    let record_map: HashMap<String, MirRecord> = records
        .iter()
        .cloned()
        .map(|record| (record.name.clone(), record))
        .collect();
    let enums = build_native_enums(program);
    let enum_map: HashMap<String, MirEnum> = enums
        .iter()
        .cloned()
        .map(|enum_decl| (enum_decl.name.clone(), enum_decl))
        .collect();
    let mut diagnostics = Vec::new();
    let mut functions = Vec::new();

    for function in &program.functions {
        match lower_function(function, &signatures, &record_map, &enum_map) {
            Ok(mir_function) => functions.push(mir_function),
            Err(mut errors) => diagnostics.append(&mut errors),
        }
    }

    if diagnostics.is_empty() {
        Ok(MirProgram {
            records,
            enums,
            functions,
        })
    } else {
        Err(diagnostics)
    }
}

fn build_native_records(program: &HirProgram) -> Vec<MirRecord> {
    program
        .records
        .iter()
        .filter(|record| record.generics.is_empty())
        .filter(|record| {
            record
                .fields
                .iter()
                .all(|field| is_native_struct_field_type(&field.ty))
        })
        .map(|record| MirRecord {
            name: record.name.clone(),
            fields: record
                .fields
                .iter()
                .map(|field| MirRecordField {
                    name: field.name.clone(),
                    ty: field.ty.clone(),
                })
                .collect(),
        })
        .collect()
}

fn build_native_enums(program: &HirProgram) -> Vec<MirEnum> {
    program
        .enums
        .iter()
        .filter(|enum_decl| enum_decl.generics.is_empty())
        .filter(|enum_decl| {
            enum_decl.variants.len() <= u8::MAX as usize
                && enum_decl.variants.iter().all(|variant| match &variant.payload {
                    None => true,
                    Some(payload) => is_native_enum_payload_type(payload),
                })
        })
        .map(|enum_decl| MirEnum {
            name: enum_decl.name.clone(),
            variants: enum_decl
                .variants
                .iter()
                .enumerate()
                .map(|(index, variant)| MirEnumVariant {
                    name: variant.name.clone(),
                    tag: index as u8,
                    payload: variant.payload.clone(),
                })
                .collect(),
        })
        .collect()
}

fn build_signatures(program: &HirProgram) -> HashMap<String, FunctionSignature> {
    let mut signatures = HashMap::new();
    for function in &program.functions {
        let effects = function
            .effects
            .iter()
            .map(|effect| {
                if let Some(argument) = effect.argument.as_deref() {
                    format!("{}({argument})", effect.name)
                } else {
                    effect.name.clone()
                }
            })
            .collect::<Vec<_>>();

        signatures.insert(
            function.name.clone(),
            FunctionSignature {
                return_type: function.return_type.clone(),
                effects,
                has_generics: !function.generics.is_empty(),
            },
        );
    }
    signatures
}

fn lower_function(
    function: &HirFunction,
    signatures: &HashMap<String, FunctionSignature>,
    records: &HashMap<String, MirRecord>,
    enums: &HashMap<String, MirEnum>,
) -> Result<MirFunction, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();

    if !function.generics.is_empty() {
        diagnostics.push(Diagnostic::error(
            format!(
                "function '{}' is generic but native lowering does not support generics yet",
                function.name
            ),
            function.span,
        ));
    }

    if function.body.is_some() && !function.effects.is_empty() {
        diagnostics.push(Diagnostic::error(
            format!(
                "function '{}' has effects and cannot be lowered to native code yet",
                function.name
            ),
            function.span,
        ));
    }

    if function.body.is_some() {
        for param in &function.params {
            if !is_native_type(&param.ty, records, enums) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' parameter '{}' uses type '{}' which is not supported by native lowering yet",
                        function.name, param.name, param.ty
                    ),
                    function.span,
                ));
            }
        }

        if !is_native_type(&function.return_type, records, enums) {
            diagnostics.push(Diagnostic::error(
                format!(
                    "function '{}' return type '{}' is not supported by native lowering yet",
                    function.name, function.return_type
                ),
                function.span,
            ));
        }
    }

    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    let mut builder = MirBuilder::new(function, signatures, records, enums);
    match &function.body {
        None => {
            builder.emit_stub_body();
        }
        Some(body) => {
            if let Err(error) = builder.lower_function_body(body) {
                return Err(vec![error]);
            }
        }
    }

    Ok(builder.finish())
}

fn is_native_scalar_type(ty: &TypeRef) -> bool {
    matches!(ty.head(), "Int" | "Bool" | "Unit")
}

fn is_native_struct_field_type(ty: &TypeRef) -> bool {
    matches!(ty.head(), "Int" | "Bool")
}

fn is_native_enum_payload_type(ty: &TypeRef) -> bool {
    matches!(ty.head(), "Int" | "Bool")
}

fn is_native_type(
    ty: &TypeRef,
    records: &HashMap<String, MirRecord>,
    _enums: &HashMap<String, MirEnum>,
) -> bool {
    if is_native_scalar_type(ty) {
        return true;
    }

    ty.args.is_empty() && records.contains_key(ty.head())
}

struct MirBuilder<'a> {
    function: &'a HirFunction,
    signatures: &'a HashMap<String, FunctionSignature>,
    records: &'a HashMap<String, MirRecord>,
    enums: &'a HashMap<String, MirEnum>,
    params: Vec<MirParam>,
    locals: Vec<MirLocal>,
    scopes: Vec<HashMap<String, usize>>,
    blocks: Vec<MirBlock>,
    next_block_id: usize,
    current_block: usize,
}

impl<'a> MirBuilder<'a> {
    fn new(
        function: &'a HirFunction,
        signatures: &'a HashMap<String, FunctionSignature>,
        records: &'a HashMap<String, MirRecord>,
        enums: &'a HashMap<String, MirEnum>,
    ) -> Self {
        let mut locals = Vec::new();
        let mut params = Vec::new();
        let mut scopes: Vec<HashMap<String, usize>> = vec![HashMap::new()];

        for param in &function.params {
            let local = locals.len();
            locals.push(MirLocal {
                name: param.name.clone(),
                ty: param.ty.clone(),
            });
            scopes[0].insert(param.name.clone(), local);
            params.push(MirParam {
                name: param.name.clone(),
                ty: param.ty.clone(),
                local,
            });
        }

        let entry_block = MirBlock {
            label: "bb0".to_string(),
            statements: Vec::new(),
            terminator: MirTerminator::ReturnDefault(TypeRef {
                name: "Unit".to_string(),
                args: Vec::new(),
            }),
        };

        Self {
            function,
            signatures,
            records,
            enums,
            params,
            locals,
            scopes,
            blocks: vec![entry_block],
            next_block_id: 1,
            current_block: 0,
        }
    }

    fn finish(self) -> MirFunction {
        let effects = self
            .function
            .effects
            .iter()
            .map(|effect| {
                if let Some(argument) = effect.argument.as_deref() {
                    format!("{}({argument})", effect.name)
                } else {
                    effect.name.clone()
                }
            })
            .collect();

        MirFunction {
            name: self.function.name.clone(),
            params: self.params,
            return_type: self.function.return_type.clone(),
            effects,
            locals: self.locals,
            blocks: self.blocks,
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        let popped = self.scopes.pop();
        if popped.is_none() {
            panic!("lowering bug: scope underflow");
        }
    }

    fn with_scope<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, Diagnostic>,
    ) -> Result<T, Diagnostic> {
        self.push_scope();
        let result = f(self);
        self.pop_scope();
        result
    }

    fn lookup_local(&self, name: &str) -> Option<usize> {
        for scope in self.scopes.iter().rev() {
            if let Some(local) = scope.get(name) {
                return Some(*local);
            }
        }
        None
    }

    fn declare_local(
        &mut self,
        name: &str,
        ty: TypeRef,
        allow_shadow: bool,
    ) -> Result<usize, Diagnostic> {
        if !allow_shadow && self.lookup_local(name).is_some() {
            return Err(Diagnostic::error(
                format!(
                    "function '{}' redefines local '{}' in lowering",
                    self.function.name, name
                ),
                self.function.span,
            ));
        }

        let local = self.locals.len();
        self.locals.push(MirLocal {
            name: name.to_string(),
            ty,
        });

        let scope = self
            .scopes
            .last_mut()
            .expect("lowering bug: no active scope");
        scope.insert(name.to_string(), local);
        Ok(local)
    }

    fn emit_stub_body(&mut self) {
        let block = &mut self.blocks[0];
        block.terminator = MirTerminator::ReturnDefault(self.function.return_type.clone());
    }

    fn lower_function_body(&mut self, body: &Block) -> Result<(), Diagnostic> {
        self.blocks[0].terminator = MirTerminator::ReturnDefault(TypeRef {
            name: "Unit".to_string(),
            args: Vec::new(),
        });

        self.lower_block_statements(body, BlockMode::FunctionBody)?;

        if self.block_is_terminated(self.current_block) {
            return Ok(());
        }

        let value = if let Some(expr) = &body.tail {
            self.lower_expr(expr)?
        } else {
            ExprValue::unit()
        };

        self.set_return_terminator(value);
        Ok(())
    }

    fn lower_block_statements(&mut self, block: &Block, mode: BlockMode) -> Result<(), Diagnostic> {
        for statement in &block.statements {
            if self.block_is_terminated(self.current_block) {
                break;
            }

            match statement {
                Statement::Let(stmt) => {
                    let value = self.lower_expr(&stmt.value)?;
                    let inferred_ty = value.ty.clone();
                    let ty = stmt.ty.as_ref().unwrap_or(&inferred_ty);

                    if *ty != inferred_ty {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' let '{}' declares type '{}' but value is '{}'",
                                self.function.name, stmt.name, ty, inferred_ty
                            ),
                            self.function.span,
                        ));
                    }

                    if !is_native_type(ty, self.records, self.enums) {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' uses let-binding '{}' with type '{}' which is not supported by native lowering yet",
                                self.function.name, stmt.name, ty
                            ),
                            self.function.span,
                        ));
                    }

                    let local = self.declare_local(&stmt.name, ty.clone(), false)?;
                    self.emit_store(local, value);
                }
                Statement::Assign(stmt) => {
                    let Some(local) = self.lookup_local(&stmt.name) else {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' assigns to unknown local '{}'",
                                self.function.name, stmt.name
                            ),
                            self.function.span,
                        ));
                    };
                    let value = self.lower_expr(&stmt.value)?;
                    self.emit_store(local, value);
                }
                Statement::Return(stmt) => {
                    let value = if let Some(expr) = &stmt.value {
                        self.lower_expr(expr)?
                    } else {
                        ExprValue::unit()
                    };
                    self.set_return_terminator(value);
                }
                Statement::Expr(expr) => {
                    let _ = self.lower_expr(expr)?;
                }
            }
        }

        if self.block_is_terminated(self.current_block) {
            return Ok(());
        }

        match mode {
            BlockMode::FunctionBody => Ok(()),
            BlockMode::Value => Ok(()),
            BlockMode::LoopBody => {
                if let Some(expr) = &block.tail {
                    let _ = self.lower_expr(expr)?;
                }
                Ok(())
            }
        }
    }

    fn lower_block_value(&mut self, block: &Block) -> Result<ExprValue, Diagnostic> {
        self.lower_block_statements(block, BlockMode::Value)?;
        if self.block_is_terminated(self.current_block) {
            return Ok(ExprValue::unreachable());
        }

        if let Some(expr) = &block.tail {
            self.lower_expr(expr)
        } else {
            Ok(ExprValue::unit())
        }
    }

    fn lower_expr(&mut self, expr: &Expr) -> Result<ExprValue, Diagnostic> {
        match expr {
            Expr::Number(raw) => {
                let value = raw.parse::<i64>().map_err(|_| {
                    Diagnostic::error(format!("invalid integer literal '{raw}'"), self.function.span)
                })?;
                Ok(ExprValue {
                    ty: TypeRef {
                        name: "Int".to_string(),
                        args: Vec::new(),
                    },
                    operand: Some(MirOperand::ConstInt(value)),
                })
            }
            Expr::Bool(value) => Ok(ExprValue {
                ty: TypeRef {
                    name: "Bool".to_string(),
                    args: Vec::new(),
                },
                operand: Some(MirOperand::ConstBool(*value)),
            }),
            Expr::String(_) => Err(Diagnostic::error(
                format!(
                    "function '{}' uses Text literal but native lowering does not support Text yet",
                    self.function.name
                ),
                self.function.span,
            )),
            Expr::Path(segments) => {
                match segments.as_slice() {
                    [name] => {
                        let Some(local) = self.lookup_local(name.as_str()) else {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' uses unknown local '{}' in body",
                                    self.function.name, name
                                ),
                                self.function.span,
                            ));
                        };
                        let ty = self.locals[local].ty.clone();
                        Ok(ExprValue {
                            ty,
                            operand: Some(MirOperand::Local(local)),
                        })
                    }
                    [base, field] => {
                        let Some(base_local) = self.lookup_local(base.as_str()) else {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' uses unknown local '{}' in body",
                                    self.function.name, base
                                ),
                                self.function.span,
                            ));
                        };

                        let base_ty = self.locals[base_local].ty.clone();
                        if base_ty.args.is_empty() && self.records.contains_key(base_ty.head()) {
                            let Some(record) = self.records.get(base_ty.head()) else {
                                return Err(Diagnostic::error(
                                    format!(
                                        "function '{}' uses record type '{}' which is not supported by native lowering yet",
                                        self.function.name, base_ty
                                    ),
                                    self.function.span,
                                ));
                            };

                            let Some((field_index, field_schema)) = record
                                .fields
                                .iter()
                                .enumerate()
                                .find(|(_, schema)| schema.name == *field)
                            else {
                                return Err(Diagnostic::error(
                                    format!(
                                        "function '{}' uses unknown field '{}' on record '{}'",
                                        self.function.name, field, base_ty
                                    ),
                                    self.function.span,
                                ));
                            };

                            if !is_native_struct_field_type(&field_schema.ty) {
                                return Err(Diagnostic::error(
                                    format!(
                                        "function '{}' projects field '{}.{}' of unsupported type '{}'",
                                        self.function.name, base_ty, field, field_schema.ty
                                    ),
                                    self.function.span,
                                ));
                            }

                            let temp = self.new_temp_local(field_schema.ty.clone());
                            self.current_block_mut()
                                .statements
                                .push(MirStatement::Assign {
                                    dst: temp,
                                    rvalue: MirRvalue::ProjectField {
                                        base: MirOperand::Local(base_local),
                                        record: record.name.clone(),
                                        index: field_index,
                                    },
                                });
                            Ok(ExprValue {
                                ty: field_schema.ty.clone(),
                                operand: Some(MirOperand::Local(temp)),
                            })
                        } else {
                            Err(Diagnostic::error(
                                format!(
                                    "function '{}' uses path '{}' which is not supported by native lowering yet",
                                    self.function.name,
                                    segments.join(".")
                                ),
                                self.function.span,
                            ))
                        }
                    }
                    _ => Err(Diagnostic::error(
                        format!(
                            "function '{}' uses path '{}' which is not supported by native lowering yet",
                            self.function.name,
                            segments.join(".")
                        ),
                        self.function.span,
                    )),
                }
            }
            Expr::Binary { op, left, right } => {
                let left_value = self.lower_expr(left)?;
                let right_value = self.lower_expr(right)?;

                let result_ty = match op {
                    BinaryOp::Add => TypeRef {
                        name: "Int".to_string(),
                        args: Vec::new(),
                    },
                    BinaryOp::Eq | BinaryOp::NotEq => TypeRef {
                        name: "Bool".to_string(),
                        args: Vec::new(),
                    },
                };

                if !is_native_scalar_type(&left_value.ty) || !is_native_scalar_type(&right_value.ty)
                {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' uses binary op on unsupported type(s) '{}' and '{}'",
                            self.function.name, left_value.ty, right_value.ty
                        ),
                        self.function.span,
                    ));
                }

                let left_op = left_value.into_operand_or_unit(self.function)?;
                let right_op = right_value.into_operand_or_unit(self.function)?;

                let temp = self.new_temp_local(result_ty.clone());
                self.current_block_mut()
                    .statements
                    .push(MirStatement::Assign {
                        dst: temp,
                        rvalue: MirRvalue::Binary {
                            op: *op,
                            left: left_op,
                            right: right_op,
                        },
                    });
                Ok(ExprValue {
                    ty: result_ty,
                    operand: Some(MirOperand::Local(temp)),
                })
            }
            Expr::Call {
                target,
                type_args,
                args,
            } => {
                if !type_args.is_empty() {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' call '{}' uses generic type arguments, which native lowering does not support yet",
                            self.function.name,
                            target.join(".")
                        ),
                        self.function.span,
                    ));
                }

                if target.len() != 1 {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' calls '{}' which is not supported by native lowering yet",
                            self.function.name,
                            target.join(".")
                        ),
                        self.function.span,
                    ));
                }

                let callee = target[0].clone();
                let Some(signature) = self.signatures.get(&callee) else {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' calls unknown function '{}'",
                            self.function.name, callee
                        ),
                        self.function.span,
                    ));
                };

                if signature.has_generics {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' calls generic function '{}' but native lowering does not support generics yet",
                            self.function.name, callee
                        ),
                        self.function.span,
                    ));
                }

                if !signature.effects.is_empty() {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' calls effectful function '{}' which native lowering cannot execute",
                            self.function.name, callee
                        ),
                        self.function.span,
                    ));
                }

                let return_ty = signature.return_type.clone();
                if !is_native_type(&return_ty, self.records, self.enums) {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' calls '{}' returning '{}' which is not supported by native lowering yet",
                            self.function.name, callee, return_ty
                        ),
                        self.function.span,
                    ));
                }

                let mut lowered_args = Vec::new();
                for arg in args {
                    let value = self.lower_expr(arg)?;
                    lowered_args.push(value.into_operand_or_unit(self.function)?);
                }

                if return_ty.head() == "Unit" {
                    self.current_block_mut().statements.push(MirStatement::Eval(
                        MirRvalue::Call {
                            callee,
                            args: lowered_args,
                        },
                    ));
                    return Ok(ExprValue::unit());
                }

                let temp = self.new_temp_local(return_ty.clone());
                self.current_block_mut()
                    .statements
                    .push(MirStatement::Assign {
                        dst: temp,
                        rvalue: MirRvalue::Call {
                            callee,
                            args: lowered_args,
                        },
                    });

                Ok(ExprValue {
                    ty: return_ty,
                    operand: Some(MirOperand::Local(temp)),
                })
            }
            Expr::If {
                cond,
                then_block,
                else_block,
            } => self.lower_if_expr(cond, then_block, else_block.as_deref()),
            Expr::While { cond, body } => self.lower_while_expr(cond, body),
            Expr::RecordLit { ty, fields } => {
                if !ty.args.is_empty() {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' uses generic record literal '{}', which native lowering does not support yet",
                            self.function.name, ty
                        ),
                        self.function.span,
                    ));
                }

                let Some(record) = self.records.get(ty.head()) else {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' uses record literal of unsupported type '{}'",
                            self.function.name,
                            ty.head()
                        ),
                        self.function.span,
                    ));
                };

                let mut lowered_fields: HashMap<&str, MirOperand> = HashMap::new();
                for field in fields {
                    let value = self.lower_expr(&field.value)?;
                    lowered_fields.insert(
                        field.name.as_str(),
                        value.into_operand_or_unit(self.function)?,
                    );
                }

                let mut ordered = Vec::new();
                for schema_field in &record.fields {
                    let Some(value) = lowered_fields.get(schema_field.name.as_str()) else {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' record literal missing field '{}' for type '{}'",
                                self.function.name, schema_field.name, record.name
                            ),
                            self.function.span,
                        ));
                    };
                    ordered.push(value.clone());
                }

                let temp = self.new_temp_local(ty.clone());
                self.current_block_mut()
                    .statements
                    .push(MirStatement::Assign {
                        dst: temp,
                        rvalue: MirRvalue::RecordLit {
                            record: record.name.clone(),
                            fields: ordered,
                        },
                    });

                Ok(ExprValue {
                    ty: ty.clone(),
                    operand: Some(MirOperand::Local(temp)),
                })
            }
            Expr::Match { .. } => Err(Diagnostic::error(
                format!(
                    "function '{}' uses expression form not supported by native lowering yet",
                    self.function.name
                ),
                self.function.span,
            )),
        }
    }

    fn lower_if_expr(
        &mut self,
        cond: &Expr,
        then_block: &Block,
        else_block: Option<&Block>,
    ) -> Result<ExprValue, Diagnostic> {
        let cond_value = self.lower_expr(cond)?;
        if cond_value.ty.head() != "Bool" {
            return Err(Diagnostic::error(
                format!(
                    "function '{}' if condition must be Bool but got '{}'",
                    self.function.name, cond_value.ty
                ),
                self.function.span,
            ));
        }
        let cond_operand = cond_value.into_operand_or_unit(self.function)?;

        let then_bb = self.new_block();
        let else_bb = self.new_block();
        let join_bb = self.new_block();

        self.set_terminator(MirTerminator::If {
            cond: cond_operand,
            then_bb: then_bb.clone(),
            else_bb: else_bb.clone(),
        });

        self.switch_to_block(&then_bb);
        let then_value = self.with_scope(|builder| builder.lower_block_value(then_block))?;
        let then_needs_join = !self.block_is_terminated(self.current_block);
        if then_needs_join {
            self.set_terminator(MirTerminator::Goto {
                target: join_bb.clone(),
            });
        }

        self.switch_to_block(&else_bb);
        let else_value = if let Some(block) = else_block {
            self.with_scope(|builder| builder.lower_block_value(block))?
        } else {
            ExprValue::unit()
        };
        let else_needs_join = !self.block_is_terminated(self.current_block);
        if else_needs_join {
            self.set_terminator(MirTerminator::Goto {
                target: join_bb.clone(),
            });
        }

        let result_ty = if else_block.is_none() {
            if then_value.ty.head() != "Unit" {
                return Err(Diagnostic::error(
                    format!(
                        "function '{}' uses if expression without else returning '{}' but expected 'Unit' for native lowering",
                        self.function.name, then_value.ty
                    ),
                    self.function.span,
                ));
            }
            TypeRef {
                name: "Unit".to_string(),
                args: Vec::new(),
            }
        } else if then_value.ty != else_value.ty {
            return Err(Diagnostic::error(
                format!(
                    "function '{}' if expression branches return '{}' and '{}' (must match for native lowering)",
                    self.function.name, then_value.ty, else_value.ty
                ),
                self.function.span,
            ));
        } else {
            then_value.ty.clone()
        };

        if !is_native_type(&result_ty, self.records, self.enums) {
            return Err(Diagnostic::error(
                format!(
                    "function '{}' if expression returns '{}' which is not supported by native lowering yet",
                    self.function.name, result_ty
                ),
                self.function.span,
            ));
        }

        let result_temp = if result_ty.head() == "Unit" {
            None
        } else {
            Some(self.new_temp_local(result_ty.clone()))
        };

        if let Some(temp) = result_temp {
            if then_needs_join {
                self.switch_to_block(&then_bb);
                self.emit_store(temp, then_value.clone());
            }
            if else_needs_join {
                self.switch_to_block(&else_bb);
                self.emit_store(temp, else_value.clone());
            }
        }

        self.switch_to_block(&join_bb);
        Ok(ExprValue {
            ty: result_ty,
            operand: result_temp.map(MirOperand::Local),
        })
    }

    fn lower_while_expr(&mut self, cond: &Expr, body: &Block) -> Result<ExprValue, Diagnostic> {
        let cond_bb = self.new_block();
        let body_bb = self.new_block();
        let exit_bb = self.new_block();

        self.set_terminator(MirTerminator::Goto {
            target: cond_bb.clone(),
        });

        self.switch_to_block(&cond_bb);
        let cond_value = self.lower_expr(cond)?;
        if cond_value.ty.head() != "Bool" {
            return Err(Diagnostic::error(
                format!(
                    "function '{}' while condition must be Bool but got '{}'",
                    self.function.name, cond_value.ty
                ),
                self.function.span,
            ));
        }
        let cond_operand = cond_value.into_operand_or_unit(self.function)?;
        self.set_terminator(MirTerminator::If {
            cond: cond_operand,
            then_bb: body_bb.clone(),
            else_bb: exit_bb.clone(),
        });

        self.switch_to_block(&body_bb);
        self.with_scope(|builder| builder.lower_block_statements(body, BlockMode::LoopBody))?;
        if !self.block_is_terminated(self.current_block) {
            self.set_terminator(MirTerminator::Goto { target: cond_bb });
        }

        self.switch_to_block(&exit_bb);
        Ok(ExprValue::unit())
    }

    fn infer_expr_type(&self, expr: &Expr) -> Result<TypeRef, Diagnostic> {
        match expr {
            Expr::Number(_) => Ok(TypeRef {
                name: "Int".to_string(),
                args: Vec::new(),
            }),
            Expr::Bool(_) => Ok(TypeRef {
                name: "Bool".to_string(),
                args: Vec::new(),
            }),
            Expr::String(_) => Ok(TypeRef {
                name: "Text".to_string(),
                args: Vec::new(),
            }),
            Expr::Path(segments) => {
                match segments.as_slice() {
                    [name] => {
                        let Some(local) = self.lookup_local(name.as_str()) else {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' uses unknown local '{}' in body",
                                    self.function.name, name
                                ),
                                self.function.span,
                            ));
                        };
                        Ok(self.locals[local].ty.clone())
                    }
                    [base, field] => {
                        let Some(base_local) = self.lookup_local(base.as_str()) else {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' uses unknown local '{}' in body",
                                    self.function.name, base
                                ),
                                self.function.span,
                            ));
                        };

                        let base_ty = self.locals[base_local].ty.clone();
                        let Some(record) = self.records.get(base_ty.head()) else {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' uses path '{}' which is not supported by native lowering yet",
                                    self.function.name,
                                    segments.join(".")
                                ),
                                self.function.span,
                            ));
                        };

                        let Some(field_schema) =
                            record.fields.iter().find(|schema| schema.name == *field)
                        else {
                            return Err(Diagnostic::error(
                                format!(
                                    "function '{}' uses unknown field '{}' on record '{}'",
                                    self.function.name, field, base_ty
                                ),
                                self.function.span,
                            ));
                        };
                        Ok(field_schema.ty.clone())
                    }
                    _ => Err(Diagnostic::error(
                        format!(
                            "function '{}' uses path '{}' which is not supported by native lowering yet",
                            self.function.name,
                            segments.join(".")
                        ),
                        self.function.span,
                    )),
                }
            }
            Expr::Binary { op, .. } => Ok(match op {
                BinaryOp::Add => TypeRef {
                    name: "Int".to_string(),
                    args: Vec::new(),
                },
                BinaryOp::Eq | BinaryOp::NotEq => TypeRef {
                    name: "Bool".to_string(),
                    args: Vec::new(),
                },
            }),
            Expr::Call {
                target, type_args, ..
            } => {
                if !type_args.is_empty() {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' call '{}' uses generic type arguments, which native lowering does not support yet",
                            self.function.name,
                            target.join(".")
                        ),
                        self.function.span,
                    ));
                }

                if target.len() != 1 {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' calls '{}' which is not supported by native lowering yet",
                            self.function.name,
                            target.join(".")
                        ),
                        self.function.span,
                    ));
                }

                let callee = target[0].as_str();
                let Some(signature) = self.signatures.get(callee) else {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' calls unknown function '{}'",
                            self.function.name, callee
                        ),
                        self.function.span,
                    ));
                };
                Ok(signature.return_type.clone())
            }
            Expr::If {
                then_block,
                else_block,
                ..
            } => self.infer_if_result_type(then_block, else_block.as_deref()),
            Expr::While { .. } => Ok(TypeRef {
                name: "Unit".to_string(),
                args: Vec::new(),
            }),
            Expr::RecordLit { ty, .. } => Ok(ty.clone()),
            Expr::Match { .. } => Err(Diagnostic::error(
                format!(
                    "function '{}' match expression type inference is not supported in native lowering yet",
                    self.function.name
                ),
                self.function.span,
            )),
        }
    }

    fn infer_if_result_type(
        &self,
        then_block: &Block,
        else_block: Option<&Block>,
    ) -> Result<TypeRef, Diagnostic> {
        let then_ty = then_block
            .tail
            .as_ref()
            .map(|expr| self.infer_expr_type(expr))
            .transpose()?
            .unwrap_or_else(|| TypeRef {
                name: "Unit".to_string(),
                args: Vec::new(),
            });

        let else_ty = else_block
            .and_then(|block| block.tail.as_ref())
            .map(|expr| self.infer_expr_type(expr))
            .transpose()?
            .unwrap_or_else(|| TypeRef {
                name: "Unit".to_string(),
                args: Vec::new(),
            });

        if then_ty != else_ty {
            return Err(Diagnostic::error(
                format!(
                    "function '{}' if expression branches return '{}' and '{}' (must match for native lowering)",
                    self.function.name, then_ty, else_ty
                ),
                self.function.span,
            ));
        }

        Ok(then_ty)
    }

    fn new_temp_local(&mut self, ty: TypeRef) -> usize {
        let local = self.locals.len();
        self.locals.push(MirLocal {
            name: format!("$t{local}"),
            ty,
        });
        local
    }

    fn new_block(&mut self) -> String {
        let label = format!("bb{}", self.next_block_id);
        self.next_block_id += 1;
        self.blocks.push(MirBlock {
            label: label.clone(),
            statements: Vec::new(),
            terminator: MirTerminator::ReturnDefault(TypeRef {
                name: "Unit".to_string(),
                args: Vec::new(),
            }),
        });
        label
    }

    fn block_index(&self, label: &str) -> Option<usize> {
        self.blocks.iter().position(|block| block.label == label)
    }

    fn switch_to_block(&mut self, label: &str) {
        if let Some(index) = self.block_index(label) {
            self.current_block = index;
            return;
        }
        panic!("lowering bug: missing block '{label}'");
    }

    fn current_block_mut(&mut self) -> &mut MirBlock {
        &mut self.blocks[self.current_block]
    }

    fn block_is_terminated(&self, block: usize) -> bool {
        !matches!(self.blocks[block].terminator, MirTerminator::ReturnDefault(_))
    }

    fn set_terminator(&mut self, terminator: MirTerminator) {
        self.blocks[self.current_block].terminator = terminator;
    }

    fn emit_store(&mut self, local: usize, value: ExprValue) {
        if value.ty.head() == "Unit" {
            return;
        }

        let Some(operand) = value.operand else {
            return;
        };

        self.current_block_mut()
            .statements
            .push(MirStatement::Assign {
                dst: local,
                rvalue: MirRvalue::Use(operand),
            });
    }

    fn set_return_terminator(&mut self, value: ExprValue) {
        if self.function.return_type.head() == "Unit" {
            self.set_terminator(MirTerminator::Return { value: None });
            return;
        }

        let operand = value.operand.unwrap_or_else(|| match self.function.return_type.head() {
            "Bool" => MirOperand::ConstBool(false),
            _ => MirOperand::ConstInt(0),
        });
        self.set_terminator(MirTerminator::Return {
            value: Some(operand),
        });
    }
}

#[derive(Clone, Copy)]
enum BlockMode {
    FunctionBody,
    Value,
    LoopBody,
}

#[derive(Debug, Clone)]
struct ExprValue {
    ty: TypeRef,
    operand: Option<MirOperand>,
}

impl ExprValue {
    fn unit() -> Self {
        Self {
            ty: TypeRef {
                name: "Unit".to_string(),
                args: Vec::new(),
            },
            operand: None,
        }
    }

    fn unreachable() -> Self {
        Self::unit()
    }

    fn into_operand_or_unit(self, function: &HirFunction) -> Result<MirOperand, Diagnostic> {
        if self.ty.head() == "Unit" {
            return Err(Diagnostic::error(
                format!(
                    "function '{}' uses Unit value where a value is required in native lowering",
                    function.name
                ),
                function.span,
            ));
        }
        self.operand.ok_or_else(|| {
            Diagnostic::error(
                format!(
                    "function '{}' lowering lost operand for type '{}'",
                    function.name, self.ty
                ),
                function.span,
            )
        })
    }
}

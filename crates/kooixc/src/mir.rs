use std::collections::HashMap;

use crate::ast::{BinaryOp, Block, Expr, Statement, TypeRef};
use crate::error::Diagnostic;
use crate::hir::{HirFunction, HirProgram};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirProgram {
    pub functions: Vec<MirFunction>,
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
    let mut diagnostics = Vec::new();
    let mut functions = Vec::new();

    for function in &program.functions {
        match lower_function(function, &signatures) {
            Ok(mir_function) => functions.push(mir_function),
            Err(mut errors) => diagnostics.append(&mut errors),
        }
    }

    if diagnostics.is_empty() {
        Ok(MirProgram { functions })
    } else {
        Err(diagnostics)
    }
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
            if !is_native_scalar_type(&param.ty) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "function '{}' parameter '{}' uses type '{}' which is not supported by native lowering yet",
                        function.name, param.name, param.ty
                    ),
                    function.span,
                ));
            }
        }

        if !is_native_scalar_type(&function.return_type) {
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

    let mut builder = MirBuilder::new(function, signatures);
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

struct MirBuilder<'a> {
    function: &'a HirFunction,
    signatures: &'a HashMap<String, FunctionSignature>,
    params: Vec<MirParam>,
    locals: Vec<MirLocal>,
    local_map: HashMap<String, usize>,
    blocks: Vec<MirBlock>,
    next_block_id: usize,
    current_block: usize,
}

impl<'a> MirBuilder<'a> {
    fn new(function: &'a HirFunction, signatures: &'a HashMap<String, FunctionSignature>) -> Self {
        let mut locals = Vec::new();
        let mut params = Vec::new();
        let mut local_map = HashMap::new();

        for param in &function.params {
            let local = locals.len();
            locals.push(MirLocal {
                name: param.name.clone(),
                ty: param.ty.clone(),
            });
            local_map.insert(param.name.clone(), local);
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
            params,
            locals,
            local_map,
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
                    let ty = if let Some(ty) = &stmt.ty {
                        ty.clone()
                    } else {
                        self.infer_expr_type(&stmt.value)?
                    };
                    if !is_native_scalar_type(&ty) {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' uses let-binding '{}' with type '{}' which is not supported by native lowering yet",
                                self.function.name, stmt.name, ty
                            ),
                            self.function.span,
                        ));
                    }

                    if self.local_map.contains_key(&stmt.name) {
                        return Err(Diagnostic::error(
                            format!(
                                "function '{}' redefines local '{}' in lowering",
                                self.function.name, stmt.name
                            ),
                            self.function.span,
                        ));
                    }

                    let local = self.locals.len();
                    self.locals.push(MirLocal {
                        name: stmt.name.clone(),
                        ty: ty.clone(),
                    });
                    self.local_map.insert(stmt.name.clone(), local);

                    let value = self.lower_expr(&stmt.value)?;
                    self.emit_store(local, value);
                }
                Statement::Assign(stmt) => {
                    let Some(&local) = self.local_map.get(&stmt.name) else {
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
                if segments.len() != 1 {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' uses path '{}' which is not supported by native lowering yet",
                            self.function.name,
                            segments.join(".")
                        ),
                        self.function.span,
                    ));
                }
                let name = segments[0].as_str();
                let Some(&local) = self.local_map.get(name) else {
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
                if !is_native_scalar_type(&return_ty) {
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
            Expr::RecordLit { .. } | Expr::Match { .. } => Err(Diagnostic::error(
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

        let result_ty = self.infer_if_result_type(then_block, else_block)?;
        if !is_native_scalar_type(&result_ty) {
            return Err(Diagnostic::error(
                format!(
                    "function '{}' if expression returns '{}' which is not supported by native lowering yet",
                    self.function.name, result_ty
                ),
                self.function.span,
            ));
        }

        let then_bb = self.new_block();
        let else_bb = self.new_block();
        let join_bb = self.new_block();

        self.set_terminator(MirTerminator::If {
            cond: cond_operand,
            then_bb: then_bb.clone(),
            else_bb: else_bb.clone(),
        });

        let result_temp = if result_ty.head() == "Unit" {
            None
        } else {
            Some(self.new_temp_local(result_ty.clone()))
        };

        self.switch_to_block(&then_bb);
        let then_value = self.lower_block_value(then_block)?;
        if !self.block_is_terminated(self.current_block) {
            if let Some(temp) = result_temp {
                self.emit_store(temp, then_value);
            }
            self.set_terminator(MirTerminator::Goto {
                target: join_bb.clone(),
            });
        }

        self.switch_to_block(&else_bb);
        let else_value = if let Some(block) = else_block {
            self.lower_block_value(block)?
        } else {
            ExprValue::unit()
        };
        if !self.block_is_terminated(self.current_block) {
            if let Some(temp) = result_temp {
                self.emit_store(temp, else_value);
            }
            self.set_terminator(MirTerminator::Goto {
                target: join_bb.clone(),
            });
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
        self.lower_block_statements(body, BlockMode::LoopBody)?;
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
                if segments.len() != 1 {
                    return Err(Diagnostic::error(
                        format!(
                            "function '{}' uses path '{}' which is not supported by native lowering yet",
                            self.function.name,
                            segments.join(".")
                        ),
                        self.function.span,
                    ));
                }
                let name = segments[0].as_str();
                let Some(&local) = self.local_map.get(name) else {
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

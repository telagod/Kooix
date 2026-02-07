use crate::ast::TypeRef;
use crate::hir::HirProgram;

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
    pub entry: MirBlock,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirParam {
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
    Nop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirTerminator {
    ReturnDefault(TypeRef),
}

pub fn lower_hir(program: &HirProgram) -> MirProgram {
    let functions = program
        .functions
        .iter()
        .map(|function| MirFunction {
            name: function.name.clone(),
            params: function
                .params
                .iter()
                .map(|param| MirParam {
                    name: param.name.clone(),
                    ty: param.ty.clone(),
                })
                .collect(),
            return_type: function.return_type.clone(),
            effects: function
                .effects
                .iter()
                .map(|effect| {
                    if let Some(argument) = effect.argument.as_deref() {
                        format!("{}({argument})", effect.name)
                    } else {
                        effect.name.clone()
                    }
                })
                .collect(),
            entry: MirBlock {
                label: "entry".to_string(),
                statements: Vec::new(),
                terminator: MirTerminator::ReturnDefault(function.return_type.clone()),
            },
        })
        .collect();

    MirProgram { functions }
}

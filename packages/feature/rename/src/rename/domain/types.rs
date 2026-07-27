use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, Path, SymbolName};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameFunctionOccurrence {
    pub path: String,
    pub span: ByteSpan,
    pub text: String,
    pub replacement: String,
}

#[derive(Debug, Clone)]
pub struct RenameFunctionRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub from: SymbolName,
    pub to: SymbolName,
}

#[derive(Debug, Clone)]
pub struct RenameFunctionPlan {
    pub dialect: Dialect,
    pub definitions: Vec<RenameFunctionOccurrence>,
    pub calls: Vec<RenameFunctionOccurrence>,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug, Clone)]
pub struct RenameMacroletRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub from: SymbolName,
    pub to: SymbolName,
}

#[derive(Debug, Clone)]
pub struct RenameMacroletPlan {
    pub dialect: Dialect,
    pub definitions: Vec<RenameFunctionOccurrence>,
    pub calls: Vec<RenameFunctionOccurrence>,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug, Clone)]
pub struct RenameSymbolMacroRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub from: SymbolName,
    pub to: SymbolName,
}

#[derive(Debug, Clone)]
pub struct RenameSymbolMacroPlan {
    pub dialect: Dialect,
    pub definitions: Vec<RenameFunctionOccurrence>,
    pub references: Vec<RenameFunctionOccurrence>,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug, Clone)]
pub struct RenameLocalFunctionRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub from: SymbolName,
    pub to: SymbolName,
}

#[derive(Debug, Clone)]
pub struct RenameLocalFunctionPlan {
    pub dialect: Dialect,
    pub definitions: Vec<RenameFunctionOccurrence>,
    pub calls: Vec<RenameFunctionOccurrence>,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug, Clone)]
pub enum RenameTarget {
    Path(Path),
    Offset(usize),
}

#[derive(Debug, Clone)]
pub struct RenameInFormRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub target: RenameTarget,
    pub from: SymbolName,
    pub to: SymbolName,
}

#[derive(Debug, Clone)]
pub struct RenameInFormPlan {
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub scope_span: ByteSpan,
    pub from: SymbolName,
    pub to: SymbolName,
    pub occurrences: Vec<ByteSpan>,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug, Clone)]
pub struct RenameBindingRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub target: RenameTarget,
    pub from: SymbolName,
    pub to: SymbolName,
}

#[derive(Debug, Clone)]
pub struct RenameBindingPlan {
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub form: String,
    pub form_span: ByteSpan,
    pub binding_span: ByteSpan,
    pub from: SymbolName,
    pub to: SymbolName,
    pub references: Vec<ByteSpan>,
    pub shadowed_scope_count: usize,
    pub rewritten: String,
    pub changed: bool,
}

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, Path, Selection, SymbolName};

pub type ExtractFunctionInsert = crate::domain::extract_shared::TopLevelInsert;

#[derive(Debug, Clone)]
pub struct ExtractFunctionRequest<'a> {
    pub input: &'a str,
    pub selection: Selection<'a>,
    pub path: Option<Path>,
    pub dialect: Dialect,
    pub name: SymbolName,
    pub explicit_params: Vec<String>,
    pub infer_params: bool,
    pub insert: ExtractFunctionInsert,
    pub anchor_path: Option<Path>,
}

#[derive(Debug, Clone)]
pub struct ExtractFunctionPlan {
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub span_start: usize,
    pub span_end: usize,
    pub name: SymbolName,
    pub params: Vec<String>,
    pub inferred_params: Vec<String>,
    pub insert: ExtractFunctionInsert,
    pub anchor_path: Option<Path>,
    pub anchor_span: Option<ByteSpan>,
    pub call: String,
    pub definition: String,
    pub rewritten: String,
    pub changed: bool,
}

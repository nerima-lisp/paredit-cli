use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, Path, SymbolName};

#[derive(Debug, Clone)]
pub struct InlineFunctionRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub definition_path: Path,
    pub call_paths: Vec<Path>,
    pub all_calls: bool,
    pub remove_definition: bool,
    pub allow_duplicate_evaluation: bool,
    pub allow_drop_arguments: bool,
}

#[derive(Debug, Clone)]
pub struct InlineFunctionPlan {
    pub dialect: Dialect,
    pub definition_path: Path,
    pub call_paths: Vec<Path>,
    pub all_calls: bool,
    pub definition_span: ByteSpan,
    pub call_spans: Vec<ByteSpan>,
    pub function_name: SymbolName,
    pub calls: Vec<InlineFunctionCallPlan>,
    pub remove_definition: bool,
    pub definition_removed: bool,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineFunctionCallPlan {
    pub call_path: Path,
    pub call_span: ByteSpan,
    pub parameters: Vec<InlineFunctionParameterPlan>,
    pub replacement: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineFunctionParameterPlan {
    pub name: String,
    pub argument: String,
    pub reference_count: usize,
}

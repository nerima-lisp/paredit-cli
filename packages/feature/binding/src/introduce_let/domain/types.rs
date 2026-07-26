use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path, SymbolName};

#[derive(Debug)]
pub struct IntroduceLetRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub target: ExpressionView,
    pub enclosing_span: ByteSpan,
    pub name: SymbolName,
    pub all_occurrences: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntroduceLetPlan {
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub selected_span: ByteSpan,
    pub enclosing_span: ByteSpan,
    pub name: SymbolName,
    pub binding_value: String,
    pub occurrence_spans: Vec<ByteSpan>,
    pub skipped_shadowed_occurrence_spans: Vec<ByteSpan>,
    pub replacement: String,
    pub rewritten: String,
    pub changed: bool,
}

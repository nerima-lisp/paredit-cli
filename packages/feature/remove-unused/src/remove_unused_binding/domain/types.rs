use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path, SymbolName};

#[derive(Debug, Clone)]
pub struct RemoveUnusedBindingRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub target: ExpressionView,
    pub name: Option<&'a SymbolName>,
    pub all_bindings: bool,
    pub allow_drop_value: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveUnusedBindingPlan {
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub form: String,
    pub form_span: ByteSpan,
    pub binding_name: Option<String>,
    pub binding_span: Option<ByteSpan>,
    pub binding_value: Option<String>,
    pub reference_count: Option<usize>,
    pub bindings: Vec<RemovedBindingPlan>,
    pub dropped_value_requires_review: bool,
    pub replacement: String,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedBindingPlan {
    pub binding_name: String,
    pub binding_span: ByteSpan,
    pub binding_value: String,
    pub reference_count: usize,
}

#[derive(Debug)]
pub struct RemoveUnusedBindingParts {
    pub form: String,
    pub form_span: ByteSpan,
    pub bindings: Vec<RemovedBindingParts>,
    pub replacement: String,
}

#[derive(Debug)]
pub struct RemovedBindingParts {
    pub name: String,
    pub binding_span: ByteSpan,
    pub binding_value: String,
    pub reference_spans: Vec<ByteSpan>,
}

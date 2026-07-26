use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path, SymbolName, SyntaxTree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnthreadStyle {
    First,
    Last,
}

impl UnthreadStyle {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Last => "last",
        }
    }

    pub(super) fn from_operator(operator: &str) -> Option<Self> {
        match operator {
            "->" => Some(Self::First),
            "->>" => Some(Self::Last),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct UnthreadExpressionRequest<'a> {
    pub input: &'a str,
    pub tree: &'a SyntaxTree,
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub target: ExpressionView,
    pub style: Option<UnthreadStyle>,
    pub operator: Option<SymbolName>,
}

#[derive(Debug)]
pub struct UnthreadExpressionPlan {
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub style: UnthreadStyle,
    pub operator: SymbolName,
    pub span: ByteSpan,
    pub base: String,
    pub steps: Vec<UnthreadExpressionStep>,
    pub replacement: String,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug, Clone)]
pub struct UnthreadExpressionStep {
    pub head: String,
    pub argument_count: usize,
    pub insertion_index: usize,
    pub span: ByteSpan,
    pub form: String,
}

#[derive(Debug)]
pub(super) struct PipelineStep {
    pub(super) head: String,
    pub(super) arguments: Vec<String>,
    pub(super) span: ByteSpan,
    pub(super) form: String,
}

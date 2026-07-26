use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path, SymbolName, SyntaxTree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadStyle {
    First,
    Last,
}

impl ThreadStyle {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Last => "last",
        }
    }

    #[must_use]
    pub const fn default_operator(self) -> &'static str {
        match self {
            Self::First => "->",
            Self::Last => "->>",
        }
    }

    pub(super) const fn threaded_child_index(self, child_count: usize) -> usize {
        match self {
            Self::First => 1,
            Self::Last => child_count.saturating_sub(1),
        }
    }
}

#[derive(Debug)]
pub struct ThreadExpressionRequest<'a> {
    pub input: &'a str,
    pub tree: &'a SyntaxTree,
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub target: ExpressionView,
    pub style: ThreadStyle,
    pub operator: SymbolName,
}

#[derive(Debug)]
pub struct ThreadExpressionPlan {
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub style: ThreadStyle,
    pub operator: SymbolName,
    pub span: ByteSpan,
    pub base: String,
    pub steps: Vec<ThreadExpressionStep>,
    pub replacement: String,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug, Clone)]
pub struct ThreadExpressionStep {
    pub head: String,
    pub argument_count: usize,
    pub threaded_argument_index: usize,
    pub span: ByteSpan,
    pub step: String,
}

#[derive(Debug)]
pub(super) struct ThreadExpressionParts {
    pub(super) base: String,
    pub(super) steps: Vec<ThreadExpressionStep>,
}

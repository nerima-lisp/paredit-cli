use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteOffset, ByteSpan, SymbolName};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameAtNamespace {
    Value,
    Function,
    GlobalMacro,
    LocalFunction,
    Macro,
    SymbolMacro,
}

impl RenameAtNamespace {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Value => "value",
            Self::Function => "function",
            Self::GlobalMacro => "global-macro",
            Self::LocalFunction => "local-function",
            Self::Macro => "macro",
            Self::SymbolMacro => "symbol-macro",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenameAtRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub at: ByteOffset,
    pub to: SymbolName,
}

#[derive(Debug, Clone)]
pub struct RenameAtPlan {
    pub dialect: Dialect,
    pub namespace: RenameAtNamespace,
    pub selection_span: ByteSpan,
    pub from: SymbolName,
    pub to: SymbolName,
    pub occurrences: Vec<ByteSpan>,
    pub rewritten: String,
    pub changed: bool,
}

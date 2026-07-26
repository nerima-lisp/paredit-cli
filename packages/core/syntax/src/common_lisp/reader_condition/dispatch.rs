use crate::domain::sexpr::ByteSpan;
#[cfg(test)]
use crate::domain::sexpr::ExpressionPath;

/// The polarity of a Common Lisp reader-conditional dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommonLispReaderConditionalKind {
    Include,
    Exclude,
}

/// One `#+` or `#-` dispatch atom found in a parsed document.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonLispReaderConditionalDispatch {
    pub kind: CommonLispReaderConditionalKind,
    pub path: ExpressionPath,
    pub span: ByteSpan,
}

/// The complete source region consumed by one reader conditional.
///
/// This covers the dispatch atom, feature expression, and guarded datum when
/// all three are present. Incomplete syntax is represented by the dispatch
/// span alone so structural transformations can still reject it safely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonLispReaderConditionalForm {
    pub kind: CommonLispReaderConditionalKind,
    pub dispatch_span: ByteSpan,
    pub span: ByteSpan,
}

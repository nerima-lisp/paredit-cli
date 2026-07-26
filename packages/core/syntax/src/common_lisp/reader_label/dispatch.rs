use crate::domain::sexpr::ByteSpan;
#[cfg(test)]
use crate::domain::sexpr::ExpressionPath;

/// The kind of a Common Lisp reader-label dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommonLispReaderLabelKind {
    Definition,
    Reference,
}

/// One `#n=` or `#n#` dispatch atom found in a parsed document.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonLispReaderLabelDispatch {
    pub kind: CommonLispReaderLabelKind,
    pub path: ExpressionPath,
    pub span: ByteSpan,
}

/// The complete source region consumed by a reader-label dispatch.
///
/// A definition consumes the following datum, while a reference is complete
/// on its own. Incomplete definitions retain their dispatch span so callers
/// can still reject them safely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonLispReaderLabelForm {
    pub kind: CommonLispReaderLabelKind,
    pub dispatch_span: ByteSpan,
    pub span: ByteSpan,
}

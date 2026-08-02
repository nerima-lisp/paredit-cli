use crate::sexpr::ByteSpan;
#[cfg(test)]
use crate::sexpr::ExpressionPath;

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
/// `span` always covers all three components — the dispatch, the feature
/// expression, and the guarded datum — because the reader consumes all three
/// as one node and refuses the document when any is missing. There is no
/// dispatch-span-only variant to handle: incomplete syntax never reaches a
/// tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonLispReaderConditionalForm {
    pub kind: CommonLispReaderConditionalKind,
    pub dispatch_span: ByteSpan,
    pub span: ByteSpan,
}

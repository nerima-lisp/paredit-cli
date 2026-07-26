mod query;

pub use query::common_lisp_reader_literals;
pub(crate) use query::reader_literal_kind;

use crate::domain::sexpr::ByteSpan;

/// A literal datum constructed by the Common Lisp reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommonLispReaderLiteral {
    pub kind: CommonLispReaderLiteralKind,
    pub span: ByteSpan,
}

/// The reader syntax that constructs a Common Lisp literal datum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommonLispReaderLiteralKind {
    /// `#(...)`, which reads as a vector rather than a callable list.
    Vector,
}

//! Typed S-expression parsing, tree navigation, spans, and balanced edit
//! primitives that back both the CLI and downstream Rust automation.

mod edit;
pub mod error;
mod formatter;
mod parser;
pub mod reader;
mod reader_policy;

pub(crate) use reader_policy::lang_directive_language as reader_policy_lang_directive;
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
mod tree;
mod types;

pub use edit::Edit;
pub use error::{
    PathError, SelectionError, SexprError, SexprResult, SpanError, StructureError, SymbolError,
};
pub use formatter::Formatter;
pub use parser::ParseError;
pub use tree::AtomOccurrenceIndex;
pub use tree::{
    AtomOccurrence, ExpressionKind, ExpressionView, OutlineEntry, ReaderPrefix, Selection,
    SourceComment, SyntaxTree,
};
pub use types::NonEmptyExpressionPath;
pub use types::{
    ByteOffset, ByteSpan, ChildIndex, Delimiter, ExpressionPath, NodeId, Path, SymbolName,
};

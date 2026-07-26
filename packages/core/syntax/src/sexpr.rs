//! Typed S-expression parsing, tree navigation, spans, and balanced edit
//! primitives that back both the CLI and downstream Rust automation.

mod edit;
mod formatter;
mod parser;
pub mod reader;
mod reader_policy;
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
mod tree;
mod types;

pub use edit::Edit;
pub use formatter::Formatter;
pub use parser::ParseError;
pub(in crate::domain) use tree::AtomOccurrenceIndex;
pub use tree::{
    AtomOccurrence, ExpressionKind, ExpressionView, OutlineEntry, ReaderPrefix, Selection,
    SyntaxTree,
};
pub(in crate::domain) use types::NonEmptyExpressionPath;
pub use types::{
    ByteOffset, ByteSpan, ChildIndex, Delimiter, ExpressionPath, NodeId, Path, SymbolName,
};

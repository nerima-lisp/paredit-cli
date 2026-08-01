//! Typed S-expression parsing, tree navigation, spans, and balanced edit
//! primitives that back both the CLI and downstream Rust automation.

mod cursor_edit;
mod edit;
pub mod error;
mod formatter;
mod navigation;
mod parser;
mod quote_edit;
pub mod reader;
mod reader_policy;
mod reader_prefix_edit;
mod reindent;
mod string_edit;
mod structural_edit;

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
pub use formatter::{Formatter, ReaderPrefixStyle, STYLE_NAMES, UnknownStyleName};
pub use navigation::{ContextKind, Direction, SourceContext};
pub use parser::ParseError;
pub use quote_edit::QuoteStyle;
pub use reindent::body_form_distinguished;
pub use structural_edit::Placement;
pub use tree::AtomOccurrenceIndex;
pub use tree::{
    AtomOccurrence, ExpressionKind, ExpressionView, OutlineEntry, ReaderPrefix, Selection,
    SourceComment, SyntaxTree,
};
pub use types::NonEmptyExpressionPath;
pub use types::{
    ByteOffset, ByteSpan, ChildIndex, Delimiter, ExpressionPath, NodeId, Path, SymbolName,
};

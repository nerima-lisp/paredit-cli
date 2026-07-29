//! Typed failures for selecting and editing S-expressions.
//!
//! Section 9.2: `anyhow::Error` is type-erased, so a caller can only tell what
//! went wrong by reading the message. This package is where that cost was
//! easiest to see. [`Edit::split`](super::edit::Edit::split) and its siblings
//! refuse for two unrelated reasons — the *shape* is wrong, or the *selection*
//! does not belong to this tree — and the old code told them apart by
//! `error.to_string().starts_with("input ")`. A prefix match on a human
//! message was load-bearing control flow.
//!
//! The failures divide into four kinds, matching what a caller can do:
//!
//! - **`Structure`** — the edit does not apply to the shape that is there.
//!   Raising a top-level form, splitting something that is not inside a list,
//!   joining two lists with different delimiters. The operation is saying "not
//!   here", and a caller can reasonably suggest a different selection.
//! - **`Selection`** — the selection does not belong to this tree, or the tree
//!   does not match the source it was built from. That is a stale handle or a
//!   programming error; no change of selection helps.
//! - **`Symbol`** / **`Path`** — a name or an address is not well-formed. These
//!   arrive from `FromStr`, so they are user input rather than tree state.
//! - **`Parse`** — the source did not parse. Already typed as [`ParseError`];
//!   it is carried through transparently.
//!
//! Messages are reproduced exactly. Section 9.2's goal is type-level
//! distinction, not better wording, and the CLI's string assertions and the
//! `inspect capabilities` golden both depend on the current text.

use thiserror::Error;

use super::parser::ParseError;

/// The requested edit does not fit the structure it was pointed at.
///
/// Every variant is a refusal about the tree's shape, so a caller can treat
/// the whole enum as "try a different selection" without matching variant by
/// variant.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StructureError {
    #[error("cannot raise a top-level expression")]
    RaiseTopLevel,

    #[error("root document cannot be edited directly")]
    RootNotEditable,

    #[error("selected node has no parent")]
    NoParent,

    #[error("selected expression is not inside a list")]
    NotInsideList,

    #[error("operation requires a list expression")]
    NotAList,

    #[error("selected expression has no enclosing list")]
    NoEnclosingListForSelection,

    #[error("selection has no enclosing list")]
    NoEnclosingList,

    // --- transpose ---
    #[error("selected expression has no next sibling to transpose")]
    NoNextSiblingToTranspose,

    #[error("selected expression has no previous sibling to transpose")]
    NoPreviousSiblingToTranspose,

    // --- slurp / barf ---
    #[error("selected list has no next sibling to slurp")]
    NoNextSiblingToSlurp,

    #[error("selected list has no previous sibling to slurp")]
    NoPreviousSiblingToSlurp,

    #[error("cannot barf from an empty list")]
    BarfFromEmptyList,

    // --- split ---
    #[error("split requires an expression directly inside a list")]
    SplitNotDirectlyInList,

    #[error("cannot split a list carrying a reader prefix")]
    SplitReaderPrefix,

    #[error("selected expression has no enclosing list to split")]
    NoEnclosingListToSplit,

    #[error("cannot split before the first element of a list")]
    SplitBeforeFirstElement,

    // --- join ---
    #[error("cannot join a list carrying a reader prefix")]
    JoinReaderPrefix,

    #[error("join requires the next sibling to also be a list")]
    JoinSiblingNotList,

    #[error("cannot join into a list carrying a reader prefix")]
    JoinIntoReaderPrefix,

    #[error("cannot join lists that use different delimiters")]
    JoinDelimiterMismatch,

    #[error("cannot join strings carrying a reader prefix")]
    JoinStringReaderPrefix,

    #[error("join only merges two adjacent lists or two adjacent strings")]
    JoinUnsupportedPair,

    #[error("selected expression has no next sibling to join")]
    NoNextSiblingToJoin,

    // --- convolute ---
    #[error("convolute requires the selected list to be nested inside a list")]
    ConvoluteNotNested,

    #[error("convolute requires the selected list to be two lists deep")]
    ConvoluteNotTwoDeep,

    #[error("cannot convolute lists carrying a reader prefix")]
    ConvoluteReaderPrefix,

    #[error("cannot convolute a form with comments outside the selected list")]
    ConvoluteCommentsOutside,

    #[error("selected list has no enclosing list to convolute")]
    NoEnclosingListToConvolute,

    #[error("selected list is not a direct child of its enclosing list")]
    NotDirectChildOfEnclosing,

    #[error("enclosing list is not a direct child of the outer list")]
    EnclosingNotDirectChildOfOuter,

    // --- delimiters and children ---
    #[error("selected list is missing an opening delimiter")]
    MissingOpenDelimiter,

    #[error("selected list is missing a closing delimiter")]
    MissingCloseDelimiter,

    #[error("enclosing list is missing a delimiter")]
    EnclosingListMissingDelimiter,

    #[error("outer list is missing a delimiter")]
    OuterListMissingDelimiter,

    #[error("enclosing list has no children to keep")]
    EnclosingListHasNoChildren,

    #[error("nothing precedes the selection to keep")]
    NothingPrecedesSelection,

    // --- reader prefixes ---
    #[error("selected expression carries no reader prefix to unwrap")]
    NoReaderPrefixToUnwrap,

    // --- raise --levels ---
    #[error("cannot raise {requested} levels: the selection is only {available} levels deep")]
    RaiseLevelsExceedDepth { requested: usize, available: usize },

    // --- transpose between arbitrary siblings ---
    #[error("transpose requires two expressions inside the same list")]
    TransposeNotSiblings,

    #[error("cannot transpose an expression with itself")]
    TransposeSameExpression,

    // --- navigation ---
    #[error("selected expression has no next sibling")]
    NoNextSibling,

    #[error("selected expression has no previous sibling")]
    NoPreviousSibling,

    #[error("selected expression has no enclosing expression to move up to")]
    NoEnclosingExpression,

    #[error("selected expression has no child expression to move down into")]
    NoChildExpression,

    // --- strings ---
    #[error("operation requires a string literal")]
    NotAStringLiteral,

    #[error("byte offset {offset} is not inside a string literal")]
    NotInsideStringLiteral { offset: usize },

    #[error("cannot split a string at its own delimiter")]
    SplitStringAtDelimiter,

    #[error("cannot split a string inside an escape sequence")]
    SplitStringInEscape,

    #[error("cannot unescape `\\{character}`: unescape only reverses \\\\ and \\\"")]
    UnescapeUnsupportedSequence { character: char },

    #[error("string literal ends with a dangling backslash")]
    UnescapeDanglingBackslash,

    #[error("cannot wrap a string literal carrying a reader prefix")]
    StringReaderPrefix,

    // --- cursor edits ---
    #[error("byte offset {offset} is outside the document, which is {length} bytes")]
    OffsetOutsideDocument { offset: usize, length: usize },

    #[error("nothing to delete at byte offset {offset}")]
    NothingToDelete { offset: usize },

    #[error("refusing to delete {delimiter}: it would unbalance the enclosing form")]
    DeleteWouldUnbalance { delimiter: char },

    #[error("refusing to delete the whitespace that keeps two symbols apart")]
    DeleteWouldFuseSymbols,

    #[error("refusing to delete the character that opens a comment")]
    DeleteWouldUncomment,

    #[error("cannot insert a newline inside {context}")]
    NewlineInsideOpaqueText { context: &'static str },
}

/// A byte span that cannot safely index the source it is applied to.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SpanError {
    #[error("span start {start} exceeds end {end}")]
    StartExceedsEnd { start: usize, end: usize },

    #[error("span end {end} exceeds input length {length}")]
    EndExceedsInput { end: usize, length: usize },

    #[error("span is not aligned to UTF-8 character boundaries")]
    NotCharBoundary,
}

/// The selection does not refer to this tree, or this source.
///
/// Distinct from [`StructureError`] because it means the caller is holding the
/// wrong handle rather than pointing at the wrong node.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SelectionError {
    #[error("input does not match the source used to build the selection")]
    SourceMismatch,

    #[error("selection belongs to a different syntax tree")]
    TreeMismatch,

    #[error("selected span is invalid: {source}")]
    InvalidSpan {
        #[source]
        source: SpanError,
    },

    #[error("no expression contains byte offset {offset}")]
    NoExpressionAtOffset { offset: usize },

    #[error("path {path} is not reachable")]
    PathNotReachable { path: String },

    /// `detail` is prose describing the arity of the form that was indexed —
    /// it varies per path, so it cannot be a variant.
    #[error("path segment {segment} is out of range: {detail}")]
    PathSegmentOutOfRange { segment: usize, detail: String },
}

/// An expression path that does not parse.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PathError {
    #[error("invalid path segment: {segment}")]
    InvalidSegment { segment: String },
}

/// A symbol name that is not a symbol name.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SymbolError {
    #[error("symbol must not be empty")]
    Empty,

    #[error("symbol contains reader delimiter or whitespace: {value}")]
    ReaderDelimiterOrWhitespace { value: String },
}

/// Anything the sexpr layer can refuse to do.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum SexprError {
    #[error(transparent)]
    Structure(#[from] StructureError),

    #[error(transparent)]
    Selection(#[from] SelectionError),

    #[error(transparent)]
    Path(#[from] PathError),

    #[error(transparent)]
    Symbol(#[from] SymbolError),

    #[error(transparent)]
    Parse(#[from] ParseError),

    /// A selection failure surfaced by an [`Edit`](super::edit::Edit) entry
    /// point, which names the operation in the message.
    ///
    /// This variant exists because the wording is part of the CLI's contract,
    /// not because the failure differs: the inner [`SelectionError`] is the
    /// whole content, and matching on it is what replaced the old
    /// `starts_with("input ")` test.
    #[error("edit {source}")]
    EditSelection {
        #[source]
        source: SelectionError,
    },
}

impl SexprError {
    /// The deepest error in the chain, as `anyhow::Error::root_cause` returned.
    #[must_use]
    pub fn root_cause(&self) -> &(dyn std::error::Error + 'static) {
        let mut cause: &(dyn std::error::Error + 'static) = self;
        while let Some(source) = std::error::Error::source(cause) {
            cause = source;
        }
        cause
    }
}

/// The result type the sexpr selection and edit entry points return.
pub type SexprResult<T> = std::result::Result<T, SexprError>;

//! Typed failures for the selector layer.
//!
//! A selector is user input that names *which form* a command acts on, so the
//! failures here split along what the caller can do about them:
//!
//! - [`PatternError`] — the `--query` pattern does not parse. Fix the pattern.
//! - [`SelectorError`] — the pattern parsed but resolved to nothing, to more
//!   than the command can accept, or to a coordinate that is not in the file.
//!
//! [`SelectorError::Ambiguous`] is the one worth naming out loud: a selector
//! that matches twelve forms is not an error for `inspect resolve` and *is* an
//! error for `edit kill`, so the arity check lives with the caller's policy
//! rather than inside resolution.

use thiserror::Error;

use crate::sexpr::SexprError;

/// A `--query` pattern that is not a well-formed pattern.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PatternError {
    #[error("pattern is empty")]
    Empty,

    #[error("pattern has {count} top-level forms; a pattern must be exactly one form")]
    NotOneForm { count: usize },

    /// The pattern is read with the same reader as source, so a malformed
    /// pattern fails the same way a malformed file does. Carrying the reader's
    /// own message keeps `--query '(defun'` explaining itself.
    #[error("pattern does not read as an S-expression: {detail}")]
    Malformed { detail: String },

    #[error("pattern nests {depth} deep; the limit is {limit}")]
    TooDeep { depth: usize, limit: usize },

    #[error("capture name is empty in `{token}`")]
    EmptyCaptureName { token: String },

    #[error("unknown capture kind `{kind}` in `{token}`: expected one of {expected}")]
    UnknownCaptureKind {
        kind: String,
        token: String,
        expected: String,
    },

    #[error("a list pattern may hold at most one `...`; found {count}")]
    MultipleEllipses { count: usize },

    #[error("`...` is only meaningful inside a list pattern")]
    TopLevelEllipsis,

    #[error("capture `?{name}` is bound twice with different kinds")]
    ConflictingCaptureKind { name: String },
}

/// A `--rewrite` template that cannot be paired with its `--query` pattern.
///
/// Separate from [`PatternError`] even where the two overlap — both refuse an
/// empty input and a two-form input — because the caller's fix differs. "The
/// pattern is empty" sends someone to `--query`; the same words about a
/// template send them to the wrong flag.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RewriteError {
    #[error("rewrite template is empty")]
    Empty,

    #[error("rewrite template has {count} top-level forms; a template must be exactly one form")]
    NotOneForm { count: usize },

    #[error("rewrite template does not read as an S-expression: {detail}")]
    Malformed { detail: String },

    /// A `?…` spelling that the pattern language itself rejects, reported
    /// through the pattern's own message so the two agree on the grammar.
    #[error("rewrite template placeholder is not well formed: {0}")]
    Placeholder(#[source] PatternError),

    #[error(
        "rewrite template writes `...`, which names no capture; \
         write `?name...` and bind the same name in the pattern"
    )]
    AnonymousRest,

    #[error("rewrite template uses `?{name}`, which the pattern does not bind; it binds {bound}")]
    UnboundCapture { name: String, bound: String },
}

/// A selector that cannot be resolved against the parsed document.
///
/// Not `Clone`, because [`Self::Sexpr`] carries a `SexprError` that is not:
/// the tree layer's failures are moved through, never duplicated.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum SelectorError {
    #[error("no selector: pass one of {expected}")]
    Missing { expected: String },

    #[error("pass only one selector; got {first} and {second}")]
    Conflict { first: String, second: String },

    #[error(transparent)]
    Pattern(#[from] PatternError),

    #[error("no form matches {selector}")]
    NoMatch { selector: String },

    #[error(
        "{selector} matches {count} forms; pass --all to act on every match, \
         or narrow the selector (see `paredit inspect resolve`)"
    )]
    Ambiguous { selector: String, count: usize },

    #[error("--capture {name} is not bound by this pattern; it binds {bound}")]
    UnknownCapture { name: String, bound: String },

    #[error("line {line} is past the end of the input, which has {line_count} lines")]
    LineOutOfRange { line: usize, line_count: usize },

    #[error("column {column} is past the end of line {line}, which has {width} columns")]
    ColumnOutOfRange {
        line: usize,
        column: usize,
        width: usize,
    },

    #[error("line and column are 1-based; got {value}")]
    ZeroLineOrColumn { value: String },

    #[error("expected LINE or LINE:COLUMN, got `{value}`")]
    MalformedPosition { value: String },

    #[error("unknown selector prefix in `{value}`: expected one of {expected}")]
    UnknownSelectorPrefix { value: String, expected: String },

    #[error("`{value}` is not a valid {kind} selector: {detail}")]
    MalformedSelector {
        kind: String,
        value: String,
        detail: String,
    },

    #[error("no form carries selector id {id}")]
    UnknownStableId { id: String },

    #[error(
        "selector id must be 16 lowercase hex characters, optionally prefixed `sel:`; got `{id}`"
    )]
    MalformedStableId { id: String },

    #[error("--parent moved above the top level after {depth} of {requested} steps")]
    ParentAboveRoot { depth: usize, requested: usize },

    #[error("--child {index} is out of range: the selected form has {count} children")]
    ChildOutOfRange { index: usize, count: usize },

    #[error("--sibling {offset} is out of range: the selected form has {count} siblings")]
    SiblingOutOfRange { offset: isize, count: usize },

    #[error("--sibling requires a form inside a list")]
    SiblingWithoutParent,

    #[error("--from and --to must select forms inside the same list")]
    RangeParentMismatch,

    #[error("--from must select a form at or before --to")]
    RangeReversed,

    #[error("{command} acts on one form; --from/--to selects a range of {count} forms")]
    RangeUnsupported { command: String, count: usize },

    #[error(transparent)]
    Sexpr(#[from] SexprError),
}

/// The result type selector resolution returns.
pub type SelectorResult<T> = std::result::Result<T, SelectorError>;

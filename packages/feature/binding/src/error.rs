//! Why a binding-form refactor or report refuses to run.
//!
//! Section 9.2. Three kinds, and they are genuinely different questions:
//!
//! - [`BindingFormShapeError`] — the binding list is not written the way this
//!   dialect writes binding lists, or a binding in it is not a shape the tool
//!   reads. Shared with `paredit_core_semantics::BindingFormError`, which asks
//!   the same question of the same forms; the wordings differ, so the variants
//!   stay separate.
//! - [`BindingContextError`] — the *position* the form sits in is one the
//!   rewrite will not touch: a top-level `progn`, an operator position, inside
//!   a reader template or a declaration. Not about the form at all.
//! - [`BindingCaptureError`] — rewriting would change which binding a name
//!   refers to. The only kind that is about meaning rather than syntax.

use thiserror::Error;

use paredit_core_edit::EditRefusal;
use paredit_core_syntax::sexpr::{SexprError, SymbolError};

/// The binding list, or a binding in it, is not a shape this tool reads.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BindingFormShapeError {
    #[error("{command} selected form is malformed")]
    MalformedForm { command: &'static str },

    #[error("{command} requires a plain binding list")]
    NotPlainBindingList { command: &'static str },

    #[error("{command} requires a termination clause")]
    MissingTerminationClause { command: &'static str },

    #[error("{command} requires unique binding names")]
    DuplicateBindingNames { command: &'static str },

    #[error("{command} requires plain variable bindings")]
    NotPlainVariableBindings { command: &'static str },

    #[error("{command} rejects destructuring or malformed bindings")]
    Destructuring { command: &'static str },

    #[error("{command} requires a plain binding name")]
    NotPlainBindingName { command: &'static str },

    #[error("invalid binding name")]
    InvalidBindingName {
        #[source]
        source: SymbolError,
    },

    // --- let-report's reader, which describes rather than rewrites ---
    #[error("dialect expects vector let bindings: [name value ...]")]
    ExpectedVectorBindings,

    #[error("vector let binding form must contain name/value pairs")]
    VectorBindingsNotPaired,

    #[error("dialect expects list-pair let bindings: ((name value) ...)")]
    ExpectedListPairBindings,

    #[error("let binding must be a symbol or a (name value) pair")]
    BindingNotASymbolOrPair,

    #[error("let binding pair must contain a name and value")]
    BindingPairIncomplete,

    #[error("let binding name must be an atom")]
    BindingNameNotAnAtom,
}

/// The form is fine; where it sits is not somewhere this rewrite will touch.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BindingContextError {
    #[error("flatten-progn refuses to rewrite a top-level progn")]
    TopLevelProgn,

    #[error("flatten-progn refuses to rewrite an operator position")]
    OperatorPosition,

    #[error("flatten-progn refuses to rewrite inside a reader template")]
    InsideReaderTemplate,

    #[error("flatten-progn refuses to rewrite inside a declaration")]
    InsideDeclaration,

    #[error("eliminate-empty-binding-form refuses top-level forms")]
    EliminateTopLevel,

    #[error("refuses reader-prefixed contexts")]
    ReaderPrefixed,

    #[error("non-empty path")]
    EmptyPath,

    #[error("known expression context required")]
    UnknownContext,

    #[error("eliminate-empty-binding-form requires a known expression position")]
    UnknownPosition,
}

/// The rewrite would change which binding a name refers to.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BindingCaptureError {
    #[error("{role} for '{name}' references earlier binding '{earlier}'")]
    ReferencesEarlier {
        role: String,
        name: String,
        earlier: String,
    },

    /// `introduce-let` would bind a name that is already bound where the
    /// target sits, so the new binding would shadow the old one.
    #[error(
        "introduce-let target is inside an existing binding for '{name}'; choose a different --name"
    )]
    WouldShadow { name: String },
}

/// Anything a binding-form refactor or report can refuse to do.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum BindingError {
    /// A refusal this package shares with every other structural edit.
    #[error(transparent)]
    Edit(#[from] EditRefusal),

    #[error(transparent)]
    Shape(#[from] BindingFormShapeError),

    #[error(transparent)]
    Context(#[from] BindingContextError),

    #[error(transparent)]
    Capture(#[from] BindingCaptureError),

    /// The source did not parse, or a reader conditional made the rewrite
    /// unsafe. Both come from below this package and are carried whole.
    #[error(transparent)]
    Parse(#[from] paredit_core_syntax::sexpr::ParseError),

    #[error(transparent)]
    ReaderConditional(#[from] paredit_core_edit::mutation_safety::ReaderConditionalSafetyError),

    /// `--binding-index` is one-based; zero is not an index.
    #[error(transparent)]
    BindingIndex(#[from] paredit_core_semantics::BindingIndexError),

    /// The dialect table does not verify this operation.
    #[error("{operation} is not supported for this dialect")]
    DialectDoesNotSupport {
        operation: &'static str,
        #[source]
        source: paredit_core_syntax::dialect::UnsupportedSemanticOperation,
    },
}

// `From` does not chain.
macro_rules! from_edit_refusal {
    ($($ty:ident),+ $(,)?) => {
        $(impl From<paredit_core_edit::$ty> for BindingError {
            fn from(error: paredit_core_edit::$ty) -> Self {
                Self::Edit(error.into())
            }
        })+
    };
}

from_edit_refusal!(
    DialectRefusal,
    DocumentRefusal,
    ConservativeRefusal,
    ShapeRefusal,
    BindingRefusal,
);

impl From<SexprError> for BindingError {
    fn from(error: SexprError) -> Self {
        Self::Edit(error.into())
    }
}

/// The result type the binding refactors and reports return.
pub type BindingResult<T> = std::result::Result<T, BindingError>;

// States which documented error code each binding refusal earns.
//
// The families were drawn for the reader; they turn out to answer the
// caller's question too, which is why this is one arm each and not a
// per-variant table.
paredit_core_cli::impl_classified_refusal!(BindingError, |error| match error {
    BindingError::Edit(edit) => paredit_core_cli::diagnosis::code_for_edit_refusal(edit),

    // "requires a plain binding list", "rejects destructuring", "refuses to
    // rewrite a top-level progn", "references earlier binding" — every one is
    // the tool being conservative about a form that is itself valid. The
    // action is to select a different form, or to rewrite by hand.
    BindingError::Shape(_)
    | BindingError::Context(_)
    | BindingError::Capture(_)
    | BindingError::ReaderConditional(_) =>
        paredit_core_cli::diagnosis::ErrorCode::InputShapeRefused,

    BindingError::Parse(_) => paredit_core_cli::diagnosis::ErrorCode::InputUnparsable,

    // `--binding-index` is one-based and zero was passed: a command-line
    // problem, fixable only by changing the command line.
    BindingError::BindingIndex(_) =>
        paredit_core_cli::diagnosis::ErrorCode::ArgumentFlagCombination,

    BindingError::DialectDoesNotSupport { .. } =>
        paredit_core_cli::diagnosis::ErrorCode::InputDialectUnsupported,
});

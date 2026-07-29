//! Why an extraction refuses to run.
//!
//! Section 9.2. Extraction is the one refactor family whose refusals are
//! mostly about **what would change meaning if the form moved**, rather than
//! about the shape it is pointed at. A constant lifted out of a quasiquote is
//! not the same constant; a `return-from` lifted into a new local function
//! targets a block that is no longer in scope. Those are the interesting
//! variants here, and they had no type before.
//!
//! The shape and dialect checks reuse `paredit_core_edit`'s vocabulary, since
//! they are the same refusals every other structural edit makes.

use thiserror::Error;

/// The selected form cannot be lifted out of where it is.
///
/// These are the capture and scope checks: moving the form would change which
/// binding, block, or tag a name refers to.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExtractionScopeError {
    #[error("extract-constant cannot select inside quote or quasiquote")]
    InsideQuote,

    #[error("extract-constant cannot select a definition head")]
    DefinitionHead,

    #[error("extract-constant cannot select an entire top-level form")]
    EntireTopLevelForm,

    #[error("extract-local-function target cannot be inside a structural binding position")]
    StructuralBindingPosition,

    #[error("extract-local-function target cannot be in a list head position")]
    ListHeadPosition,

    #[error(
        "local function name '{name}' would capture an existing call or function designator in the enclosing list"
    )]
    NameWouldCapture { name: String },

    #[error("extract-local-function cannot move {head} across a function boundary")]
    CrossesFunctionBoundary { head: String },
}

/// The path or selection does not identify something extractable.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExtractionTargetError {
    #[error("selected expression path could not be resolved")]
    PathUnresolved,

    #[error("path {path} is out of range")]
    PathOutOfRange { path: String },

    #[error("extract-constant target path has no parent")]
    TargetHasNoParent,

    #[error("extract-constant target path is empty")]
    TargetPathEmpty,

    #[error("extract-local-function requires a path selection")]
    RequiresPathSelection,

    #[error("extract-local-function paths and selections must refer to the input tree")]
    SelectionFromAnotherTree,

    #[error("extract-local-function enclosing selection must be a list")]
    EnclosingNotAList,

    #[error("extract-local-function target must be a proper descendant of the enclosing list")]
    NotAProperDescendant,

    /// Keeps the dialect's own verification failure as the source, which
    /// `.context()` did — the message alone does not say which operation the
    /// dialect table refused.
    #[error("extract-function is not supported for this dialect")]
    DialectDoesNotSupportExtractFunction {
        #[source]
        source: paredit_core_syntax::dialect::UnsupportedSemanticOperation,
    },
}

/// Anything an extraction can refuse to do.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum ExtractionError {
    /// A refusal this package shares with every other structural edit.
    #[error(transparent)]
    Edit(#[from] paredit_core_edit::EditRefusal),

    #[error(transparent)]
    Scope(#[from] ExtractionScopeError),

    #[error(transparent)]
    Target(#[from] ExtractionTargetError),

    /// The source did not parse, or a reader conditional made the rewrite
    /// unsafe. Both come from below this package and are carried whole.
    #[error(transparent)]
    Parse(#[from] paredit_core_syntax::sexpr::ParseError),

    #[error(transparent)]
    ReaderConditional(#[from] paredit_core_edit::mutation_safety::ReaderConditionalSafetyError),
}

// `From` does not chain, so the sub-enums this package raises directly need
// the middle step spelled out.
macro_rules! from_edit_refusal {
    ($($ty:ident),+ $(,)?) => {
        $(impl From<paredit_core_edit::$ty> for ExtractionError {
            fn from(error: paredit_core_edit::$ty) -> Self {
                Self::Edit(error.into())
            }
        })+
    };
}

from_edit_refusal!(DialectRefusal, DocumentRefusal, InsertionRefusal);

// Same for the syntax layer's sub-enums, which reach here through SexprError.
macro_rules! from_sexpr_error {
    ($($ty:ident),+ $(,)?) => {
        $(impl From<paredit_core_syntax::sexpr::$ty> for ExtractionError {
            fn from(error: paredit_core_syntax::sexpr::$ty) -> Self {
                Self::Edit(paredit_core_syntax::sexpr::SexprError::from(error).into())
            }
        })+
    };
}

from_sexpr_error!(SelectionError, PathError, SymbolError, StructureError);

impl From<paredit_core_syntax::sexpr::SexprError> for ExtractionError {
    fn from(error: paredit_core_syntax::sexpr::SexprError) -> Self {
        Self::Edit(error.into())
    }
}

/// The result type the extraction planners return.
pub type ExtractionResult<T> = std::result::Result<T, ExtractionError>;

// States which documented error code each extraction refusal earns.
paredit_core_cli::impl_classified_refusal!(ExtractionError, |error| match error {
    ExtractionError::Edit(edit) => paredit_core_cli::diagnosis::code_for_edit_refusal(edit),

    // "cannot select inside quote", "cannot select a definition head": the
    // selection landed somewhere this extraction will not work from.
    ExtractionError::Scope(_) | ExtractionError::ReaderConditional(_) =>
        paredit_core_cli::diagnosis::ErrorCode::InputShapeRefused,

    // The path itself does not name a form in this tree.
    ExtractionError::Target(_) => paredit_core_cli::diagnosis::ErrorCode::SelectionPathNotReachable,

    ExtractionError::Parse(_) => paredit_core_cli::diagnosis::ErrorCode::InputUnparsable,
});

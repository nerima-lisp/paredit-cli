//! Why a form transformation refuses to run.
//!
//! Section 9.2. This package holds six independent transformations —
//! `replace-forms`, `sort-definitions`, `split-file`, `thread-expression`,
//! `unthread-expression`, `unwrap-call` — and unlike `core/edit` they share
//! almost no wording. What they *do* share is the shape of their refusals:
//!
//! - [`TransformSelectorError`] — the `--path`/`--name`/`--kind` selectors did
//!   not pick anything out, or picked out the same thing twice. Raised before
//!   any rewriting; the fix is always a different selector.
//! - [`TransformTargetError`] — the selected form is not the shape the
//!   transformation operates on. A `thread-expression` target that is not a
//!   call, an `unwrap-call` target with no atom head, an `unthread-expression`
//!   pipeline with no steps.
//! - Parse and dialect refusals reuse `paredit_core_edit`'s vocabulary, as
//!   everywhere else.

use thiserror::Error;

use paredit_core_edit::EditRefusal;
use paredit_core_syntax::sexpr::SexprError;

/// The transformation is not defined for this dialect.
///
/// Four separate variants for what is arguably one refusal, because the five
/// transformations word it four different ways and §9.2.1 forbids tidying the
/// wording during a type change. The duplication is now visible in one place,
/// which is the first step to removing it.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransformDialectError {
    #[error("sort-definitions does not support the unknown dialect")]
    SortDefinitionsUnknown,

    #[error("{operation} does not support dialect unknown")]
    Unknown { operation: &'static str },

    #[error("{operation} requires a known dialect")]
    RequiresKnown { operation: &'static str },
}

/// The `--path`/`--name`/`--kind` selectors did not identify work to do.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransformSelectorError {
    #[error("invalid --path {path}")]
    InvalidPath { path: String },

    #[error("replace-forms requires at least one --path")]
    ReplaceFormsNeedsPath,

    #[error("replace-forms paths must not overlap: {first} and {second}")]
    OverlappingPaths { first: String, second: String },

    #[error("split-file requires at least one --path, --name, or --kind selector")]
    SplitFileNeedsSelector,

    #[error("duplicate split-file path: {path}")]
    DuplicateSplitPath { path: String },

    #[error("top-level path {path} is out of range")]
    TopLevelPathOutOfRange { path: String },

    #[error("split-file --name did not match a top-level definition: {name}")]
    NameDidNotMatch { name: String },

    #[error("split-file --kind did not match any top-level definitions: {kinds}")]
    KindDidNotMatch { kinds: String },

    #[error("split-file selectors did not match any top-level definitions")]
    NothingSelected,

    #[error("{command} requires a top-level path, for example --path 2")]
    NotATopLevelPath { command: &'static str },

    #[error("refusing overlapping rewrite spans")]
    OverlappingRewriteSpans,

    #[error("argument index {index} is out of range for {count} argument(s)")]
    ArgumentIndexOutOfRange { index: usize, count: usize },

    #[error("duplicate --path: {path}")]
    DuplicatePath { path: String },

    #[error(
        "replace-forms --require-same-shape expected all selected forms to share shape; {path} differs"
    )]
    ShapeMismatch { path: String },

    #[error("replace-forms input does not match the source used to build the syntax tree")]
    InputDoesNotMatchTree,

    #[error("--with must contain exactly one top-level S-expression")]
    WithNotOneForm,
}

/// The selected form is not the shape this transformation operates on.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransformTargetError {
    // --- split-file ---
    #[error("selected top-level form is not a list definition: {path}")]
    NotAListDefinition { path: String },

    #[error("selected top-level form is not recognized as a definition at {path}: {head}")]
    NotADefinition { path: String, head: String },

    // --- thread-expression ---
    #[error("thread-expression target must be a parenthesized call with arguments")]
    ThreadTargetNotACall,

    #[error("thread-expression target must start with an atom head")]
    ThreadTargetHeadNotAnAtom,

    #[error("thread-expression target is missing the threaded argument")]
    ThreadTargetMissingArgument,

    #[error("thread-expression selection is already threaded with {operator}")]
    AlreadyThreaded { operator: String },

    #[error("thread-expression target did not produce any pipeline steps")]
    ThreadProducedNoSteps,

    // --- unthread-expression ---
    #[error("unthread-expression target must be a parenthesized threading pipeline")]
    UnthreadTargetNotAPipeline,

    #[error("unthread-expression target must start with an atom operator")]
    UnthreadTargetHeadNotAnAtom,

    #[error("unthread-expression operator mismatch: selected {head}, expected {expected}")]
    UnthreadOperatorMismatch { head: String, expected: String },

    #[error(
        "unthread-expression operator {operator} is not a recognized threading operator (->, ->>); pass --operator to confirm a custom threading macro"
    )]
    UnthreadOperatorUnrecognized { operator: String },

    #[error("unthread-expression custom operator {operator} requires --style")]
    UnthreadCustomOperatorNeedsStyle { operator: String },

    #[error("unthread-expression pipeline must contain a base and at least one step")]
    UnthreadPipelineTooShort,

    #[error("unthread-expression atom step must have symbol text")]
    UnthreadAtomStepHasNoText,

    #[error("unthread-expression list step must start with an atom head")]
    UnthreadListStepHeadNotAnAtom,

    #[error("unthread-expression step must be an atom or parenthesized call at {start}..{end}")]
    UnthreadStepNotAtomOrCall { start: usize, end: usize },

    // --- unwrap-call ---
    #[error("unwrap-call target must be a parenthesized call")]
    UnwrapTargetNotACall,

    #[error("unwrap-call target must have an atom function head")]
    UnwrapTargetHeadNotAnAtom,

    #[error("unwrap-call expected function {expected}, found {found}")]
    UnwrapFunctionMismatch { expected: String, found: String },

    #[error("--argument-index is too large to address any call argument")]
    UnwrapArgumentIndexTooLarge,
}

/// The transformation rebuilds the form from parsed parts, so a comment
/// inside it would be silently dropped.
///
/// Its own type because the refusal is *conservative correctness*, not a shape
/// complaint: the form is well-formed and the transformation is applicable —
/// it simply cannot carry the comment across, and dropping it silently would
/// lose the user's writing.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommentWouldBeDiscardedError {
    #[error(
        "thread-expression target contains a comment, which would be discarded by \
             flattening into a pipeline; remove or relocate the comment before threading"
    )]
    Threading,

    #[error(
        "unthread-expression target contains a comment, which would be discarded by \
             re-nesting into calls; remove or relocate the comment before unthreading"
    )]
    Unthreading,
}

/// Anything a form transformation can refuse to do.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum FormTransformError {
    /// A refusal this package shares with every other structural edit.
    #[error(transparent)]
    Edit(#[from] EditRefusal),

    #[error(transparent)]
    Dialect(#[from] TransformDialectError),

    #[error(transparent)]
    Selector(#[from] TransformSelectorError),

    #[error(transparent)]
    Target(#[from] TransformTargetError),

    #[error(transparent)]
    CommentWouldBeDiscarded(#[from] CommentWouldBeDiscardedError),

    #[error(transparent)]
    Parse(#[from] paredit_core_syntax::sexpr::ParseError),

    #[error(transparent)]
    ReaderConditional(#[from] paredit_core_edit::mutation_safety::ReaderConditionalSafetyError),

    #[error(transparent)]
    Symbol(#[from] paredit_core_syntax::sexpr::SymbolError),

    #[error("failed to parse {path}")]
    ParseFailed {
        path: String,
        #[source]
        source: paredit_core_syntax::sexpr::ParseError,
    },

    #[error("destination file is not a valid S-expression document: {path}")]
    DestinationNotAnSexprDocument {
        path: String,
        #[source]
        source: paredit_core_syntax::sexpr::ParseError,
    },

    /// `side` is the source or the destination, which is the difference
    /// between "the split broke the file it left" and "it broke the file it
    /// arrived in".
    #[error("{side} file would become invalid after {action} definitions: {path}")]
    WouldBecomeInvalid {
        side: &'static str,
        action: &'static str,
        path: String,
        #[source]
        source: paredit_core_syntax::sexpr::ParseError,
    },
}

// `From` does not chain.
macro_rules! from_edit_refusal {
    ($($ty:ident),+ $(,)?) => {
        $(impl From<paredit_core_edit::$ty> for FormTransformError {
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
    ShapeRefusal
);

impl From<SexprError> for FormTransformError {
    fn from(error: SexprError) -> Self {
        Self::Edit(error.into())
    }
}

/// The result type the form transformations return.
pub type FormTransformResult<T> = std::result::Result<T, FormTransformError>;

//! Why removing something unused refuses to run.
//!
//! [`BindingListError`] reads Common Lisp binding lists and reports their
//! unsupported shapes. It remains separate from
//! `paredit_core_semantics::BindingFormError` and
//! `paredit_feature_binding::BindingFormShapeError` because the passes cover
//! different form families and diagnostics.

use thiserror::Error;

use paredit_core_edit::EditRefusal;
use paredit_core_syntax::sexpr::{SexprError, SymbolError};

/// A binding list, or a binding in it, is not a shape this pass reads.
///
/// `form` is the operator whose binding list it is (`let`, `flet`, `do`,
/// `with-slots`, …), which is what varies between otherwise identical
/// complaints.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BindingListError {
    // --- the dialect writes these bindings a different way ---
    #[error("dialect expects vector let bindings: [name value ...]")]
    ExpectedVectorLet,

    #[error("dialect expects list-pair let bindings: ((name value) ...)")]
    ExpectedListPairLet,

    #[error("dialect expects {form} bindings: (variable-spec ...)")]
    ExpectedVariableSpecs { form: String },

    #[error("dialect expects with-slots bindings: (slot-or-pair ...)")]
    ExpectedWithSlots,

    #[error("dialect expects with-accessors bindings: ((name accessor) ...)")]
    ExpectedWithAccessors,

    #[error("dialect expects list-pair {form} bindings: ((name lambda-list form*) ...)")]
    ExpectedListPairCallable { form: String },

    // --- no dialect would accept this binding ---
    #[error("vector let binding form must contain name/value pairs")]
    VectorNotPaired,

    #[error("let binding must be a name, (name), or (name value)")]
    LetBindingNotANameOrPair,

    #[error("let binding pair must be (name) or (name value)")]
    LetBindingPairWrongArity,

    #[error("let binding name must be an atom")]
    LetBindingNameNotAnAtom,

    #[error("iteration binding name must be an atom")]
    IterationNameNotAnAtom,

    #[error("{form} binding must be a symbol or variable spec list")]
    NotASymbolOrVariableSpec { form: String },

    #[error("{form} variable spec has an unsupported arity")]
    VariableSpecWrongArity { form: String },

    #[error("with-slots bare binding name must be an atom")]
    WithSlotsBareNameNotAnAtom,

    #[error("with-slots binding must be a slot name or (name slot-name) pair")]
    WithSlotsNotANameOrPair,

    #[error("with-slots binding pair must contain a name and slot name")]
    WithSlotsPairIncomplete,

    #[error("with-slots binding name must be an atom")]
    WithSlotsNameNotAnAtom,

    #[error("with-accessors binding must be a (name accessor) pair")]
    WithAccessorsNotAPair,

    #[error("with-accessors binding pair must contain a name and accessor")]
    WithAccessorsPairIncomplete,

    #[error("with-accessors binding name must be an atom")]
    WithAccessorsNameNotAnAtom,

    #[error("{form} binding must be a (name lambda-list form*) list")]
    CallableBindingNotAList { form: String },

    #[error("{form} binding must contain a name and {body_label}")]
    CallableBindingIncomplete { form: String, body_label: String },

    #[error("{form} binding name must be an atom")]
    CallableNameNotAnAtom { form: String },
}

/// The command line does not describe a removable thing.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RemoveRequestError {
    #[error("remove-unused-binding accepts either --name or --all-bindings, not both")]
    NameAndAllBindings,

    #[error("remove-unused-binding requires --name or --all-bindings")]
    NeitherNameNorAllBindings,

    #[error("remove-unused-binding --all-bindings found no unused bindings")]
    NoUnusedBindings,

    #[error("binding {name} was not found in selected binding form")]
    BindingNotFound { name: String },

    #[error("remove-definition requires a top-level definition path, for example --path 2")]
    NotATopLevelPath,

    #[error("top-level path {path} is out of range")]
    TopLevelPathOutOfRange { path: String },

    #[error("selected top-level form is not a list definition")]
    NotAListDefinition,

    #[error("selected top-level form is not recognized as a definition: {head}")]
    NotADefinition { head: String },
}

/// The selection is not a form this pass removes bindings from, or the removal
/// would not be safe.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RemoveSelectionError {
    #[error(
        "remove-unused-binding selection must be a let, let*, symbol-macrolet, flet, labels, macrolet, compiler-macrolet, with-slots, with-accessors, do, do*, prog, or prog* list"
    )]
    NotABindingFormList,

    #[error(
        "remove-unused-binding selection must start with let, let*, symbol-macrolet, flet, labels, macrolet, compiler-macrolet, with-slots, with-accessors, do, do*, prog, or prog*"
    )]
    UnsupportedHead,

    #[error("remove-unused-binding requires a supported binding form with bindings and a body")]
    MissingBindingsOrBody,

    #[error(
        "remove-unused-binding requires a with-slots or with-accessors form with bindings, an instance expression, and a body"
    )]
    SlotFormIncomplete,

    #[error("remove-unused-binding requires a do or do* form with bindings and an end clause")]
    DoFormIncomplete,

    #[error("remove-unused-binding form must start with an atom")]
    HeadNotAnAtom,

    #[error("remove-unused-binding does not support this Common Lisp binding form")]
    UnsupportedBindingForm,

    #[error("remove-unused-binding requires zero in-scope references; found {count}")]
    HasReferences { count: usize },

    #[error("remove-unused-binding unsupported reference scope")]
    UnsupportedReferenceScope,

    #[error("remove-unused-binding target span is not present in the input")]
    TargetSpanNotInInput,

    #[error("remove-unused-binding target does not match the input")]
    TargetDoesNotMatchInput,

    #[error("failed to resolve later binding value")]
    LaterBindingValueUnresolved,

    #[error("overlapping replacement spans are not supported")]
    OverlappingReplacementSpans,

    // --- invariants that should hold after validation; a defect if they fire ---
    #[error("remove-unused-binding could not classify variable binding form")]
    ClassificationFailed,

    #[error("remove-unused-binding variable binding classification mismatch")]
    ClassificationMismatch,

    #[error("remove-unused-binding expected at least one body expression after validation")]
    NoBodyAfterValidation,

    #[error("remove-unused-binding replacement must contain exactly one form")]
    ReplacementNotOneForm,
}

/// A `block`/`tagbody` removal refuses.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RemoveControlError {
    #[error("remove-unused-block requires a plain symbol block name")]
    BlockNameNotPlain,

    #[error("selected block name does not match --name")]
    BlockNameMismatch,

    #[error("remove-unused-block found {count} matching return-from reference(s)")]
    BlockHasReferences { count: usize },

    #[error("remove-unused-tag requires exactly one matching tag definition")]
    TagNotUnique,

    #[error("remove-unused-tag found {count} matching go reference(s)")]
    TagHasReferences { count: usize },

    #[error("{operation} requires an unqualified symbol or integer tag")]
    TagNotUnqualified { operation: &'static str },

    #[error("{operation} requires an unqualified symbol name")]
    NameNotUnqualified { operation: &'static str },

    #[error("remove-unused-block found malformed nested block")]
    MalformedNestedBlock,

    #[error("remove-unused-block found malformed return-from")]
    MalformedReturnFrom,

    #[error("remove-unused-tag found malformed go")]
    MalformedGo,

    // --- the position, not the form ---
    #[error("remove-unused-block refuses top-level or unknown contexts")]
    TopLevelOrUnknownContext,

    #[error("remove-unused-block refuses reader-prefixed contexts")]
    ReaderPrefixedContext,

    #[error("remove-unused-block requires a non-empty path")]
    EmptyPath,

    #[error("remove-unused-block requires a known expression context")]
    UnknownContext,

    #[error("remove-unused-block requires a known expression position")]
    UnknownPosition,
}

/// A parallel analysis pass did not complete.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AnalysisWorkerError {
    #[error("unused-definition candidate worker thread panicked")]
    CandidateWorkerPanicked,

    #[error("unused-definition reference worker thread panicked")]
    ReferenceWorkerPanicked,
}

/// Anything the remove-unused passes can refuse to do.
///
/// Not `PartialEq`, because `Source` carries an `anyhow::Error`. The sub-enums
/// all are, which is where the matching actually happens.
#[derive(Debug, Error)]
pub enum RemoveUnusedError {
    /// Whatever the definition source port's adapter failed with.
    ///
    /// The port still does not enumerate its adapters' failures — see
    /// `DefinitionSourcePort::Error` — but what arrives here is a `CliError`
    /// rather than an `anyhow::Error`, so it still carries a classification.
    /// Boxed only to keep this enum small.
    #[error(transparent)]
    Source(Box<paredit_core_cli::CliError>),

    /// A refusal this package shares with every other structural edit.
    #[error(transparent)]
    Edit(#[from] EditRefusal),

    #[error(transparent)]
    BindingList(#[from] BindingListError),

    #[error(transparent)]
    Request(#[from] RemoveRequestError),

    #[error(transparent)]
    Selection(#[from] RemoveSelectionError),

    #[error(transparent)]
    Control(#[from] RemoveControlError),

    #[error(transparent)]
    Worker(#[from] AnalysisWorkerError),

    /// The source did not parse, or a reader conditional made the removal
    /// unsafe. Both come from below this package and are carried whole.
    #[error(transparent)]
    Parse(#[from] paredit_core_syntax::sexpr::ParseError),

    #[error(transparent)]
    ReaderConditional(#[from] paredit_core_edit::mutation_safety::ReaderConditionalSafetyError),

    #[error(transparent)]
    Symbol(#[from] SymbolError),

    /// A dialect this pass cannot analyse, named because "unknown" is the
    /// common case and the caller can fix it with `--dialect`.
    #[error("{operation} does not support dialect {dialect}")]
    UnsupportedDialect {
        operation: &'static str,
        dialect: String,
    },

    #[error("failed to parse {path}")]
    ParseFailed {
        path: String,
        #[source]
        source: paredit_core_syntax::sexpr::ParseError,
    },

    /// The rewrite would leave the file unparseable. `stage` is before or
    /// after the removal, which is the difference between "this file was
    /// already broken" and "this removal broke it".
    #[error("file would become invalid {stage} removing {what}: {path}")]
    WouldBecomeInvalid {
        stage: &'static str,
        what: &'static str,
        path: String,
        #[source]
        source: paredit_core_syntax::sexpr::ParseError,
    },

    #[error("{operation} found invalid symbol '{name}' in {path}")]
    InvalidSymbol {
        operation: &'static str,
        name: String,
        path: String,
        #[source]
        source: SymbolError,
    },
}

// `From` does not chain.
macro_rules! from_edit_refusal {
    ($($ty:ident),+ $(,)?) => {
        $(impl From<paredit_core_edit::$ty> for RemoveUnusedError {
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

impl From<SexprError> for RemoveUnusedError {
    fn from(error: SexprError) -> Self {
        Self::Edit(error.into())
    }
}

/// The result type the remove-unused passes return.
pub type RemoveUnusedResult<T> = std::result::Result<T, RemoveUnusedError>;

/// Which documented error code a removal refusal earns.
///
/// Public because `paredit-feature-project-analysis` composes this package and
/// has to answer the same question.
#[must_use]
pub fn code_of(error: &RemoveUnusedError) -> paredit_core_cli::diagnosis::ErrorCode {
    match error {
        // The adapter already classified itself; asking it again would be a
        // second, worse answer.
        RemoveUnusedError::Source(inner) => paredit_core_cli::diagnosis::code_for_cli_error(inner),

        RemoveUnusedError::Edit(edit) => paredit_core_cli::diagnosis::code_for_edit_refusal(edit),

        // "requires --name or --all-bindings" and its siblings: a command line
        // that does not describe a runnable request.
        RemoveUnusedError::Request(_) => {
            paredit_core_cli::diagnosis::ErrorCode::ArgumentFlagCombination
        }

        // The removal found nothing where it was pointed.
        RemoveUnusedError::Selection(_) => paredit_core_cli::diagnosis::ErrorCode::SelectionNoMatch,

        RemoveUnusedError::BindingList(_)
        | RemoveUnusedError::Control(_)
        | RemoveUnusedError::ReaderConditional(_) => {
            paredit_core_cli::diagnosis::ErrorCode::InputShapeRefused
        }

        RemoveUnusedError::Parse(_) | RemoveUnusedError::ParseFailed { .. } => {
            paredit_core_cli::diagnosis::ErrorCode::InputUnparsable
        }

        RemoveUnusedError::Symbol(_) | RemoveUnusedError::InvalidSymbol { .. } => {
            paredit_core_cli::diagnosis::ErrorCode::InputSymbolInvalid
        }

        RemoveUnusedError::UnsupportedDialect { .. } => {
            paredit_core_cli::diagnosis::ErrorCode::InputDialectUnsupported
        }

        // The removal ran and the result would not read back. Same code as the
        // CLI's own write guard: this tool will not leave a file it cannot parse.
        RemoveUnusedError::WouldBecomeInvalid { .. } => {
            paredit_core_cli::diagnosis::ErrorCode::RefusalRewriteDoesNotReparse
        }

        // A panicked analysis worker is a defect here, not in the caller's input.
        RemoveUnusedError::Worker(_) => paredit_core_cli::diagnosis::ErrorCode::Internal,
    }
}

paredit_core_cli::impl_classified_refusal!(RemoveUnusedError, |error| code_of(error));

/// Why a definition could not be moved between files.
///
/// The `definition_movement` slice guards both ends of a move: the file the
/// definition leaves and the file it arrives in must each still parse. Written
/// as `.context(...)` these all reached the boundary as
/// `internal.unclassified` — the move declining rather than corrupting two
/// files at once, reported as a defect.
#[derive(Debug, thiserror::Error)]
pub enum DefinitionMovementError {
    /// The destination did not parse before anything was written to it.
    #[error("destination file is not a valid S-expression document: {path}")]
    DestinationNotAnSexprDocument {
        path: String,
        #[source]
        source: paredit_core_syntax::sexpr::ParseError,
    },

    /// One end of the move would not read back.
    ///
    /// `side` and `what` carry the two axes the six messages varied on, so the
    /// rendering is unchanged and the distinction a reader cares about — which
    /// file broke — stays in the type.
    #[error("{side} file would become invalid after {action} {what}: {path}")]
    WouldBecomeInvalid {
        side: &'static str,
        action: &'static str,
        what: &'static str,
        path: String,
        #[source]
        source: paredit_core_syntax::sexpr::ParseError,
    },

    /// `--with` did not hold one complete top-level form.
    #[error("{summary}")]
    WithArgument {
        summary: &'static str,
        #[source]
        source: paredit_core_syntax::sexpr::ParseError,
    },

    /// The text this insertion produced does not parse.
    #[error("insertion produced invalid Lisp syntax")]
    InsertionInvalid {
        #[source]
        source: paredit_core_syntax::sexpr::ParseError,
    },

    /// A directory on the destination path could not be created.
    #[error("failed to create {path}")]
    CreateDirectory {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

paredit_core_cli::impl_classified_refusal!(DefinitionMovementError, |error| match error {
    // The caller's own file, as it already was.
    DefinitionMovementError::DestinationNotAnSexprDocument { .. } =>
        paredit_core_cli::diagnosis::ErrorCode::InputUnparsable,

    // This tool will not leave behind a file it cannot read back — the same
    // refusal, and the same code, as the CLI's own write guard.
    DefinitionMovementError::WouldBecomeInvalid { .. }
    | DefinitionMovementError::InsertionInvalid { .. } =>
        paredit_core_cli::diagnosis::ErrorCode::RefusalRewriteDoesNotReparse,

    // A flag's value, fixable only on the command line.
    DefinitionMovementError::WithArgument { .. } =>
        paredit_core_cli::diagnosis::ErrorCode::ArgumentFlagCombination,

    DefinitionMovementError::CreateDirectory { .. } =>
        paredit_core_cli::diagnosis::ErrorCode::EnvironmentIo,
});

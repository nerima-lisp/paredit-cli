//! Why an inline refuses to run.
//!
//! Section 9.2, and the largest package in the tree: 221 refusal sites, 167
//! distinct messages. The design question it forces is the one `core/cli`
//! asked about `.context()` — *when does a variant name the failure, and when
//! does it name the call site?*
//!
//! Seventy-four of these are the `inline-function` lambda-list and
//! destructuring reader saying some form of:
//!
//! ```text
//! inline-function currently supports only simple symbol &rest parameters
//! inline-function does not support &optional parameters after &rest or &body
//! inline-function supports at most one &whole parameter
//! inline-function &environment must be followed by a binding name
//! ```
//!
//! Seventy-four variants would name seventy-four call sites. They all mean one
//! thing to a caller — *this lambda list is more complex than inlining
//! handles, so do not inline it* — and the construct that was too complex is
//! the payload, not the kind. So [`UnsupportedLambdaList`] has eight variants
//! and carries the construct as a `String`, in the same spirit as
//! `CliError::Io` carrying its context.
//!
//! The refusals that *are* distinct kinds get distinct types:
//!
//! - [`InlineSafetyError`] — inlining would change what the program does:
//!   drop an argument, duplicate an evaluation, capture a variable. These are
//!   the ones a caller may override with `--allow-*`, and the only family
//!   where the refusal is about meaning rather than shape.
//! - [`InlineSelectionError`] — the selected form is not the thing being
//!   inlined. `operation` distinguishes the six inline commands, which
//!   otherwise write the same refusals about their own forms.
//! - [`CallBindingError`] — a call site's arguments do not bind to the
//!   definition's parameters.
//! - [`InlineInternalError`] — an invariant this package established and then
//!   failed to hold. Not the user's doing.

use thiserror::Error;

use paredit_core_edit::EditRefusal;
use paredit_core_syntax::sexpr::SexprError;

/// The lambda list or destructuring pattern is more complex than inlining
/// handles.
///
/// Eight variants for seventy-four messages. The construct is the payload
/// because a caller's response does not vary with it: this definition cannot
/// be inlined, and the text says which part was the problem.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UnsupportedLambdaList {
    #[error("inline-function currently supports only {supported}")]
    SupportsOnly { supported: String },

    #[error("inline-function does not support {construct} after {after}")]
    NotSupportedAfter { construct: String, after: String },

    #[error("inline-function supports at most one {construct}")]
    AtMostOne { construct: String },

    #[error("inline-function {marker} must be followed by {expected}")]
    MustBeFollowedBy { marker: String, expected: String },

    #[error("inline-function {subject} must {requirement}")]
    Requirement {
        subject: String,
        requirement: String,
    },

    #[error("inline-function requires a binding name for {parameter}")]
    RequiresBindingName { parameter: String },

    #[error("inline-function function parameter modifiers are not supported: {marker}")]
    ModifierNotSupported { marker: String },

    #[error("inline-function currently supports {construct} only {restriction}")]
    SupportsOnlyWhen {
        construct: String,
        restriction: String,
    },
}

/// Inlining would change what the program does.
///
/// The only family here that is about meaning rather than shape, and the only
/// one a caller can deliberately override — which is why the messages name the
/// `--allow-*` flag and why these deserve to be matchable.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InlineSafetyError {
    #[error(
        "inline-function would drop argument '{argument}' for unused parameter '{parameter}'; pass --allow-drop-arguments to permit it"
    )]
    WouldDropArgument { argument: String, parameter: String },

    #[error(
        "inline-function would duplicate argument '{argument}' for parameter '{parameter}'; pass --allow-duplicate-evaluation to permit it"
    )]
    WouldDuplicateArgument { argument: String, parameter: String },

    #[error("inline-let would drop an unused binding value")]
    LetWouldDropBinding,

    #[error(
        "inline-let would duplicate binding value evaluation; pass --allow-duplicate-evaluation to permit it"
    )]
    LetWouldDuplicateEvaluation,

    #[error(
        "inline-let would capture variable `{name}`: it is free in the binding value but a nested binding form rebinds it at a reference site"
    )]
    LetWouldCapture { name: String },

    #[error("{operation} rejects references used as mutation places")]
    MutationPlace { operation: &'static str },

    #[error("inline-local-function rejects recursive or same-name calls in the definition body")]
    RecursiveLocalFunction,

    #[error("inline-local-function rejects non-local control transfer or declarations")]
    NonLocalControlTransfer,

    #[error("inline-lambda rejects control transfer or declarations tied to a function boundary")]
    LambdaControlTransfer,

    #[error("inline-symbol-macro rejects declarations")]
    SymbolMacroDeclarations,

    #[error(
        "inline-function cannot inline macros that reference &environment parameter '{parameter}' in the {context}; source-level inlining cannot reconstruct macro expansion environments"
    )]
    ReferencesEnvironment { parameter: String, context: String },

    #[error(
        "inline-function cannot remove a definition that contains a comment; \
             the comment is not copied to call sites and would be discarded. \
             Drop --remove-definition or remove the comment first"
    )]
    RemoveDefinitionWithComment,

    #[error("inline-function does not support dialect {dialect}")]
    UnsupportedDialect { dialect: String },

    #[error("inline-function definition and call selections must not overlap")]
    DefinitionAndCallOverlap,

    #[error(
        "inline-local-function requires parameter '{parameter}' to be referenced exactly once; found {count}"
    )]
    ParameterNotReferencedOnce { parameter: String, count: usize },
}

/// The selected form is not the thing being inlined.
///
/// `operation` distinguishes the six inline commands. `problem` is the rest of
/// the sentence, because the six commands describe their own forms in their
/// own words and §9.2.1 forbids unifying the wording during a type change.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InlineSelectionError {
    #[error("{operation} {problem}")]
    Shape {
        operation: &'static str,
        problem: String,
    },

    #[error("{operation} requires a plain {role}")]
    NotPlain {
        operation: &'static str,
        role: String,
    },

    #[error("{operation} has invalid {role}")]
    Invalid {
        operation: &'static str,
        role: String,
    },

    /// A refusal that names no command, because it is about a shape shared by
    /// several (`function definition must include a symbol name`).
    #[error("{message}")]
    Unnamed { message: String },

    /// `unsupported_inline_function_definition_message` builds the whole
    /// sentence from the head and the dialect, so it is the message.
    #[error("{message}")]
    UnsupportedDefinition { message: String },
}

/// A call site's arguments do not bind to the definition's parameters.
/// Not `Clone`: `DestructuringArgumentDoesNotParse` carries a `ParseError`.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum CallBindingError {
    #[error(
        "inline-function arity mismatch for {function}: definition requires {required} positional argument(s), call has {actual} argument(s)"
    )]
    PositionalArity {
        function: String,
        required: usize,
        actual: usize,
    },

    #[error(
        "inline-function arity mismatch for {function}: definition has {parameters} parameter(s), call has {arguments} argument(s)"
    )]
    ParameterArity {
        function: String,
        parameters: usize,
        arguments: usize,
    },

    #[error(
        "inline-function keyword arguments for {function} must be supplied as keyword/value pairs"
    )]
    KeywordPairsRequired { function: String },

    #[error("inline-function expected keyword argument for {function}, found {found}")]
    ExpectedKeyword { function: String, found: String },

    #[error("inline-function call for {function} supplies duplicate keyword {keyword}")]
    DuplicateKeyword { function: String, keyword: String },

    #[error("inline-function call for {function} supplies unsupported keyword {keyword}")]
    UnsupportedKeyword { function: String, keyword: String },

    #[error(
        "inline-function cannot determine whether {qualifier}:allow-other-keys value {value} suppresses unknown keyword"
    )]
    AllowOtherKeysNotLiteral { qualifier: String, value: String },

    #[error("inline-function expected a single argument expression for destructuring")]
    ExpectedSingleArgument,

    /// The destructuring family mirrors the call-binding family one level
    /// down: a macro's pattern against the argument it was given.
    #[error("inline-function macro destructuring {problem}")]
    Destructuring { problem: String },

    #[error("inline-function inner &key destructuring {problem}")]
    InnerKeyDestructuring { problem: String },

    #[error("{operation} requires exact call arity")]
    ExactArityRequired { operation: &'static str },

    #[error("{command} --call-path {path} must select a function call list")]
    CallPathNotACallList { command: &'static str, path: String },

    #[error(
        "{command} --call-path {path} head '{head}' does not match selected definition '{function}'"
    )]
    CallPathHeadMismatch {
        command: &'static str,
        path: String,
        head: String,
        function: String,
    },

    #[error(
        "{command} --call-path {path} resolves to a call shadowed by a local callable binding or overlaps the selected definition"
    )]
    CallPathShadowed { command: &'static str, path: String },

    #[error("{command} accepts either --all-calls or repeated --call-path, not both")]
    AllCallsAndCallPath { command: &'static str },

    #[error("{command} --all-calls found no same-file calls for {function}")]
    NoSameFileCalls {
        command: &'static str,
        function: String,
    },

    #[error("{command} requires at least one --call-path or --all-calls")]
    NoCallSelector { command: &'static str },

    #[error("inline-function could not parse macro destructuring argument: {argument}")]
    DestructuringArgumentDoesNotParse {
        argument: String,
        #[source]
        source: paredit_core_syntax::sexpr::ParseError,
    },
}

/// An invariant this package established and then failed to hold.
///
/// Its own type because none of these are the user's doing: reaching one means
/// a bug here, and a caller should report it rather than change the input.
/// Not `Clone`: `CouldNotParse` carries a `ParseError`.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum InlineInternalError {
    #[error("inline-function internal error: keyword parameter missing keyword")]
    KeywordParameterMissingKeyword,

    #[error("inline-function internal error: expected simple parameter binding")]
    ExpectedSimpleParameterBinding,

    #[error("inline-function internal error: &environment parameter must use a simple binding")]
    EnvironmentNotSimple,

    #[error("inline-function expected a non-empty effective body after validation")]
    EmptyBodyAfterValidation,

    #[error("inline-let body disappeared after validation")]
    LetBodyDisappeared,

    #[error(
        "inline-function resolved inconsistent function name: expected {expected}, found {found}"
    )]
    InconsistentFunctionName { expected: String, found: String },

    #[error("inline-function expected atom text in macro body")]
    ExpectedAtomTextInMacroBody,

    #[error("inline-function expected delimited list in macro body")]
    ExpectedDelimitedListInMacroBody,

    #[error("invalid unquote form")]
    InvalidUnquote,

    #[error("invalid ,@ expansion")]
    InvalidSpliceExpansion,

    #[error("refusing overlapping rewrite spans")]
    OverlappingRewriteSpans,

    #[error("inline-function requires ,@ expansions to produce a list form")]
    SpliceMustProduceList,

    #[error("inline-function found unsupported top-level ,@expr in defmacro body")]
    UnsupportedTopLevelSplice,

    #[error("inline-function macro body must be a single expression")]
    MacroBodyNotSingleExpression,

    #[error("inline-function could not parse {context}: {value}")]
    CouldNotParse {
        context: String,
        value: String,
        #[source]
        source: paredit_core_syntax::sexpr::ParseError,
    },

    #[error("inline-function default value must be a single S-expression")]
    DefaultValueNotSingleExpression,
}

/// Anything an inline can refuse to do.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum InlineError {
    /// A refusal this package shares with every other structural edit.
    #[error(transparent)]
    Edit(#[from] EditRefusal),

    #[error(transparent)]
    LambdaList(#[from] UnsupportedLambdaList),

    #[error(transparent)]
    Safety(#[from] InlineSafetyError),

    #[error(transparent)]
    Selection(#[from] InlineSelectionError),

    #[error(transparent)]
    CallBinding(#[from] CallBindingError),

    #[error(transparent)]
    Internal(#[from] InlineInternalError),

    #[error(transparent)]
    Parse(#[from] paredit_core_syntax::sexpr::ParseError),

    #[error(transparent)]
    Symbol(#[from] paredit_core_syntax::sexpr::SymbolError),

    #[error(transparent)]
    Path(#[from] paredit_core_syntax::sexpr::PathError),

    #[error(transparent)]
    ReaderConditional(#[from] paredit_core_edit::mutation_safety::ReaderConditionalSafetyError),

    /// `inline-literal-constant` reuses `feature/rename`'s reference walker to
    /// find the constant's uses, so a rename refusal can reach here whole.
    #[error(transparent)]
    Rename(#[from] paredit_feature_rename::RenameError),
}

// `From` does not chain.
macro_rules! from_edit_refusal {
    ($($ty:ident),+ $(,)?) => {
        $(impl From<paredit_core_edit::$ty> for InlineError {
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

impl From<SexprError> for InlineError {
    fn from(error: SexprError) -> Self {
        Self::Edit(error.into())
    }
}

/// The result type the inline planners return.
pub type InlineResult<T> = std::result::Result<T, InlineError>;

// States which documented error code each inline refusal earns.
paredit_core_cli::impl_classified_refusal!(InlineError, |error| match error {
    InlineError::Edit(edit) => paredit_core_cli::diagnosis::code_for_edit_refusal(edit),

    // The rename this inline delegates to already answered the question.
    InlineError::Rename(rename) => paredit_feature_rename::error::code_of(rename),

    InlineError::LambdaList(_)
    | InlineError::Safety(_)
    | InlineError::CallBinding(_)
    | InlineError::ReaderConditional(_) =>
        paredit_core_cli::diagnosis::ErrorCode::InputShapeRefused,

    InlineError::Selection(_) => paredit_core_cli::diagnosis::ErrorCode::SelectionNoMatch,
    InlineError::Path(_) => paredit_core_cli::diagnosis::ErrorCode::SelectionPathInvalid,
    InlineError::Parse(_) => paredit_core_cli::diagnosis::ErrorCode::InputUnparsable,
    InlineError::Symbol(_) => paredit_core_cli::diagnosis::ErrorCode::InputSymbolInvalid,

    // Named `Internal` by this package for the same reason the code is: a
    // defect here, not something the caller can fix.
    InlineError::Internal(_) => paredit_core_cli::diagnosis::ErrorCode::Internal,
});

// `CallBindingError` also reaches the boundary on its own.
paredit_core_cli::impl_classified_refusal!(CallBindingError, |_error| {
    paredit_core_cli::diagnosis::ErrorCode::InputShapeRefused
});

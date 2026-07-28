//! Why a parameter-list refactor refuses to run.
//!
//! Section 9.2. 93 refusals across 18 files, and the shape that dominates is
//! the **call site**:
//!
//! ```text
//! {command} call to '{function}' at {start}..{end} {problem}
//! ```
//!
//! Fifteen messages, five commands, one shape. Adding, removing, reordering
//! and swapping a parameter all have to visit every call and can all give up
//! at one — and when they do, the useful information is *which call* (the
//! byte range) and *why*. [`CallArgumentError`] carries the location on every
//! variant so a caller can point at the call rather than describing it.
//!
//! The other recurring pair is worth naming because it is the same refusal
//! written five times: `{command} call path {path} overlaps the selected
//! definition` and `{command} output is not a valid S-expression document`,
//! once per command. Both now carry `operation`.

use thiserror::Error;

use paredit_core_edit::EditRefusal;
use paredit_core_syntax::sexpr::SexprError;

/// A call site cannot take the parameter change.
///
/// `start`/`end` are the call's byte range, present on every variant because
/// the whole point of these messages is to say *which* call gave up.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CallArgumentError {
    #[error(
        "{command} call to '{function}' at {start}..{end} does not have {count} positional argument(s) before {before}"
    )]
    MissingPositional {
        command: &'static str,
        function: String,
        start: usize,
        end: usize,
        count: usize,
        before: &'static str,
    },

    #[error(
        "{command} call to '{function}' at {start}..{end} already contains keyword argument {keyword}"
    )]
    KeywordAlreadyPresent {
        command: &'static str,
        function: String,
        start: usize,
        end: usize,
        keyword: String,
    },

    #[error(
        "{command} call to '{function}' at {start}..{end} has keyword argument without a value"
    )]
    KeywordWithoutValue {
        command: &'static str,
        function: String,
        start: usize,
        end: usize,
    },

    #[error(
        "{command} call to '{function}' at {start}..{end} does not have argument at parameter index {index}"
    )]
    MissingArgumentAtIndex {
        command: &'static str,
        function: String,
        start: usize,
        end: usize,
        index: usize,
    },

    #[error(
        "{command} call to '{function}' at {start}..{end} does not have keyword argument {keyword}"
    )]
    KeywordMissing {
        command: &'static str,
        function: String,
        start: usize,
        end: usize,
        keyword: String,
    },

    #[error(
        "{command} call to '{function}' at {start}..{end} contains duplicate keyword argument {keyword}"
    )]
    DuplicateKeyword {
        command: &'static str,
        function: String,
        start: usize,
        end: usize,
        keyword: String,
    },

    #[error(
        "{command} call to '{function}' at {start}..{end} has keyword {keyword} without a value"
    )]
    NamedKeywordWithoutValue {
        command: &'static str,
        function: String,
        start: usize,
        end: usize,
        keyword: String,
    },

    #[error(
        "{command} call to '{function}' at {start}..{end} has {actual} arguments but needs at least {needed} positional arguments"
    )]
    TooFewArguments {
        command: &'static str,
        function: String,
        start: usize,
        end: usize,
        actual: usize,
        needed: usize,
    },

    #[error(
        "{command} call to '{function}' at {start}..{end} has {actual} arguments but needs at least {needed} positional arguments before keyword arguments"
    )]
    TooFewBeforeKeywords {
        command: &'static str,
        function: String,
        start: usize,
        end: usize,
        actual: usize,
        needed: usize,
    },

    #[error(
        "{command} call to '{function}' at {start}..{end} has an incomplete keyword argument list"
    )]
    IncompleteKeywordList {
        command: &'static str,
        function: String,
        start: usize,
        end: usize,
    },

    #[error("{command} call at {start}..{end} has an incomplete keyword argument list")]
    UnnamedIncompleteKeywordList {
        command: &'static str,
        start: usize,
        end: usize,
    },

    #[error("{command} call at {start}..{end} does not contain keyword argument {keyword}")]
    UnnamedKeywordMissing {
        command: &'static str,
        start: usize,
        end: usize,
        keyword: String,
    },

    #[error("{command} keyword parameter must have keyword metadata")]
    KeywordMetadataMissing { command: &'static str },

    #[error("{command} keyword parameter must have positional prefix metadata")]
    PositionalPrefixMetadataMissing { command: &'static str },

    #[error("{command} call at {start}..{end} does not have argument at parameter index {index}")]
    UnnamedMissingArgumentAtIndex {
        command: &'static str,
        start: usize,
        end: usize,
        index: usize,
    },

    /// The parameter metadata this package built lacks a field it should have.
    /// A defect rather than a user error, which is why `field` is a
    /// `&'static str` naming the internal field.
    #[error(
        "{command} metadata for '{function}' at {start}..{end} is missing {field} for {kind} parameter '{parameter}'"
    )]
    MetadataMissing {
        command: &'static str,
        function: String,
        start: usize,
        end: usize,
        field: &'static str,
        kind: &'static str,
        parameter: String,
    },
}

/// The `--call-path`/`--all-calls` selectors did not identify calls to change.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CallSelectionError {
    #[error("{command} accepts either --all-calls or repeated --call-path, not both")]
    AllCallsAndCallPath { command: &'static str },

    #[error("{command} --all-calls found no same-file calls for {function}")]
    NoSameFileCalls {
        command: &'static str,
        function: String,
    },

    #[error("{command} requires at least one --call-path or --all-calls")]
    NoCallSelector { command: &'static str },

    #[error("{command} --call-path {path} must select a function call list")]
    NotACallList { command: &'static str, path: String },

    #[error(
        "{command} --call-path {path} head '{head}' does not match selected definition '{function}'"
    )]
    HeadMismatch {
        command: &'static str,
        path: String,
        head: String,
        function: String,
    },

    #[error(
        "{command} --call-path {path} resolves to a call shadowed by a local callable binding or overlaps the selected definition"
    )]
    ShadowedOrInert { command: &'static str, path: String },

    #[error("{command} call selection must be a function call list")]
    SelectionNotACallList { command: &'static str },

    #[error("{command} call must not be empty")]
    CallEmpty { command: &'static str },

    #[error("{command} call must start with an atom")]
    CallHeadNotAnAtom { command: &'static str },

    #[error("{command} call head '{head}' does not match selected definition '{function}'")]
    SelectionHeadMismatch {
        command: &'static str,
        head: String,
        function: String,
    },

    #[error("{command} setf call must contain a place form")]
    SetfMissingPlace { command: &'static str },

    #[error("{command} setf place must be a function call list")]
    SetfPlaceNotACallList { command: &'static str },

    #[error("{command} call path {path} overlaps the selected definition")]
    OverlapsDefinition { command: &'static str, path: String },
}

/// The lambda list is not a shape these refactors rewrite.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LambdaListError {
    #[error("{operation} function parameter form must be a list or vector")]
    NotAListOrVector { operation: &'static str },

    #[error("{operation} dotted lambda-list separators are not supported")]
    DottedNotSupported { operation: &'static str },

    #[error("{operation} dotted lambda-list separator must follow required parameters")]
    DottedAfterRequired { operation: &'static str },

    #[error("{operation} dotted lambda-list separator must follow at least one parameter")]
    DottedNeedsParameter { operation: &'static str },

    #[error("{operation} dotted lambda-list tail must be a symbol")]
    DottedTailNotASymbol { operation: &'static str },

    #[error("{operation} dotted lambda-list tail must be the final parameter")]
    DottedTailNotFinal { operation: &'static str },

    #[error("{operation} function parameter modifiers are not supported: {marker}")]
    ModifierNotSupported {
        operation: &'static str,
        marker: String,
    },

    #[error("{operation} lambda-list marker &allow-other-keys is only supported after &key")]
    AllowOtherKeysWithoutKey { operation: &'static str },

    #[error("{operation} unsupported lambda-list marker: {marker}")]
    UnsupportedMarker {
        operation: &'static str,
        marker: String,
    },

    #[error(
        "{operation} does not support parameters after &allow-other-keys before another lambda-list marker"
    )]
    ParametersAfterAllowOtherKeys { operation: &'static str },

    #[error("{operation} currently supports only simple parameters")]
    OnlySimpleParameters { operation: &'static str },

    #[error("{operation} dotted lambda-list separator must be followed by a parameter")]
    DottedSeparatorNeedsParameter { operation: &'static str },

    #[error("add-function-parameter found duplicate &key marker")]
    DuplicateKeyMarker,

    #[error("add-function-parameter found duplicate &optional marker")]
    DuplicateOptionalMarker,

    #[error(
        "add-function-parameter currently supports only flat positional parameter lists, existing Common Lisp required parameter sections before lambda-list markers, existing Common Lisp &optional parameter lists, or existing Common Lisp &key parameter lists"
    )]
    AddOnlyFlatOrMarkers,
}

/// The selected form is not a definition these refactors read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DefinitionShapeError {
    #[error("{operation} definition selection must be a function definition list")]
    NotADefinitionList { operation: &'static str },

    #[error("{operation} definition must include a name and parameters")]
    MissingNameOrParameters { operation: &'static str },

    #[error("{operation} definition must start with a definition atom")]
    HeadNotADefinitionAtom { operation: &'static str },

    #[error("scheme define selection must include a signature list: (define (name args...) body)")]
    SchemeDefineMissingSignature,

    #[error("{operation} currently supports scheme procedure defines, not variable defines")]
    SchemeVariableDefine { operation: &'static str },

    #[error("scheme define signature must start with a function name")]
    SchemeSignatureMissingName,

    #[error("function definition must include a symbol name")]
    MissingSymbolName,

    #[error("function definition must include a parameter list")]
    MissingParameterList,

    #[error(
        "{operation} does not support short-form defsetf; select a long-form defsetf with an accessor lambda list"
    )]
    ShortFormDefsetf { operation: &'static str },

    /// The message body is computed by
    /// `unsupported_function_parameter_definition_message`, which names the
    /// head and the operation; it is prose that varies per head, so it is a
    /// `String` rather than a variant per definition form.
    #[error("{message}")]
    UnsupportedDefinitionForm { message: String },

    #[error("local callable binding must include a symbol name")]
    LocalCallableMissingName,

    #[error("local callable binding must include a lambda list")]
    LocalCallableMissingLambdaList,

    #[error("{head} binding must include a parenthesized lambda list")]
    BindingLambdaListNotParenthesized { head: String },

    #[error("{operation} defmethod definition must include a specialized lambda list")]
    DefmethodMissingSpecializedLambdaList { operation: &'static str },
}

/// The parameter named on the command line does not identify one parameter.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParameterSelectionError {
    #[error("{operation} parameter '{name}' appears more than once")]
    Duplicate {
        operation: &'static str,
        name: String,
    },

    #[error("{operation} parameter '{name}' was not found")]
    NotFound {
        operation: &'static str,
        name: String,
    },

    #[error("add-function-parameter found invalid parameter symbol '{name}'")]
    InvalidSymbol { name: String },

    #[error("{operation} found invalid parameter symbol '{name}'")]
    InvalidSymbolFor {
        operation: &'static str,
        name: String,
    },

    #[error("add-function-parameter parameter '{name}' already exists in {function}")]
    AlreadyExists { name: String, function: String },

    #[error("move-function-parameter target index {index} is out of bounds for {count} parameters")]
    TargetIndexOutOfBounds { index: usize, count: usize },

    #[error("swap-function-parameters requires two distinct parameter names")]
    SwapNeedsDistinctNames,

    #[error("reorder-function-parameters requires at least one --parameter")]
    ReorderNeedsParameter,

    #[error(
        "reorder-function-parameters requested {requested} parameters but definition has {actual}"
    )]
    ReorderCountMismatch { requested: usize, actual: usize },

    #[error("reorder-function-parameters cannot reorder duplicate definition parameter '{name}'")]
    ReorderDuplicateDefinitionParameter { name: String },

    #[error("reorder-function-parameters requested parameter '{name}' more than once")]
    ReorderRequestedTwice { name: String },

    #[error("reorder-function-parameters missing parameter '{name}' from requested order")]
    ReorderMissingParameter { name: String },

    #[error("reorder-function-parameters requested unknown parameter '{name}'")]
    ReorderUnknownParameter { name: String },

    #[error("{command} cannot move '{name}' across Common Lisp lambda-list sections")]
    CannotCrossSections { command: &'static str, name: String },

    #[error("{command} parameter '{name}' is not aligned with reorderable positional arguments")]
    NotAlignedWithPositional { command: &'static str, name: String },

    #[error(
        "{operation} does not support reordering parameter '{name}' because it is not a direct call argument"
    )]
    NotADirectCallArgument {
        operation: &'static str,
        name: String,
    },
}

/// The rewrite's own list edits are inconsistent.
///
/// These are invariants rather than user errors, kept separate for that
/// reason: an out-of-bounds item index after the parameter was already looked
/// up is a defect in this package.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ListEditError {
    #[error("add-function-parameter insertion target must be a list")]
    InsertionTargetNotAList,

    #[error("add-function-parameter insertion target has an invalid span")]
    InsertionTargetInvalidSpan,

    #[error("remove-function-parameter removal target must be a list")]
    RemovalTargetNotAList,

    #[error("remove-function-parameter removal item index {index} is out of bounds")]
    RemovalIndexOutOfBounds { index: usize },

    #[error("remove-function-parameter dotted tail must follow a parameter binding")]
    DottedTailWithoutBinding,

    #[error("refusing overlapping rewrite spans")]
    OverlappingRewriteSpans,

    #[error("{operation} reorder target must be a list")]
    ReorderTargetNotAList { operation: &'static str },

    #[error(
        "{operation} cannot reorder a parameter list that contains a comment, \
             which would be discarded when the list is rebuilt; remove or relocate \
             the comment first"
    )]
    ReorderWouldDiscardComment { operation: &'static str },

    #[error("{operation} definition reorder produced an incomplete parameter list")]
    ReorderProducedIncompleteList { operation: &'static str },

    #[error("--argument must not be empty")]
    ArgumentEmpty,

    #[error("--argument must contain exactly one top-level S-expression")]
    ArgumentNotOneForm,
}

/// Anything a parameter-list refactor can refuse to do.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum FunctionParameterError {
    /// A refusal this package shares with every other structural edit.
    #[error(transparent)]
    Edit(#[from] EditRefusal),

    #[error(transparent)]
    CallArgument(#[from] CallArgumentError),

    #[error(transparent)]
    CallSelection(#[from] CallSelectionError),

    #[error(transparent)]
    LambdaList(#[from] LambdaListError),

    #[error(transparent)]
    DefinitionShape(#[from] DefinitionShapeError),

    #[error(transparent)]
    ParameterSelection(#[from] ParameterSelectionError),

    #[error(transparent)]
    ListEdit(#[from] ListEditError),

    #[error(transparent)]
    Parse(#[from] paredit_core_syntax::sexpr::ParseError),

    #[error(transparent)]
    Symbol(#[from] paredit_core_syntax::sexpr::SymbolError),

    #[error(transparent)]
    ReaderConditional(#[from] paredit_core_edit::mutation_safety::ReaderConditionalSafetyError),

    #[error("add-function-parameter argument is not a valid S-expression")]
    ArgumentDoesNotParse {
        #[source]
        source: paredit_core_syntax::sexpr::ParseError,
    },
}

// `From` does not chain.
macro_rules! from_edit_refusal {
    ($($ty:ident),+ $(,)?) => {
        $(impl From<paredit_core_edit::$ty> for FunctionParameterError {
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

impl From<SexprError> for FunctionParameterError {
    fn from(error: SexprError) -> Self {
        Self::Edit(error.into())
    }
}

/// The result type the parameter-list refactors return.
pub type FunctionParameterResult<T> = std::result::Result<T, FunctionParameterError>;

//! Why a rename refuses to run.
//!
//! Section 9.2, and the largest feature package by refusal count: 120
//! messages across 18 files. The repetition is the design input.
//!
//! Three shapes recur across every binding form this package knows — `let`,
//! `flet`, `defmethod`, `handler-case`, `with-slots`, `do`, and a dozen more:
//!
//! ```text
//! selected {form} form must contain {what}
//! binding '{from}' was not found in selected {form}
//! binding '{from}' was found in multiple selected {form} {where}; select an unambiguous binding
//! ```
//!
//! So [`BindingSelectionError`] has three variants carrying `form`, not one
//! variant per form. The middle two are the pair a caller most wants to tell
//! apart: *not found* means the name is wrong, *ambiguous* means the name is
//! right and needs a narrower selection. Those are opposite instructions to
//! give a user, and they were previously two prose sentences.
//!
//! [`BindingListError`] is the **fourth** appearance of the Common Lisp
//! binding-list reader in this tree, after `core/semantics`,
//! `feature/binding`, and `feature/remove-unused`. Four types with
//! overlapping wordings is not obviously right — but it is now four types
//! rather than four unrelated sets of strings, which is what makes the
//! question askable.

use thiserror::Error;

use paredit_core_edit::EditRefusal;
use paredit_core_syntax::sexpr::SexprError;

/// The binding to rename could not be identified inside the selected form.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BindingSelectionError {
    /// `what` is the part that is missing: `parameters`, `bindings`, `slot
    /// specs`, `variable specs`, `an iteration binding`, …
    #[error("selected {form} form must contain {what}")]
    FormMissingPart { form: String, what: String },

    #[error("binding '{from}' was not found in selected {form}")]
    NotFound { from: String, form: String },

    /// `location` is where the duplicates were: `clauses`, `specs`, `local
    /// callable lambda lists`, `handler functions`.
    #[error(
        "binding '{from}' was found in multiple selected {form} {location}; select an unambiguous binding form"
    )]
    Ambiguous {
        from: String,
        form: String,
        location: String,
    },

    /// Same as [`Self::NotFound`], but the search was narrowed to part of the
    /// form, and the message says which part.
    #[error("binding '{from}' was not found in selected {form} {location}")]
    NotFoundIn {
        from: String,
        form: String,
        location: String,
    },

    #[error("selected form is not a supported binding form")]
    UnsupportedForm,

    #[error("selected let form must contain bindings")]
    LetMissingBindings,

    #[error("symbol-macrolet binding must contain a symbol and expansion")]
    SymbolMacroletBindingIncomplete,

    #[error("selected atom is not a symbol")]
    NotASymbol,

    #[error("path index {index} is out of bounds for {children} children")]
    PathIndexOutOfBounds { index: usize, children: usize },

    #[error("overlapping edits at {first_start}..{first_end} and {second_start}..{second_end}")]
    OverlappingEdits {
        first_start: usize,
        first_end: usize,
        second_start: usize,
        second_end: usize,
    },
}

/// A binding list, or a binding in it, is not a shape this pass reads.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BindingListError {
    #[error("unknown binding form delimiter")]
    UnknownDelimiter,

    #[error("parameter form must be a list")]
    ParameterFormNotAList,

    #[error("specialized parameter form must be a list")]
    SpecializedParameterFormNotAList,

    #[error("dialect expects vector let bindings: [name value ...]")]
    ExpectedVectorLet,

    #[error("vector let binding form must contain name/value pairs")]
    VectorNotPaired,

    #[error("dialect expects list-pair let bindings: ((name value) ...)")]
    ExpectedListPairLet,

    #[error("let binding must be a name, (name), or (name value)")]
    BindingNotANameOrPair,

    #[error("bare let binding must contain one binding name")]
    BareBindingNotSingle,

    #[error("let binding pair must be (name) or (name value)")]
    BindingPairWrongArity,

    #[error("let binding pattern must contain at least one binding name")]
    PatternBindsNothing,
}

/// A `--call-path` does not point at a call this pass will rewrite.
///
/// The first three variants are shared verbatim by `replace-function-calls`,
/// `wrap-function-calls` and `unwrap-function-calls`, which is why they are
/// one type rather than three.
///
/// Not `Clone`: `WrapperTemplateDoesNotParse` carries a `ParseError`.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum CallSiteError {
    #[error("call-path {path} is not in an executable reader context")]
    NotExecutable { path: String },

    #[error("call-path {path} is shadowed by a local callable named {name}")]
    Shadowed { path: String, name: String },

    #[error("call-path {path} is not a call to {function}")]
    NotACall { path: String, function: String },

    #[error("call-path {path} is not a unary {wrapper} wrapper around {function}")]
    NotAUnaryWrapper {
        path: String,
        wrapper: String,
        function: String,
    },

    #[error("failed to parse wrapper template")]
    WrapperTemplateDoesNotParse {
        #[source]
        source: paredit_core_syntax::sexpr::ParseError,
    },

    #[error("wrapper template root form must be a parenthesized list")]
    WrapperTemplateNotAList,

    #[error("wrapper template must contain exactly one root form")]
    WrapperTemplateNotOneForm,

    #[error("wrapper template head must match --wrapper ({wrapper})")]
    WrapperTemplateHeadMismatch { wrapper: String },

    #[error("wrapper template must contain exactly one _ placeholder atom")]
    WrapperTemplateNotOnePlaceholder,
}

/// An invariant of the verified semantic binding shape did not hold.
///
/// These are defects rather than user errors: the dialect's semantic policy
/// said the form has a binding shape, and then the shape did not have the part
/// the policy promised. Grouped so a reader can see at a glance that this
/// whole family means "the tables and the tree disagree", which is a bug
/// report rather than something to retype.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SemanticShapeError {
    #[error("selected form has no verified semantic binding shape")]
    NoVerifiedShape,

    #[error("binding container is missing")]
    BindingContainerMissing,

    #[error("named scope name is missing")]
    NamedScopeNameMissing,

    #[error("named callable name is missing")]
    NamedCallableNameMissing,

    #[error("selected definition has no lexical parameters")]
    NoLexicalParameters,

    #[error("binding group index is missing")]
    BindingGroupIndexMissing,

    #[error("binding name was not found")]
    BindingNameNotFound,

    #[error("binding name is ambiguous in the selected form")]
    BindingNameAmbiguous,

    #[error("parameter container is missing")]
    ParameterContainerMissing,

    #[error("parameter layout starts outside its container")]
    ParameterLayoutOutsideContainer,

    #[error("binding pattern has no name")]
    PatternHasNoName,

    #[error("binding pattern does not identify one name")]
    PatternNotOneName,

    #[error("clause parameters require clause body metadata")]
    ClauseBodyMetadataMissing,

    #[error("selected parameter is outside callable clauses")]
    ParameterOutsideClauses,

    #[error("selected callable clause is missing")]
    ClauseMissing,
}

/// A `block`/`tagbody` rename refuses.
///
/// The `would collide` / `would capture` pair is the interesting half: those
/// are not shape complaints but *meaning* ones — the rename would silently
/// change which block a `return-from` targets.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RenameControlError {
    #[error("rename-block requires a plain block name")]
    BlockNameNotPlain,

    #[error("selected block name does not match --from")]
    BlockNameMismatch,

    #[error("rename-block found malformed nested block")]
    MalformedNestedBlock,

    #[error("rename-block target would collide with a nested block")]
    BlockCollides,

    #[error("rename-block found malformed return-from")]
    MalformedReturnFrom,

    #[error("rename-block target would capture an existing return-from")]
    BlockCaptures,

    #[error("rename-tag requires exactly one matching tag definition")]
    TagNotUnique,

    #[error("rename-tag target duplicates an existing tag")]
    TagDuplicates,

    #[error("rename-tag target collides with a nested tagbody")]
    TagCollides,

    #[error("rename-tag found malformed go")]
    MalformedGo,

    #[error("rename-tag target would capture an existing go")]
    TagCaptures,

    #[error("selected form requires a plain head")]
    HeadNotPlain,

    #[error("{operation} requires unqualified symbols")]
    NotUnqualified { operation: &'static str },
}

/// Anything a rename can refuse to do.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum RenameError {
    /// A refusal this package shares with every other structural edit.
    #[error(transparent)]
    Edit(#[from] EditRefusal),

    #[error(transparent)]
    Binding(#[from] BindingSelectionError),

    #[error(transparent)]
    BindingList(#[from] BindingListError),

    #[error(transparent)]
    CallSite(#[from] CallSiteError),

    #[error(transparent)]
    SemanticShape(#[from] SemanticShapeError),

    #[error(transparent)]
    Control(#[from] RenameControlError),

    /// The `rename-at` slice's own typed error, which predates §9.2 and is
    /// carried whole rather than flattened.
    #[error(transparent)]
    RenameAt(#[from] crate::rename::domain::RenameAtError),

    #[error(transparent)]
    Parse(#[from] paredit_core_syntax::sexpr::ParseError),

    #[error(transparent)]
    Symbol(#[from] paredit_core_syntax::sexpr::SymbolError),

    #[error(transparent)]
    ReaderConditional(#[from] paredit_core_edit::mutation_safety::ReaderConditionalSafetyError),

    #[error("{operation} requires a known dialect")]
    RequiresKnownDialect { operation: &'static str },

    #[error("rename-binding is not supported for this dialect")]
    BindingRenameUnsupportedDialect {
        #[source]
        source: paredit_core_syntax::dialect::UnsupportedSemanticOperation,
    },
}

// `From` does not chain.
macro_rules! from_edit_refusal {
    ($($ty:ident),+ $(,)?) => {
        $(impl From<paredit_core_edit::$ty> for RenameError {
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

impl From<SexprError> for RenameError {
    fn from(error: SexprError) -> Self {
        Self::Edit(error.into())
    }
}

/// The result type the rename passes return.
pub type RenameResult<T> = std::result::Result<T, RenameError>;

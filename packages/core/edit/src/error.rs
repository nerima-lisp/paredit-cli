//! Why a structural edit refuses to run.
//!
//! Section 9.2, and the package where the measurement mattered most. The 107
//! refusal messages here normalise to 64 shapes, and 59% of the messages
//! collapse into 20 of them: seven edit families — `convert-*`,
//! `merge-nested-*`, `split-let*`, `flatten-progn`,
//! `eliminate-empty-binding-form` — were each spelling out *the same seven
//! reasons* with their own operation name pasted in.
//!
//! So the types are organised by **reason**, not by operation, and every
//! variant carries `operation`. That is not a refactor of the strings: the
//! code already threaded `operation: &str` through shared helpers like
//! `require_supported_dialect` and `require_named_form`. The parameter was
//! there; only the type was missing.
//!
//! What a caller gains: `merge-nested-let` refusing because the form has
//! comments and `split-let-star` refusing because the form has comments are
//! now the same value with a different `operation`, so a caller can say "this
//! edit family is conservative about comments" once instead of matching six
//! message prefixes.
//!
//! Messages are reproduced exactly, per §9.2.1 — the CLI's string assertions
//! and the `inspect capabilities` golden both depend on the current text, and
//! several shapes differ only in wording (`rejects declarations` versus
//! `conservatively rejects declarations`). Those stay separate variants rather
//! than being unified, because unifying them would be a behaviour change
//! wearing a type change's clothes.

use paredit_core_syntax::sexpr::{ParseError, SpanError, SymbolError};
use thiserror::Error;

/// The edit is not defined for this dialect.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DialectRefusal {
    #[error("{operation} supports only Common Lisp")]
    CommonLispOnly { operation: &'static str },

    #[error("{operation} currently supports only Common Lisp")]
    CurrentlyCommonLispOnly { operation: &'static str },

    #[error("{operation} supports only Common Lisp and Emacs Lisp")]
    CommonLispAndEmacsLisp { operation: &'static str },

    #[error("{operation} currently supports only Common Lisp and Emacs Lisp")]
    CurrentlyCommonLispAndEmacsLisp { operation: &'static str },

    /// Same refusal, spelled with the dialect *identifiers* rather than their
    /// English names. A separate variant because the wording is part of the
    /// CLI's contract and unifying it would change output.
    #[error("{operation} supports only common-lisp and emacs-lisp")]
    LowercaseCommonLispAndEmacsLisp { operation: &'static str },
}

/// The source or the rewritten result is not a document this edit can use.
///
/// `Output` variants are self-checks: the edit produced text and re-parsed it
/// before offering it, so an output failure is a bug in the edit rather than a
/// problem with the caller's input.
/// Each variant keeps the [`ParseError`] as its `#[source]`, so the rendered
/// chain is what `anyhow`'s `.context()` produced: the edit's summary, then
/// the byte offset the parser objected at. Dropping it would have made the
/// message strictly less useful while looking like a pure type change.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum DocumentRefusal {
    #[error("{operation} input is not valid")]
    InputInvalid {
        operation: &'static str,
        #[source]
        source: ParseError,
    },

    #[error("{operation} input is not a valid S-expression document")]
    InputNotAnSexprDocument {
        operation: &'static str,
        #[source]
        source: ParseError,
    },

    #[error("input is not a valid S-expression document")]
    UnnamedInputNotAnSexprDocument {
        #[source]
        source: ParseError,
    },

    #[error("{operation} output is not valid")]
    OutputInvalid {
        operation: &'static str,
        #[source]
        source: ParseError,
    },

    #[error("{operation} output is not a valid S-expression document")]
    OutputNotAnSexprDocument {
        operation: &'static str,
        #[source]
        source: ParseError,
    },
}

/// The edit could run, but declines to, because the form carries something it
/// would have to move blindly.
///
/// Comments, reader prefixes and declarations have no structural position an
/// edit can reason about, so rewriting around them risks changing meaning.
/// Refusing is the conservative choice and is deliberate — see each edit's
/// documentation for what it would take to support the case.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConservativeRefusal {
    #[error("{operation} cannot rewrite a form containing comments")]
    Comments { operation: &'static str },

    #[error("{operation} cannot rewrite comments or reader prefixes")]
    CommentsOrReaderPrefixes { operation: &'static str },

    #[error("{operation} conservatively rejects comments or reader prefixes")]
    ConservativeCommentsOrReaderPrefixes { operation: &'static str },

    #[error("{operation} conservatively rejects reader prefixes")]
    ReaderPrefixes { operation: &'static str },

    #[error("{operation} requires a form without reader prefixes")]
    RequiresNoReaderPrefixes { operation: &'static str },

    #[error("{operation} conservatively rejects declarations")]
    Declarations { operation: &'static str },

    #[error("{operation} rejects declarations")]
    PlainDeclarations { operation: &'static str },

    #[error("{operation} rejects comments, prefixes, or declarations")]
    CommentsPrefixesOrDeclarations { operation: &'static str },
}

/// The selected form is not the shape the edit operates on.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShapeRefusal {
    #[error("{operation} selected form must be a {expected} form")]
    NotExpectedForm {
        operation: &'static str,
        expected: String,
    },

    #[error("{operation} selected form must be a plain {expected} form")]
    NotPlainExpectedForm {
        operation: &'static str,
        expected: String,
    },

    #[error("{operation} selected form must be a plain {expected}")]
    NotPlainExpected {
        operation: &'static str,
        expected: String,
    },

    /// `role` names which of two forms failed (`outer`, `inner`), so a nested
    /// edit can say which one it was looking at.
    #[error("{operation} {role} must be a plain flet form")]
    RoleNotPlainFlet {
        operation: &'static str,
        role: String,
    },

    #[error("{role} must be a plain {expected} form")]
    UnnamedRoleNotPlainForm { role: String, expected: String },

    /// Same, without `plain`. The two wordings are a CLI contract, so they
    /// stay distinct variants rather than one with a flag.
    #[error("{role} must be a {expected} form")]
    UnnamedRoleNotExpectedForm { role: String, expected: String },

    #[error("{operation} selected form must have a plain head")]
    HeadNotPlain { operation: &'static str },

    #[error("{operation} requires a plain let or let* form")]
    NotPlainLetOrLetStar { operation: &'static str },

    #[error("missing binding form head")]
    MissingBindingFormHead,

    #[error("selected form is not let or let*")]
    NotLetOrLetStar,

    #[error("{operation} requires (if test then [else])")]
    NotIfForm { operation: &'static str },

    #[error("{operation} requires at least one clause")]
    NoClauses { operation: &'static str },

    #[error("{operation} has no clauses")]
    ClausesEmpty { operation: &'static str },

    #[error("{operation} requires each clause to contain exactly test and consequent")]
    ClauseNotTestAndConsequent { operation: &'static str },
}

/// The binding list, or a binding in it, is not one the edit can rewrite.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum BindingRefusal {
    #[error("{operation} requires a binding list")]
    MissingBindingList { operation: &'static str },

    #[error("{operation} requires a plain binding list")]
    NotPlainBindingList { operation: &'static str },

    #[error("{operation} requires plain, non-destructuring bindings")]
    Destructuring { operation: &'static str },

    #[error("{operation} requires a plain binding name")]
    NotPlainBindingName { operation: &'static str },

    #[error("{operation} requires unique binding names")]
    DuplicateBindingNames { operation: &'static str },

    #[error("binding list must be empty")]
    BindingListNotEmpty,

    #[error("binding name")]
    BindingName,

    #[error("binding name is not a symbol")]
    BindingNameNotASymbol,

    #[error("invalid binding name")]
    InvalidBindingName {
        #[source]
        source: SymbolError,
    },

    #[error("{operation} requires a body")]
    MissingBody { operation: &'static str },

    #[error("{operation} requires the outer body to contain only one form")]
    OuterBodyNotSingleForm { operation: &'static str },

    /// `inner` names the inner binding form (`let`, `let*`, `flet`), which is
    /// the part of the message that varies.
    #[error("{operation} requires the inner {inner} to have a body")]
    InnerHasNoBody {
        operation: &'static str,
        inner: &'static str,
    },

    #[error("{operation} --binding-index must be between 1 and {maximum}")]
    BindingIndexOutOfRange {
        operation: &'static str,
        maximum: usize,
    },

    // --- capture: the edit would change which binding a name means ---
    #[error("{operation} initializer references earlier binding '{earlier}'")]
    ReferencesEarlierBinding {
        operation: &'static str,
        earlier: String,
    },

    #[error("inner initializer for '{name}' references outer binding '{outer_name}'")]
    InnerReferencesOuterBinding { name: String, outer_name: String },

    #[error("splitting would capture reference to '{outer_name}' in initializer for '{name}'")]
    SplitWouldCapture { outer_name: String, name: String },
}

/// A local function binding (`flet`, `labels`) the edit cannot rewrite.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LocalFunctionRefusal {
    #[error("{operation} requires plain local function definitions")]
    NotPlainDefinitions { operation: &'static str },

    #[error("{operation} requires a plain {role} definition list")]
    NotPlainDefinitionList {
        operation: &'static str,
        role: String,
    },

    #[error("{operation} requires a plain lambda list")]
    NotPlainLambdaList { operation: &'static str },

    #[error("{operation} requires unique local function names")]
    DuplicateNames { operation: &'static str },

    #[error("{operation} requires a plain local function name")]
    NotPlainName { operation: &'static str },

    #[error("local function name is not plain")]
    NameNotPlain,

    #[error("{operation} cannot capture local function references in definition bodies")]
    WouldCaptureReferences { operation: &'static str },

    #[error("{operation} cannot convert recursive or mutually recursive definitions")]
    Recursive { operation: &'static str },

    #[error(
        "{operation} cannot move an inner definition outside the scope of an outer local function"
    )]
    WouldEscapeOuterScope { operation: &'static str },
}

/// Where an extracted form should go, and why it cannot go there.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InsertionRefusal {
    #[error("--insert before/after requires --anchor-path")]
    MissingAnchorPath,

    #[error("anchor top-level path {anchor_path} is out of range")]
    AnchorOutOfRange { anchor_path: String },

    #[error("append insertion does not use an anchor")]
    AppendTakesNoAnchor,

    #[error("{command} requires a top-level path, for example --path 2")]
    NotTopLevelPath { command: &'static str },

    #[error("replacement span is invalid: {source}")]
    InvalidReplacementSpan {
        #[source]
        source: SpanError,
    },

    #[error("replacement output size overflow")]
    ReplacementSizeOverflow,
}

/// Anything a structural edit can refuse to do.
///
/// Not `Clone`: several variants carry a `ParseError` or a `SexprError`, and
/// cloning an error is not something any caller here needs.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum EditRefusal {
    #[error(transparent)]
    Dialect(#[from] DialectRefusal),

    #[error(transparent)]
    Document(#[from] DocumentRefusal),

    #[error(transparent)]
    Conservative(#[from] ConservativeRefusal),

    #[error(transparent)]
    Shape(#[from] ShapeRefusal),

    #[error(transparent)]
    Binding(#[from] BindingRefusal),

    #[error(transparent)]
    LocalFunction(#[from] LocalFunctionRefusal),

    #[error(transparent)]
    Insertion(#[from] InsertionRefusal),

    /// The edit resolved a path or a selection and the syntax layer refused.
    ///
    /// Distinct from every other variant: those are the edit declining, this
    /// is the tree underneath it declining.
    #[error(transparent)]
    Selection(#[from] paredit_core_syntax::sexpr::SexprError),
}

/// The result type the edit planners return.
pub type EditResult<T> = std::result::Result<T, EditRefusal>;

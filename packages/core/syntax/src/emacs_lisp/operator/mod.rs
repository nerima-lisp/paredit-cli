mod classify;
mod kind;
mod table;

pub use kind::EmacsLispOperator;

use crate::definition::DefinitionCategory;

use super::forms::{
    EmacsLispBinderShape, EmacsLispBindingScope, EmacsLispCallableShape, EmacsLispDefinitionCell,
    EmacsLispDependencyForm, EmacsLispLocalCallableForm,
};

impl EmacsLispOperator {
    /// Resolves an operator from a head symbol, matching case-sensitively.
    #[must_use]
    pub fn from_head(head: &str) -> Option<Self> {
        table::emacs_lisp_operator_from_head(head)
    }

    /// Where this form's binders sit, when it opens a lexical scope.
    #[must_use]
    pub const fn binder_shape(self) -> Option<EmacsLispBinderShape> {
        classify::binder_shape(self)
    }

    /// Whether this form opens a lexical scope at all.
    #[must_use]
    pub const fn is_binding_form(self) -> bool {
        self.binder_shape().is_some()
    }

    #[must_use]
    pub const fn local_callable_form(self) -> Option<EmacsLispLocalCallableForm> {
        classify::local_callable_form(self)
    }

    /// Whether the names this form binds are lexical, dynamic, or decided by
    /// the file's `lexical-binding` header.
    #[must_use]
    pub const fn binding_scope(self) -> EmacsLispBindingScope {
        classify::binding_scope(self)
    }

    /// Whether the names this form binds are symbol macros rather than
    /// variables.
    #[must_use]
    pub const fn binds_symbol_macro(self) -> bool {
        matches!(self, Self::ClSymbolMacrolet)
    }

    #[must_use]
    pub const fn callable_shape(self) -> Option<EmacsLispCallableShape> {
        classify::callable_shape(self)
    }

    #[must_use]
    pub const fn definition_cell(self) -> Option<EmacsLispDefinitionCell> {
        classify::definition_cell(self)
    }

    #[must_use]
    pub const fn definition_category(self) -> Option<DefinitionCategory> {
        classify::definition_category(self)
    }

    #[must_use]
    pub const fn dependency_form(self) -> Option<EmacsLispDependencyForm> {
        classify::dependency_form(self)
    }

    /// Whether this form defines a global name.
    #[must_use]
    pub const fn is_definition(self) -> bool {
        self.definition_cell().is_some()
    }

    /// The child index holding the name this form defines.
    ///
    /// Every Emacs Lisp definition form names its subject at index 1, unlike
    /// Common Lisp's `defmethod`, whose name may be preceded by nothing but
    /// whose lambda list may not.
    #[must_use]
    pub const fn definition_name_child_index(self) -> Option<usize> {
        if self.is_definition() { Some(1) } else { None }
    }

    /// Whether the lambda list is found by scanning for the first list at or
    /// after index 2 rather than at a fixed index.
    ///
    /// True only for `cl-defmethod`, whose optional qualifier
    /// (`(cl-defmethod foo :around ((x integer)) …)`) shifts everything after
    /// it by one.
    #[must_use]
    pub const fn definition_arglist_is_first_list_at_or_after(self) -> Option<usize> {
        match self {
            Self::ClDefmethod => Some(2),
            _ => None,
        }
    }

    /// Whether this form declares its subject a dynamically scoped variable
    /// for the rest of the file — and, in a loaded library, globally.
    ///
    /// This is the Emacs Lisp counterpart of a Common Lisp `special`
    /// proclamation, and it is far more load-bearing here: under
    /// `lexical-binding: t` a plain `let` binds lexically *unless* the name
    /// was declared this way, in which case the same `let` binds dynamically.
    /// The same source text means two different things depending on a form
    /// that may be in another file entirely.
    #[must_use]
    pub const fn declares_dynamic_variable(self) -> bool {
        matches!(
            self,
            Self::Defvar
                | Self::DefvarLocal
                | Self::DefvarKeymap
                | Self::Defconst
                | Self::Defcustom
                | Self::Defvaralias
        )
    }

    /// The Common Lisp head whose form layout is identical to this one's, if
    /// any.
    ///
    /// This exists so the shape tables the refactoring layer already has for
    /// Common Lisp can be reused rather than duplicated: `cl-destructuring-bind`
    /// takes the same three parts in the same three places as
    /// `destructuring-bind`, so a refactor that can rewrite one can rewrite
    /// the other.
    ///
    /// It is a mapping of *layouts*, not of meanings, and only the layouts
    /// that genuinely coincide are listed. `defcustom` is absent even though
    /// its first three children match `defparameter`'s, because everything
    /// after them is a keyword-argument tail that `defparameter` has no
    /// counterpart for; `condition-case` is absent because it puts its
    /// variable where `handler-case` puts its protected form.
    #[must_use]
    pub const fn common_lisp_shape_head(self) -> Option<&'static str> {
        Some(match self {
            Self::Let => "let",
            Self::LetStar => "let*",
            Self::ClSymbolMacrolet => "symbol-macrolet",
            Self::ClDestructuringBind => "destructuring-bind",
            Self::ClMultipleValueBind => "multiple-value-bind",
            Self::Dolist | Self::ClDolist => "dolist",
            Self::Dotimes | Self::ClDotimes => "dotimes",
            Self::ClDo => "do",
            Self::ClDoStar => "do*",
            Self::ClFlet | Self::Flet => "flet",
            Self::ClLabels | Self::Labels => "labels",
            Self::ClMacrolet => "macrolet",
            Self::WithSlots => "with-slots",
            Self::Lambda => "lambda",
            Self::ClLoop => "loop",
            Self::Defun | Self::Defsubst | Self::ClDefun | Self::ClDefsubst => "defun",
            Self::Defmacro | Self::ClDefmacro => "defmacro",
            Self::ClDefgeneric => "defgeneric",
            Self::ClDefmethod => "defmethod",
            Self::Defclass => "defclass",
            Self::ClDefstruct => "defstruct",
            Self::ClDeftype => "deftype",
            Self::Defvar | Self::DefvarLocal => "defvar",
            Self::Defconst => "defconstant",
            Self::Require => "require",
            Self::Provide => "provide",
            Self::Load => "load",
            Self::LoadFile => "load-file",
            Self::LoadLibrary => "load-library",
            _ => return None,
        })
    }

    /// Whether this form is `(declare …)` or `(cl-declare …)`, whose contents
    /// are metadata the evaluator never runs.
    #[must_use]
    pub const fn is_declaration_form(self) -> bool {
        matches!(self, Self::Declare | Self::ClDeclare)
    }

    /// Whether this form evaluates its subforms where they are written, so
    /// that an assignment inside one is visible to a reader of the source.
    ///
    /// This is what separates a known head from an unknown one for opacity
    /// purposes: `(with-temp-buffer (setq x 1))` reassigns `x` in plain
    /// sight, whereas an unregistered macro might expand into the same `setq`
    /// with nothing in the source to show for it.
    #[must_use]
    pub const fn evaluates_subforms_in_place(self) -> bool {
        matches!(
            self,
            Self::Progn
                | Self::Prog1
                | Self::Prog2
                | Self::ClBlock
                | Self::ClReturnFrom
                | Self::Catch
                | Self::UnwindProtect
                | Self::SaveExcursion
                | Self::SaveRestriction
                | Self::SaveMatchData
                | Self::SaveCurrentBuffer
                | Self::SaveWindowExcursion
                | Self::WithCurrentBuffer
                | Self::WithTempBuffer
                | Self::WithTempFile
                | Self::WithOutputToString
                | Self::WithOutputToTempBuffer
                | Self::WithSilentModifications
                | Self::WithSelectedWindow
                | Self::WithSyntaxTable
                | Self::EvalWhenCompile
                | Self::EvalAndCompile
                | Self::WithNoWarnings
                | Self::WithSuppressedWarnings
                | Self::Pcase
                | Self::PcaseExhaustive
                | Self::ClCase
                | Self::ClEcase
                | Self::ClTypecase
                | Self::ClEtypecase
        )
    }
}

//! Emacs Lisp syntax knowledge: operators, binding shapes, and the two
//! file-level conventions (`lexical-binding`, `;;;###autoload`) that change
//! what the code in a file means.
//!
//! This module is deliberately *not* a thin layer over
//! [`crate::common_lisp`]. The two dialects share a surface syntax and very
//! little else at the level this tool reasons about:
//!
//! * Emacs Lisp reads symbols **case-sensitively** and has no package system,
//!   so `LET` is not `let` and `cl:let` is one symbol with a colon in its
//!   name. Routing Emacs Lisp through the Common Lisp head normalizer folded
//!   both distinctions away.
//! * Whether a `let` binds lexically is decided by a *comment on line 1*, not
//!   by the form. See `header`.
//! * The `cl-lib` and `subr-x` families spell familiar shapes differently
//!   (`cl-flet`, `if-let*`, `pcase-let*`, `seq-let`) and add ones Common Lisp
//!   has no counterpart for.

mod autoload;
mod forms;
mod header;
mod operator;
mod symbol;

pub use autoload::{
    EmacsLispAutoloadCookie, EmacsLispAutoloadPayload, emacs_lisp_autoload_cookies,
    is_standard_emacs_lisp_autoload_cookie,
};
pub use forms::{
    EmacsLispBinderShape, EmacsLispBindingScope, EmacsLispBindingVisibility,
    EmacsLispCallableShape, EmacsLispDefinitionCell, EmacsLispDependencyForm,
    EmacsLispLocalCallableForm,
};
pub use header::{EmacsLispFileHeader, EmacsLispLexicalBinding, emacs_lisp_file_header};
pub use operator::EmacsLispOperator;
pub use symbol::{
    emacs_lisp_symbol_has_prefix, is_emacs_lisp_internal_symbol_name, is_emacs_lisp_predicate_name,
    is_emacs_lisp_reserved_prefix,
};

#[cfg(test)]
mod tests;

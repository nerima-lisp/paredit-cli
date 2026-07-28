//! Clojure syntax knowledge: operator classification, binding layouts,
//! threading macros, and canonical indentation.
//!
//! This module is the Clojure counterpart of [`crate::common_lisp`]. It exists
//! because Clojure's structure differs from Common Lisp's in ways that a
//! renamed Common Lisp table cannot express: bindings live in a vector of flat
//! pairs rather than a list of two-element lists, every binding form is
//! sequential, callables can carry several arity clauses, and threading macros
//! reorder their arguments.

mod indent;
mod operator;

pub use indent::{ClojureIndentStyle, clojure_indent_style_for_head};
pub use operator::{
    ClojureBindingForm, ClojureOperator, ClojureThreadingForm, is_clojure_definition_head,
    normalize_clojure_operator_head,
};

#[cfg(test)]
mod tests;

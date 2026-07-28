//! Building a [`super::model::BindingTable`] from a parsed file.
//!
//! One pre-order walk carrying a scope stack. The binding knowledge itself is
//! not restated here: which forms bind what comes from
//! `common_lisp::CommonLispOperator`, which names a pattern binds comes from
//! `lexical_scope::patterns`, and where each subform is evaluated is taken
//! form by form from `lexical_scope::traversal::binding_forms`. What is new is
//! only the *direction* — a table keyed by position instead of a query keyed
//! by name.

mod assignments;
mod binders;
mod builder;
mod lambda_lists;
mod opacity;
mod references;
mod scope_stack;
mod special_names;

#[cfg(test)]
mod tests;

pub use builder::build_binding_table;

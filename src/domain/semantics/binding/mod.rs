//! The binding context: every name a file binds, and what each reference in
//! that file resolves to.
//!
//! `lexical_scope` already knows which forms bind what and how they shadow,
//! but asks the question one symbol at a time: "where is `x` referenced,
//! accounting for shadowing?". The value and type layers need the inverse —
//! "what does the name at *this* position mean?" — which no per-symbol query
//! can answer. This module keeps the same knowledge and turns it inside out
//! into a table built once per file.
//!
//! The shadowing rules themselves are not restated here: the builder shares
//! `lexical_scope`'s traversal and name-extraction helpers, and a differential
//! test pins the two against each other.

pub mod model;
pub mod policy;
pub mod service;

pub use model::{
    Binding, BindingId, BindingKind, BindingTable, OpacityCause, OpacityCauseKind, ScopeId,
};
pub use service::build_binding_table;

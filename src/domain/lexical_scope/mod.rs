//! Lexical binding and scope helpers shared by refactoring use cases.

mod bindings;
mod capture;
mod patterns;
mod syntax;
mod traversal;

#[cfg(test)]
mod tests;

pub use capture::value_capture;
pub use traversal::collect_unshadowed_symbol_references;

/// The name-extraction half of the binding knowledge, shared with the
/// persistent binding table so the two cannot disagree about what a form binds.
#[allow(
    unused_imports,
    reason = "the binding-table builder that consumes these lands with the value layer"
)]
pub(crate) use patterns::{BoundName, binding_pattern_bound_names, lambda_list_bound_names};

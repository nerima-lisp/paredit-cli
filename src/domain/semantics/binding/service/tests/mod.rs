//! Tests for the binding-table builder.

use crate::domain::dialect::Dialect;
use crate::domain::semantics::binding::model::{BindingId, BindingTable};
use crate::domain::sexpr::{ByteSpan, SyntaxTree};

use super::build_binding_table;

mod assignments;
mod differential;
mod property;
mod scoping;

pub(super) fn build(input: &str) -> BindingTable {
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
    build_binding_table(Dialect::CommonLisp, &tree, input)
}

/// Every binding as `name@definition-start`, in registration order.
pub(super) fn binding_labels(table: &BindingTable, input: &str) -> Vec<String> {
    table
        .bindings()
        .map(|(_, binding)| label(binding.definition(), input))
        .collect()
}

/// The binding whose defining atom starts at `offset`.
pub(super) fn binding_at(table: &BindingTable, offset: usize) -> BindingId {
    table
        .bindings()
        .find(|(_, binding)| binding.definition().start().get() == offset)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("no binding defined at byte {offset}"))
}

/// The source text of every reference attributed to `id`, tagged with its
/// offset so two occurrences of the same name stay distinguishable.
pub(super) fn reference_labels(table: &BindingTable, id: BindingId, input: &str) -> Vec<String> {
    table
        .binding(id)
        .references()
        .iter()
        .map(|span| label(*span, input))
        .collect()
}

pub(super) fn assignment_labels(table: &BindingTable, id: BindingId, input: &str) -> Vec<String> {
    table
        .binding(id)
        .assignments()
        .iter()
        .map(|span| label(*span, input))
        .collect()
}

fn label(span: ByteSpan, input: &str) -> String {
    format!("{}@{}", span.slice(input), span.start().get())
}

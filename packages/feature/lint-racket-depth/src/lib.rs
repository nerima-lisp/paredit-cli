#![doc = include_str!("../README.md")]

pub mod begin0_single_form;
pub mod case_lambda_single_clause;
pub mod for_comprehension_value_discarded;
pub mod match_unreachable_clause;
pub mod parameterize_empty_bindings;
pub mod support;

#[cfg(test)]
mod cost_tests;

// One module per rule: a registry names each rule's META and RULE across the
// crate boundary, and this package is deliberately not in one yet.

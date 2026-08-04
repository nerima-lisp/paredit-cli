#![doc = include_str!("../README.md")]

pub mod support;
pub mod unreachable_except_clause;

#[cfg(test)]
mod cost_tests;

// One module per rule: a registry names each rule's META and RULE across the
// crate boundary, and this package is deliberately not in one yet.

#![doc = include_str!("../README.md")]

pub mod bare_except;
pub mod catch_swallows_exit;
pub mod identity_comparison_with_literal;
pub mod mutable_default_argument;

mod shared;

#[cfg(test)]
mod tests;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary; all four are registered there, and the `const` assertions in
// `src/lint/registry/catalog.rs` pin the counts they move.

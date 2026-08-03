#![doc = include_str!("../README.md")]

pub mod defconstant_non_eql_value;
pub mod eval_when_body_never_runs;
pub mod eval_when_execute_only;
pub mod support;

#[cfg(test)]
mod corpus_tests;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.

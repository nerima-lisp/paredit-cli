#![doc = include_str!("../README.md")]

#[cfg(test)]
mod corpus;
#[cfg(test)]
mod corpus_audit;
#[cfg(test)]
mod cost_tests;
#[cfg(test)]
mod engine_pass_tests;

pub mod macro_body_destroys_argument_form;
pub mod macrolet_expander_captures_lexical_variable;
pub mod support;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary; this package is deliberately left unregistered until a separate
// wiring pass adds it.

#![doc = include_str!("../README.md")]

pub mod condition_type_datum_with_string_initarg;
pub mod support;
pub mod unwind_protect_cleanup_signals;

// Cost measurement for both rules, run through the real `HeadFilter::Heads`
// dispatch rather than a hand-rolled walk, together with the control that
// rejected a third rule. Not part of the shipped surface.
#[cfg(test)]
mod cost_tests;

// A false-positive corpus: realistic *correct* Common Lisp that touches every
// head these rules anchor on, linted through the real dispatch, paired with a
// dangerous twin that fires each rule exactly once.
#[cfg(test)]
mod realistic_corpus;

// The third-party false-positive audit over SBCL's and Quicklisp's sources.
// `#[ignore]`d: it needs corpora that only exist on a machine with both.
#[cfg(test)]
mod corpus_audit;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2). This package is deliberately **not** registered.

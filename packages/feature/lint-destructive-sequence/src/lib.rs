#![doc = include_str!("../README.md")]

pub mod discarded_destructive_sequence_result;
pub mod support;

// Cost measurement, run through the real `HeadFilter::Heads` dispatch rather
// than a hand-rolled walk, against a shipped rule's shape measured in the same
// run. Not part of the shipped surface.
#[cfg(test)]
mod cost_tests;

// A false-positive corpus: realistic *correct* Common Lisp that touches every
// head this rule anchors on, linted through the real dispatch, paired with a
// dangerous twin that fires the rule exactly once.
#[cfg(test)]
mod realistic_corpus;

// The third-party false-positive audit over SBCL's and Quicklisp's sources.
// `#[ignore]`d: it needs corpora that only exist on a machine with both.
#[cfg(test)]
mod corpus_audit;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary. This package is deliberately **not** registered.

#![doc = include_str!("../README.md")]

pub mod deprecated_thread_macro;
pub mod support;

#[cfg(test)]
mod corpus_tests;
#[cfg(test)]
mod engine_pass_tests;

/// The rules this package publishes, in the order a registry should list them.
///
/// `cfg(test)` because the root crate owns the catalogue; this exists so the
/// package's own engine-driven tests and the eventual wiring pass name the
/// same rule.
#[cfg(test)]
pub(crate) static ENTRIES: [paredit_core_lint_engine::rule::RuleEntry; 1] =
    [paredit_core_lint_engine::rule::RuleEntry::new(
        &deprecated_thread_macro::rule::META,
        &deprecated_thread_macro::rule::RULE,
    )];

#![doc = include_str!("../README.md")]

pub mod dead_clause;
pub mod illegal_guard_call;
pub mod support;

#[cfg(test)]
mod corpus_audit;
#[cfg(test)]
mod corpus_tests;
#[cfg(test)]
mod cost_tests;
#[cfg(test)]
mod engine_pass_tests;
#[cfg(test)]
mod reader_contract;

/// The rules this package publishes, in the order a registry should list them.
///
/// `cfg(test)` because the root crate owns the catalogue; this exists so the
/// package's own engine-driven tests and the eventual wiring pass name the
/// same rule.
#[cfg(test)]
pub(crate) static ENTRIES: [paredit_core_lint_engine::rule::RuleEntry; 2] = [
    paredit_core_lint_engine::rule::RuleEntry::new(
        &dead_clause::rule::META,
        &dead_clause::rule::RULE,
    ),
    paredit_core_lint_engine::rule::RuleEntry::new(
        &illegal_guard_call::rule::META,
        &illegal_guard_call::rule::RULE,
    ),
];

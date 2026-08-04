#![doc = include_str!("../README.md")]

pub mod fennel_bad_unpack;
pub mod fennel_nested_associative_operator;
pub mod fennel_redundant_do;
pub mod janet_dead_branch_on_constant_condition;
pub mod janet_unreachable_match_clause;
pub mod support;

#[cfg(test)]
mod corpus_tests;
#[cfg(test)]
mod cost_tests;
#[cfg(test)]
mod engine_pass_tests;

/// The rules this package publishes, in the order a registry should list them.
///
/// Exposed only to this crate's own tests, exactly as
/// `paredit-feature-lint-fennel-janet-idiom` does it: the root crate owns the
/// catalogue, and this package is deliberately left unregistered. It exists so
/// the engine-driven tests and the eventual wiring pass name the same five
/// rules and cannot drift.
#[cfg(test)]
pub(crate) static ENTRIES: [paredit_core_lint_engine::rule::RuleEntry; 5] = [
    paredit_core_lint_engine::rule::RuleEntry::new(
        &fennel_bad_unpack::rule::META,
        &fennel_bad_unpack::rule::RULE,
    ),
    paredit_core_lint_engine::rule::RuleEntry::new(
        &fennel_nested_associative_operator::rule::META,
        &fennel_nested_associative_operator::rule::RULE,
    ),
    paredit_core_lint_engine::rule::RuleEntry::new(
        &fennel_redundant_do::rule::META,
        &fennel_redundant_do::rule::RULE,
    ),
    paredit_core_lint_engine::rule::RuleEntry::new(
        &janet_dead_branch_on_constant_condition::rule::META,
        &janet_dead_branch_on_constant_condition::rule::RULE,
    ),
    paredit_core_lint_engine::rule::RuleEntry::new(
        &janet_unreachable_match_clause::rule::META,
        &janet_unreachable_match_clause::rule::RULE,
    ),
];

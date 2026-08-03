#![doc = include_str!("../README.md")]

pub mod fennel_deprecated_form;
pub mod fennel_each_over_non_iterator;
pub mod janet_empty_loop_body;
pub mod janet_mutating_immutable_literal;
pub mod support;
pub mod var_never_set;

#[cfg(test)]
mod corpus_tests;
#[cfg(test)]
mod engine_pass_tests;

/// The rules this package publishes, in the order a registry should list them.
///
/// Exposed as a function rather than a `RuleCatalog` constant because the root
/// crate owns the catalogue (section 4.2); this exists so the package's own
/// engine-driven tests and the eventual wiring pass name the same five rules.
#[cfg(test)]
pub(crate) static ENTRIES: [paredit_core_lint_engine::rule::RuleEntry; 5] = [
    paredit_core_lint_engine::rule::RuleEntry::new(
        &fennel_deprecated_form::rule::META,
        &fennel_deprecated_form::rule::RULE,
    ),
    paredit_core_lint_engine::rule::RuleEntry::new(
        &fennel_each_over_non_iterator::rule::META,
        &fennel_each_over_non_iterator::rule::RULE,
    ),
    paredit_core_lint_engine::rule::RuleEntry::new(
        &janet_empty_loop_body::rule::META,
        &janet_empty_loop_body::rule::RULE,
    ),
    paredit_core_lint_engine::rule::RuleEntry::new(
        &janet_mutating_immutable_literal::rule::META,
        &janet_mutating_immutable_literal::rule::RULE,
    ),
    paredit_core_lint_engine::rule::RuleEntry::new(
        &var_never_set::rule::META,
        &var_never_set::rule::RULE,
    ),
];

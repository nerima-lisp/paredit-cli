//! A local rule catalogue for this package's integration tests.
//!
//! The shipped `REGISTRY` lives in the root crate and this package is
//! deliberately not in it, so the tests build the same thing the registry would:
//! one `RuleEntry` per rule, in a `RuleCatalog`, run through the real
//! dispatcher.
//!
//! Testing against the real engine rather than calling each `examine_*` directly
//! is the entire point. The head index and the dialect filter both live there,
//! and they are exactly where the two silent failure modes hide:
//!
//! - a `HeadFilter` naming a head the rule cannot match, or missing one it can,
//!   which drops every finding at that head while every unit test still passes;
//! - a missing `dialect_scope()`, whose default is `COMMON_LISP_ONLY` — a rule
//!   that forgets it never runs on Racket at all, and nothing but a pass through
//!   the dispatcher notices.

use std::path::Path;

use paredit_core_lint_engine::engine::{
    HeadIndex, PassOptions, build_head_index, collect_lint_pass,
};
use paredit_core_lint_engine::model::LintOutcome;
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use paredit_feature_lint_racket_depth::{
    begin0_single_form, case_lambda_single_clause, for_comprehension_value_discarded,
    match_unreachable_clause, parameterize_empty_bindings,
};

/// Every rule this package publishes.
pub static ENTRIES: [RuleEntry; 5] = [
    RuleEntry::new(
        &begin0_single_form::rule::META,
        &begin0_single_form::rule::RULE,
    ),
    RuleEntry::new(
        &case_lambda_single_clause::rule::META,
        &case_lambda_single_clause::rule::RULE,
    ),
    RuleEntry::new(
        &for_comprehension_value_discarded::rule::META,
        &for_comprehension_value_discarded::rule::RULE,
    ),
    RuleEntry::new(
        &match_unreachable_clause::rule::META,
        &match_unreachable_clause::rule::RULE,
    ),
    RuleEntry::new(
        &parameterize_empty_bindings::rule::META,
        &parameterize_empty_bindings::rule::RULE,
    ),
];

/// Every rule name this package publishes, in `ENTRIES` order.
pub const RULE_NAMES: [&str; 5] = [
    "racket-begin0-single-form",
    "racket-case-lambda-single-clause",
    "racket-for-comprehension-value-discarded",
    "racket-match-unreachable-clause",
    "racket-parameterize-empty-bindings",
];

#[must_use]
pub fn catalog() -> RuleCatalog {
    RuleCatalog::new(&ENTRIES)
}

#[must_use]
pub fn index() -> HeadIndex {
    build_head_index(catalog())
}

/// Runs the whole package over one source string through the real dispatcher.
#[must_use]
pub fn run(source: &str, dialect: Dialect, path: &Path) -> Vec<LintOutcome> {
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse fixture");
    let index = index();
    collect_lint_pass(
        catalog(),
        &index,
        path,
        dialect,
        &tree,
        source,
        RuleSelection::All,
        PassOptions::default(),
    )
    .expect("lint pass")
    .outcomes
}

/// The rule names of every finding, in report order.
///
/// `#[allow(dead_code)]` because this module is compiled once per integration
/// test binary and each one uses a different part of it: `corpus_audit` reads
/// findings through [`run`] so it can report line numbers, and never calls
/// this.
#[allow(dead_code)]
#[must_use]
pub fn rules_fired(source: &str, dialect: Dialect, path: &Path) -> Vec<&'static str> {
    run(source, dialect, path)
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect()
}

//! A local rule catalogue for this package's integration tests.
//!
//! The shipped `REGISTRY` lives in the root crate and this package is not in
//! it yet, so the tests build the same thing the registry would: one
//! `RuleEntry` per rule, in a `RuleCatalog`, run through the real dispatcher.
//! Testing against the real engine rather than calling each `examine_*`
//! directly is the point — the head index, the dialect filter and the quoting
//! of a node all live there, and a rule that works in isolation and never
//! reaches the dispatcher is a rule that does nothing.

use std::path::Path;

use paredit_core_lint_engine::engine::{
    HeadIndex, PassOptions, build_head_index, collect_lint_pass,
};
use paredit_core_lint_engine::model::LintOutcome;
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use paredit_feature_lint_scheme_idiom::{
    begin_single_form, let_star_independent_bindings, memq_assq_literal_key, named_let_never_recurs,
};

/// Every rule this package publishes, in the order the registry would list
/// them.
pub static ENTRIES: [RuleEntry; 4] = [
    RuleEntry::new(
        &begin_single_form::rule::META,
        &begin_single_form::rule::RULE,
    ),
    RuleEntry::new(
        &let_star_independent_bindings::rule::META,
        &let_star_independent_bindings::rule::RULE,
    ),
    RuleEntry::new(
        &memq_assq_literal_key::rule::META,
        &memq_assq_literal_key::rule::RULE,
    ),
    RuleEntry::new(
        &named_let_never_recurs::rule::META,
        &named_let_never_recurs::rule::RULE,
    ),
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
#[must_use]
pub fn rules_fired(source: &str, dialect: Dialect, path: &Path) -> Vec<&'static str> {
    run(source, dialect, path)
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect()
}

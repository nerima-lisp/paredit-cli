//! Both rules driven through the *engine*, rather than through their own
//! `examine_*`.
//!
//! Two declarations decide whether a rule is reachable at all, and neither is
//! visible to a domain test, which calls `examine_*` on a node it picked itself:
//!
//! - the `HeadFilter::Heads` list, which is what the dispatcher's head index is
//!   built from. A head spelled wrongly — or a head the domain matches but the
//!   list omits — leaves every `examine_*` test green while the rule never
//!   receives a single node in production;
//! - the `RuleDialectScope`, which the dispatcher consults *before* walking
//!   anything.

#![cfg(test)]

use std::path::Path;

use paredit_core_lint_engine::engine::{
    PassOptions, build_head_index, collect_lint_outcomes, collect_lint_pass,
};
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

/// This package's shipped rules, as the engine sees them.
pub(crate) static ENTRIES: [RuleEntry; 2] = [
    RuleEntry::new(
        &crate::macro_body_destroys_argument_form::rule::META,
        &crate::macro_body_destroys_argument_form::rule::RULE,
    ),
    RuleEntry::new(
        &crate::macrolet_expander_captures_lexical_variable::rule::META,
        &crate::macrolet_expander_captures_lexical_variable::rule::RULE,
    ),
];

/// The rule names that fire on `source`, sorted so the assertions do not depend
/// on registration order.
///
/// `pub(crate)` because `cost_tests` asserts that its "clean" cost fixture
/// really is clean — a cost number measured on a file that reports is the cost
/// of reporting, not the cost of declining.
pub(crate) fn fired_names(source: &str, dialect: Dialect) -> Vec<&'static str> {
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
    let mut names: Vec<&'static str> = collect_lint_outcomes(
        catalog,
        &index,
        Path::new("t.lisp"),
        dialect,
        &tree,
        source,
        RuleSelection::All,
    )
    .expect("lint pass")
    .into_iter()
    .map(|outcome| outcome.into_parts().0.rule)
    .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// How many nodes each rule's `check` was actually handed.
///
/// This is the **denominator**. A zero-finding sweep over zero candidates is a
/// false clean: it passes just as well when a rule's head list is misspelled
/// and it never runs at all.
fn invocations(source: &str, dialect: Dialect) -> Vec<(&'static str, u64)> {
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
    let outcome = collect_lint_pass(
        catalog,
        &index,
        Path::new("t.lisp"),
        dialect,
        &tree,
        source,
        RuleSelection::All,
        PassOptions {
            settings: None,
            measure: true,
        },
    )
    .expect("measured pass");
    outcome
        .timings
        .expect("timings")
        .entries()
        .map(|(position, _, count)| (catalog.entries()[position].meta().name().as_str(), count))
        .collect()
}

fn invocations_of(source: &str, rule: &str) -> u64 {
    invocations(source, Dialect::CommonLisp)
        .into_iter()
        .find(|(name, _)| *name == rule)
        .map(|(_, count)| count)
        .expect("the rule is in the catalogue")
}

#[test]
fn the_destructive_rule_fires_through_the_dispatcher() {
    assert_eq!(
        fired_names(
            "(defmacro bad (&body forms) `(progn ,@(nreverse forms)))",
            Dialect::CommonLisp
        ),
        vec!["macro-body-destroys-argument-form"]
    );
}

/// The `define-compiler-macro` head must be in the list too: the domain matches
/// it, and a head the domain reads but the filter omits is invisible in
/// production.
#[test]
fn the_destructive_rule_reaches_define_compiler_macro_through_the_dispatcher() {
    assert_eq!(
        fired_names(
            "(define-compiler-macro f (&rest args) `(g ,@(nreverse args)))",
            Dialect::CommonLisp
        ),
        vec!["macro-body-destroys-argument-form"]
    );
    assert_eq!(
        invocations_of(
            "(define-compiler-macro f (a) a)",
            "macro-body-destroys-argument-form"
        ),
        1
    );
}

#[test]
fn the_macrolet_rule_fires_through_the_dispatcher() {
    assert_eq!(
        fired_names(
            "(let ((n 3)) (macrolet ((rep () n)) (rep)))",
            Dialect::CommonLisp
        ),
        vec!["macrolet-expander-captures-lexical-variable"]
    );
}

/// A `macrolet` nested deep inside a function body must still be dispatched:
/// the head index is not restricted to top-level forms.
#[test]
fn the_macrolet_rule_reaches_a_deeply_nested_macrolet() {
    assert_eq!(
        fired_names(
            "(defun f (n)\n  (when t\n    (dolist (x '(1 2))\n      (let ((limit n))\n        \
             (macrolet ((rep () limit)) (rep))))))",
            Dialect::CommonLisp
        ),
        vec!["macrolet-expander-captures-lexical-variable"]
    );
}

#[test]
fn correct_macro_authoring_fires_nothing() {
    assert_eq!(
        fired_names(
            "(defmacro ok (&body forms) `(progn ,@(reverse forms)))\n\
             (let ((code '())) (macrolet ((emit (op) `(push ,op code))) (emit 1)))",
            Dialect::CommonLisp
        ),
        Vec::<&str>::new()
    );
}

/// Both rules are Common Lisp only, and the dispatcher resolves that *before*
/// walking.
#[test]
fn no_rule_runs_for_a_dialect_it_does_not_model() {
    for dialect in [Dialect::Clojure, Dialect::Scheme, Dialect::EmacsLisp] {
        for (name, count) in invocations("(defmacro m (a) a)\n(macrolet ((x () 1)) 1)", dialect) {
            assert_eq!(count, 0, "{name} ran for {dialect:?}");
        }
    }
}

/// The denominator: a file full of the heads must invoke each rule once per
/// head, which is what makes a zero-finding sweep over it meaningful.
#[test]
fn every_declared_head_reaches_its_rule() {
    let source = "(defmacro a (&body b) `(progn ,@b))\n\
                  (define-compiler-macro c (x) x)\n\
                  (macrolet ((d () 1)) (d))\n";
    assert_eq!(
        invocations_of(source, "macro-body-destroys-argument-form"),
        2,
        "defmacro and define-compiler-macro"
    );
    assert_eq!(
        invocations_of(source, "macrolet-expander-captures-lexical-variable"),
        1
    );
}

/// A quoted definition is still *dispatched* — the head index does not know
/// about quoting — so the quote guard has to live in the rule, and this is the
/// test that says so.
#[test]
fn a_quoted_definition_is_dispatched_but_reports_nothing() {
    let source = "(setf template '(defmacro bad (&body b) `(progn ,@(nreverse b))))";
    assert_eq!(
        invocations_of(source, "macro-body-destroys-argument-form"),
        1,
        "the dispatcher hands the rule the quoted form"
    );
    assert_eq!(
        fired_names(source, Dialect::CommonLisp),
        Vec::<&str>::new(),
        "and the rule declines it"
    );
}

//! What each rule here costs when it is handed thousands of its own heads.
//!
//! Both rules in this package correlate a node with something *outside* it —
//! `typed-racket-arity-mismatch` reads the top-level form above its `define`,
//! and `clojure-pre-post-vacuous` asks whether its `defn` is quoted data. That
//! is the exact shape that has twice produced a rule which is linear per invocation
//! and therefore quadratic per file: two shipped rules that re-scanned every
//! top-level form per match were 98% of a whole lint run at 480 definitions,
//! with a cost ratio of 3.7 per doubling where linear is 2.0.
//!
//! Nothing here can be caught by a correctness test: every one of those rules
//! produced exactly the right findings. So the measurement is the test.
//!
//! # How to read the numbers
//!
//! `cargo test -p paredit-feature-lint-contract-annotation --lib cost_ -- \
//!  --nocapture` prints, per rule and per file size, the microseconds the
//! dispatcher attributes to that rule's `check` calls and how many times it was
//! called. Two controls make those numbers mean something:
//!
//! - a **no-op rule** declaring the *same* heads and the same dialect scope, so
//!   the difference between the two columns is this package's own work rather
//!   than the dispatcher's.
//! - a **doubling ratio** across a 8× range of file sizes. Linear work gives
//!   ≈8×; the quadratic shape gives ≈64×.
//!
//! The assertion is deliberately loose — an 8× range asserted under 20× — so
//! that only an asymptotic regression can trip it. Wall-clock numbers on a
//! shared machine swing by large factors between runs, and a tight bound here
//! would fail for reasons that have nothing to do with this code.

use std::path::Path;
use std::time::Duration;

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{
    PassOptions, RuleContext, RuleSink, build_head_index, collect_lint_pass,
};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::{RuleDialectScope, RuleSelection};
use paredit_core_lint_engine::rule::{LintRule, RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionView, SyntaxTree};

/// A rule that matches the same heads and does nothing, so the difference
/// between its column and a real rule's is that rule's own work.
#[derive(Debug)]
struct NoopRule;

const NOOP_HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("define"),
    NormalizedHead::new("defn"),
    NormalizedHead::new("defn-"),
];

const NOOP_META: RuleMeta = RuleMeta::new(
    "cost-control-noop",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a control that matches the same heads and does nothing",
    Fixability::ReportOnly,
);

static NOOP_RULE: NoopRule = NoopRule;

impl LintRule for NoopRule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&NOOP_HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&[Dialect::Racket, Dialect::Clojure, Dialect::CommonLisp])
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        _view: &ExpressionView,
        _sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        Ok(())
    }
}

static ENTRIES: [RuleEntry; 3] = [
    RuleEntry::new(&NOOP_META, &NOOP_RULE),
    RuleEntry::new(
        &crate::clojure_pre_post_vacuous::rule::META,
        &crate::clojure_pre_post_vacuous::rule::RULE,
    ),
    RuleEntry::new(
        &crate::typed_racket_arity_mismatch::rule::META,
        &crate::typed_racket_arity_mismatch::rule::RULE,
    ),
];

/// One measured pass: per-rule microseconds and invocation counts.
fn measure(source: &str, dialect: Dialect) -> Vec<(&'static str, Duration, u64)> {
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
    let outcome = collect_lint_pass(
        catalog,
        &index,
        Path::new("cost.src"),
        dialect,
        &tree,
        source,
        RuleSelection::All,
        PassOptions {
            settings: None,
            measure: true,
        },
    )
    .expect("measured lint pass");

    let timings = outcome.timings.expect("measure: true produces timings");
    timings
        .entries()
        .map(|(position, elapsed, invocations)| {
            (
                catalog.entries()[position].meta().name().as_str(),
                elapsed,
                invocations,
            )
        })
        .collect()
}

fn micros_of(rows: &[(&'static str, Duration, u64)], rule: &str) -> u128 {
    rows.iter()
        .find(|(name, _, _)| *name == rule)
        .map(|(_, elapsed, _)| elapsed.as_micros())
        .expect("the rule is in the catalogue")
}

fn invocations_of(rows: &[(&'static str, Duration, u64)], rule: &str) -> u64 {
    rows.iter()
        .find(|(name, _, _)| *name == rule)
        .map(|(_, _, invocations)| *invocations)
        .expect("the rule is in the catalogue")
}

/// A Racket file of `count` annotated definitions, every one of them correct —
/// the zero-finding shape the `clean/forms/*` benchmarks measure.
fn racket_source(count: usize) -> String {
    let mut out = String::from("#lang typed/racket\n");
    for index in 0..count {
        out.push_str(&format!(
            "(: f{index} (-> Integer Integer))\n(define (f{index} x) (* x {index}))\n"
        ));
    }
    out
}

/// A Clojure file of `count` correct contracted functions.
fn clojure_source(count: usize) -> String {
    let mut out = String::from("(ns cost.core)\n");
    for index in 0..count {
        out.push_str(&format!(
            "(defn f{index} [x]\n  {{:pre [(pos? x)] :post [(pos? %)]}}\n  (* x {index}))\n"
        ));
    }
    out
}

const SIZES: [usize; 4] = [1000, 2000, 4000, 8000];

/// The doubling ratio over an 8× range. Linear is ≈8; the quadratic shape this
/// exists to catch is ≈64.
const MAX_RATIO_OVER_8X: u128 = 20;

fn report(label: &str, rows: &[(&'static str, Duration, u64)], size: usize, rules: &[&str]) {
    for rule in rules {
        println!(
            "{label:>7} n={size:<5} {rule:<34} {:>8}us  invocations={}",
            micros_of(rows, rule),
            invocations_of(rows, rule)
        );
    }
    println!(
        "{label:>7} n={size:<5} {:<34} {:>8}us  invocations={}",
        "cost-control-noop (control)",
        micros_of(rows, "cost-control-noop"),
        invocations_of(rows, "cost-control-noop")
    );
}

/// Guards against a zero-denominator ratio on a machine fast enough to report
/// 0µs for the smallest size.
fn ratio(small: u128, large: u128) -> u128 {
    large / small.max(1)
}

#[test]
fn cost_typed_racket_arity_mismatch_is_linear_in_the_file() {
    let rules = ["typed-racket-arity-mismatch"];
    let mut micros = Vec::new();
    for size in SIZES {
        let source = racket_source(size);
        let rows = measure(&source, Dialect::Racket);
        report("racket", &rows, size, &rules);

        // The head index must hand the rule exactly one node per `define`, and
        // no more: a rule invoked per *node* rather than per head is the other
        // way this cost goes wrong.
        assert_eq!(
            invocations_of(&rows, "typed-racket-arity-mismatch"),
            size as u64,
            "one invocation per define, not per node"
        );
        micros.push(micros_of(&rows, "typed-racket-arity-mismatch"));
    }

    let ratio = ratio(micros[0], micros[3]);
    println!("racket  typed-racket-arity-mismatch 8x ratio = {ratio}");
    assert!(
        ratio < MAX_RATIO_OVER_8X,
        "8x more definitions cost {ratio}x more: the annotation lookup is scanning the file, \
         not binary-searching it ({micros:?}us over {SIZES:?})"
    );
}

#[test]
fn cost_clojure_pre_post_vacuous_is_linear_in_the_file() {
    let rules = ["clojure-pre-post-vacuous"];
    let mut micros = Vec::new();
    for size in SIZES {
        let source = clojure_source(size);
        let rows = measure(&source, Dialect::Clojure);
        report("clojure", &rows, size, &rules);

        assert_eq!(
            invocations_of(&rows, "clojure-pre-post-vacuous"),
            size as u64,
            "one invocation per defn, not per node"
        );
        micros.push(micros_of(&rows, "clojure-pre-post-vacuous"));
    }

    let ratio = ratio(micros[0], micros[3]);
    println!("clojure clojure-pre-post-vacuous 8x ratio = {ratio}");
    assert!(
        ratio < MAX_RATIO_OVER_8X,
        "8x more definitions cost {ratio}x more ({micros:?}us over {SIZES:?})"
    );
}

/// The zero-finding path, which is what the CI `bench-compare` gate measures:
/// a file with none of these rules' heads must not reach a single `check`.
#[test]
fn cost_a_file_without_these_heads_never_reaches_a_check() {
    let source: String = (0..4000)
        .map(|index| format!("(let [x{index} 1] (inc x{index}))\n"))
        .collect();
    let rows = measure(&source, Dialect::Clojure);
    for rule in ["clojure-pre-post-vacuous", "cost-control-noop"] {
        assert_eq!(
            invocations_of(&rows, rule),
            0,
            "{rule} was invoked on a file with none of its heads"
        );
    }
}

/// A rule must not be dispatched at all for a dialect it does not model — the
/// dispatcher resolves the dialect scope once, before the walk. So for a
/// dialect neither of these two claims, this package's whole per-file cost is
/// zero, even on a file full of forms whose heads it would otherwise match.
#[test]
fn cost_is_zero_for_a_dialect_no_rule_here_models() {
    let source: String = (0..4000)
        .map(|index| format!("(fn f{index} [x] (* x {index}))\n"))
        .collect();
    let rows = measure(&source, Dialect::Fennel);
    for (name, elapsed, invocations) in rows {
        assert_eq!(invocations, 0, "{name} ran for Fennel");
        assert_eq!(elapsed, Duration::ZERO, "{name} was timed for Fennel");
    }
}

/// The same thing said the other way, and the sharper version of it: on a
/// **Common Lisp** file whose every form *does* head-match, neither rule is
/// dispatched at all. Common Lisp is the [`RuleDialectScope`] trait default, so
/// this is where a rule that silently lost its scope override would start
/// costing a whole extra pass over every `.lisp` file in a repository — which
/// is exactly what the `clean/forms/*` benchmark gate measures.
///
/// The `define` head is what makes this non-vacuous: the control *is* invoked
/// 4000 times on the same bytes, so the head index really did match and the two
/// zeroes below are the dialect scope talking, not an unmatched head.
///
/// This package once shipped a deliberately Common-Lisp-scoped rule; it was
/// dropped, so the correct expectation here is now zero rather than one.
#[test]
fn cost_is_zero_on_a_common_lisp_file_whose_forms_all_head_match() {
    let source: String = (0..4000)
        .map(|index| format!("(define f{index} (lambda (x) (* x {index})))\n"))
        .collect();
    let rows = measure(&source, Dialect::CommonLisp);
    for rule in ["clojure-pre-post-vacuous", "typed-racket-arity-mismatch"] {
        assert_eq!(invocations_of(&rows, rule), 0, "{rule} ran for Common Lisp");
    }
    assert_eq!(
        invocations_of(&rows, "cost-control-noop"),
        4000,
        "the control must match these heads, or the two zeroes above prove nothing"
    );
}

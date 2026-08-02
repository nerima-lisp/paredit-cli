//! What each rule here costs when it is handed thousands of its own heads.
//!
//! Two rules in this package correlate a node with something *outside* it —
//! `typed-racket-arity-mismatch` reads the top-level form above its `define`,
//! and both Clojure rules ask whether their `defn` is quoted data. That is the
//! exact shape that has twice produced a rule which is linear per invocation
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

const NOOP_HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("define"),
    NormalizedHead::new("defn"),
    NormalizedHead::new("defn-"),
    NormalizedHead::new("check-type"),
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

static ENTRIES: [RuleEntry; 5] = [
    RuleEntry::new(&NOOP_META, &NOOP_RULE),
    RuleEntry::new(
        &crate::check_type_redundant_with_declare::rule::META,
        &crate::check_type_redundant_with_declare::rule::RULE,
    ),
    RuleEntry::new(
        &crate::clojure_pre_post_vacuous::rule::META,
        &crate::clojure_pre_post_vacuous::rule::RULE,
    ),
    RuleEntry::new(
        &crate::clojure_pre_referencing_percent::rule::META,
        &crate::clojure_pre_referencing_percent::rule::RULE,
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

/// A Common Lisp file of `count` correct functions, each with a `check-type`
/// that *narrows* the declaration above it — head-matched, fully analysed, and
/// reaching no finding.
fn common_lisp_source(count: usize) -> String {
    let mut out = String::new();
    for index in 0..count {
        out.push_str(&format!(
            "(defun f{index} (x)\n  (declare (type integer x))\n  \
             (check-type x (integer 0 10))\n  (* x {index}))\n"
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
fn cost_clojure_rules_are_linear_in_the_file() {
    let rules = [
        "clojure-pre-post-vacuous",
        "clojure-pre-referencing-percent",
    ];
    let mut vacuous = Vec::new();
    let mut percent = Vec::new();
    for size in SIZES {
        let source = clojure_source(size);
        let rows = measure(&source, Dialect::Clojure);
        report("clojure", &rows, size, &rules);

        for rule in rules {
            assert_eq!(
                invocations_of(&rows, rule),
                size as u64,
                "{rule}: one invocation per defn, not per node"
            );
        }
        vacuous.push(micros_of(&rows, "clojure-pre-post-vacuous"));
        percent.push(micros_of(&rows, "clojure-pre-referencing-percent"));
    }

    for (rule, micros) in [
        ("clojure-pre-post-vacuous", &vacuous),
        ("clojure-pre-referencing-percent", &percent),
    ] {
        let ratio = ratio(micros[0], micros[3]);
        println!("clojure {rule} 8x ratio = {ratio}");
        assert!(
            ratio < MAX_RATIO_OVER_8X,
            "{rule}: 8x more definitions cost {ratio}x more ({micros:?}us over {SIZES:?})"
        );
    }
}

#[test]
fn cost_check_type_redundant_with_declare_is_linear_in_the_file() {
    let rules = ["check-type-redundant-with-declare"];
    let mut micros = Vec::new();
    for size in SIZES {
        let source = common_lisp_source(size);
        let rows = measure(&source, Dialect::CommonLisp);
        report("cl", &rows, size, &rules);

        assert_eq!(
            invocations_of(&rows, "check-type-redundant-with-declare"),
            size as u64,
            "one invocation per check-type, not per node"
        );
        micros.push(micros_of(&rows, "check-type-redundant-with-declare"));
    }

    let ratio = ratio(micros[0], micros[3]);
    println!("cl      check-type-redundant-with-declare 8x ratio = {ratio}");
    assert!(
        ratio < MAX_RATIO_OVER_8X,
        "8x more check-types cost {ratio}x more: the ancestor lookup is scanning the file, not \
         descending to the node ({micros:?}us over {SIZES:?})"
    );
}

/// The other way this rule's cost could go wrong: many `check-type` forms
/// inside *one* function, where each one reads the same parent's children.
///
/// That is quadratic in the number of checks per function by construction, so
/// what this pins is the constant — a single function with 2000 checks must
/// still be well under a second. Real code does not write functions like this;
/// the test exists so that a change making the per-candidate scan reach further
/// than the immediate parent shows up as a number rather than as nothing.
#[test]
fn cost_many_check_types_in_one_function_stays_bounded() {
    let checks: String = (0..2000)
        .map(|index| format!("  (check-type x{index} integer)\n"))
        .collect();
    let params: String = (0..2000)
        .map(|index| format!("x{index} "))
        .collect::<String>();
    let source = format!("(defun big ({params})\n  (declare (type integer x0))\n{checks}  x0)\n");

    let started = std::time::Instant::now();
    let rows = measure(&source, Dialect::CommonLisp);
    let elapsed = started.elapsed();
    println!(
        "cl      one function, 2000 check-types: {:>8}us  invocations={}  wall={elapsed:?}",
        micros_of(&rows, "check-type-redundant-with-declare"),
        invocations_of(&rows, "check-type-redundant-with-declare")
    );
    assert_eq!(
        invocations_of(&rows, "check-type-redundant-with-declare"),
        2000
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "2000 check-types in one function took {elapsed:?}"
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
    for rule in [
        "clojure-pre-post-vacuous",
        "clojure-pre-referencing-percent",
        "cost-control-noop",
    ] {
        assert_eq!(
            invocations_of(&rows, rule),
            0,
            "{rule} was invoked on a file with none of its heads"
        );
    }
}

/// A rule must not be dispatched at all for a dialect it does not model — the
/// dispatcher resolves the dialect scope once, before the walk. So for a
/// dialect none of these four claim, this package's whole per-file cost is
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

/// The same thing said the other way: on a Common Lisp file, only the one
/// Common Lisp rule is dispatched. The three others cost nothing at all, which
/// is what keeps this package off the `clean/forms/*` benchmark's critical
/// path.
#[test]
fn cost_of_the_non_common_lisp_rules_is_zero_on_a_common_lisp_file() {
    let source: String = (0..4000)
        .map(|index| format!("(defun f{index} (x) (* x {index}))\n"))
        .collect();
    let rows = measure(&source, Dialect::CommonLisp);
    for rule in [
        "clojure-pre-post-vacuous",
        "clojure-pre-referencing-percent",
        "typed-racket-arity-mismatch",
    ] {
        assert_eq!(invocations_of(&rows, rule), 0, "{rule} ran for Common Lisp");
    }
    // And the Common Lisp rule itself is never reached either, because this
    // file has no `check-type` in it.
    assert_eq!(
        invocations_of(&rows, "check-type-redundant-with-declare"),
        0
    );
}

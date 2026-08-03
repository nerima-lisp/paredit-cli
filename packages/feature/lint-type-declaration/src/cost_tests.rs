//! What each rule here costs when it is handed thousands of its own heads.
//!
//! Every rule in this package reaches *outside* the node it is given: each asks
//! [`crate::support::is_unevaluated_at`] once a finding is in hand, and that
//! descends from the file's root. That is the exact shape that has twice
//! produced a rule which is linear per invocation and therefore quadratic per
//! file: two shipped rules that re-scanned every top-level form per match were
//! 98% of a whole lint run at 480 definitions, at 3.7 per doubling where linear
//! is 2.0. It is why `is_unevaluated_at` binary-searches each level rather than
//! scanning it.
//!
//! Nothing here can be caught by a correctness test: those rules produced
//! exactly the right findings. So the measurement is the test.
//!
//! # How to read the numbers
//!
//! ```text
//! cargo test -p paredit-feature-lint-type-declaration --lib cost_ -- --nocapture
//! ```
//!
//! prints, per rule and per file size, the microseconds the dispatcher
//! attributes to that rule's `check` calls and how many times it was called.
//! Two controls make those numbers mean something:
//!
//! - a **no-op rule** declaring the *same* heads and the same dialect scope, so
//!   the difference between the two columns is this package's own work rather
//!   than the dispatcher's;
//! - a **doubling ratio** across an 8× range of file sizes. Linear work gives
//!   ≈8×; the quadratic shape gives ≈64×.
//!
//! A comparator from another *shipped* lint package would be better still, but
//! `tests/cli/feature_dependency_contract.rs` scans the whole manifest text for
//! `paredit-feature-` — `[dev-dependencies]` included — so a cross-feature dev
//! dependency would trip a contract this package is not allowed to edit. The
//! no-op control is this repository's established substitute.
//!
//! # What runs unattended
//!
//! Only the **invocation counts**. They are decided by the head index and the
//! dialect scope before any `check` body runs, so they are the same number on an
//! idle machine and a loaded one.
//!
//! The **doubling ratio is a benchmark, not a test**, and is `#[ignore]`d: a
//! ratio between two wall-clock durations is unstable under parallel load at any
//! threshold, and a sibling package's version of the same idea failed CI on a
//! busy runner. Run it deliberately:
//!
//! ```text
//! cargo test -p paredit-feature-lint-type-declaration \
//!   -- --ignored --nocapture ignored_bench_
//! ```

use std::path::Path;
use std::time::Duration;

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{
    PassOptions, RuleContext, RuleSink, build_head_index, collect_lint_pass,
};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::{RuleDialectScope, RuleSelection};
use paredit_core_lint_engine::rule::{LintRule, RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionView, SyntaxTree};

use crate::support::{COMMON_LISP_ONLY, DECLARATION_BODY_RULE_HEADS};

/// A rule that matches the same heads and does nothing, so the difference
/// between its column and a real rule's is that rule's own work.
#[derive(Debug)]
struct NoopRule;

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
        HeadFilter::Heads(&DECLARATION_BODY_RULE_HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        COMMON_LISP_ONLY
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

static ENTRIES: [RuleEntry; 6] = [
    RuleEntry::new(&NOOP_META, &NOOP_RULE),
    RuleEntry::new(
        &crate::declare_not_at_head_of_body::rule::META,
        &crate::declare_not_at_head_of_body::rule::RULE,
    ),
    RuleEntry::new(
        &crate::declaim_inside_body::rule::META,
        &crate::declaim_inside_body::rule::RULE,
    ),
    RuleEntry::new(
        &crate::type_declaration_contradicts_initform::rule::META,
        &crate::type_declaration_contradicts_initform::rule::RULE,
    ),
    RuleEntry::new(
        &crate::the_form_with_impossible_type::rule::META,
        &crate::the_form_with_impossible_type::rule::RULE,
    ),
    RuleEntry::new(
        &crate::type_declaration_on_rest_parameter::rule::META,
        &crate::type_declaration_on_rest_parameter::rule::RULE,
    ),
];

const RULES: [&str; 5] = [
    "declare-not-at-head-of-body",
    "declaim-inside-body",
    "type-declaration-contradicts-initform",
    "the-form-with-impossible-type",
    "type-declaration-on-rest-parameter",
];

/// One measured pass: per-rule microseconds and invocation counts.
fn measure(source: &str) -> Vec<(&'static str, Duration, u64)> {
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
    let outcome = collect_lint_pass(
        catalog,
        &index,
        Path::new("cost.lisp"),
        Dialect::CommonLisp,
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

/// `count` **correct** declaration-carrying definitions — the zero-finding shape
/// the `clean/forms/*` benchmarks measure. Every rule here must decline all of
/// them, and the cost of declining is what these numbers are.
fn clean_source(count: usize) -> String {
    let mut out = String::from(";;; correct declarations throughout\n");
    for index in 0..count {
        out.push_str(&format!(
            "(defun f{index} (a b &rest more)\n  \"Doc.\"\n  (declare (fixnum a) (ignore b) \
             (list more) (optimize (speed 3)))\n  (let ((total 0))\n    (declare (fixnum total))\n\
             \x20   (the fixnum (+ a total))))\n"
        ));
    }
    out
}

/// An adversarial shape for `declare-not-at-head-of-body`: every definition
/// carries a misplaced declaration, so the rule reports on every one.
///
/// The fixtures that *report* are what expose a per-finding cost. A rule that is
/// linear while declining can still be quadratic once it starts reaching outside
/// its node to confirm a finding, and only a fixture like this shows it.
fn misplaced_heavy_source(count: usize) -> String {
    (0..count)
        .map(|index| format!("(defun h{index} (a) (print a) (declare (fixnum a)) a)\n"))
        .collect()
}

/// The same, for `the-form-with-impossible-type` — an unrelated rule with a
/// different head, a different analysis and a different reason to report.
///
/// The control for
/// [`ignored_bench_the_per_finding_cost_is_shared_by_unrelated_rules`]: two
/// rules that share nothing but `sink.report` and cost the same per finding are
/// not both implemented wrongly in the same way.
fn the_heavy_source(count: usize) -> String {
    (0..count)
        .map(|index| format!("(defun k{index} () (the null (list {index} 2)))\n"))
        .collect()
}

const SIZES: [usize; 4] = [500, 1000, 2000, 4000];

/// Every node in `source`, so an invocation count can be read against the number
/// of nodes a per-node rule would have been called on.
fn node_count(source: &str) -> u64 {
    fn walk(view: &ExpressionView) -> u64 {
        1 + view.children.iter().map(walk).sum::<u64>()
    }
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
    tree.root_view().children.iter().map(walk).sum()
}

fn report(label: &str, rows: &[(&'static str, Duration, u64)], size: usize) {
    for rule in RULES.iter().chain(std::iter::once(&"cost-control-noop")) {
        let invocations = invocations_of(rows, rule);
        let micros = micros_of(rows, rule);
        let per_invocation = if invocations == 0 {
            0
        } else {
            micros * 1000 / u128::from(invocations)
        };
        println!(
            "{label:>8} n={size:<5} {rule:<40} {micros:>7}us  invocations={invocations:<6} \
             {per_invocation:>6}ns/inv"
        );
    }
}

/// Guards against a zero-denominator ratio on a machine fast enough to report
/// 0µs for the smallest size.
fn ratio(small: u128, large: u128) -> u128 {
    large / small.max(1)
}

/// The head index must hand each rule one node per *form it declares a head
/// for*, and no more. A rule invoked per node rather than per head is one of the
/// two ways this cost goes wrong, and it is the way that can be pinned exactly:
/// the count is a property of the head index, not of the machine.
///
/// The node count is the control. Without it, "N invocations at n=N" is also
/// what a per-node dispatch would report on a file that happened to have N
/// nodes.
#[test]
fn cost_each_rule_is_dispatched_once_per_head_not_per_node() {
    for size in SIZES {
        let source = clean_source(size);
        let nodes = node_count(&source);
        let rows = measure(&source);
        report("clean", &rows, size);

        assert!(
            nodes > (size as u64) * 20,
            "the fixture has {nodes} nodes for {size} definitions; a per-head count cannot be \
             told apart from a per-node one"
        );

        // One `defun` and one `let` per definition.
        for rule in ["declare-not-at-head-of-body", "declaim-inside-body"] {
            assert_eq!(
                invocations_of(&rows, rule),
                (size as u64) * 2,
                "{rule}: one per defun and one per let, not per node ({nodes} nodes)"
            );
        }
        // `the` occurs once per definition and matches nothing else.
        assert_eq!(
            invocations_of(&rows, "the-form-with-impossible-type"),
            size as u64,
            "the-form-with-impossible-type is dispatched once per `the` form"
        );
        // Only the `let` carries this rule's heads.
        assert_eq!(
            invocations_of(&rows, "type-declaration-contradicts-initform"),
            size as u64
        );
        // Only the `defun` carries this one's.
        assert_eq!(
            invocations_of(&rows, "type-declaration-on-rest-parameter"),
            size as u64
        );
    }
}

/// A file with none of these heads must not reach a single `check` body. This is
/// what the CI `bench-compare` gate measures, and the reason every rule here
/// declares [`HeadFilter::Heads`] rather than `WholeTree`.
#[test]
fn cost_a_file_without_these_heads_never_reaches_a_check() {
    let source: String = (0..4000)
        .map(|index| format!("(setq x{index} (+ {index} 1))\n"))
        .collect();
    let rows = measure(&source);
    for rule in RULES.iter().chain(std::iter::once(&"cost-control-noop")) {
        assert_eq!(
            invocations_of(&rows, rule),
            0,
            "{rule} was invoked on a file with none of its heads"
        );
    }
}

/// A rule must not be dispatched at all for a dialect it does not model — the
/// dispatcher resolves the dialect scope once, before the walk. Every rule here
/// is Common Lisp only, so on a Clojure file whose every form head-matches, this
/// package's whole per-file cost is zero.
#[test]
fn cost_is_zero_for_a_dialect_no_rule_here_models() {
    let source: String = (0..2000)
        .map(|index| format!("(let [x{index} 1] (inc x{index}))\n"))
        .collect();
    let rows = measure_as(&source, Dialect::Clojure);
    for (name, elapsed, invocations) in rows {
        assert_eq!(invocations, 0, "{name} ran for Clojure");
        assert_eq!(elapsed, Duration::ZERO, "{name} was timed for Clojure");
    }
}

/// [`measure`], for a dialect other than Common Lisp.
fn measure_as(source: &str, dialect: Dialect) -> Vec<(&'static str, Duration, u64)> {
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
    let timings = outcome.timings.expect("timings");
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

/// A rule that *reports* on every form is still dispatched once per form. The
/// invocation count is linear — that is what this pins — but the work per
/// finding is a separate question, which the benchmark below measures.
#[test]
fn cost_a_reporting_rule_is_still_dispatched_once_per_form() {
    for size in [500, 1000] {
        let rows = measure(&misplaced_heavy_source(size));
        report("misplcd", &rows, size);
        assert_eq!(
            invocations_of(&rows, "declare-not-at-head-of-body"),
            size as u64,
            "one per defun even when every one of them reports"
        );
    }
}

/// Where the per-*finding* cost lives.
///
/// A rule here costs roughly 1.2µs per finding on a file where every form
/// reports, against ~30ns per declining invocation — and that per-finding number
/// itself **doubles when the file doubles**, which is a linear scan per finding
/// somewhere downstream of `sink.report`.
///
/// It is not this package's analysis. Two rules with different heads, different
/// analyses and no shared code measured 1.13µs and 1.24µs per finding at n=500,
/// and 2.27µs and 2.44µs at n=1000 — the same number and the same growth. What
/// they share is the engine's finding materialisation.
///
/// So the number is reported rather than optimised. It prints and does not
/// assert, because it is wall-clock; what matters is that the two columns track
/// each other, which is the evidence that the cost is common to both.
#[test]
#[ignore = "a benchmark: wall-clock per-finding costs are unstable under load"]
fn ignored_bench_the_per_finding_cost_is_shared_by_unrelated_rules() {
    for size in [500, 1000] {
        let rows = measure(&misplaced_heavy_source(size));
        report("misplcd", &rows, size);
        let rows = measure(&the_heavy_source(size));
        report("the", &rows, size);
    }
}

/// The other way the cost goes wrong: the right number of invocations, each of
/// which re-derives a whole-file answer. Invocation counts cannot see that —
/// only the growth rate can, and the growth rate is wall-clock.
///
/// Over the 8× range in [`SIZES`], linear work gives ≈8 and the quadratic shape
/// this exists to catch gives ≈64.
#[test]
#[ignore = "a benchmark: wall-clock ratios are unstable under parallel load"]
fn ignored_bench_doubling_ratio() {
    for (label, generate) in [
        ("clean", clean_source as fn(usize) -> String),
        ("misplcd", misplaced_heavy_source as fn(usize) -> String),
    ] {
        let mut columns: Vec<Vec<u128>> = vec![Vec::new(); RULES.len() + 1];
        for size in SIZES {
            let rows = measure(&generate(size));
            report(label, &rows, size);
            for (slot, rule) in RULES
                .iter()
                .chain(std::iter::once(&"cost-control-noop"))
                .enumerate()
            {
                columns[slot].push(micros_of(&rows, rule));
            }
        }
        for (slot, rule) in RULES
            .iter()
            .chain(std::iter::once(&"cost-control-noop"))
            .enumerate()
        {
            let micros = &columns[slot];
            println!(
                "{label:>8} {rule:<40} 8x ratio = {:>3} ({micros:?}us over {SIZES:?}) \
                 -- linear is ~8, a per-match whole-file scan is ~64",
                ratio(micros[0], micros[3])
            );
        }
    }
}

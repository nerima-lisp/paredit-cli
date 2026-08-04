//! What each rule here costs when it is handed thousands of its own heads.
//!
//! Both shipped rules are **local to the form the dispatcher hands them** in
//! the common case, and reach [`SyntaxTree::root_view`] only once a finding is
//! otherwise ready to report. This file is what establishes that rather than
//! asserting it.
//!
//! # The controls
//!
//! - **`cost-control-noop`** declares the same heads and the same dialect
//!   scope and does nothing, so the difference between its column and a real
//!   rule's is that rule's own work rather than the dispatcher's.
//! - **`cost-control-eager-root-view`** is the *ordering mistake*, written out:
//!   it calls `root_view` first and does its cheap check second. It is the
//!   same shape an earlier batch in this workspace measured at 450843 ns/call
//!   against 28 ns/call, and it exists so the cost of getting the order wrong
//!   is a number in this file rather than a warning in a comment.
//!
//! # What runs unattended
//!
//! Only the **invocation counts** and the cheap/expensive *ordering*
//! assertions. They are decided by the head index and by control flow, not by
//! the clock. The **doubling ratios are benchmarks, not tests**, and are
//! `#[ignore]`d: a ratio between two wall-clock durations is unstable under
//! the parallel load this repository's agents generate, and this workspace has
//! a standing finding that its `bench-compare` gate is statistically invalid
//! in both directions.
//!
//! ```text
//! cargo test -p paredit-feature-lint-lfe-carp-depth --lib cost_ -- --nocapture
//! cargo test -p paredit-feature-lint-lfe-carp-depth --lib -- --ignored --nocapture ignored_bench_
//! ```

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

use crate::dead_clause;
use crate::illegal_guard_call;
use crate::support::node_context;

/// Every head the two shipped rules together declare.
const ALL_HEADS: [NormalizedHead; 5] = [
    NormalizedHead::new("case"),
    NormalizedHead::new("receive"),
    NormalizedHead::new("match-lambda"),
    NormalizedHead::new("defun"),
    NormalizedHead::new("when"),
];

const LFE_ONLY: [Dialect; 1] = [Dialect::Lfe];

// -- the no-op control --------------------------------------------------------

#[derive(Debug)]
struct NoopRule;

const NOOP_META: RuleMeta = RuleMeta::new(
    "cost-control-noop",
    RuleCategory::DeadCode,
    Severity::Warning,
    "a control that matches the same heads and does nothing",
    Fixability::ReportOnly,
);

static NOOP_RULE: NoopRule = NoopRule;

impl LintRule for NoopRule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&ALL_HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&LFE_ONLY)
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        _view: &ExpressionView,
        _sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        Ok(())
    }
}

// -- the ordering mistake, written out ----------------------------------------

/// The same work the clause rule does, in the wrong order: the document-wide
/// `node_context` descent *first*, the cheap local `examine` second.
///
/// Every visited head pays for a full materialization of the document, whether
/// or not it could ever produce a finding. This is the shape that measured
/// four orders of magnitude in an earlier batch, and its column in the report
/// is what that mistake would cost here.
#[derive(Debug)]
struct EagerRootViewRule;

const EAGER_META: RuleMeta = RuleMeta::new(
    "cost-control-eager-root-view",
    RuleCategory::DeadCode,
    Severity::Warning,
    "a control that materializes the document before its cheap check",
    Fixability::ReportOnly,
);

static EAGER_RULE: EagerRootViewRule = EagerRootViewRule;

impl LintRule for EagerRootViewRule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&ALL_HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&LFE_ONLY)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        _sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        // Deliberately backwards.
        let context_at = node_context(context.tree(), view.span);
        if context_at.suppresses_findings() {
            return Ok(());
        }
        let _ = dead_clause::domain::examine(context.dialect(), view);
        Ok(())
    }
}

// -- the catalogue under measurement ------------------------------------------

static COST_ENTRIES: [RuleEntry; 4] = [
    RuleEntry::new(&dead_clause::rule::META, &dead_clause::rule::RULE),
    RuleEntry::new(
        &illegal_guard_call::rule::META,
        &illegal_guard_call::rule::RULE,
    ),
    RuleEntry::new(&NOOP_META, &NOOP_RULE),
    RuleEntry::new(&EAGER_META, &EAGER_RULE),
];

// -- measurement --------------------------------------------------------------

fn measure_as(source: &str, dialect: Dialect) -> Vec<(&'static str, Duration, u64)> {
    let catalog = RuleCatalog::new(&COST_ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
    let outcome = collect_lint_pass(
        catalog,
        &index,
        Path::new("cost.lfe"),
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
    outcome
        .timings
        .expect("measure: true produces timings")
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

fn measure(source: &str) -> Vec<(&'static str, Duration, u64)> {
    measure_as(source, Dialect::Lfe)
}

fn nanos_of(rows: &[(&'static str, Duration, u64)], rule: &str) -> u128 {
    rows.iter()
        .find(|(name, _, _)| *name == rule)
        .map(|(_, elapsed, _)| elapsed.as_nanos())
        .expect("the rule is in the catalogue")
}

fn invocations_of(rows: &[(&'static str, Duration, u64)], rule: &str) -> u64 {
    rows.iter()
        .find(|(name, _, _)| *name == rule)
        .map(|(_, _, invocations)| *invocations)
        .expect("the rule is in the catalogue")
}

// -- fixtures -----------------------------------------------------------------

/// `count` correct functions: every rule's heads present, no rule firing.
///
/// This is the case that matters for cost. Findings are rare; the work a rule
/// does on clean code is what a user actually pays.
fn clean_source(count: usize) -> String {
    let mut out = String::from("(defmodule cost (export all))\n");
    for index in 0..count {
        out.push_str(&format!(
            "(defun f{index}
  ((x) (when (is_integer x)) 'int)
  ((x) (when (erlang:is_atom x)) 'atom)
  ((_) 'other))
(defun g{index} (a b)
  (case (+ a b)
    ('one 1)
    ('two 2)
    (_ 'many)))
(defun h{index} ()
  (receive
    ('ping 'pong)
    (msg (when (is_tuple msg)) 'tuple)
    (after 1000 'timeout)))
"
        ));
    }
    out
}

/// `count` functions each carrying one finding for each rule, so the reporting
/// path — including the `node_context` descent — is measured too.
fn dirty_source(count: usize) -> String {
    let mut out = String::from("(defmodule cost (export all))\n");
    for index in 0..count {
        out.push_str(&format!(
            "(defun f{index}
  ((x) (when (lists:member x '(1 2))) 'yes)
  ((_) 'no))
(defun g{index} (a b)
  (case (+ a b)
    (_ 'many)
    ('two 2)))
"
        ));
    }
    out
}

fn node_count(source: &str) -> u64 {
    fn walk(view: &ExpressionView) -> u64 {
        1 + view.children.iter().map(walk).sum::<u64>()
    }
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::Lfe).expect("parse");
    walk(&tree.root_view())
}

fn report(label: &str, rows: &[(&'static str, Duration, u64)], size: usize) {
    println!("-- {label} n={size} --");
    let mut sorted: Vec<_> = rows.iter().collect();
    sorted.sort_by_key(|(_, elapsed, _)| std::cmp::Reverse(elapsed.as_nanos()));
    for (name, elapsed, invocations) in sorted {
        let per = if *invocations == 0 {
            0
        } else {
            elapsed.as_nanos() / u128::from(*invocations)
        };
        println!(
            "   {name:34} {:>12} ns  {invocations:>7} inv  {per:>9} ns/inv",
            elapsed.as_nanos()
        );
    }
}

fn ratio(small: u128, large: u128) -> u128 {
    (large * 100).checked_div(small).unwrap_or(0)
}

// -- what runs unattended -----------------------------------------------------

/// The head index decides invocations, not the file's size in nodes. A rule
/// invoked once per *node* rather than once per *head* is the failure this
/// pins.
#[test]
fn cost_each_rule_is_dispatched_once_per_head_not_per_node() {
    let source = clean_source(40);
    let rows = measure(&source);
    let nodes = node_count(&source);

    // 40 iterations x (2 `when` in f, 1 `when` in h) = 120 `when` forms.
    assert_eq!(invocations_of(&rows, "lfe-illegal-guard-call"), 120);
    // 40 x (defun f, defun g, defun h, case, receive) = 200.
    assert_eq!(invocations_of(&rows, "lfe-clause-after-catch-all"), 200);
    // The no-op sees every head either rule declares.
    assert_eq!(invocations_of(&rows, "cost-control-noop"), 320);

    assert!(
        nodes > 2000,
        "the fixture must be large enough for the distinction to mean something, was {nodes}"
    );
    assert!(
        u64::from(u32::try_from(nodes).expect("fits")) > invocations_of(&rows, "cost-control-noop"),
        "dispatch must be driven by the head index, not the node count"
    );
}

/// A file with none of these heads never reaches a `check`.
#[test]
fn cost_a_file_without_these_heads_never_reaches_a_check() {
    let source = "(defmodule m (export all))\n(defrecord point x y)\n".repeat(50);
    let rows = measure(&source);
    assert_eq!(invocations_of(&rows, "lfe-illegal-guard-call"), 0);
    assert_eq!(invocations_of(&rows, "lfe-clause-after-catch-all"), 0);
}

/// The dialect gate is applied before dispatch, so a dialect no rule here
/// models costs nothing at all.
#[test]
fn cost_is_zero_for_a_dialect_no_rule_here_models() {
    let source = clean_source(20);
    for dialect in [Dialect::CommonLisp, Dialect::Clojure, Dialect::Carp] {
        let rows = measure_as(&source, dialect);
        assert_eq!(
            invocations_of(&rows, "lfe-illegal-guard-call"),
            0,
            "{dialect:?}"
        );
        assert_eq!(
            invocations_of(&rows, "lfe-clause-after-catch-all"),
            0,
            "{dialect:?}"
        );
    }
}

/// **The ordering assertion.** On clean code, where no finding exists, the two
/// shipped rules must cost far less than the control that materializes the
/// document first.
///
/// This is a control-flow fact, not a timing coincidence: the shipped rules
/// return before `node_context` on every one of these forms, and the control
/// calls it on every one. The threshold is deliberately loose — a factor of
/// three, where the measured difference is far larger — so that it fails only
/// if the *ordering* regresses, not when the machine is loaded.
#[test]
fn cost_the_cheap_check_comes_before_the_expensive_one() {
    let source = clean_source(60);
    let rows = measure(&source);

    let eager = nanos_of(&rows, "cost-control-eager-root-view");
    let clause = nanos_of(&rows, "lfe-clause-after-catch-all");
    let guard = nanos_of(&rows, "lfe-illegal-guard-call");

    assert!(
        clause * 3 < eager,
        "the clause rule ({clause} ns) must be far cheaper than the eager control ({eager} ns); \
         if it is not, `node_context` has moved ahead of `examine`"
    );
    assert!(
        guard * 3 < eager,
        "the guard rule ({guard} ns) must be far cheaper than the eager control ({eager} ns)"
    );
}

/// And the reason the ordering is safe: on clean code the expensive descent
/// runs zero times. Reporting code still has to pay it, which the dirty
/// fixture exercises.
#[test]
fn cost_a_reporting_rule_is_still_dispatched_once_per_form() {
    let source = dirty_source(30);
    let rows = measure(&source);
    assert_eq!(invocations_of(&rows, "lfe-illegal-guard-call"), 30);
    // Three heads per iteration for the clause rule: the matching `defun f`,
    // the traditional `defun g`, and the `case` inside it. The traditional
    // `defun` is dispatched like any other and rejected by `clause_form`.
    assert_eq!(invocations_of(&rows, "lfe-clause-after-catch-all"), 90);
}

// -- the benchmark ------------------------------------------------------------

/// Not a test. Prints the cost table at two file sizes and the doubling ratio.
///
/// Read the ratios, not the absolutes: this repository runs many agents in
/// parallel and the load average moves the absolute numbers by more than any
/// change here would.
#[test]
#[ignore = "benchmark: wall-clock ratios are unstable under parallel load"]
fn ignored_bench_doubling_ratio() {
    for (label, make) in [
        ("clean", clean_source as fn(usize) -> String),
        ("dirty", dirty_source as fn(usize) -> String),
    ] {
        let small = make(250);
        let large = make(500);
        let small_rows = measure(&small);
        let large_rows = measure(&large);
        report(label, &small_rows, 250);
        report(label, &large_rows, 500);
        println!("-- {label} doubling ratios (100 = linear would be 200) --");
        for (name, _, _) in &small_rows {
            println!(
                "   {name:34} 2x ratio = {:>6}",
                ratio(nanos_of(&small_rows, name), nanos_of(&large_rows, name))
            );
        }
    }
}

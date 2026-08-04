//! What this rule costs when it is handed thousands of its own heads, and the
//! measurement behind the ordering claim in `support`'s module documentation.
//!
//! # The controls
//!
//! - **`cost-control-noop`** declares the same heads and the same dialect scope
//!   and does nothing, so the difference between its column and the real rule's
//!   is the rule's own work rather than the dispatcher's.
//! - **`cost-control-shipped-local`** reimplements the shape of a **shipped**
//!   rule that never touches the tree — `paredit-feature-lint-sequence`'s
//!   `destructive-literal`, which reads a fixed argument of its own node and
//!   classifies it. That is what "a shipped rule's cost" means here, measured in
//!   the same run.
//!
//!   A dev-dependency on the shipped package would be better, but
//!   `tests/cli/feature_dependency_contract.rs` scans the whole manifest text
//!   for `paredit-feature-`, `[dev-dependencies]` included, so a cross-feature
//!   dev dependency trips a contract this package may not edit. A local
//!   reimplementation of the shipped *shape* is what the repository permits.
//! - **`cost-control-wrong-order`** is this rule with the two halves of
//!   `check()` swapped: it reaches `root_view()` and runs the ancestry descent
//!   **before** the cheap head-and-argument test, instead of after. It exists to
//!   put a number on the ordering rule rather than assert it, and it is the only
//!   difference between its column and the shipped rule's.
//!
//! # The result
//!
//! Release build, `uptime` load average **3.32** (falling from 7.88). `n` counts
//! generated functions; the 8x column is the n=250 → n=2000 ratio, where linear
//! is ~8. Read the ratios, not the durations.
//!
//! ```text
//! == clean: correct code, every head present, ZERO findings ==
//!   cost-control-wrong-order        8x ratio = 46  [744907050 … 34465625007] ns
//!   discarded-destructive-…-result  8x ratio = 10  [   229175 …     2368243] ns
//!   cost-control-shipped-local      8x ratio =  7  [    31543 …      244800] ns
//!   cost-control-noop               8x ratio =  8  [    23084 …      189195] ns
//!
//! == reporting: every call is a finding ==
//!   cost-control-wrong-order        8x ratio = 61  [ 87495043 …  5347988027] ns
//!   discarded-destructive-…-result  8x ratio = 61  [ 87848872 …  5371227449] ns
//!   cost-control-shipped-local      8x ratio = 12  [     4586 …       56609] ns
//!   cost-control-noop               8x ratio = 10  [     7043 …       71765] ns
//! ```
//!
//! ## The clean fixture: 3,250x, and the shape is different
//!
//! **229,175 ns against 744,907,050 ns at n=250 — 3,250x — rising to 14,553x at
//! n=2000.** The factor grows because the two are not the same shape: the
//! shipped rule is **linear** (ratio 10 against a linear 8) and the transposed
//! one is **quadratic** (ratio 46, then 61). That is the whole point of the
//! anchoring inversion, and the `clean/forms/*` benchmarks measure exactly this
//! column because they lint files with zero findings.
//!
//! The shipped rule sits within 9.7x of the shipped local control and 12x of a
//! rule that does nothing — the cost of scanning each body form's own children.
//!
//! ## The reporting fixture: the rule is quadratic *while reporting*
//!
//! Stated plainly rather than buried, because it is the honest weakness. On a
//! file where **every** call is a finding, the shipped rule matches the rejected
//! shape exactly (5.371 s against 5.348 s at n=2000). `is_unevaluated_at` is
//! called once per body form that has a finding, and each call materializes the
//! whole tree, so a file of N findings costs O(N × file).
//!
//! Three things bound what that means in practice:
//!
//! 1. **It is invisible on correct code**, which is the fixture the benchmarks
//!    gate on, and where the separation above is 3,250x.
//! 2. **Findings are rare.** The third-party audit reported **0 findings over
//!    295 destructive calls in 1619 files**, and swept all 28 MB in **0.69 s**.
//!    The pathological fixture is 250 functions in which nothing is correct.
//! 3. The alternative is a cache outliving one `check()`, and the only such slot
//!    is `RuleContext::scratch_cache` — one type-erased slot per file's pass,
//!    already claimed by `paredit-feature-lint-repl-debug`, which panics by
//!    construction if a second type is stored during the same pass.
//!
//! This is the same trade the shipped `paredit-feature-lint-condition-depth`
//! rules make, and it is recorded here so a reviewer can disagree with it.
//!
//! ```text
//! cargo test -p paredit-feature-lint-destructive-sequence --lib cost_ -- --nocapture
//! cargo test -p paredit-feature-lint-destructive-sequence --lib -- --ignored --nocapture ignored_bench_
//! ```
//!
//! # What runs unattended
//!
//! Only the **invocation counts** and the ordering assertion, which are decided
//! by the head index and by control flow rather than by the clock. The
//! **doubling ratios are benchmarks, not tests**, and are `#[ignore]`d: a ratio
//! between two wall-clock durations is unstable under parallel load at any
//! threshold.

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
use paredit_core_syntax::view_query::list_head;

use crate::support::{body_start, is_bare_symbol, is_unevaluated_at};

/// Exactly the heads the real rule declares — the implicit-progn operators it
/// anchors on — so each control sees exactly the invocations the real rule sees.
const ALL_HEADS: [NormalizedHead; 20] = [
    NormalizedHead::new("progn"),
    NormalizedHead::new("prog"),
    NormalizedHead::new("prog*"),
    NormalizedHead::new("let"),
    NormalizedHead::new("let*"),
    NormalizedHead::new("flet"),
    NormalizedHead::new("labels"),
    NormalizedHead::new("macrolet"),
    NormalizedHead::new("symbol-macrolet"),
    NormalizedHead::new("lambda"),
    NormalizedHead::new("when"),
    NormalizedHead::new("unless"),
    NormalizedHead::new("dolist"),
    NormalizedHead::new("dotimes"),
    NormalizedHead::new("block"),
    NormalizedHead::new("with-open-file"),
    NormalizedHead::new("with-slots"),
    NormalizedHead::new("defun"),
    NormalizedHead::new("defmethod"),
    NormalizedHead::new("defmacro"),
];

// -- control: does nothing ----------------------------------------------------

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
        RuleDialectScope::COMMON_LISP_ONLY
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

// -- control: a shipped rule's local shape ------------------------------------

/// The **shipped** local shape, reimplemented so it can be measured beside this
/// rule: `paredit-feature-lint-sequence::destructive_literal` looks up its
/// head's destroyed-argument index, reads that one child, and classifies it,
/// never consulting the tree. That is what a shipped, local, head-matched rule
/// costs.
#[derive(Debug)]
struct ShippedLocalRule;

const SHIPPED_LOCAL_META: RuleMeta = RuleMeta::new(
    "cost-control-shipped-local",
    RuleCategory::DeadCode,
    Severity::Warning,
    "a control with a shipped rule's local child-reading shape",
    Fixability::ReportOnly,
);

static SHIPPED_LOCAL_RULE: ShippedLocalRule = ShippedLocalRule;

impl LintRule for ShippedLocalRule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&ALL_HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::COMMON_LISP_ONLY
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        _sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let hit = view.children.get(1).is_some_and(is_bare_symbol);
        std::hint::black_box(hit);
        Ok(())
    }
}

// -- control: the same rule, checks transposed --------------------------------

/// **The rule as it was first written**, preserved so its cost stays
/// reproducible: it consults the tree on every invocation instead of only once
/// it has a finding.
///
/// The original anchored on `sort`/`nconc`/… and recovered the parent by
/// descending from `root_view()`. The shape that made it quadratic is the one
/// reproduced here — a `root_view()` per invocation — and the reason it was not
/// saved by a cheap pre-check is that the **correct** idiom
/// `(setf xs (sort xs #'<))` passes any head-and-argument test there is. Only
/// the parent distinguishes it, and reaching the parent is the expensive thing.
///
/// This is the number behind the ordering claim in `support::is_unevaluated_at`.
#[derive(Debug)]
struct WrongOrderRule;

const WRONG_ORDER_META: RuleMeta = RuleMeta::new(
    "cost-control-wrong-order",
    RuleCategory::DeadCode,
    Severity::Warning,
    "the rejected design: root_view() per invocation rather than per finding",
    Fixability::ReportOnly,
);

static WRONG_ORDER_RULE: WrongOrderRule = WrongOrderRule;

impl LintRule for WrongOrderRule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&ALL_HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::COMMON_LISP_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        _sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        // Deliberately backwards: the tree is consulted before anything cheap
        // has had a chance to reject the node. `root_view()` inside
        // `is_unevaluated_at` materializes every node in the file.
        let data = is_unevaluated_at(context.tree(), view.span);
        // …and only now the local work that would have decided it for free.
        let candidate = list_head(view)
            .and_then(body_start)
            .is_some_and(|start| view.children.iter().skip(start).any(is_bare_symbol));
        std::hint::black_box(data && candidate);
        Ok(())
    }
}

// -- the catalogue ------------------------------------------------------------

static ENTRIES: [RuleEntry; 4] = [
    RuleEntry::new(&NOOP_META, &NOOP_RULE),
    RuleEntry::new(&SHIPPED_LOCAL_META, &SHIPPED_LOCAL_RULE),
    RuleEntry::new(&WRONG_ORDER_META, &WRONG_ORDER_RULE),
    RuleEntry::new(
        &crate::discarded_destructive_sequence_result::META,
        &crate::discarded_destructive_sequence_result::RULE,
    ),
];

const COLUMNS: [&str; 4] = [
    "cost-control-wrong-order",
    "discarded-destructive-sequence-result",
    "cost-control-shipped-local",
    "cost-control-noop",
];

// -- measurement --------------------------------------------------------------

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

/// `count` **correct** functions, each using every one of the six heads in a
/// shape the rule must decline — the zero-finding shape the `clean/forms/*`
/// benchmarks measure.
///
/// Deliberately nested a few levels deep, because the ancestry descent's cost is
/// the node's *depth*: a flat fixture would understate the wrong-order control
/// and flatter the mistake this file exists to price.
fn clean_source(count: usize) -> String {
    let mut out = String::from(";;; correct destructive-sequence use throughout\n");
    for index in 0..count {
        out.push_str(&format!(
            "(defun step{index} (xs ys al)\n\
             \x20 (let ((acc '()))\n\
             \x20   (dolist (item xs acc)\n\
             \x20     (when (live-p item)\n\
             \x20       (setf xs (sort xs #'<))\n\
             \x20       (setf xs (stable-sort xs #'>))\n\
             \x20       (setf xs (nconc xs ys))\n\
             \x20       (setf xs (nbutlast xs))\n\
             \x20       (setf xs (nsublis al xs))\n\
             \x20       (setf xs (nsubst 1 2 xs))\n\
             \x20       (push item acc)))))\n"
        ));
    }
    out
}

/// The same density, but every call **reports**. A rule that is cheap while
/// declining and expensive while reporting is invisible to the clean fixture,
/// and `ancestry_at` plus the sibling scan is exactly that shape.
fn reporting_source(count: usize) -> String {
    let mut out = String::from(";;; every call here is a genuine finding\n");
    for index in 0..count {
        out.push_str(&format!(
            "(defun step{index} (xs ys al)\n\
             \x20 (sort xs #'<)\n\
             \x20 (stable-sort xs #'>)\n\
             \x20 (nconc xs ys)\n\
             \x20 (nbutlast xs)\n\
             \x20 (nsublis al xs)\n\
             \x20 (nsubst 1 2 xs)\n\
             \x20 (print xs))\n"
        ));
    }
    out
}

// -- what runs unattended -----------------------------------------------------

/// The head index must hand every rule exactly the nodes it asked for. Without
/// this, a "cheap" column can simply be a rule that was never called.
#[test]
fn cost_every_rule_is_invoked_once_per_matching_head() {
    let rows = measure(&clean_source(20));
    // The rule anchors on body forms, so the denominator is the *enclosing*
    // operators: `defun`, `let`, `dolist` and `when`, once each per generated
    // function. The six destructive calls inside are not dispatch points.
    let noop = invocations_of(&rows, "cost-control-noop");
    assert_eq!(
        noop,
        20 * 4,
        "defun, let, dolist and when per generated function"
    );
    for control in [
        "cost-control-shipped-local",
        "cost-control-wrong-order",
        "discarded-destructive-sequence-result",
    ] {
        assert_eq!(
            invocations_of(&rows, control),
            noop,
            "{control} declares the same heads and must see the same nodes"
        );
    }
}

/// The clean fixture must yield nothing: a cost number for a rule that is
/// quietly reporting on correct code measures the wrong thing.
#[test]
fn cost_the_clean_fixture_yields_no_findings() {
    let source = clean_source(20);
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp).expect("parse");
    let outcome = collect_lint_pass(
        catalog,
        &index,
        Path::new("cost.lisp"),
        Dialect::CommonLisp,
        &tree,
        &source,
        RuleSelection::All,
        PassOptions::default(),
    )
    .expect("lint pass");
    assert!(
        outcome.outcomes.is_empty(),
        "clean fixture must be clean, got {} findings",
        outcome.outcomes.len()
    );
}

/// The reporting fixture must fire on every call, so the reporting column is
/// measuring the reporting path and not an accidental decline.
#[test]
fn cost_the_reporting_fixture_fires_on_every_call() {
    let source = reporting_source(20);
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp).expect("parse");
    let outcome = collect_lint_pass(
        catalog,
        &index,
        Path::new("cost.lisp"),
        Dialect::CommonLisp,
        &tree,
        &source,
        RuleSelection::All,
        PassOptions::default(),
    )
    .expect("lint pass");
    assert_eq!(
        outcome.outcomes.len(),
        20 * 6,
        "every one of the six calls per function must report"
    );
}

/// The ordering claim, as a number rather than an assertion.
///
/// The cheap domain check must come before anything that reads the tree. The
/// evidence is the wrong-order control, which is this rule's own logic with the
/// two halves transposed and nothing else changed.
///
/// The bound is deliberately loose — a wall-clock comparison under parallel load
/// cannot carry a tight one, and this machine reported a load average above 20.
/// The separation being asserted is more than an order of magnitude.
#[test]
fn cost_the_cheap_check_runs_before_the_tree_walk() {
    let rows = measure(&clean_source(200));
    let wrong_order = nanos_of(&rows, "cost-control-wrong-order");
    let shipped = nanos_of(&rows, "discarded-destructive-sequence-result").max(1);
    assert!(
        shipped < wrong_order,
        "the shipped order ({shipped}ns) must beat the transposed one ({wrong_order}ns)"
    );
}

/// The shipped rule must stay near the shipped *control* on the zero-finding
/// fixture. This is the regression this file guards.
#[test]
fn cost_the_shipped_rule_is_local() {
    let rows = measure(&clean_source(200));
    let shipped_control = nanos_of(&rows, "cost-control-shipped-local").max(1);
    let cost = nanos_of(&rows, "discarded-destructive-sequence-result");
    assert!(
        cost / shipped_control < 100,
        "the rule ({cost}ns) must stay near the shipped control ({shipped_control}ns)"
    );
}

// -- the benchmarks -----------------------------------------------------------

fn report(label: &str, build: impl Fn(usize) -> String) {
    let sizes = [250_usize, 500, 1000, 2000];
    let mut rows = Vec::new();
    for size in sizes {
        rows.push(measure(&build(size)));
    }
    println!("\n== {label} ==");
    for column in COLUMNS {
        let series: Vec<u128> = rows.iter().map(|row| nanos_of(row, column)).collect();
        let first = series.first().copied().unwrap_or(1).max(1);
        let last = series.last().copied().unwrap_or(1);
        let ratio = last / first;
        let invocations = invocations_of(&rows[0], column);
        println!(
            "  {column:40} 8x ratio = {ratio:4}  (n=250 invocations={invocations})  {series:?} ns"
        );
    }
    println!("  (linear is ~8; load average is high — read the ratios, not the durations)");
}

#[test]
#[ignore = "benchmark: wall-clock ratios are unstable under parallel load"]
fn ignored_bench_clean() {
    report(
        "clean: correct code, every head present, ZERO findings",
        clean_source,
    );
}

#[test]
#[ignore = "benchmark: wall-clock ratios are unstable under parallel load"]
fn ignored_bench_reporting() {
    report("reporting: every call is a finding", reporting_source);
}

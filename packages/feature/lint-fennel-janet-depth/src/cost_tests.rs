//! What each rule costs per `check` call, measured by the engine itself.
//!
//! Run the table with
//! `cargo test -p paredit-feature-lint-fennel-janet-depth cost -- --nocapture`.
//!
//! # Why the baseline is a rule defined here rather than a shipped one
//!
//! The brief asks for a comparison against a shipped rule in the same run.
//! That is not available to this package: `tests/cli/feature_dependency_contract.rs`
//! scans each manifest's **whole text** for `paredit-feature-`, so even a
//! `[dev-dependencies]` edge onto another feature package trips the allowlist,
//! and this task may not edit `tests/`. The baseline is therefore
//! [`materializing::RULE`], written here to have the shape the expensive
//! shipped rules have — a `Heads` rule that asks
//! [`crate::support::is_unevaluated_at`] **before** its domain check, which is
//! what makes it materialize the whole document once per matched node.
//!
//! That comparison isolates the order of the domain check and the expensive
//! guard. Every shipped rule in this package puts the domain check first; this
//! file verifies that the cheaper ordering remains in place.
//!
//! # Reading the numbers
//!
//! Absolute nanoseconds from this machine are worthless: the audit ran with a
//! load average above 10 and several sibling agents building in parallel. The
//! printed table is for the report.
//!
//! # What runs unattended
//!
//! Two things, both with enough headroom that machine load cannot decide them:
//!
//! - the **invocation counts**, which the head index and the dialect scope fix
//!   before any `check` body runs, so they are identical idle and loaded;
//! - the **gap to [`materializing::RULE`]**, checked against a conservative
//!   floor. This catches a guard moved above its domain check.
//!
//! The **per-call doubling ratio is a benchmark, not a test**, and is
//! `#[ignore]`d — see [`ignored_bench_per_call_cost_does_not_grow_with_the_file`].
//! It was asserted at `< 1.8` and failed a downstream `nix` build at 2.04 on
//! untouched code. Two reasons it cannot be a gate:
//!
//! - each call is short enough that the ratio of two measurements is dominated
//!   by scheduler noise;
//! - doubling the file doubles the tree, so the traversal's working set stops
//!   fitting in cache. That is constant work per call taking more time per
//!   call, consuming the budget without indicating a complexity change.
//!
//! The property it was reaching for is not lost: a `check` that materializes
//! the document is what
//! [`every_rule_stays_far_cheaper_than_a_materializing_one`] measures directly,
//! against a control built to have that exact shape.

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

/// The anti-pattern, as a rule: guard first, domain check second.
mod materializing {
    use super::*;

    pub static META: RuleMeta = RuleMeta::new(
        "reference-materializing",
        RuleCategory::Suspicious,
        Severity::Warning,
        "a deliberately mis-ordered reference rule; never registered",
        Fixability::ReportOnly,
    );

    const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("when")];
    const DIALECTS: [Dialect; 1] = [Dialect::Fennel];

    #[derive(Debug)]
    pub struct Rule;

    pub static RULE: Rule = Rule;

    impl LintRule for Rule {
        fn head_filter(&self) -> HeadFilter {
            HeadFilter::Heads(&HEADS)
        }

        fn dialect_scope(&self) -> RuleDialectScope {
            RuleDialectScope::new(&DIALECTS)
        }

        fn check(
            &self,
            context: &RuleContext<'_>,
            view: &ExpressionView,
            _sink: &mut RuleSink<'_, '_>,
        ) -> LintResult {
            // The whole point: this runs on *every* matched node, not only
            // once a finding exists. `root_view` is O(file).
            let unevaluated = crate::support::is_unevaluated_at(context.tree(), view.span);
            // Consume the answer so nothing is optimized away.
            if unevaluated && view.children.is_empty() {
                return Ok(());
            }
            Ok(())
        }
    }
}

static COST_ENTRIES: [RuleEntry; 6] = [
    RuleEntry::new(
        &crate::fennel_bad_unpack::rule::META,
        &crate::fennel_bad_unpack::rule::RULE,
    ),
    RuleEntry::new(
        &crate::fennel_nested_associative_operator::rule::META,
        &crate::fennel_nested_associative_operator::rule::RULE,
    ),
    RuleEntry::new(
        &crate::fennel_redundant_do::rule::META,
        &crate::fennel_redundant_do::rule::RULE,
    ),
    RuleEntry::new(
        &crate::janet_dead_branch_on_constant_condition::rule::META,
        &crate::janet_dead_branch_on_constant_condition::rule::RULE,
    ),
    RuleEntry::new(
        &crate::janet_unreachable_match_clause::rule::META,
        &crate::janet_unreachable_match_clause::rule::RULE,
    ),
    RuleEntry::new(&materializing::META, &materializing::RULE),
];

/// One repetition of a form dense in every head this package registers, so no
/// rule is measured over a file it never matches.
const DENSE_FENNEL: &str = r#"
(fn handler-N [items prefix opts]
  (when (and opts.enabled (or opts.name opts.path))
    (let [out []]
      (each [_ item (ipairs items)]
        (table.insert out (.. prefix (tostring item))))
      (table.concat out ", ")))
  (for [i 1 (length items)]
    (print (+ i (. items i))))
  (while (< 0 (length items))
    (table.remove items))
  (lambda [x] (bor (band x 255) (lshift 1 8))))
"#;

/// The same, for the Janet rules. Two fixtures are needed because
/// `dialect_scope` drops a rule before the walk, so a Fennel-only fixture
/// leaves the Janet rules at zero invocations and measures nothing about them.
const DENSE_JANET: &str = r#"
(defn handler-N [code opts]
  (when (opts :enabled)
    (print "on"))
  (unless (opts :quiet)
    (print "loud"))
  (if (opts :strict) :strict :lenient)
  (if-not (opts :name) :anonymous :named)
  (match code
    404 :not-found
    500 :server-error
    [:redirect target] target
    _ :unknown))
"#;

fn dense_fennel(repetitions: usize) -> String {
    (0..repetitions)
        .map(|index| DENSE_FENNEL.replace('N', &index.to_string()))
        .collect()
}

fn dense_janet(repetitions: usize) -> String {
    (0..repetitions)
        .map(|index| DENSE_JANET.replace('N', &index.to_string()))
        .collect()
}

struct Measured {
    name: &'static str,
    per_call: f64,
    invocations: u64,
}

/// One measured pass, as `(rule name, ns per invocation, invocations)`.
fn measure(source: &str, dialect: Dialect) -> Vec<Measured> {
    let catalog = RuleCatalog::new(&COST_ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
    let path = if dialect == Dialect::Janet {
        Path::new("cost.janet")
    } else {
        Path::new("cost.fnl")
    };
    let outcome = collect_lint_pass(
        catalog,
        &index,
        path,
        dialect,
        &tree,
        source,
        RuleSelection::All,
        PassOptions {
            settings: None,
            measure: true,
        },
    )
    .expect("lint pass");
    let timings = outcome.timings.expect("measure: true must produce timings");
    timings
        .entries()
        .map(|(position, elapsed, invocations)| Measured {
            name: COST_ENTRIES[position].meta().name().as_str(),
            per_call: if invocations == 0 {
                0.0
            } else {
                elapsed.as_nanos() as f64 / invocations as f64
            },
            invocations,
        })
        .collect()
}

fn total_of(measured: &[Measured]) -> f64 {
    measured
        .iter()
        .map(|item| item.per_call * item.invocations as f64)
        .sum()
}

/// Both dialects' passes, merged: each rule's row comes from whichever fixture
/// actually invoked it. A rule that neither fixture invokes keeps zero
/// invocations and is caught by the assertion below.
fn measure_once(repetitions: usize) -> Vec<Measured> {
    let fennel = measure(&dense_fennel(repetitions), Dialect::Fennel);
    let janet = measure(&dense_janet(repetitions), Dialect::Janet);
    fennel
        .into_iter()
        .zip(janet)
        .map(|(f, j)| if j.invocations > f.invocations { j } else { f })
        .collect()
}

/// How many passes to take the best of.
///
/// The real rules land at 27-160 ns per call, which is close enough to the
/// clock's own resolution that a single sample is mostly scheduler noise —
/// this ran at a load average of 45 with several sibling agents building. The
/// minimum is the right estimator here because interference can only ever add
/// time, never remove it, so the smallest observation is the one least
/// contaminated. Without this the doubling assertion below reported a ratio of
/// 5.74 for a rule the very next run measured at 2.12.
const SAMPLES: usize = 5;

fn measure_both(repetitions: usize) -> Vec<Measured> {
    let mut best = measure_once(repetitions);
    for _ in 1..SAMPLES {
        for (slot, sample) in best.iter_mut().zip(measure_once(repetitions)) {
            if sample.invocations > 0 && sample.per_call < slot.per_call {
                slot.per_call = sample.per_call;
            }
        }
    }
    best
}

/// The table, plus the two invariants that survive a noisy machine.
#[test]
fn every_rule_stays_far_cheaper_than_a_materializing_one() {
    let small = measure_both(200);
    let large = measure_both(400);

    println!(
        "\n== cost, {} + {} bytes then {} + {} bytes (Fennel + Janet fixtures) ==",
        dense_fennel(200).len(),
        dense_janet(200).len(),
        dense_fennel(400).len(),
        dense_janet(400).len()
    );
    println!(
        "{:<42} {:>12} {:>7} {:>12} {:>7} {:>9}",
        "rule", "ns/call@200", "calls", "ns/call@400", "calls", "total x2"
    );
    for (a, b) in small.iter().zip(large.iter()) {
        let ratio = if a.per_call * a.invocations as f64 == 0.0 {
            0.0
        } else {
            (b.per_call * b.invocations as f64) / (a.per_call * a.invocations as f64)
        };
        println!(
            "{:<42} {:>12.0} {:>7} {:>12.0} {:>7} {:>9.2}",
            a.name, a.per_call, a.invocations, b.per_call, b.invocations, ratio
        );
    }
    println!(
        "total elapsed: {:?} then {:?}, ratio {:.2}",
        Duration::from_nanos(total_of(&small) as u64),
        Duration::from_nanos(total_of(&large) as u64),
        total_of(&large) / total_of(&small)
    );

    // Every rule really ran, or the comparison below means nothing.
    for item in &large {
        assert!(
            item.invocations > 0,
            "{} was never invoked; the dense fixture is missing its head",
            item.name
        );
    }

    let reference = large
        .iter()
        .find(|item| item.name == "reference-materializing")
        .expect("the reference rule must be in the catalogue");
    let worst = large
        .iter()
        .filter(|item| item.name != "reference-materializing")
        .max_by(|a, b| a.per_call.total_cmp(&b.per_call))
        .expect("at least one real rule");

    // The deliberately loose floor tolerates machine load while still catching
    // a rule whose guard moves above its domain check.
    assert!(
        reference.per_call > worst.per_call * 10.0,
        "the guard-first reference rule ({:.0} ns/call) is not far more \
         expensive than the worst real rule {} ({:.0} ns/call) — has a rule \
         started calling is_unevaluated_* before its domain check?",
        reference.per_call,
        worst.name,
        worst.per_call
    );
}

/// Every rule is dispatched once per occurrence of one of its heads, and no
/// more — so doubling the file doubles the dispatches rather than squaring
/// them.
///
/// **Counts, not wall clock.** The head index and the dialect scope decide
/// these before any `check` body runs, so a loaded machine reports exactly
/// what an idle one does. A rule invoked more often than its heads occur is
/// one re-walking the document per match, which is the dispatch-side shape of
/// the same defect [`every_rule_stays_far_cheaper_than_a_materializing_one`]
/// catches on the cost side.
#[test]
fn each_rule_is_dispatched_once_per_head_occurrence() {
    // Heads per repetition of the two fixtures, in `COST_ENTRIES` order:
    //
    //   fennel-bad-unpack                        9  DENSE_FENNEL
    //   fennel-nested-associative-operator       5  DENSE_FENNEL
    //   fennel-redundant-do                      7  `fn` `when` `let` `each`
    //                                               `for` `while` `lambda`
    //   janet-dead-branch-on-constant-condition  4  `when` `unless` `if` `if-not`
    //   janet-unreachable-match-clause           1  `match`
    //   reference-materializing                  1  `when`, Fennel only
    const PER_REPETITION: [u64; 6] = [9, 5, 7, 4, 1, 1];

    let counts = |repetitions: usize| -> Vec<u64> {
        measure_once(repetitions)
            .into_iter()
            .map(|item| item.invocations)
            .collect()
    };

    for repetitions in [200, 400] {
        let observed = counts(repetitions);
        let expected: Vec<u64> = PER_REPETITION
            .iter()
            .map(|per| per * repetitions as u64)
            .collect();
        assert_eq!(
            observed, expected,
            "invocations must equal head occurrences at {repetitions} \
             repetitions; a rule dispatched more often than its heads occur is \
             being handed nodes its head filter should have excluded"
        );
    }
}

/// The per-call doubling ratio, as a benchmark rather than a gate.
///
///     cargo test -p paredit-feature-lint-fennel-janet-depth --release \
///       --lib -- --ignored --nocapture ignored_bench_per_call
///
/// A `check` that is O(depth) costs the same per call whatever the file size;
/// one that materializes the document is O(file) per call, so doubling the
/// file doubles it. That is the difference between `reference-materializing`
/// and every rule here, and it is worth reading whenever a rule changes.
///
/// It is not worth failing a build on. See the module docs: at 16-75 ns per
/// call in release the ratio is dominated by scheduler noise and by cache
/// working-set growth, and asserting it at `< 1.8` broke a downstream `nix`
/// build at 2.04 on code nobody had touched.
#[test]
#[ignore = "a benchmark: wall-clock ratios are unstable under parallel load"]
fn ignored_bench_per_call_cost_does_not_grow_with_the_file() {
    let small = measure_both(200);
    let large = measure_both(400);

    let find = |set: &[Measured], name: &str| -> f64 {
        set.iter()
            .find(|item| item.name == name)
            .map(|item| item.per_call)
            .expect("rule present")
    };

    for entry in &COST_ENTRIES {
        let name = entry.meta().name().as_str();
        let at_small = find(&small, name);
        let at_large = find(&large, name);
        eprintln!(
            "{name}: {at_small:.0} ns/call @200 → {at_large:.0} ns/call @400, \
             ratio {:.2}",
            at_large / at_small.max(1e-9)
        );
    }
}

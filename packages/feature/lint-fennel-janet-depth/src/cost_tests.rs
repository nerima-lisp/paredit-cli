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
//! That is the comparison that actually matters. The earlier Fennel/Janet
//! batch measured 450,843 ns/call against 28 ns/call — a 16,000x difference —
//! and the *only* difference between the two was the order of those two lines.
//! Every rule in this package puts the guard second; this file is the proof
//! that it stayed that way.
//!
//! # Reading the numbers
//!
//! Absolute nanoseconds from this machine are worthless: the audit ran with a
//! load average above 10 and several sibling agents building in parallel. The
//! assertions are all *ratios*, and the printed table is for the report.

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

    // The ordering invariant. The earlier batch measured this gap at 16,000x;
    // 10x is a floor loose enough to survive a loaded machine and tight enough
    // that moving any rule's guard above its domain check trips it.
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

/// Doubling the file must not change what one `check` call costs.
///
/// This is deliberately expressed as **ns per call across two file sizes**
/// rather than as a ratio of totals. Both say the same thing when the totals
/// are large, but the totals here are not: at 27-160 ns per call the real
/// rules are near the clock's resolution, so their totals swing with the
/// scheduler while their per-call cost does not.
///
/// The property is exact. A rule whose `check` is O(depth) costs the same per
/// call whatever the file size, so its ratio sits at 1. A rule that
/// materializes the document inside `check` is O(file) per call, so doubling
/// the file doubles its per-call cost — which is precisely what separates
/// `reference-materializing` (measured at 2.2) from every rule in this
/// package (measured at 0.5 to 1.3).
#[test]
fn a_rules_per_call_cost_does_not_grow_with_the_file() {
    let small = measure_both(200);
    let large = measure_both(400);

    let ratio_of = |name: &str| -> f64 {
        let find = |set: &[Measured]| -> f64 {
            set.iter()
                .find(|item| item.name == name)
                .map(|item| item.per_call)
                .expect("rule present")
        };
        find(&large) / find(&small).max(1.0)
    };

    for entry in &COST_ENTRIES {
        let name = entry.meta().name().as_str();
        if name == "reference-materializing" {
            continue;
        }
        let ratio = ratio_of(name);
        assert!(
            ratio < 1.8,
            "{name} cost {ratio:.2}x as much per call on a file twice the \
             size; a `check` that is O(file) rather than O(depth) is the usual \
             cause, and calling is_unevaluated_* before the domain check is \
             how that happens"
        );
    }

    // The control. Without it the test above passes just as happily when the
    // measurement is broken and every ratio is 1.
    //
    // Stated as an absolute magnitude rather than as the reference rule's own
    // doubling ratio. The ratio is theoretically 2 but was observed between
    // 1.48 and 2.96 across runs on this machine: taking the best of five
    // samples pulls the *small* file's number down hardest, which compresses
    // it. Six orders of magnitude between the two shapes is not compressible
    // by any amount of scheduler noise.
    let reference = large
        .iter()
        .find(|item| item.name == "reference-materializing")
        .expect("rule present");
    assert!(
        reference.per_call > 100_000.0,
        "the reference rule cost only {:.0} ns per call; it materializes the \
         whole document on every one of them, so either the fixture stopped \
         matching `when` or the measurement is not live",
        reference.per_call
    );
}

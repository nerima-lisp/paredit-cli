//! What each rule here costs when it is handed thousands of its own heads.
//!
//! Nothing here can be caught by a correctness test: a rule that is far too
//! slow still produces exactly the right findings.
//!
//! # How to read the numbers
//!
//! ```text
//! cargo test -p paredit-feature-lint-hy-depth --release --lib cost_ -- --ignored --nocapture
//! ```
//!
//! prints, per rule and per file size, the nanoseconds per `check` invocation
//! and the invocation count. Two controls make those numbers mean something:
//!
//! - a **no-op rule** declaring the *same* heads and the same dialect scope, so
//!   the difference between the two columns is this package's own work rather
//!   than the dispatcher's;
//! - a **doubling ratio** across an 8× range of file sizes. Linear work per
//!   file gives a per-invocation ratio near 1; the quadratic shape that has got
//!   rules dropped from sibling packages gives a ratio near 8.
//!
//! The comparison against a *shipped* rule in the same pass was taken with a
//! temporary dev-dependency on the sibling Hy package and is recorded in this
//! package's README. It is not kept here, because a feature-to-feature edge
//! needs an entry in the dependency allowlist contract and a scratch benchmark
//! does not earn one.
//!
//! # What runs unattended
//!
//! Only the **invocation counts**, which are decided by the head index and the
//! dialect scope before any `check` body runs, and so are identical on an idle
//! machine and a loaded one.
//!
//! The **doubling ratio is a benchmark, not a test**, and is `#[ignore]`d. A
//! ratio between two wall-clock durations is unstable under parallel load at
//! any threshold, and this workspace runs many agents at once; a sibling
//! package's asserting version failed CI on a busy runner. It prints instead.

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

/// A rule that matches the same heads as the package and does nothing, so the
/// difference between its column and a real rule's is that rule's own work.
#[derive(Debug)]
struct NoopRule;

const NOOP_HEADS: [NormalizedHead; 1] = [NormalizedHead::new("try")];

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
        RuleDialectScope::new(&[Dialect::Hy])
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

static ENTRIES: [RuleEntry; 2] = [
    RuleEntry::new(&NOOP_META, &NOOP_RULE),
    RuleEntry::new(
        &crate::unreachable_except_clause::rule::META,
        &crate::unreachable_except_clause::rule::RULE,
    ),
];

/// One measured pass: per-rule duration and invocation count.
fn measure(source: &str) -> Vec<(&'static str, Duration, u64)> {
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::Hy).expect("parse");
    let outcome = collect_lint_pass(
        catalog,
        &index,
        Path::new("bench.hy"),
        Dialect::Hy,
        &tree,
        source,
        RuleSelection::All,
        // The whole point: without `measure` the timings are not collected.
        PassOptions {
            settings: None,
            measure: true,
        },
    )
    .expect("lint pass");
    outcome
        .timings
        .expect("PassOptions::measure was set, so timings must be present")
        .entries()
        .map(|(position, elapsed, invocations)| {
            (
                ENTRIES[position].meta().name().as_str(),
                elapsed,
                invocations,
            )
        })
        .collect()
}

/// A file of `count` realistic *clean* Hy definitions.
///
/// Clean on purpose: the overwhelmingly common case is a file with no findings,
/// and a rule's cost there is what a lint run actually pays. Every anchored
/// head appears, and each `try` carries a realistic three-clause handler chain
/// in the correct narrow-to-broad order, so the rule does its whole comparison
/// and then reports nothing.
fn corpus(count: usize) -> String {
    let mut source = String::from("(import os json)\n");
    for index in 0..count {
        source.push_str(&format!(
            "(defn load-{index} [path]\n\
             \x20 (try\n\
             \x20   (with [handle (open path)]\n\
             \x20     (json.loads (.read handle)))\n\
             \x20   (except [e FileNotFoundError]\n\
             \x20     (print \"missing\" path))\n\
             \x20   (except [e [KeyError IndexError]]\n\
             \x20     (print \"malformed\" e))\n\
             \x20   (except [e OSError]\n\
             \x20     (print \"io\" e))\n\
             \x20   (else\n\
             \x20     (print \"ok\"))\n\
             \x20   (finally\n\
             \x20     (print \"done\"))))\n"
        ));
    }
    source
}

/// Invocation counts are decided by the head index before any `check` body
/// runs, so they are stable under load and can be asserted.
///
/// What this pins is that the rule really is reached once per `try` and **not**
/// once per node — a `HeadFilter` accidentally widened to `AllNodes` would show
/// up here as a count in the thousands.
#[test]
fn cost_invocation_counts_track_the_anchored_heads_not_the_node_count() {
    let source = corpus(20);
    let timings = measure(&source);
    let count_of = |rule: &str| {
        timings
            .iter()
            .find(|(name, _, _)| *name == rule)
            .map(|(_, _, invocations)| *invocations)
            .unwrap_or_else(|| panic!("{rule} was never invoked"))
    };

    // Exactly one `try` per generated definition, for the rule and for the
    // control alike.
    assert_eq!(count_of("cost-control-noop"), 20);
    assert_eq!(count_of("hy-unreachable-except-clause"), 20);
}

/// The clean corpus really is clean, so the numbers above are the cost of the
/// common case rather than the cost of reporting.
#[test]
fn cost_the_benchmark_corpus_is_clean() {
    let source = corpus(20);
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(&source, Dialect::Hy).expect("parse");
    let outcome = collect_lint_pass(
        catalog,
        &index,
        Path::new("bench.hy"),
        Dialect::Hy,
        &tree,
        &source,
        RuleSelection::All,
        PassOptions::default(),
    )
    .expect("lint pass");
    let fired: Vec<&str> = outcome
        .outcomes
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect();
    assert_eq!(
        fired,
        Vec::<&str>::new(),
        "the benchmark corpus must be clean"
    );
}

/// The doubling ratio, printed rather than asserted. See the module docs.
#[test]
#[ignore = "a wall-clock ratio is a benchmark, not a test; run it deliberately"]
fn cost_growth_is_linear() {
    println!("\n=== per-invocation cost, Hy depth rules ===");
    println!(
        "{:<44} {:>10} {:>10} {:>10} {:>8}",
        "rule", "ns/call@x1", "ns/call@x8", "ratio", "calls@x8"
    );

    let small = measure(&corpus(50));
    let large = measure(&corpus(400));

    for (rule, small_elapsed, small_calls) in &small {
        let Some((_, large_elapsed, large_calls)) = large.iter().find(|(name, _, _)| name == rule)
        else {
            continue;
        };
        let small_ns = small_elapsed.as_nanos() as f64 / (*small_calls).max(1) as f64;
        let large_ns = large_elapsed.as_nanos() as f64 / (*large_calls).max(1) as f64;
        let ratio = if small_ns > 0.0 {
            large_ns / small_ns
        } else {
            f64::NAN
        };
        println!("{rule:<44} {small_ns:>10.0} {large_ns:>10.0} {ratio:>10.2} {large_calls:>8}");
    }
    println!(
        "\nPer-*invocation* cost is what matters: a rule that is linear per call \
         is quadratic per file, and shows a ratio near 8 in this column rather than near 1."
    );
}

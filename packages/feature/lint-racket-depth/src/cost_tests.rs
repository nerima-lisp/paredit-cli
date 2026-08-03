//! What each rule here costs when it is handed thousands of its own heads.
//!
//! Nothing here can be caught by a correctness test: a rule that is far too
//! slow still produces exactly the right findings. So the measurement is the
//! test — and it has already removed a rule from this package.
//!
//! # `racket-contract-out-arity-mismatch`, built and dropped
//!
//! A seventh rule was written, tested, and audited before being deleted on
//! these numbers. It compared a `(contract-out [f (-> integer? integer?)])`
//! entry against `f`'s own `define`, which is real: `raco make` compiles the
//! disagreement silently and running the module then fails at instantiation
//! with "f: broke its own contract" (verified on Racket v9.2).
//!
//! It was correct, and it was dropped anyway, because it correlates a node with
//! something outside it — the exact shape this project has dropped a rule over
//! **twice**, both times on measurement rather than soundness. Registered for
//! `provide`, with a source-slice gate so that a `provide` without the
//! substring `contract-out` returned immediately:
//!
//! ```text
//! rule                                 ns/call@x1  ns/call@x8   ratio
//! cost-control-noop                            24          24    0.96
//! racket-contract-out-arity-mismatch        11374       33945    2.98   (provide per define)
//! racket-contract-out-arity-mismatch        80125     1251375   15.62   (one provide per file)
//! ```
//!
//! Super-linear per *invocation* in the first layout, which makes it quadratic
//! per file; and 1.25 ms for a single realistic file in the second, about
//! 40000x the no-op control. The gate did not save it, because past the gate
//! the rule must still scan the top level, and a `Heads`-dispatched rule gets
//! no per-file index to amortize that against — `RuleContext::scratch_cache` is
//! a single type-erased slot already owned by `lint-repl-debug`, and a second
//! type in it panics.
//!
//! It found **zero** findings across the audited corpus's 214 `contract-out`
//! occurrences in 70 files, so the measured cost bought nothing.
//!
//! The rule is viable as a `HeadFilter::WholeTree` rule, which is invoked once
//! per file and could collect provides and definitions in a single O(file)
//! pass — the variant's own documentation names "correlating separate top-level
//! definitions" as its purpose. That is the shape to rebuild it in.
//!
//! # How to read the numbers
//!
//! ```text
//! cargo test -p paredit-feature-lint-racket-depth --release --lib cost_ -- --nocapture
//! ```
//!
//! prints, per rule and per file size, the nanoseconds per `check` invocation
//! and the invocation count. Two controls make those numbers mean something:
//!
//! - a **no-op rule** declaring the *same* heads and the same dialect scope, so
//!   the difference between the two columns is this package's own work rather
//!   than the dispatcher's;
//! - a **doubling ratio** across an 8× range of file sizes. Linear work gives
//!   ≈8×; the quadratic shape that got the two earlier rules dropped gives ≈64×.
//!
//! # What runs unattended
//!
//! Only the **invocation counts**, which are decided by the head index and the
//! dialect scope before any `check` body runs and so are identical on an idle
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

const NOOP_HEADS: [NormalizedHead; 5] = [
    NormalizedHead::new("match"),
    NormalizedHead::new("begin0"),
    NormalizedHead::new("case-lambda"),
    NormalizedHead::new("parameterize"),
    NormalizedHead::new("define"),
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
        RuleDialectScope::new(&[Dialect::Racket])
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
        &crate::begin0_single_form::rule::META,
        &crate::begin0_single_form::rule::RULE,
    ),
    RuleEntry::new(
        &crate::case_lambda_single_clause::rule::META,
        &crate::case_lambda_single_clause::rule::RULE,
    ),
    RuleEntry::new(
        &crate::for_comprehension_value_discarded::rule::META,
        &crate::for_comprehension_value_discarded::rule::RULE,
    ),
    RuleEntry::new(
        &crate::match_unreachable_clause::rule::META,
        &crate::match_unreachable_clause::rule::RULE,
    ),
    RuleEntry::new(
        &crate::parameterize_empty_bindings::rule::META,
        &crate::parameterize_empty_bindings::rule::RULE,
    ),
];

/// One measured pass: per-rule duration and invocation count.
fn measure(source: &str) -> Vec<(&'static str, Duration, u64)> {
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::Racket).expect("parse");
    let outcome = collect_lint_pass(
        catalog,
        &index,
        Path::new("bench.rkt"),
        Dialect::Racket,
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
    // `entries()` yields registration positions; this module owns `ENTRIES`, so
    // it is the one place that can pair a position with a rule name.
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

/// A file of `count` realistic *clean* Racket definitions.
///
/// Clean on purpose: the overwhelmingly common case is a file with no findings,
/// and a rule's cost there is what a lint run actually pays. Every anchored head
/// appears, so every rule is invoked.
fn corpus(count: usize) -> String {
    let mut source = String::from("#lang racket/base\n(require racket/match racket/contract)\n");
    for index in 0..count {
        source.push_str(&format!(
            "(provide (contract-out [fn{index} (-> integer? integer?)]))\n\
             (define (fn{index} x)\n\
             \x20 (match x\n\
             \x20   [(? number?) (begin0 (+ x 1) (log-it x))]\n\
             \x20   [(? string?) (string-length x)]\n\
             \x20   [_ 0]))\n\
             (define handler{index}\n\
             \x20 (case-lambda [(a) a] [(a b) (+ a b)]))\n\
             (define (run{index} lst)\n\
             \x20 (parameterize ([current-output-port (open-output-string)])\n\
             \x20   (for ([item (in-list lst)]) (displayln item))\n\
             \x20   (for/list ([item (in-list lst)]) (* item 2))))\n"
        ));
    }
    source
}

/// Invocation counts are decided by the head index before any `check` body
/// runs, so they are stable under load and can be asserted.
///
/// What this pins is that each rule really is reached once per occurrence of
/// its own heads and **not** once per node — a `HeadFilter` accidentally
/// widened to `AllNodes` would show up here as a count in the thousands.
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

    // One `begin0`, one `case-lambda`, one `parameterize` and one `match` per
    // generated definition.
    assert_eq!(count_of("racket-begin0-single-form"), 20);
    assert_eq!(count_of("racket-case-lambda-single-clause"), 20);
    assert_eq!(count_of("racket-parameterize-empty-bindings"), 20);
    assert_eq!(count_of("racket-match-unreachable-clause"), 20);
    // The body-form rule anchors on many heads: 3 `define`s, 1 `parameterize`
    // and 1 `lambda`-free body per definition.
    assert_eq!(count_of("racket-for-comprehension-value-discarded"), 80);
}

/// The clean corpus really is clean, so the numbers above are the cost of the
/// common case rather than the cost of reporting.
#[test]
fn cost_the_benchmark_corpus_is_clean() {
    let source = corpus(20);
    let catalog = RuleCatalog::new(&ENTRIES);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(&source, Dialect::Racket).expect("parse");
    let outcome = collect_lint_pass(
        catalog,
        &index,
        Path::new("bench.rkt"),
        Dialect::Racket,
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
fn ignored_bench_cost_growth_is_linear() {
    println!("\n=== per-invocation cost, Racket depth rules ===");
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

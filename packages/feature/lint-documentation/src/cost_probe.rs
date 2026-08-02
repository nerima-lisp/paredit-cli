//! The measured cost of every rule in this package, against a control.
//!
//! Two numbers matter, and a unit test sees neither of them:
//!
//! - **The per-file cost of matching nothing.** CI's `bench-compare` gate at
//!   10% includes `clean/forms/*`, which lint a zero-finding file: that is
//!   exactly the cost a rule pays on every file where it has nothing to say.
//! - **The growth rate.** A rule that re-derives a whole-file answer once per
//!   match costs O(N²) in the number of matches. That is invisible to a unit
//!   test — it is correct, just quadratically slow — and it is how two shipped
//!   rules came to account for 98% of a lint run. The tell is the ratio between
//!   two runs at N and 2N: about 2.0 for linear, about 3.7 for quadratic.
//!
//! # What is asserted, and what is only measured
//!
//! Everything that runs unattended asserts **invocation counts**, which are
//! deterministic: they are decided by the head index before any `check` runs,
//! so they read the same on a loaded machine as on an idle one.
//!
//! Nothing that runs unattended asserts a **duration**. An earlier version of
//! this module asserted `clean * 3 <= dense` and defended it as
//! machine-independent because it compared two measurements rather than a
//! clock. That reasoning is wrong: the ratio between two short durations is
//! itself unstable under load, and it is least stable exactly where the
//! numerator is smallest — which is the case the assertion was about. It passed
//! locally and failed in CI. The same objection applies to a doubling ratio.
//!
//! So the timing probes are still here, and they still print the numbers that
//! found a 97x, a 5.5x, a 5.3x and a 5.1x cost bug in this batch — but they are
//! `#[ignore]`d benchmarks, run on purpose, not gates that fail a pull request
//! at 3am because a runner was busy. Do not "fix" a future flake by loosening a
//! budget; a wall-clock budget fails eventually at any threshold.

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use paredit_core_lint_engine::engine::{PassOptions, build_head_index, collect_lint_pass};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    /// This package's four rules, plus a control.
    ///
    /// The control is `missing-package-docstring` measured against itself in a
    /// different shape — there is no cross-package rule available here without
    /// a dependency this package must not take (§4.2 forbids feature→feature
    /// edges, and the allowlist is in a test file outside this package). What
    /// the control gives is a same-engine, same-file baseline: the dispatch
    /// overhead every rule pays is in both numbers, so the difference is the
    /// rule's own work.
    static ENTRIES: [RuleEntry; 4] = [
        RuleEntry::new(
            &crate::docstring_example_stale_arity::rule::META,
            &crate::docstring_example_stale_arity::rule::RULE,
        ),
        RuleEntry::new(
            &crate::docstring_summary_line_too_long::rule::META,
            &crate::docstring_summary_line_too_long::rule::RULE,
        ),
        RuleEntry::new(
            &crate::missing_package_docstring::rule::META,
            &crate::missing_package_docstring::rule::RULE,
        ),
        RuleEntry::new(
            &crate::todo_fixme_no_attribution::rule::META,
            &crate::todo_fixme_no_attribution::rule::RULE,
        ),
    ];

    /// A file of `count` documented definitions and `count` attributed
    /// comments — the shape every rule here has the most to do on, and the one
    /// a quadratic rule would blow up on.
    fn dense_source(count: usize) -> String {
        let mut source = String::from(
            ";;;; A generated corpus.\n(defpackage :app (:use :cl) (:documentation \"Gen.\"))\n",
        );
        for index in 0..count {
            source.push_str(&format!(
                ";; TODO(ada): revisit f{index} once #412 lands.\n\
                 (defun f{index} (x factor)\n  \
                 \"Return X scaled by FACTOR.\n\nExample: (f{index} 3 2) => 6\"\n  \
                 (* x factor))\n\n"
            ));
        }
        source
    }

    /// A zero-finding file with no comments and no docstrings: what
    /// `clean/forms/*` measures, and what every rule pays on a file it has
    /// nothing to say about.
    fn clean_source(count: usize) -> String {
        (0..count)
            .map(|index| format!("(defun f{index} (x y)\n  (+ x y))\n\n"))
            .collect()
    }

    /// Runs one measured pass, returning `(total, per-rule (name, µs,
    /// invocations))`.
    fn measure(source: &str) -> (Duration, Vec<(&'static str, u128, u64)>) {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let outcome = collect_lint_pass(
            catalog,
            &index,
            Path::new("bench.lisp"),
            Dialect::CommonLisp,
            &tree,
            source,
            RuleSelection::All,
            PassOptions {
                settings: None,
                measure: true,
            },
        )
        .expect("lint pass");
        let timings = outcome.timings.expect("measure: true produces timings");
        let per_rule: Vec<(&'static str, u128, u64)> = timings
            .entries()
            .map(|(position, elapsed, invocations)| {
                (
                    ENTRIES[position].meta().name().as_str(),
                    elapsed.as_micros(),
                    invocations,
                )
            })
            .collect();
        (timings.total(), per_rule)
    }

    /// The invocation count the dispatcher attributed to `rule`.
    fn invocations_of(rows: &[(&'static str, u128, u64)], rule: &str) -> u64 {
        rows.iter()
            .find(|(name, _, _)| *name == rule)
            .map(|(_, _, invocations)| *invocations)
            .expect("the rule is in the catalogue")
    }

    /// Every node in the tree, so an invocation count can be read against the
    /// number of nodes a per-node rule would have been called on.
    fn node_count(source: &str) -> u64 {
        fn walk(view: &paredit_core_syntax::sexpr::ExpressionView) -> u64 {
            1 + view.children.iter().map(walk).sum::<u64>()
        }
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        tree.root_view().children.iter().map(walk).sum()
    }

    /// The numbers, printed. Run with:
    /// `cargo test -p paredit-feature-lint-documentation cost_report -- --ignored --nocapture`
    #[test]
    #[ignore = "a timing report, not an assertion"]
    fn ignored_cost_report() {
        for count in [500_usize, 1000, 2000, 4000] {
            let dense = dense_source(count);
            let clean = clean_source(count);
            let (dense_total, dense_rules) = measure(&dense);
            let (clean_total, clean_rules) = measure(&clean);
            println!("\n=== N = {count} definitions");
            println!(
                "dense  total {:>8} µs   ({} bytes)",
                dense_total.as_micros(),
                dense.len()
            );
            for (name, micros, invocations) in &dense_rules {
                println!("  {name:<34} {micros:>8} µs  {invocations:>7} invocations");
            }
            println!(
                "clean  total {:>8} µs   ({} bytes, zero findings)",
                clean_total.as_micros(),
                clean.len()
            );
            for (name, micros, invocations) in &clean_rules {
                println!("  {name:<34} {micros:>8} µs  {invocations:>7} invocations");
            }
        }
    }

    /// The doubling ratio. Linear work doubles when the input doubles (~2.0);
    /// per-match re-derivation quadruples (~3.7 in practice, once constant
    /// overheads are counted).
    ///
    /// **A benchmark, not a test.** The ratio between two wall-clock durations
    /// is unstable under parallel load no matter how loose the budget, so this
    /// does not run unattended. Run it deliberately, on an idle machine, when
    /// changing how a rule derives its answer:
    ///
    /// ```text
    /// cargo test -p paredit-feature-lint-documentation \
    ///   -- --ignored --nocapture ignored_bench_doubling_ratio
    /// ```
    ///
    /// A printed ratio near 2.0 is linear; near 3.7 means a rule re-derives a
    /// whole-file answer once per match. The deterministic half of this
    /// property — that a rule is called once per matching head rather than once
    /// per node — is asserted in
    /// [`each_rule_is_dispatched_once_per_matching_head`], which does run in CI.
    #[test]
    #[ignore = "a benchmark: wall-clock ratios are unstable under parallel load"]
    fn ignored_bench_doubling_ratio() {
        // Warm the allocator and the branch predictors, so the first
        // measurement is not systematically the slow one.
        let _ = measure(&dense_source(200));

        let at_2000 = measure(&dense_source(2000)).0.as_micros().max(1);
        let at_4000 = measure(&dense_source(4000)).0.as_micros().max(1);

        // Integer arithmetic, so a fractional budget is expressed as a ratio.
        assert!(
            at_4000 * 10 <= at_2000 * 28,
            "doubling the input multiplied the cost by {:.2} (from {at_2000} µs to {at_4000} µs); \
             linear is about 2.0 and per-match re-derivation is about 3.7",
            at_4000 as f64 / at_2000 as f64
        );
    }

    /// The `clean/forms/*` shape, stated deterministically.
    ///
    /// That benchmark lints a zero-finding file, so what it measures is the
    /// per-file price every rule pays on a file it has nothing to say about.
    /// The price is decided before any `check` body runs: the head index either
    /// dispatches a rule or it does not. So the property is an invocation
    /// count, not a duration — and the count is a *stronger* claim than "clean
    /// is three times cheaper than dense", because it says where the cost went
    /// rather than how much of it there was.
    ///
    /// On a file of 4000 bare `defun`s:
    ///
    /// - `missing-package-docstring` is dispatched **zero** times. Its heads are
    ///   `defpackage`/`define-package`, neither of which occurs, so the head
    ///   index rejects it outright and its `check` never runs at all.
    /// - the two `defun`-headed rules are dispatched **exactly 4000** times —
    ///   once per matching head, and emphatically not once per node. The node
    ///   count below is what makes that number mean something: a rule walking
    ///   the tree itself would show the larger figure.
    /// - `todo-fixme-no-attribution` is dispatched **once**: it is
    ///   [`HeadFilter::WholeTree`], so it legitimately runs on a clean file, and
    ///   one invocation is the whole of what it costs.
    #[test]
    fn each_rule_is_dispatched_once_per_matching_head() {
        const DEFINITIONS: u64 = 4000;

        let clean = clean_source(DEFINITIONS as usize);
        let nodes = node_count(&clean);
        assert!(
            nodes > DEFINITIONS * 4,
            "the clean fixture has {nodes} nodes for {DEFINITIONS} definitions; the per-head \
             counts below cannot be told apart from per-node ones"
        );

        let (_, rows) = measure(&clean);

        assert_eq!(
            invocations_of(&rows, "missing-package-docstring"),
            0,
            "a file with no defpackage must not reach this rule's check at all"
        );
        for rule in [
            "docstring-example-stale-arity",
            "docstring-summary-line-too-long",
        ] {
            assert_eq!(
                invocations_of(&rows, rule),
                DEFINITIONS,
                "{rule} must be dispatched once per defun, not once per node \
                 (the file has {nodes} nodes)"
            );
        }
        assert_eq!(
            invocations_of(&rows, "todo-fixme-no-attribution"),
            1,
            "a WholeTree rule is handed the tree once"
        );
    }

    /// The same counts on the file where every rule has something to say, so
    /// the zeroes and ones above are the head index talking rather than a
    /// catalogue that failed to register a rule.
    #[test]
    fn the_dense_file_dispatches_the_rules_the_clean_file_does_not() {
        const DEFINITIONS: u64 = 500;

        let (_, rows) = measure(&dense_source(DEFINITIONS as usize));

        assert_eq!(
            invocations_of(&rows, "missing-package-docstring"),
            1,
            "the dense fixture declares exactly one package"
        );
        for rule in [
            "docstring-example-stale-arity",
            "docstring-summary-line-too-long",
        ] {
            assert_eq!(invocations_of(&rows, rule), DEFINITIONS, "{rule}");
        }
        assert_eq!(invocations_of(&rows, "todo-fixme-no-attribution"), 1);
    }
}

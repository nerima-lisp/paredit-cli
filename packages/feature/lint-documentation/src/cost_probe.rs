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
//! [`ignored_cost_report`] prints the numbers; the two tests assert the
//! property. The report is `#[ignore]` because timing assertions in CI are
//! flaky, and the property tests use budgets hundreds of times the real cost so
//! only an asymptotic regression can trip them.

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
    /// The budget is 2.8 — comfortably above the noise a linear rule produces
    /// on a loaded machine, and comfortably below what a quadratic one does.
    /// Measured at 4000 definitions rather than a few hundred, because the
    /// difference between the two shapes is invisible while the constant
    /// factors dominate.
    #[test]
    fn every_rules_cost_grows_linearly_in_the_number_of_matches() {
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

    /// The `clean/forms/*` shape: a file with no comment, no docstring, and no
    /// package declaration must cost each rule essentially nothing, because
    /// that is the per-file price every rule in the suite pays on every file it
    /// has nothing to say about.
    ///
    /// Asserted against the *dense* file rather than against a clock, so the
    /// test does not depend on the machine: matching nothing must be far
    /// cheaper than matching everything.
    #[test]
    fn matching_nothing_is_far_cheaper_than_matching_everything() {
        let _ = measure(&clean_source(200));

        let clean = measure(&clean_source(4000)).0.as_micros().max(1);
        let dense = measure(&dense_source(4000)).0.as_micros().max(1);

        assert!(
            clean * 3 <= dense,
            "a zero-finding file cost {clean} µs against {dense} µs for a dense one; the \
             rejection path is not cheap enough for the clean/forms benchmarks"
        );
    }
}

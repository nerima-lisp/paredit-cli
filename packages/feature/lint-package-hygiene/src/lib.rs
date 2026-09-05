#![doc = include_str!("../README.md")]

pub mod package_circular_in_package_chain;
pub mod support;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.

/// The catalogue this package's tests drive the engine with.
///
/// A `#[cfg(test)]` mirror of the one line the root's `REGISTRY` will hold, so
/// the head filter and the `RuleDialectScope` — neither of which a `domain`
/// test can see — are exercised here rather than only after wiring.
#[cfg(test)]
mod test_catalog {
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};

    /// This package's rules, in the order the root registers them.
    pub static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(
        &crate::package_circular_in_package_chain::rule::META,
        &crate::package_circular_in_package_chain::rule::RULE,
    )];

    pub fn catalog() -> RuleCatalog {
        RuleCatalog::new(&ENTRIES)
    }
}

/// Every rule here driven through the *engine*, rather than through its own
/// `build_*_report`.
///
/// The two entry points do not share their quote handling. The report walks the
/// top level itself and never looks at a quoted form; a head-filtered rule is
/// handed matched nodes by the dispatcher *including* the ones inside `'(…)`,
/// and depends on `check`'s [`crate::support::is_unevaluated_at`] call to
/// decline them. Testing only the report would leave that call unexercised.
///
/// Running the real pass also covers the two declarations a domain test cannot
/// see: the head filter, and the `RuleDialectScope` that keeps this Common Lisp
/// only.
#[cfg(test)]
mod engine_pass_tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    use crate::test_catalog::catalog;

    /// The rule names that fire on `source`, sorted so the assertions do not
    /// depend on registration order.
    fn fired(source: &str, dialect: Dialect) -> Vec<&'static str> {
        let catalog = catalog();
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        let mut names: Vec<&'static str> = collect_lint_outcomes(
            catalog,
            &index,
            Path::new("t.lisp"),
            dialect,
            &tree,
            source,
            RuleSelection::All,
        )
        .expect("lint pass")
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect();
        names.sort_unstable();
        names
    }

    const REENTRY: &str = "(in-package :app)\n\
                           (defun start () 1)\n\
                           (in-package :app/test)\n\
                           (defun check () 2)\n\
                           (in-package :app)\n\
                           (defun stop () 3)\n";

    // -- each rule reaches the engine ---------------------------------------

    #[test]
    fn every_rule_fires_through_the_real_dispatch() {
        assert_eq!(
            fired(REENTRY, Dialect::CommonLisp),
            vec!["package-circular-in-package-chain"]
        );
    }

    /// The head index is keyed on the normalized head, so the qualified and
    /// upcased spellings must reach the rule through it too.
    #[test]
    fn a_qualified_or_upcased_head_still_reaches_the_rule() {
        for spelling in ["cl:in-package", "CL:IN-PACKAGE", "IN-PACKAGE"] {
            let source = format!("({spelling} :app)\n({spelling} :other)\n({spelling} :app)\n");
            assert_eq!(
                fired(&source, Dialect::CommonLisp),
                vec!["package-circular-in-package-chain"],
                "`{spelling}` did not reach the rule"
            );
        }
    }

    /// A file with none of this package's heads must trip nothing at all.
    #[test]
    fn a_file_with_none_of_our_heads_trips_nothing() {
        for source in [
            "",
            "(defun f (a b)\n  \"Return A plus B.\"\n  (+ a b))\n",
            "(defpackage :app (:use :cl) (:export #:run))\n",
            "(defsystem \"app\" :version \"1.0.0\")\n",
            "(find-package :app)\n(package-name *package*)\n",
        ] {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "`{source}` should trip nothing"
            );
        }
    }

    #[test]
    fn a_single_switch_or_a_forward_only_chain_trips_nothing() {
        for source in [
            "(in-package :app)\n(defun f () 1)\n",
            "(in-package :a)\n(in-package :b)\n(in-package :c)\n",
            "(in-package :app)\n(in-package :app)\n",
        ] {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "`{source}` should trip nothing"
            );
        }
    }

    // -- the guard the report path cannot exercise --------------------------

    /// The dispatcher hands a rule every head-matched node, quoted or not.
    /// Without `check`'s `is_unevaluated_at` call — reached through
    /// `examine_in_package` — the third form here is read as a real switch.
    #[test]
    fn no_quoted_switch_fires_through_the_real_dispatch() {
        for source in [
            "(in-package :a)\n(in-package :b)\n'(in-package :a)\n",
            "(in-package :a)\n(in-package :b)\n(quote (in-package :a))\n",
            "(in-package :a)\n(in-package :b)\n`(in-package :a)\n",
            "(in-package :a)\n(in-package :b)\n'(x ,(in-package :a))\n",
            "(in-package :a)\n(in-package :b)\n`(x ,(in-package :a))\n",
        ] {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "`{source}` is not a live top-level switch"
            );
        }
    }

    /// A macro template that expands to a switch is data, not a switch.
    #[test]
    fn a_switch_inside_a_macro_template_does_not_fire() {
        assert_eq!(
            fired(
                "(in-package :a)\n\
                 (in-package :b)\n\
                 (defmacro with-app (&body body) `(progn (in-package :a) ,@body))\n",
                Dialect::CommonLisp
            ),
            Vec::<&str>::new()
        );
    }

    /// The dispatcher visits every node, so `check` *is* called on an
    /// `in-package` nested inside `eval-when` or a function body. It switches
    /// nothing the reader will act on, so it must close no loop — and only the
    /// engine pass reaches that guard, because the report's own walk never
    /// looks below the top level.
    #[test]
    fn a_nested_switch_does_not_close_a_loop_through_the_real_dispatch() {
        for source in [
            "(in-package :a)\n(in-package :b)\n(eval-when (:execute) (in-package :a))\n",
            "(in-package :a)\n(in-package :b)\n(progn (in-package :a))\n",
            "(in-package :a)\n(in-package :b)\n(defun f () (in-package :a))\n",
        ] {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "`{source}` has no top-level re-entry"
            );
        }
    }

    #[test]
    fn a_switch_inside_a_string_literal_does_not_fire() {
        assert_eq!(
            fired(
                "(in-package :a)\n(in-package :b)\n(defvar *doc* \"(in-package :a)\")\n",
                Dialect::CommonLisp
            ),
            Vec::<&str>::new()
        );
    }

    // -- dialect scope -------------------------------------------------------

    /// `RuleDialectScope::COMMON_LISP_ONLY` is resolved before the walk, so a
    /// non-Common-Lisp file must not reach `check` at all. Only the engine pass
    /// can see this: `examine_in_package` itself has no dialect parameter.
    #[test]
    fn the_rule_is_common_lisp_only() {
        for dialect in [Dialect::Clojure, Dialect::Scheme, Dialect::EmacsLisp] {
            assert_eq!(
                fired(REENTRY, dialect),
                Vec::<&str>::new(),
                "{dialect:?} is out of scope"
            );
        }
    }
}

/// The rule run over realistic, *correct* Common Lisp, and over the repository's
/// own `.lisp` fixtures.
///
/// A reviewer's first move is to point the rule at code that is fine and see
/// what it says. This makes that a permanent test rather than a one-off, and it
/// asserts the corpus actually *exercises* the rule: a sweep over code that
/// contains none of the rule's candidates proves nothing at all, so the
/// candidate count is asserted alongside the finding count.
#[cfg(test)]
mod corpus_sweep_tests {
    use std::path::{Path, PathBuf};

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    use crate::support::top_level_in_package_chain;
    use crate::test_catalog::catalog;

    /// How many findings the real engine produces for `source`.
    fn finding_count(source: &str) -> usize {
        let catalog = catalog();
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        collect_lint_outcomes(
            catalog,
            &index,
            Path::new("corpus.lisp"),
            Dialect::CommonLisp,
            &tree,
            source,
            RuleSelection::All,
        )
        .expect("lint pass")
        .len()
    }

    /// How many *candidates* the corpus contains: top-level `in-package`
    /// switches, the thing the rule looks at. Counting these is what makes a
    /// zero-finding sweep meaningful.
    fn candidate_count(source: &str) -> usize {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        top_level_in_package_chain(&tree).len()
    }

    /// Hand-written, realistic, correct Common Lisp. Every one of these is a
    /// shape a working project actually contains, and none of them is a defect.
    const REALISTIC_CORRECT: &[(&str, &str)] = &[
        (
            "the universal package.lisp: declarations, no switch at all",
            "(defpackage #:app\n\
             \x20 (:use #:cl)\n\
             \x20 (:export #:run #:stop))\n\
             \n\
             (defpackage #:app/test\n\
             \x20 (:use #:cl #:app))\n",
        ),
        (
            "the ordinary implementation file: one switch, then definitions",
            "(in-package #:app)\n\
             \n\
             (defvar *running* nil\n\
             \x20 \"Whether the service is up.\")\n\
             \n\
             (defun run ()\n\
             \x20 \"Start the service.\"\n\
             \x20 (setf *running* t))\n\
             \n\
             (defun stop ()\n\
             \x20 \"Stop the service.\"\n\
             \x20 (setf *running* nil))\n",
        ),
        (
            "the single-file library: bounce through CL-USER to declare each package",
            "(in-package :cl-user)\n\
             \n\
             (defpackage :tiny\n\
             \x20 (:use :cl)\n\
             \x20 (:export #:clamp))\n\
             \n\
             (in-package :tiny)\n\
             \n\
             (defun clamp (x lo hi)\n\
             \x20 \"Clamp X into [LO, HI].\"\n\
             \x20 (max lo (min hi x)))\n\
             \n\
             (in-package :cl-user)\n\
             \n\
             (defpackage :tiny/test\n\
             \x20 (:use :cl :tiny))\n\
             \n\
             (in-package :tiny/test)\n\
             \n\
             (defun check ()\n\
             \x20 \"Exercise CLAMP.\"\n\
             \x20 (assert (= 5 (clamp 9 0 5))))\n",
        ),
        (
            "a forward-only chain: three packages, each entered once",
            "(in-package :app/base)\n\
             (defun base () 1)\n\
             (in-package :app/middle)\n\
             (defun middle () 2)\n\
             (in-package :app/top)\n\
             (defun top () 3)\n",
        ),
        (
            "an .asd file: enters ASDF-USER and stays",
            "(in-package :asdf-user)\n\
             \n\
             (defsystem \"app\"\n\
             \x20 :version \"1.0.0\"\n\
             \x20 :depends-on (\"alexandria\")\n\
             \x20 :components ((:file \"package\") (:file \"app\")))\n",
        ),
        (
            "the package-inferred-system layout",
            "(uiop:define-package :app/util\n\
             \x20 (:use :cl)\n\
             \x20 (:export #:clamp))\n\
             (in-package :app/util)\n\
             \n\
             (defun clamp (x lo hi) (max lo (min hi x)))\n",
        ),
        (
            "a macro whose template switches packages: data, not a switch",
            "(in-package :app)\n\
             \n\
             (defmacro in-scratch (&body body)\n\
             \x20 \"Run BODY with the scratch package entered.\"\n\
             \x20 `(progn (in-package :app/scratch) ,@body))\n",
        ),
        (
            "a redundant but harmless repeat: entered twice in a row",
            "(in-package :app)\n(in-package :app)\n(defun f () 1)\n",
        ),
        (
            "a conditional switch behind a reader conditional",
            "(in-package :app)\n\
             #+sbcl (in-package :app/sbcl)\n\
             (defun f () 1)\n",
        ),
    ];

    /// Correct code stays silent, and the corpus is not silent by accident.
    #[test]
    fn the_realistic_corpus_is_clean_and_actually_exercises_the_rule() {
        let mut candidates = 0;
        for (label, source) in REALISTIC_CORRECT {
            let findings = finding_count(source);
            assert_eq!(findings, 0, "false positive on `{label}`");
            candidates += candidate_count(source);
        }
        // Reject a vacuous pass on a corpus containing no `in-package` forms.
        assert!(
            candidates >= 12,
            "the corpus only contains {candidates} top-level in-package switches, \
             which is too few to have exercised anything"
        );
    }

    /// The dangerous twin: the same shapes, each minimally perturbed into the
    /// defect. If the corpus above is silent because the rule is broken rather
    /// than because the code is correct, these are silent too.
    const DANGEROUS_TWINS: &[(&str, &str)] = &[
        (
            "the implementation file, with a detour and a return",
            "(in-package #:app)\n\
             (defun run () 1)\n\
             (in-package #:app/internal)\n\
             (defun helper () 2)\n\
             (in-package #:app)\n\
             (defun stop () (helper))\n",
        ),
        (
            "the single-file library, but the second package is entered before the first is done",
            "(in-package :tiny)\n\
             (defun clamp (x lo hi) (max lo (min hi x)))\n\
             (in-package :tiny/test)\n\
             (defun check () (assert (= 5 (clamp 9 0 5))))\n\
             (in-package :tiny)\n\
             (defun scale (x) (* x 2))\n",
        ),
        (
            "the forward-only chain, doubled back",
            "(in-package :app/base)\n\
             (defun base () 1)\n\
             (in-package :app/middle)\n\
             (defun middle () 2)\n\
             (in-package :app/base)\n\
             (defun base-again () 3)\n",
        ),
        (
            "the redundant repeat, with a real detour between the two",
            "(in-package :app)\n(in-package :other)\n(in-package :app)\n(defun f () 1)\n",
        ),
    ];

    #[test]
    fn every_dangerous_twin_fires_exactly_once() {
        for (label, source) in DANGEROUS_TWINS {
            assert_eq!(finding_count(source), 1, "missed the defect in `{label}`");
        }
    }

    /// The repository's own Common Lisp fixtures, swept through the real
    /// engine.
    ///
    /// Best-effort by design: these files live outside this package, and a
    /// sandbox that does not check them out must not fail this test. What is
    /// *not* best-effort is the assertion — if the files are there, they are
    /// realistic correct Common Lisp and the rule must be silent on all of
    /// them, and they must contain candidates for that silence to mean
    /// anything.
    fn repo_lisp_fixtures() -> Vec<PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures");
        let mut found = Vec::new();
        let mut pending = vec![root];
        while let Some(directory) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|ext| ext == "lisp") {
                    found.push(path);
                }
            }
        }
        found.sort();
        found
    }

    #[test]
    fn the_repositorys_own_lisp_fixtures_are_clean() {
        let fixtures = repo_lisp_fixtures();
        if fixtures.is_empty() {
            // No checkout of tests/fixtures — the embedded corpus above still
            // ran, so this test has nothing left to say.
            return;
        }
        let mut candidates = 0;
        let mut scanned = 0;
        for path in &fixtures {
            let Ok(source) = std::fs::read_to_string(path) else {
                continue;
            };
            // A fixture that deliberately does not parse is some other test's
            // subject, not this one's.
            if SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp).is_err() {
                continue;
            }
            scanned += 1;
            candidates += candidate_count(&source);
            assert_eq!(
                finding_count(&source),
                0,
                "false positive on the repository fixture {}",
                path.display()
            );
        }
        assert!(scanned > 0, "no repository fixture parsed as Common Lisp");
        assert!(
            candidates > 0,
            "{scanned} repository fixtures parsed but none contains a top-level in-package, \
             so this sweep exercised nothing"
        );
    }
}

/// What this package's rule costs the engine, measured through the engine.
///
/// Two shapes are pinned here, and they are pinned separately because they fail
/// separately:
///
/// - **Invocation count.** Deterministic, and the thing that actually goes
///   wrong: a rule that is dispatched once per *definition* rather than once per
///   `in-package` pays its whole-file scan N times, and N×file is the quadratic
///   this codebase has found twice at 480 definitions.
/// - **Wall clock.** Noisy, so it is measured best-of-several and asserted only
///   loosely — enough to separate linear (ratio ≈ 2 per doubling) from
///   quadratic (ratio ≈ 4), and no tighter.
#[cfg(test)]
mod measurement {
    use std::path::Path;
    use std::time::Duration;

    use paredit_core_lint_engine::LintResult;
    use paredit_core_lint_engine::engine::{
        PassOptions, RuleContext, RuleSink, build_head_index, collect_lint_pass,
    };
    use paredit_core_lint_engine::model::{
        Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
    };
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{LintRule, RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{ExpressionView, SyntaxTree};

    /// The control.
    ///
    /// A feature package may not depend on another feature package without an
    /// entry in `tests/cli/feature_dependency_contract.rs`, and that contract
    /// scans the whole manifest text rather than only `[dependencies]` — so a
    /// *dev*-dependency on a shipped rule would trip it too. The control is
    /// therefore declared here: a `Heads` rule that matches the file's dense
    /// head and does nothing at all, which measures pure dispatch overhead and
    /// isolates what this package's rule adds on top of it.
    #[derive(Debug)]
    struct DispatchOnly;

    const CONTROL_META: RuleMeta = RuleMeta::new(
        "measurement-control",
        RuleCategory::Suspicious,
        Severity::Warning,
        "a test-only control that matches and does nothing",
        Fixability::ReportOnly,
    );

    const CONTROL_HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defun")];
    static CONTROL: DispatchOnly = DispatchOnly;

    impl LintRule for DispatchOnly {
        fn head_filter(&self) -> HeadFilter {
            HeadFilter::Heads(&CONTROL_HEADS)
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
        RuleEntry::new(
            &crate::package_circular_in_package_chain::rule::META,
            &crate::package_circular_in_package_chain::rule::RULE,
        ),
        RuleEntry::new(&CONTROL_META, &CONTROL),
    ];

    /// Registration positions in [`ENTRIES`], which is how `RuleTimings`
    /// indexes its rows.
    const SUBJECT: usize = 0;
    const CONTROL_POSITION: usize = 1;

    /// A file with `definitions` top-level definitions, a `defpackage`, and a
    /// realistic three-switch `in-package` chain that doubles back once.
    ///
    /// The switches sit at the *start*, the *middle* and the *end*, so the
    /// per-invocation top-level scan is paid at three different distances into
    /// the document rather than three times at the same one.
    fn source_with(definitions: usize) -> String {
        let mut out = String::new();
        out.push_str("(defpackage :app (:use :cl) (:export #:run))\n");
        out.push_str("(in-package :app)\n");
        let half = definitions / 2;
        for index in 0..half {
            out.push_str(&format!(
                "(defun app-fn-{index} (a b)\n  \"Return A plus twice B.\"\n  (+ a (* b 2)))\n"
            ));
        }
        out.push_str("(in-package :app/internal)\n");
        for index in half..definitions {
            out.push_str(&format!(
                "(defun app-fn-{index} (a b)\n  \"Return A plus twice B.\"\n  (+ a (* b 2)))\n"
            ));
        }
        out.push_str("(in-package :app)\n");
        out.push_str("(defun app-last (x)\n  \"Identity.\"\n  x)\n");
        out
    }

    /// The shape almost every real Common Lisp file has: one `in-package` at
    /// the top, then definitions. No chain, so no finding is possible — and the
    /// bounded scan means the rule must not read past the switch to know that.
    fn ordinary_file(definitions: usize) -> String {
        let mut out = String::new();
        out.push_str("(in-package :app)\n");
        for index in 0..definitions {
            out.push_str(&format!(
                "(defun app-fn-{index} (a b)\n  \"Return A plus twice B.\"\n  (+ a (* b 2)))\n"
            ));
        }
        out
    }

    struct Measured {
        subject: Duration,
        subject_invocations: u64,
        control: Duration,
        control_invocations: u64,
        findings: usize,
    }

    fn measure_once(source: &str) -> Measured {
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
        let rows: Vec<(usize, Duration, u64)> = timings.entries().collect();
        Measured {
            subject: rows[SUBJECT].1,
            subject_invocations: rows[SUBJECT].2,
            control: rows[CONTROL_POSITION].1,
            control_invocations: rows[CONTROL_POSITION].2,
            findings: outcome.outcomes.len(),
        }
    }

    /// Best of `runs`, by the subject's own time. A single timed run on a
    /// loaded machine is not a measurement; the minimum is the least
    /// contaminated sample available.
    fn measure(source: &str, runs: usize) -> Measured {
        let mut best = measure_once(source);
        for _ in 1..runs {
            let next = measure_once(source);
            if next.subject < best.subject {
                best = next;
            }
        }
        best
    }

    /// The deterministic half: the rule is dispatched once per `in-package`
    /// form and never once per definition, whatever the file's size.
    #[test]
    fn the_rule_is_invoked_once_per_switch_not_once_per_definition() {
        for definitions in [64_usize, 512, 4096] {
            let measured = measure(&source_with(definitions), 1);
            assert_eq!(
                measured.subject_invocations, 3,
                "{definitions} definitions produced {} invocations; the chain has three switches",
                measured.subject_invocations
            );
            assert_eq!(
                measured.control_invocations,
                definitions as u64 + 1,
                "the control should see every defun"
            );
            assert_eq!(
                measured.findings, 1,
                "the fixture doubles back exactly once"
            );
        }
    }

    /// The number that decides what this rule costs a real project: the
    /// ordinary file, with one switch at the top and nothing to find.
    ///
    /// The bounded scan makes this independent of the file's size — the rule
    /// stops after the switch it was dispatched at — so quadrupling the
    /// definitions must not multiply the cost.
    ///
    /// The assertion is stated **against the control measured in the same
    /// pass**, not against a microsecond constant. An earlier version used a
    /// constant tuned on `--release` and then failed in `debug`, where every
    /// number is about ten times larger; a comparison between two rules in one
    /// pass has no such profile dependence. The control is dispatched once per
    /// `defun`, so "cheaper than the control" says this rule did not read the
    /// definitions — which is exactly the property the bound exists for.
    #[test]
    fn the_ordinary_one_switch_file_costs_almost_nothing() {
        const RUNS: usize = 9;
        let small = measure(&ordinary_file(1024), RUNS);
        let large = measure(&ordinary_file(4096), RUNS);

        for (definitions, measured) in [(1024, &small), (4096, &large)] {
            println!(
                "ordinary file (1 switch, no finding) {definitions:>5} defs: \
                 {:>8.2}us over {} invocation(s)   control {:>8.2}us over {} invocations",
                measured.subject.as_secs_f64() * 1e6,
                measured.subject_invocations,
                measured.control.as_secs_f64() * 1e6,
                measured.control_invocations,
            );
        }

        assert_eq!(small.subject_invocations, 1);
        assert_eq!(large.subject_invocations, 1);
        assert_eq!(small.findings, 0);
        assert_eq!(large.findings, 0);

        // The whole point of bounding the scan: a switch at the top of the file
        // does not read the file below it. A rule that walked all 4096
        // definitions once would cost about what the control costs being
        // dispatched at all 4096 of them; this one costs a small fraction of
        // it. Removing the bound in `top_level_in_package_chain_through` makes
        // this fail, which is how it was checked.
        let subject_us = large.subject.as_secs_f64() * 1e6;
        let control_us = large.control.as_secs_f64() * 1e6;
        assert!(
            subject_us < control_us,
            "one switch at the top of a 4096-definition file cost {subject_us:.2}us, against \
             {control_us:.2}us for a control rule dispatched at every definition; the scan \
             should stop at the switch, not walk the definitions"
        );
        // And it must not grow with the file at all beyond measurement noise.
        let growth = subject_us / (small.subject.as_secs_f64() * 1e6).max(f64::MIN_POSITIVE);
        println!("4x growth on the ordinary file: {growth:.2} (a bounded scan is ~1.0)");
        assert!(
            growth < 2.5,
            "the ordinary file's cost grew {growth:.2}x when it quadrupled; a scan that stops \
             at the first switch should not grow at all"
        );
    }

    /// The noisy half: how the cost grows with the file.
    ///
    /// The step is **4x**, not 2x, on purpose. A doubling separates linear
    /// (≈2.0) from quadratic (≈4.0) by a factor the machine's own noise can
    /// cover — three consecutive runs of an earlier version of this test
    /// produced 2.59, 2.78 and 3.19 for identical code. Quadrupling separates
    /// them by 4.0 against 16.0, which nothing short of a real regression
    /// crosses.
    #[test]
    fn the_cost_grows_linearly_with_the_file() {
        const RUNS: usize = 9;
        let small = measure(&source_with(1024), RUNS);
        let large = measure(&source_with(4096), RUNS);

        println!(
            "package-circular-in-package-chain  1024 defs: {:>8.1}us over {} invocations \
             (control {:>8.1}us over {} invocations)",
            small.subject.as_secs_f64() * 1e6,
            small.subject_invocations,
            small.control.as_secs_f64() * 1e6,
            small.control_invocations,
        );
        println!(
            "package-circular-in-package-chain  4096 defs: {:>8.1}us over {} invocations \
             (control {:>8.1}us over {} invocations)",
            large.subject.as_secs_f64() * 1e6,
            large.subject_invocations,
            large.control.as_secs_f64() * 1e6,
            large.control_invocations,
        );

        let small_us = small.subject.as_secs_f64() * 1e6;
        let large_us = large.subject.as_secs_f64() * 1e6;
        // Below this the clock's own resolution dominates and a ratio says
        // nothing. Asserting anyway is how a timing test becomes flaky.
        if small_us < 10.0 {
            println!("4x ratio: not asserted; {small_us:.1}us is inside the clock noise");
            return;
        }
        let ratio = large_us / small_us;
        // The control is dispatched once per `defun`, so its own cost is linear
        // in the file by construction. Measuring it in the same pass gives a
        // reference that absorbs whatever the machine was doing at the time,
        // which a bare constant cannot: a loaded machine moves both ratios
        // together.
        let control_ratio = (large.control.as_secs_f64() * 1e6).max(f64::MIN_POSITIVE)
            / (small.control.as_secs_f64() * 1e6).max(f64::MIN_POSITIVE);
        println!(
            "4x ratio: {ratio:.2} (control {control_ratio:.2}; \
             linear is ~4.0, quadratic is ~16.0)"
        );
        // Both bounds have to hold. The absolute one catches a regression on a
        // quiet machine; the relative one catches it on a busy one. Neither is
        // set at 4.0: the observed gap above linear is not algorithmic — the
        // number of forms visited is exactly linear, and the invocation count
        // is pinned separately — but the scan chases node ids through a larger
        // array in the larger file, so its cost *per form visited* grows.
        // Measured at ~20ns per form at 2048 definitions and ~30ns at 4096.
        // That is cache locality, and it is bounded.
        assert!(
            ratio < 9.0,
            "cost grew {ratio:.2}x when the file quadrupled ({small_us:.1}us -> {large_us:.1}us); \
             linear is ~4.0 and quadratic is ~16.0"
        );
        assert!(
            ratio < control_ratio * 2.2,
            "cost grew {ratio:.2}x when the file quadrupled, against {control_ratio:.2}x for a \
             control rule measured in the same pass; a rule that rescans the whole file on \
             every invocation grows about four times as fast as one that does not"
        );
    }
}

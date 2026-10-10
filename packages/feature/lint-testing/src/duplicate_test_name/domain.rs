//! `duplicate-test-name` detection: two top-level test definitions in one file
//! that share a name, so loading the file silently keeps only the second.
//!
//! Nothing reports this at run time. The suite's count goes down by one, both
//! definitions still sit in the file, and the one that no longer runs is the
//! one someone wrote first.
//!
//! # What this attempts
//!
//! Two *top-level* test definitions in the *same file* whose names are equal
//! after case folding and package-qualifier stripping. The finding is placed on
//! the later one, because that is the definition that does the shadowing.
//!
//! # What this does not attempt
//!
//! - **A nested definition.** A `deftest` inside a `let`, a `macrolet` or
//!   another form is not a top-level definition, and whether it shadows
//!   anything depends on when its enclosing form runs. Both sides of a
//!   comparison must be root-level forms.
//! - **Cross-file duplicates.** `inspect redifinition` already answers that for
//!   Common Lisp, with the `in-package` tracking that makes the answer correct;
//!   duplicating it here without that tracking would produce a worse answer to
//!   the same question. This rule is deliberately one file, no packages — which
//!   is also what makes it useful for Emacs Lisp and Clojure, where that report
//!   does not apply at all.
//! - **A name that is not a plain symbol.** A `(name :suite s)` designator or a
//!   macro-built name is not compared.
//!
//! # Cost
//!
//! Linear in the file, and — this is the part that took three attempts to get
//! right — linear with a small constant, because the *view* is borrowed rather
//! than rebuilt.
//!
//! Every function here takes an already-materialized root [`ExpressionView`]
//! and none of them calls [`SyntaxTree::root_view`]. That is not a style
//! preference. `root_view` deep-materializes the document, allocating two
//! `Vec`s — children and reader prefixes — for *every node in the file*, and it
//! is uncached, so each call rebuilds all of it. The lint dispatcher already
//! builds exactly one root view per file and hands it to every `WholeTree`
//! rule, so a rule that takes the view pays nothing and a rule that calls
//! `root_view` itself pays for a second whole document.
//!
//! This rule did the latter, twice — once in each pass — and on the
//! `clean/forms/1024` benchmark that cost 1195µs against a 5403µs pass, or
//! +22%, which is the whole of the `bench-compare` regression this rule was
//! reverted for. The rule path now runs [`shadowing_test_definitions`] alone:
//! one walk over the *top-level* forms of a view it was given. The denominator
//! pass is not run there at all, because the rule reports findings and discards
//! the count; only the standalone `inspect duplicate-test-name` report, which
//! publishes the denominator, pays for it.
//!
//! Within each pass, nothing compares one test against another. Names are
//! hashed as they are read, so a repeat is a hash-map hit rather than a search
//! — the difference between reading a name once and reading it once per
//! definition that might share it.
//!
//! That distinction was not academic either. The first version of this rule ran
//! a whole-file byte scan per definition and then compared each definition
//! against every earlier one, which is T×T for a file of T tests: a 146 KB file
//! of 4000 `deftest` forms took 13.8s where the rest of the suite took 0.03s,
//! and — because `inspect lint` collects every rule's findings and filters
//! afterwards — no `--rule`, `--exclude` or `--category` could get a user out of
//! paying it. Nothing here may reintroduce a per-definition scan of the whole
//! source; `a_file_of_many_tests_stays_linear` fails outright if it does.
//!
//! So there are two cost invariants, and they pull in opposite directions —
//! the first fix for one introduced the other. Neither may be traded for the
//! other: no per-definition whole-file work, and no per-file document rebuild.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

use crate::support::{
    TEST_DIALECTS, for_each_evaluated_root_child, for_each_evaluated_subview, read_test_form,
};

#[derive(Debug, Clone)]
pub struct DuplicateTestNameItem {
    /// The span of the later definition — the one that shadows.
    pub span: ByteSpan,
    /// The shared name.
    pub test_name: String,
    /// Where the definition it shadows starts, so a reader can find it.
    pub shadowed_offset: usize,
}

impl Finding for DuplicateTestNameItem {
    fn kind(&self) -> &'static str {
        "duplicate-test-name"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("test={}", self.test_name),
            format!("shadows_offset={}", self.shadowed_offset),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("test", json!(self.test_name)),
            ("shadows_offset", json!(self.shadowed_offset)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "test {} is defined twice in this file; the earlier definition never runs",
            self.test_name
        )
    }
}

/// How many readable test definitions the file contains, anywhere in evaluated
/// code — the denominator, which is a count of what was *examined* and so is
/// not restricted to the top level the findings are.
///
/// One pre-order walk over the *whole* document. Each node is read once and
/// nothing is compared against anything, but unlike
/// [`shadowing_test_definitions`] this does descend into every form, so it is
/// the more expensive of the two passes and the lint rule does not run it: see
/// the module docs on why the denominator is the standalone report's alone.
fn count_test_definitions(root: &ExpressionView, dialect: Dialect) -> usize {
    let mut count = 0;
    for_each_evaluated_subview(root, |view| {
        if list_head(view).is_some()
            && read_test_form(view, dialect).is_some_and(|form| form.name_text().is_some())
        {
            count += 1;
        }
    });
    count
}

/// Every top-level test definition that repeats an earlier top-level name, in
/// document order.
///
/// One pass over the top-level forms, with each name's first definition kept in
/// a hash map. A definition is therefore read once, matched against the names
/// already seen in one lookup, and never revisited — so a third definition of a
/// name points back at the *first*, which is the one that would have run had
/// nothing shadowed it.
///
/// Takes the root view rather than the [`SyntaxTree`] because the lint rule's
/// caller — the dispatcher — already has one and building a second is the
/// dominant cost of this rule; see the module docs.
#[must_use]
pub fn shadowing_test_definitions(
    root: &ExpressionView,
    dialect: Dialect,
) -> Vec<DuplicateTestNameItem> {
    let mut first_definition: HashMap<String, usize> = HashMap::new();
    let mut violations = Vec::new();

    for_each_evaluated_root_child(root, |child| {
        // Only a plain-symbol name is comparable; `read_test_form` has already
        // declined every shape this cannot name.
        let Some(test_name) = read_test_form(child, dialect).and_then(|form| form.name_text())
        else {
            return;
        };
        match first_definition.entry(test_name) {
            Entry::Vacant(slot) => {
                slot.insert(child.span.start().get());
            }
            Entry::Occupied(earlier) => violations.push(DuplicateTestNameItem {
                span: child.span,
                test_name: earlier.key().clone(),
                shadowed_offset: *earlier.get(),
            }),
        }
    });

    violations
}

/// Collects every shadowed test name in one file, with the number of readable
/// test definitions beside them.
///
/// This is the standalone `inspect duplicate-test-name` path, which has no
/// dispatcher to borrow a root view from and so builds one — *once*, shared by
/// both passes. The lint rule does not come through here; it calls
/// [`shadowing_test_definitions`] directly on the view it is handed.
pub fn build_duplicate_test_name_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DuplicateTestNameItem>> {
    let modelled = TEST_DIALECTS.contains(&dialect);
    let (test_form_count, violations) = if modelled {
        let root = tree.root_view();
        (
            count_test_definitions(&root, dialect),
            shadowing_test_definitions(&root, dialect),
        )
    } else {
        (0, Vec::new())
    };

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        violations,
        vec![("test_form_count", json!(test_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::time::Duration;

    use paredit_core_lint_engine::LintResult;
    use paredit_core_lint_engine::engine::{
        RuleContext, RuleSink, build_head_index, collect_lint_outcomes,
    };
    use paredit_core_lint_engine::model::HeadFilter;
    use paredit_core_lint_engine::policy::{RuleDialectScope, RuleSelection};
    use paredit_core_lint_engine::rule::{LintRule, RuleCatalog, RuleEntry};

    use crate::duplicate_test_name::rule::{META, RULE};

    thread_local! {
        static RULE_TIMING: Cell<(Duration, u64)> = const { Cell::new((Duration::ZERO, 0)) };
    }

    #[derive(Debug)]
    struct MeasuredRule {
        rebuild: bool,
    }

    impl LintRule for MeasuredRule {
        fn head_filter(&self) -> HeadFilter {
            RULE.head_filter()
        }

        fn dialect_scope(&self) -> RuleDialectScope {
            RULE.dialect_scope()
        }

        fn check(
            &self,
            context: &RuleContext<'_>,
            view: &ExpressionView,
            sink: &mut RuleSink<'_, '_>,
        ) -> LintResult {
            let (result, elapsed) = measure_analysis(|| {
                if self.rebuild {
                    let root = context.tree().root_view();
                    RULE.check(context, &root, sink)
                } else {
                    RULE.check(context, view, sink)
                }
            });
            RULE_TIMING.with(|timing| {
                let (duration, calls) = timing.get();
                timing.set((duration + elapsed, calls + 1));
            });
            result
        }
    }

    static MEASURED_RULES: [MeasuredRule; 2] = [
        MeasuredRule { rebuild: false },
        MeasuredRule { rebuild: true },
    ];
    static MEASURED_ENTRIES: [[RuleEntry; 1]; 2] = [
        [RuleEntry::new(&META, &MEASURED_RULES[0])],
        [RuleEntry::new(&META, &MEASURED_RULES[1])],
    ];

    fn measure_analysis<T>(analysis: impl FnOnce() -> T) -> (T, Duration) {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let started = thread_cpu_time();
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let started = std::time::Instant::now();
        let result = analysis();
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let elapsed = thread_cpu_time()
            .checked_sub(started)
            .expect("thread CPU clock is monotonic");
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let elapsed = started.elapsed();
        (result, elapsed)
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[allow(unsafe_code)]
    fn thread_cpu_time() -> Duration {
        let mut time = std::mem::MaybeUninit::<libc::timespec>::uninit();
        // SAFETY: the live timespec is initialized on success and read only then.
        let result =
            unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, time.as_mut_ptr()) };
        assert_eq!(result, 0, "clock_gettime(CLOCK_THREAD_CPUTIME_ID) failed");
        // SAFETY: clock_gettime returned zero, so time is initialized.
        let time = unsafe { time.assume_init() };
        let seconds = u64::try_from(time.tv_sec).expect("CPU time seconds are nonnegative");
        let nanos = u32::try_from(time.tv_nsec).expect("CPU time nanoseconds are nonnegative");
        assert!(nanos < 1_000_000_000, "normalized timespec nanoseconds");
        Duration::new(seconds, nanos)
    }

    fn measured_rule_path(tree: &SyntaxTree, source: &str, rebuild: bool) -> Duration {
        RULE_TIMING.with(|timing| timing.set((Duration::ZERO, 0)));
        let catalog = RuleCatalog::new(&MEASURED_ENTRIES[usize::from(rebuild)]);
        let index = build_head_index(catalog);
        let outcomes = collect_lint_outcomes(
            catalog,
            &index,
            Path::new("t.lisp"),
            Dialect::CommonLisp,
            tree,
            source,
            RuleSelection::All,
        )
        .expect("run actual rule");
        assert!(outcomes.is_empty());
        let (elapsed, calls) = RULE_TIMING.with(Cell::get);
        assert_eq!(calls, 1, "WholeTree must dispatch exactly once");
        elapsed
    }

    fn findings(input: &str, dialect: Dialect) -> Vec<DuplicateTestNameItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse input");
        build_duplicate_test_name_report(Path::new("test.lisp"), dialect, &tree)
            .expect("build report")
            .findings
    }

    fn names(input: &str, dialect: Dialect) -> Vec<String> {
        findings(input, dialect)
            .into_iter()
            .map(|item| item.test_name)
            .collect()
    }

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_a_repeated_name_in_each_modelled_dialect() {
        assert_eq!(
            names(
                "(deftest adds (is true))\n(deftest adds (is false))\n",
                Dialect::Clojure
            ),
            vec!["adds"]
        );
        assert_eq!(
            names(
                "(ert-deftest adds () (should t))\n(ert-deftest adds () (should nil))\n",
                Dialect::EmacsLisp
            ),
            vec!["adds"]
        );
        assert_eq!(
            names(
                "(def-test adds () (is t))\n(def-test adds () (is nil))\n",
                Dialect::CommonLisp
            ),
            vec!["adds"]
        );
    }

    #[test]
    fn reports_the_later_definition_not_the_earlier_one() {
        let input = "(deftest adds (is true))\n(deftest adds (is false))\n";
        let items = findings(input, Dialect::Clojure);
        assert_eq!(items.len(), 1);
        // The finding sits on the second form and points back at the first.
        assert!(items[0].span.start().get() > items[0].shadowed_offset);
        assert_eq!(items[0].shadowed_offset, 0);
    }

    #[test]
    fn a_third_definition_is_reported_too() {
        assert_eq!(
            names(
                "(deftest adds (is true))\n(deftest adds (is true))\n(deftest adds (is true))\n",
                Dialect::Clojure
            )
            .len(),
            2
        );
    }

    /// A package qualifier and a case difference name the same Common Lisp
    /// symbol.
    #[test]
    fn a_qualified_or_differently_cased_name_is_the_same_name() {
        assert_eq!(
            names(
                "(def-test adds () (is t))\n(def-test ADDS () (is t))\n",
                Dialect::CommonLisp
            ),
            vec!["adds"]
        );
    }

    /// Two different definition macros still define the same test name.
    #[test]
    fn two_different_test_macros_can_still_collide() {
        assert_eq!(
            names(
                "(def-test adds () (is t))\n(define-test adds (assert-true t))\n",
                Dialect::CommonLisp
            ),
            vec!["adds"]
        );
    }

    // -- near misses ---------------------------------------------------------

    #[test]
    fn distinct_names_are_silent() {
        assert!(
            names(
                "(deftest adds (is true))\n(deftest subtracts (is true))\n",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    /// The byte guard clears, but the structural comparison finds one
    /// definition — the second mention is a call.
    #[test]
    fn a_name_mentioned_in_a_body_is_not_a_second_definition() {
        assert!(names("(deftest adds (is (= 3 (adds 1 2))))\n", Dialect::Clojure).is_empty());
    }

    /// One is a test, the other is not.
    #[test]
    fn a_test_and_a_function_of_the_same_name_do_not_collide_here() {
        assert!(
            names(
                "(defn adds [a b] (+ a b))\n(deftest adds (is true))\n",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    /// Only top-level definitions are compared: whether a nested one shadows
    /// depends on when its enclosing form runs.
    #[test]
    fn a_nested_definition_is_not_compared() {
        assert!(
            names(
                "(deftest adds (is true))\n(let [x 1] (deftest adds (is true)))\n",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    #[test]
    fn a_name_that_is_not_a_symbol_is_not_compared() {
        assert!(
            names(
                "(deftest (adds :suite s) (is t))\n(deftest (adds :suite s) (is t))\n",
                Dialect::CommonLisp
            )
            .is_empty()
        );
    }

    #[test]
    fn a_test_using_an_unmodelled_framework_is_silent() {
        assert!(
            names(
                "(fact \"adds\" (+ 1 2) => 3)\n(fact \"adds\" (+ 1 2) => 3)\n",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    /// A name repeated in a comment clears the byte guard and nothing else.
    #[test]
    fn a_name_repeated_in_a_comment_is_not_a_definition() {
        assert!(
            names(
                "; adds is covered below\n(deftest adds (is true))\n",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    // -- quote and string negatives ------------------------------------------

    #[test]
    fn a_quoted_definition_is_data_and_does_not_collide() {
        assert!(
            names(
                "(deftest adds (is true))\n'(deftest adds (is true))\n",
                Dialect::Clojure
            )
            .is_empty()
        );
        assert!(
            names(
                "(deftest adds (is true))\n(quote (deftest adds (is true)))\n",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    /// Written in Common Lisp on purpose: `,` is an unquote there and plain
    /// whitespace in Clojure.
    #[test]
    fn a_comma_inside_a_hard_quote_does_not_make_a_definition_code_again() {
        assert!(
            names(
                "(def-test adds () (is t))\n'(a ,(def-test adds () (is t)))\n",
                Dialect::CommonLisp
            )
            .is_empty()
        );
    }

    /// An unquote *does* escape back to code — Clojure spells it `~` — but the
    /// escaped form sits inside the quasiquoted list, so it is not a root
    /// child and this rule's top-level requirement excludes it anyway.
    ///
    /// This is the one rule here for which "unquoted, therefore code" is not
    /// also "therefore reported", and pinning that keeps the difference from
    /// looking like a bug later.
    #[test]
    fn an_unquoted_definition_is_code_but_is_still_not_top_level() {
        assert!(
            names(
                "(deftest adds (is true))\n`(a ~(deftest adds (is true)))\n",
                Dialect::Clojure
            )
            .is_empty()
        );
        assert!(
            names(
                "(def-test adds () (is t))\n`(a ,(def-test adds () (is t)))\n",
                Dialect::CommonLisp
            )
            .is_empty()
        );
    }

    #[test]
    fn a_quasiquoted_macro_template_does_not_collide_with_a_real_test() {
        assert!(
            names(
                "(deftest adds (is true))\n(defmacro m [] `(deftest adds (is true)))\n",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    #[test]
    fn a_definition_spelled_inside_a_string_does_not_collide() {
        assert!(
            names(
                "(deftest adds (is (= \"(deftest adds ...)\" s)))\n",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    // -- cost ----------------------------------------------------------------

    /// A file of `count` distinct tests, each of which also calls itself — the
    /// shape that cleared the old byte-scan guard and forced the pairwise
    /// comparison for every definition in the file.
    fn many_tests(count: usize, duplicate_of: Option<usize>) -> String {
        let mut source = String::new();
        for index in 0..count {
            source.push_str(&format!("(deftest n{index} (is (= 1 1)) (n{index}))\n"));
        }
        if let Some(index) = duplicate_of {
            source.push_str(&format!("(deftest n{index} (is (= 2 2)))\n"));
        }
        source
    }

    /// The correctness half: the answer on a large file is still exactly the
    /// duplicates and nothing else, with the denominator counting every
    /// definition rather than every finding.
    #[test]
    fn a_large_file_reports_exactly_its_duplicates() {
        let source = many_tests(400, Some(7));
        let tree = SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp).expect("parse");
        let report =
            build_duplicate_test_name_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
                .expect("build report");
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].test_name, "n7");
        // The `(n7)` self-call in the body is not a definition, so the finding
        // points at the earlier `deftest` and not at it.
        assert_eq!(
            report.findings[0].shadowed_offset,
            source
                .find("(deftest n7 ")
                .expect("the first n7 definition")
        );
        assert_eq!(report.summary, vec![("test_form_count", json!(401))]);
    }

    /// The cost half, and the one that fails on the implementation this
    /// replaced.
    ///
    /// A wall clock is a poor assertion and this one is deliberately built so
    /// that only an *asymptotic* regression can trip it: the quadratic version
    /// took ~50s on this input in a debug build on an idle machine and minutes
    /// on a loaded one, against a budget of 20s and a linear cost of well under
    /// one second. Nothing between those two numbers is a plausible outcome, so
    /// a failure here means the per-definition whole-file scan is back rather
    /// than that the machine was busy.
    #[test]
    fn a_file_of_many_tests_stays_linear() {
        let source = many_tests(4000, None);
        let tree = SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp).expect("parse");
        let started = std::time::Instant::now();
        let report =
            build_duplicate_test_name_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
                .expect("build report");
        let elapsed = started.elapsed();
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("test_form_count", json!(4000))]);
        assert!(
            elapsed < std::time::Duration::from_secs(20),
            "4000 test definitions took {elapsed:?}; the analysis is no longer linear"
        );
    }

    // -- the other cost invariant --------------------------------------------

    /// Measure the actual rule's analysis, excluding dispatcher's view creation.
    /// Thread CPU time excludes scheduler waits on Linux and macOS; it does not
    /// eliminate contention costs. Other platforms retain wall-clock timing.
    /// The report also materializes the document and counts every test form.
    #[test]
    fn the_rule_path_does_not_rebuild_the_document() {
        let source = many_tests(4000, None);
        let tree = SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp).expect("parse");
        let rule_path = measured_rule_path(&tree, &source, false);
        let (report, report_path) = measure_analysis(|| {
            build_duplicate_test_name_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
                .expect("build report")
        });

        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("test_form_count", json!(4000))]);
        assert!(
            rule_path * 2 < report_path,
            "the rule path took {rule_path:?} against the report's {report_path:?}; it is doing \
             the report's work again rather than reading the view it was handed"
        );
    }

    #[test]
    fn the_cost_gate_rejects_rebuilding_the_document() {
        let source = many_tests(4000, None);
        let tree = SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp).expect("parse");
        let rebuilding_path = measured_rule_path(&tree, &source, true);
        let (report, report_path) = measure_analysis(|| {
            build_duplicate_test_name_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
                .expect("build report")
        });
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("test_form_count", json!(4000))]);
        assert!(
            rebuilding_path * 2 >= report_path,
            "the unchanged half-of gate accepted one root_view rebuild: \
             {rebuilding_path:?} against the report's {report_path:?}"
        );
    }

    /// And it must still answer the same thing. Equivalence is what stops the
    /// cost invariant above from being satisfied by a rule that has quietly
    /// stopped finding things.
    #[test]
    fn the_rule_path_and_the_report_path_agree() {
        for source in [
            "(deftest a)(deftest a)",
            "(deftest a)(deftest b)",
            "(deftest a)(deftest b)(deftest a)(deftest b)",
            "'(deftest a)(deftest a)",
            "(deftest a (deftest a))",
            "",
            "(defun f () 1)",
        ] {
            let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
            let report =
                build_duplicate_test_name_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
                    .expect("build report");
            let direct = shadowing_test_definitions(&tree.root_view(), Dialect::CommonLisp);
            assert_eq!(
                report
                    .findings
                    .iter()
                    .map(|item| (item.span, item.test_name.clone(), item.shadowed_offset))
                    .collect::<Vec<_>>(),
                direct
                    .iter()
                    .map(|item| (item.span, item.test_name.clone(), item.shadowed_offset))
                    .collect::<Vec<_>>(),
                "the rule and the report disagree on `{source}`"
            );
            let expected = report
                .findings
                .iter()
                .map(|item| (item.span, item.message(), item.kind()))
                .collect::<Vec<_>>();
            for entries in &MEASURED_ENTRIES {
                RULE_TIMING.with(|timing| timing.set((Duration::ZERO, 0)));
                let catalog = RuleCatalog::new(entries);
                let index = build_head_index(catalog);
                let actual = collect_lint_outcomes(
                    catalog,
                    &index,
                    Path::new("t.lisp"),
                    Dialect::CommonLisp,
                    &tree,
                    source,
                    RuleSelection::All,
                )
                .expect("run actual rule")
                .into_iter()
                .map(|outcome| {
                    let (finding, fix) = outcome.into_parts();
                    assert!(fix.is_none(), "duplicate names are report-only");
                    (finding.span, finding.message, finding.rule)
                })
                .collect::<Vec<_>>();
                assert_eq!(actual, expected, "actual rule disagrees on `{source}`");
                assert_eq!(RULE_TIMING.with(Cell::get).1, 1);
            }
        }
    }

    // -- report envelope -----------------------------------------------------

    #[test]
    fn an_unmodelled_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(deftest a)(deftest a)", Dialect::Scheme)
            .expect("parse");
        let report = build_duplicate_test_name_report(Path::new("a.scm"), Dialect::Scheme, &tree)
            .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_finding_carries_its_line_and_the_name_it_shadows() {
        let tree = SyntaxTree::parse_with_dialect(
            "(deftest adds\n  (is true))\n(deftest adds\n  (is false))\n",
            Dialect::Clojure,
        )
        .expect("parse");
        let report = build_duplicate_test_name_report(Path::new("t.clj"), Dialect::Clojure, &tree)
            .expect("build report");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "duplicate-test-name");
        assert_eq!(finding.text_columns()[0], "test=adds");
        assert_eq!(report.summary, vec![("test_form_count", json!(2))]);
    }
}

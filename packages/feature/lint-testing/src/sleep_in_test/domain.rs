//! `sleep-in-test` detection: a wall-clock sleep inside a test body, which
//! makes the test's outcome depend on how loaded the machine running it is.
//!
//! A sleep is how a test waits for something it cannot observe directly. It
//! fails on a slow CI runner and passes locally, and it costs its duration on
//! every run whether or not it was needed — which is why it is worth reporting
//! even though the code is perfectly well-formed.
//!
//! # What this attempts
//!
//! A call to a per-dialect sleep function *sequenced* in the evaluated code of
//! a readable test definition. The finding is placed on the *call*, not on the
//! test, because that is what a reader has to go and look at.
//!
//! Every spelling was verified against the language: `sleep` is a
//! `COMMON-LISP` function, `sleep-for` and `sit-for` are subrs in Emacs, and
//! `Thread/sleep` is Java interop rather than a Clojure var — so it is matched
//! as the host-interop symbol it is, in both its bare and fully-qualified
//! spellings.
//!
//! # What this does not attempt
//!
//! - **A sleep that is the subject under test.** In
//!   `(is (eq :timeout (with-timeout (0.01) (sleep 5))))` the sleep is the
//!   workload handed to the thing being asserted, and the assertion holds
//!   precisely *because* the timeout fires before the sleep finishes — the
//!   test's outcome is deliberately independent of the sleep's duration, which
//!   is the opposite of what this rule is about. Clojure's
//!   `(is (= :timeout (deref (future (Thread/sleep 5000)) 10 :timeout)))` is the
//!   canonical spelling of the same thing, so without this every timeout,
//!   debounce, rate-limiter and retry-backoff library would get a finding on
//!   its own suite. A sleep anywhere below an assertion form is therefore not
//!   reported; a sleep *sequenced beside* one still is.
//!
//! - **A `sit-for` that is a poll interval.** `sit-for` is not a sleep: it
//!   redisplays and returns `nil` as soon as input arrives, which is why it is
//!   the only spelling Emacs has for the poll loop this rule's header
//!   recommends *instead* of a sleep. Inside a `while`/`dotimes`/`cl-loop`,
//!   where the loop condition and not the clock decides when waiting stops, it
//!   is that poll loop and is not reported. `sleep-for` in the same position
//!   still is: it cannot return early, so every iteration burns its full
//!   duration whether or not the thing being waited for has arrived. This is
//!   the same judgement that already keeps `accept-process-output` off the
//!   list below.
//!
//! - **A zero-length sleep.** `(sit-for 0)` is the Emacs idiom for forcing a
//!   redisplay and waits for nothing; so is `(sleep 0)`. A literal zero
//!   duration is never reported.
//! - **A computed duration of zero.** `(sleep delay)` where `delay` happens to
//!   be `0` is still reported: this rule reads literals, not values.
//! - **A sleep outside a test.** Production code may have every reason to
//!   sleep. This rule anchors on the test definition and looks inward.
//! - **Suggesting what to wait on instead.** `Fixability::ReportOnly`: the
//!   answer is a condition variable, a poll loop, or a fake clock, and which
//!   one is not derivable from the source.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, list_head};
use serde_json::{Value, json};

use crate::support::{
    TEST_DIALECTS, assertion_heads, calls_any, for_each_evaluated_subview, read_test_form,
    span_contains,
};

#[derive(Debug, Clone)]
pub struct SleepInTestItem {
    /// The span of the sleeping call itself.
    pub span: ByteSpan,
    /// The enclosing test's name.
    pub test_name: String,
    /// The sleeping function as written, before case folding.
    pub head: String,
}

impl Finding for SleepInTestItem {
    fn kind(&self) -> &'static str {
        "sleep-in-test"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("test={}", self.test_name),
            format!("call={}", self.head),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("test", json!(self.test_name)), ("call", json!(self.head))]
    }

    fn message(&self) -> String {
        format!(
            "{} inside test {}; the test's outcome depends on wall-clock timing",
            self.head, self.test_name
        )
    }
}

/// The sleeping calls this rule recognizes for `dialect`.
///
/// `Thread/sleep` is listed under both spellings because it is Java interop,
/// not a namespaced Clojure var: neither `unqualified` nor a namespace alias
/// reduces one spelling to the other.
const fn sleep_heads(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::CommonLisp => &["sleep"],
        Dialect::EmacsLisp => &["sleep-for", "sit-for"],
        Dialect::Clojure => &["Thread/sleep", "java.lang.Thread/sleep"],
        _ => &[],
    }
}

/// The waits that stop early once the thing being waited for arrives, and so
/// read as a poll interval rather than as a duration when one is the body of a
/// loop.
///
/// `sit-for` alone. Emacs' other waits either always burn their full argument
/// (`sleep-for`) or are already absent from [`sleep_heads`] because they are
/// unambiguously event waits (`accept-process-output`).
const POLL_INTERVAL_HEADS: [&str; 1] = ["sit-for"];

/// The loop forms whose condition, rather than the clock, decides when waiting
/// stops.
///
/// Emacs Lisp only: it is the dialect whose poll interval is spelled with a
/// wait this rule otherwise recognizes. A Clojure `(dotimes [i n]
/// (Thread/sleep 10))` sleeps `n` times over and is not a poll loop.
const fn poll_loop_heads(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::EmacsLisp => &[
            "while",
            "dotimes",
            "dolist",
            "cl-loop",
            "cl-do",
            "cl-do*",
            "cl-dotimes",
            "cl-dolist",
        ],
        _ => &[],
    }
}

/// Whether a call's first argument is a literal zero duration, which waits for
/// nothing and is a redisplay idiom rather than a sleep.
fn sleeps_for_zero(call: &ExpressionView) -> bool {
    call.children
        .get(1)
        .and_then(atom_text)
        .and_then(|text| text.parse::<f64>().ok())
        .is_some_and(|duration| duration == 0.0)
}

/// Whether `inner` sits strictly below one of `outer`.
///
/// The spans are collected by the same pre-order walk that finds `inner`, so
/// every enclosing form has already been recorded by the time this is asked; a
/// span that is not an ancestor is a preceding sibling's, and one of those
/// cannot contain `inner`.
fn nested_in(outer: &[ByteSpan], inner: ByteSpan) -> bool {
    outer
        .iter()
        .any(|candidate| *candidate != inner && span_contains(*candidate, inner))
}

/// Examines one node. The caller guarantees `view` is evaluated code.
pub fn examine_test(
    view: &ExpressionView,
    dialect: Dialect,
    test_form_count: &mut usize,
    violations: &mut Vec<SleepInTestItem>,
) {
    let Some(form) = read_test_form(view, dialect) else {
        return;
    };
    let Some(test_name) = form.name_text() else {
        return;
    };
    *test_form_count += 1;

    let heads = sleep_heads(dialect);
    let assertions = assertion_heads(dialect);
    let loops = poll_loop_heads(dialect);
    let mut enclosing_assertions: Vec<ByteSpan> = Vec::new();
    let mut enclosing_loops: Vec<ByteSpan> = Vec::new();

    for body_form in form.body {
        for_each_evaluated_subview(body_form, |candidate| {
            if calls_any(candidate, assertions) {
                enclosing_assertions.push(candidate.span);
            }
            if calls_any(candidate, loops) {
                enclosing_loops.push(candidate.span);
            }
            if !calls_any(candidate, heads) || sleeps_for_zero(candidate) {
                return;
            }
            // The sleep is an argument of what is being asserted, so the
            // assertion's outcome does not depend on how long it takes.
            if nested_in(&enclosing_assertions, candidate.span) {
                return;
            }
            // A wait that returns early, used as a loop's interval, is the poll
            // loop this rule recommends rather than the wait it reports.
            if calls_any(candidate, &POLL_INTERVAL_HEADS)
                && nested_in(&enclosing_loops, candidate.span)
            {
                return;
            }
            let Some(head) = list_head(candidate) else {
                return;
            };
            violations.push(SleepInTestItem {
                span: candidate.span,
                test_name: test_name.clone(),
                head: head.to_owned(),
            });
        });
    }
}

/// Collects every sleeping call inside a test in one file, with the number of
/// readable test definitions beside them.
pub fn build_sleep_in_test_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SleepInTestItem>> {
    let modelled = TEST_DIALECTS.contains(&dialect);
    let mut test_form_count = 0;
    let mut violations = Vec::new();

    if modelled {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            if list_head(view).is_some() {
                examine_test(view, dialect, &mut test_form_count, &mut violations);
            }
        });
    }

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

    fn findings(input: &str, dialect: Dialect) -> Vec<SleepInTestItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse input");
        build_sleep_in_test_report(Path::new("test.lisp"), dialect, &tree)
            .expect("build report")
            .findings
    }

    fn calls(input: &str, dialect: Dialect) -> Vec<String> {
        findings(input, dialect)
            .into_iter()
            .map(|item| item.head)
            .collect()
    }

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_a_sleep_in_each_modelled_dialect() {
        assert_eq!(
            calls("(def-test waits () (sleep 1) (is t))", Dialect::CommonLisp),
            vec!["sleep"]
        );
        assert_eq!(
            calls(
                "(ert-deftest waits () (sleep-for 1) (should t))",
                Dialect::EmacsLisp
            ),
            vec!["sleep-for"]
        );
        assert_eq!(
            calls(
                "(deftest waits (Thread/sleep 100) (is true))",
                Dialect::Clojure
            ),
            vec!["Thread/sleep"]
        );
    }

    #[test]
    fn flags_a_sleep_nested_deep_in_the_body() {
        assert_eq!(
            calls(
                "(deftest waits (let [n 3] (dotimes [i n] (Thread/sleep 10))) (is true))",
                Dialect::Clojure
            ),
            vec!["Thread/sleep"]
        );
    }

    #[test]
    fn flags_the_fully_qualified_java_spelling() {
        assert_eq!(
            calls(
                "(deftest waits (java.lang.Thread/sleep 100) (is true))",
                Dialect::Clojure
            ),
            vec!["java.lang.Thread/sleep"]
        );
    }

    #[test]
    fn flags_a_sit_for_with_a_real_duration() {
        assert_eq!(
            calls("(ert-deftest waits () (sit-for 0.5))", Dialect::EmacsLisp),
            vec!["sit-for"]
        );
    }

    #[test]
    fn reports_each_sleep_separately() {
        assert_eq!(
            calls(
                "(deftest waits (Thread/sleep 10) (is true) (Thread/sleep 20))",
                Dialect::Clojure
            )
            .len(),
            2
        );
    }

    // -- near misses ---------------------------------------------------------

    /// The Emacs redisplay idiom, which waits for nothing.
    #[test]
    fn a_zero_length_wait_is_not_a_sleep() {
        assert!(calls("(ert-deftest waits () (sit-for 0))", Dialect::EmacsLisp).is_empty());
        assert!(calls("(ert-deftest waits () (sleep-for 0.0))", Dialect::EmacsLisp).is_empty());
        assert!(calls("(def-test waits () (sleep 0))", Dialect::CommonLisp).is_empty());
    }

    // -- the sleep is the subject under test ---------------------------------

    /// The FiveAM shape: the assertion holds because the timeout fires *before*
    /// the sleep finishes, so the test's outcome does not depend on the clock
    /// the way this rule's message claims.
    #[test]
    fn a_sleep_the_assertion_is_about_is_not_a_timing_dependency() {
        assert!(
            calls(
                "(def-test with-timeout-fires ()\n  (is (eq :timeout (my-pkg:with-timeout (0.01) (sleep 5)))))",
                Dialect::CommonLisp
            )
            .is_empty()
        );
    }

    /// The canonical Clojure spelling of a timeout test. Without this, every
    /// timeout, debounce, rate-limiter and retry-backoff library gets a finding
    /// on its own suite.
    #[test]
    fn a_deref_with_timeout_is_not_a_timing_dependency() {
        assert!(
            calls(
                "(deftest deref-timeout-fires\n  (is (= :timeout (deref (future (Thread/sleep 5000)) 10 :timeout))))",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    /// The boundary: an assertion *beside* the sleep does not excuse it. This
    /// is the rate-limiter test the rule still exists for.
    #[test]
    fn a_sleep_sequenced_beside_an_assertion_is_still_reported() {
        assert_eq!(
            calls(
                "(def-test rate-limited ()\n  (my-pkg:request)\n  (sleep 1.1)\n  (is (my-pkg:request)))",
                Dialect::CommonLisp
            ),
            vec!["sleep"]
        );
        assert_eq!(
            calls(
                "(deftest rate-limited (request) (Thread/sleep 1100) (is (request)))",
                Dialect::Clojure
            ),
            vec!["Thread/sleep"]
        );
    }

    // -- sit-for as a poll interval ------------------------------------------

    /// A bounded poll loop, which is the alternative this rule's own header
    /// recommends, in the only spelling Emacs has for it.
    #[test]
    fn a_sit_for_that_is_a_poll_interval_is_not_a_sleep() {
        assert!(
            calls(
                "(ert-deftest my-pkg-bounded-poll ()\n  (let ((deadline (+ (float-time) 2.0)))\n    (while (and (not (my-pkg-ready-p)) (< (float-time) deadline))\n      (sit-for 0.01))\n    (should (my-pkg-ready-p))))",
                Dialect::EmacsLisp
            )
            .is_empty()
        );
        assert!(
            calls(
                "(ert-deftest my-pkg-poll ()\n  (cl-loop until (my-pkg-ready-p) do (sit-for 0.01))\n  (should t))",
                Dialect::EmacsLisp
            )
            .is_empty()
        );
    }

    /// The boundary, twice over: `sleep-for` cannot return early, so the same
    /// loop with it is still a wall-clock wait; and a `sit-for` outside any
    /// loop is a duration rather than an interval.
    #[test]
    fn only_a_wait_that_returns_early_reads_as_a_poll_interval() {
        assert_eq!(
            calls(
                "(ert-deftest waits ()\n  (while (not (my-pkg-ready-p))\n    (sleep-for 0.01))\n  (should t))",
                Dialect::EmacsLisp
            ),
            vec!["sleep-for"]
        );
        assert_eq!(
            calls(
                "(ert-deftest waits () (sit-for 0.5) (should t))",
                Dialect::EmacsLisp
            ),
            vec!["sit-for"]
        );
    }

    #[test]
    fn a_sleep_outside_any_test_is_not_this_rules_subject() {
        assert!(calls("(defun waits () (sleep 1))", Dialect::CommonLisp).is_empty());
        assert!(calls("(defn waits [] (Thread/sleep 100))", Dialect::Clojure).is_empty());
    }

    #[test]
    fn a_sleep_spelling_from_another_dialect_is_not_recognized() {
        // `sleep-for` is Emacs Lisp; a Common Lisp function of that name is
        // some project's own.
        assert!(calls("(def-test waits () (sleep-for 1))", Dialect::CommonLisp).is_empty());
        // `sleep` is Common Lisp; Clojure has no such core var.
        assert!(calls("(deftest waits (sleep 1) (is true))", Dialect::Clojure).is_empty());
    }

    #[test]
    fn a_similarly_named_call_is_not_a_sleep() {
        assert!(
            calls(
                "(deftest waits (sleep-until-ready) (is true))",
                Dialect::Clojure
            )
            .is_empty()
        );
        assert!(calls("(def-test waits () (sleepy 1))", Dialect::CommonLisp).is_empty());
    }

    // -- quote and string negatives ------------------------------------------

    #[test]
    fn a_quoted_test_form_is_data_and_is_not_flagged() {
        assert!(calls("'(deftest waits (Thread/sleep 1))", Dialect::Clojure).is_empty());
        assert!(calls("(quote (deftest waits (Thread/sleep 1)))", Dialect::Clojure).is_empty());
    }

    /// The sleep is data even though the test around it is code.
    #[test]
    fn a_sleep_inside_quoted_data_in_a_real_test_is_not_a_call() {
        assert!(
            calls(
                "(deftest waits (is (= '(Thread/sleep 1) form)))",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    /// Written in Common Lisp on purpose: `,` is an unquote there and plain
    /// whitespace in Clojure.
    #[test]
    fn a_comma_inside_a_hard_quote_does_not_make_a_sleep_code_again() {
        assert!(calls("'(a ,(def-test waits () (sleep 1)))", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_quasiquoted_macro_template_is_data() {
        assert!(
            calls(
                "(defmacro m [n] `(deftest ~n (Thread/sleep 1)))",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    /// Clojure spells unquote `~`; a `,` there is whitespace.
    #[test]
    fn an_unquoted_sleep_inside_a_quasiquote_is_code() {
        assert_eq!(
            calls("`(a ~(deftest waits (Thread/sleep 1)))", Dialect::Clojure),
            vec!["Thread/sleep"]
        );
        assert_eq!(
            calls("`(a ,(def-test waits () (sleep 1)))", Dialect::CommonLisp),
            vec!["sleep"]
        );
    }

    #[test]
    fn a_sleep_spelled_inside_a_string_is_not_a_call() {
        assert!(
            calls(
                "(deftest waits (is (= \"(Thread/sleep 100)\" s)))",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    // -- report envelope -----------------------------------------------------

    #[test]
    fn an_unmodelled_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(deftest waits (sleep 1))", Dialect::Scheme)
            .expect("parse");
        let report = build_sleep_in_test_report(Path::new("a.scm"), Dialect::Scheme, &tree)
            .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_finding_names_its_test_and_its_call() {
        let tree = SyntaxTree::parse_with_dialect(
            "(ns app)\n(deftest waits\n  (Thread/sleep 100)\n  (is true))\n",
            Dialect::Clojure,
        )
        .expect("parse");
        let report = build_sleep_in_test_report(Path::new("t.clj"), Dialect::Clojure, &tree)
            .expect("build report");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "sleep-in-test");
        assert_eq!(
            finding.json_fields(),
            vec![("test", json!("waits")), ("call", json!("Thread/sleep")),]
        );
    }
}

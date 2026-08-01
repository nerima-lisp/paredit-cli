//! Common Lisp `loop` dead-epilogue detection: an epilogue form written after
//! a `finally` clause that already returns.
//!
//! Every `finally` clause's compound forms are collected, in order, into one
//! epilogue that runs once when the loop ends. A `(return …)` or
//! `(return-from …)` written directly in a `finally` clause leaves the loop at
//! that point, so every epilogue form after it — whether in the same `finally`
//! clause or a later one — can never run.
//!
//! ```text
//! (loop for x in items
//!       finally (return (length items))
//!               (print "done"))          ; dead
//! ```
//!
//! # What this rule deliberately does not flag
//!
//! - **A non-`finally` clause written after a returning `finally`.** This is
//!   the tempting extension and it would be wrong. `finally` does not have to
//!   be the last clause; the CLHS grammar admits `initial-final` as both a
//!   variable clause and a main clause. In
//!   `(loop for x in items finally (return 1) collect x)` the `collect` is
//!   still evaluated on every iteration — the accumulated list is merely
//!   discarded. That is a different complaint, and calling it unreachable
//!   would be false.
//! - **A conditional return**: only a `(return …)`/`(return-from …)` that is a
//!   *direct* compound form of the `finally` clause counts. A guarded
//!   `(when p (return 1))` may or may not exit, so nothing after it is
//!   provably dead.
//! - **`loop-finish`**, which runs the epilogue rather than skipping it.
//! - **`initially` clauses after a returning `finally`.** Their forms run
//!   before the loop body, not in the epilogue, so they are perfectly alive.
//! - **Anything in a form [`crate::loop_syntax`] declines to read.**
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::render_expression;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{list_head, unqualified};
use serde_json::{Value, json};

use crate::loop_syntax::LoopScan;
use crate::support::for_each_evaluated_subview;

#[derive(Debug, Clone)]
pub struct LoopUnreachableFinallyItem {
    /// The span of the epilogue form that can never run.
    pub span: ByteSpan,
    /// That form, re-rendered from the tree.
    pub form: String,
    /// The operator that already left the loop (`return` or `return-from`).
    pub after: String,
}

impl Finding for LoopUnreachableFinallyItem {
    fn kind(&self) -> &'static str {
        "loop-unreachable-finally-clause"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("form={}", self.form),
            format!("after={}", self.after),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("form", json!(self.form)), ("after", json!(self.after))]
    }

    /// The same sentence the `loop-unreachable-finally-clause` lint rule
    /// writes, so a SARIF or JUnit consumer reading both sees one finding
    /// described one way.
    fn message(&self) -> String {
        format!(
            "loop epilogue form {} can never run: an earlier finally clause already \
             left the loop with `{}`",
            self.form, self.after
        )
    }
}

/// The operator of an unconditional loop exit written directly in a `finally`
/// clause, or `None` for any other form.
fn unconditional_exit(view: &ExpressionView) -> Option<String> {
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    let head = list_head(view)?;
    let head = unqualified(head).to_ascii_lowercase();
    (head == "return" || head == "return-from").then_some(head)
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_loop_unreachable_finally(
    view: &ExpressionView,
    scan: &mut LoopScan,
    violations: &mut Vec<LoopUnreachableFinallyItem>,
) {
    let Some(clauses) = scan.read(view) else {
        return;
    };

    // The exit operator an earlier `finally` clause already used, if any.
    let mut exited: Option<String> = None;
    let mut in_finally = false;

    for token in &clauses.tokens {
        if let Some(keyword) = token.keyword() {
            // Any clause keyword closes the current `finally`; only another
            // `finally` re-opens the epilogue.
            in_finally = keyword == "finally";
            continue;
        }
        if !in_finally {
            continue;
        }

        match &exited {
            Some(after) => violations.push(LoopUnreachableFinallyItem {
                span: token.span(),
                form: render_expression(token.view),
                after: after.clone(),
            }),
            None => exited = unconditional_exit(token.view),
        }
    }
}

/// Collects every dead `loop` epilogue form in one file, with the number of
/// `loop` forms scanned and the number actually modelled beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every epilogue form can run" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_loop_unreachable_finally_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<LoopUnreachableFinallyItem>> {
    let mut scan = LoopScan::default();
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        for index in 0..tree.root_children().len() {
            let view = tree.select_path(&SexprPath::root_child(index))?.view();
            for_each_evaluated_subview(&view, |subview| {
                examine_loop_unreachable_finally(subview, &mut scan, &mut violations);
            });
        }
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::CommonLisp,
        tree.source(),
        violations,
        vec![
            ("loop_form_count", json!(scan.loop_form_count)),
            (
                "modelled_loop_form_count",
                json!(scan.modelled_loop_form_count),
            ),
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<LoopUnreachableFinallyItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_loop_unreachable_finally_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build loop unreachable finally report")
    }

    fn violations(input: &str) -> Vec<LoopUnreachableFinallyItem> {
        report(input).findings
    }

    #[test]
    fn flags_a_form_after_a_return_in_the_same_finally_clause() {
        let found = violations("(loop for x in items finally (return x) (print \"unreachable\"))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].after, "return");
        assert!(found[0].form.contains("print"));
    }

    #[test]
    fn flags_a_later_finally_clause_after_a_returning_one() {
        let found = violations("(loop for x in items finally (return x) finally (cleanup))");
        assert_eq!(found.len(), 1);
        assert!(found[0].form.contains("cleanup"));
    }

    #[test]
    fn flags_every_dead_form_after_the_return() {
        let found = violations("(loop for x in items finally (return x) (a) (b))");
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn recognises_return_from_as_an_exit() {
        let found =
            violations("(loop named outer for x in items finally (return-from outer x) (a))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].after, "return-from");
    }

    #[test]
    fn does_not_flag_a_finally_whose_return_is_last() {
        assert!(violations("(loop for x in items finally (return x))").is_empty());
        assert!(violations("(loop for x in items finally (log) (return x))").is_empty());
    }

    /// The extension this rule must refuse to make: a `collect` after a
    /// returning `finally` still runs on every iteration.
    #[test]
    fn does_not_flag_a_non_finally_clause_after_a_returning_finally() {
        assert!(violations("(loop for x in items finally (return 1) collect x)").is_empty());
        assert!(
            violations("(loop for x in items finally (return 1) do (side-effect x))").is_empty()
        );
    }

    /// `initially` forms run before the body, never in the epilogue.
    #[test]
    fn does_not_flag_an_initially_clause_after_a_returning_finally() {
        assert!(
            violations("(loop for x in items finally (return 1) initially (setup))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_conditional_return() {
        assert!(
            violations("(loop for x in items finally (when (p x) (return x)) (cleanup))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_loop_finish_which_runs_the_epilogue() {
        assert!(violations("(loop for x in items finally (loop-finish) (cleanup))").is_empty());
    }

    #[test]
    fn does_not_flag_a_realistic_extended_loop() {
        let found = violations(
            "(loop for x from 1 to 10\n\
             \x20     with total = 0\n\
             \x20     while (< x 8)\n\
             \x20     collect x into taken\n\
             \x20     finally (setf total (length taken))\n\
             \x20             (return (values taken total)))",
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn does_not_flag_a_loop_with_when_and_else() {
        let found = violations(
            "(loop for x in items\n\
             \x20     when (evenp x) collect x into evens\n\
             \x20     else collect x into odds\n\
             \x20     end\n\
             \x20     finally (return (list evens odds)))",
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn says_nothing_about_a_being_the_hash_keys_loop() {
        let report =
            report("(loop for k being the hash-keys of table finally (return k) (cleanup))");
        assert!(report.findings.is_empty());
        assert_eq!(
            report.summary,
            vec![
                ("loop_form_count", json!(1)),
                ("modelled_loop_form_count", json!(0)),
            ]
        );
    }

    #[test]
    fn does_not_flag_a_quoted_loop() {
        assert!(violations("(defparameter *f* '(loop finally (return 1) (dead)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_quasiquoted_loop_with_no_unquote() {
        assert!(violations("(defmacro m () `(loop finally (return 1) (dead)))").is_empty());
    }

    /// The quote need not sit on the `loop` itself: a target nested *inside*
    /// quoted data is still data.
    #[test]
    fn does_not_flag_a_loop_nested_inside_quoted_data() {
        assert!(
            violations("(defparameter *f* '(progn (loop finally (return 1) (dead))))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_loop_nested_inside_a_macro_template() {
        assert!(
            violations(
                "(defmacro m (&body body)\n  `(progn (loop finally (return 1) (dead)) ,@body))"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_clause_keywords_inside_a_string_literal() {
        assert!(violations("(defparameter *doc* \"loop finally (return 1) (dead)\")").is_empty());
    }

    #[test]
    fn does_not_flag_a_loop_whose_variable_is_named_finally() {
        // `finally` here is a `for` variable, not a clause.
        assert!(violations("(loop for finally in items collect finally)").is_empty());
    }

    #[test]
    fn finds_a_dead_epilogue_form_nested_in_a_function_body() {
        let found =
            violations("(defun f (items)\n  (loop for x in items finally (return x) (dead)))");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(loop for x in items finally (return x) (dead))",
            Dialect::Clojure,
        )
        .expect("parse");
        let report =
            build_loop_unreachable_finally_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build loop unreachable finally report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(loop for x in items collect x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_fields() {
        let report =
            report("(defun f (items)\n  (loop for x in items finally (return x) (cleanup)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "loop-unreachable-finally-clause");
        assert_eq!(
            finding.json_fields(),
            vec![("form", json!("(cleanup)")), ("after", json!("return"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["form=(cleanup)".to_owned(), "after=return".to_owned()]
        );
        assert_eq!(
            finding.message(),
            "loop epilogue form (cleanup) can never run: an earlier finally clause \
             already left the loop with `return`"
        );
    }

    #[test]
    fn the_summary_counts_every_loop_scanned_not_only_the_flagged_ones() {
        let report = report(
            "(loop finally (return 1) (dead) for x in items)\n(loop for y in items collect y)\n",
        );
        assert_eq!(
            report.summary,
            vec![
                ("loop_form_count", json!(2)),
                ("modelled_loop_form_count", json!(2)),
            ]
        );
        assert_eq!(report.findings.len(), 1);
    }
}

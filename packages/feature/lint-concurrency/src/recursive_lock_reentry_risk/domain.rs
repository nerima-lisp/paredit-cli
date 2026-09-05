//! The same non-recursive lock taken again inside its own scope.
//!
//! **This is a heuristic, and the finding says so.** A plain
//! `bordeaux-threads` lock is not reentrant: a thread that already holds one
//! and asks for it again blocks on itself, forever. When the two acquisitions
//! are written one inside the other and name the same symbol, that is very
//! often what happens — but not always, because the inner form may sit in a
//! `lambda` that some other thread runs later, or be guarded by a test that is
//! never true on this path. This rule reports a *risk*, not a proven deadlock,
//! and its message is phrased that way on purpose.
//!
//! # The shape it requires
//!
//! An outer lock form — `with-lock-held`, `with-mutex`, `acquire-lock` or
//! `grab-mutex` — whose subtree contains another lock form naming the **same
//! bare symbol**. Both designators must be plain symbols: `(with-lock-held
//! ((lock-of x)) …)` names nothing this can compare, and two computed
//! designators that happen to look alike are not evidence that they are the
//! same lock.
//!
//! # What it excludes, and why
//!
//! - **Recursive locks.** `with-recursive-lock-held` (`bordeaux-threads`) and
//!   `with-recursive-lock` (`sb-thread`) exist to be reentered. If either side
//!   of the nesting is one of them, nothing is reported.
//! - **Clojure's `locking`.** It compiles to a JVM `monitorenter`, and JVM
//!   monitors are reentrant — `(locking o (locking o …))` is correct Clojure.
//!   This is why the rule is Common Lisp only despite `locking` being in the
//!   shared lock vocabulary.
//! - **A lock taken inside a nested closure.** The walk stops at `lambda`,
//!   `flet`, `labels` and `defun`. A closure written inside a lock scope is
//!   usually *stored* rather than called — registering a locking callback under
//!   the registry lock is a common, correct shape — and whether it runs on this
//!   thread, later, or never is not visible here. The cost is that a genuine
//!   same-thread reentry through an immediately-applied lambda (`(mapcar
//!   (lambda (x) (with-lock-held (*l*) …)) xs)`) is missed. For a rule that is
//!   already only a heuristic, a missed case is much cheaper than a wrong one.
//!
//!   This subsumes the nested-thread case, which is why there is no separate
//!   guard for it: a Common Lisp thread body is *always* a closure, so
//!   `(with-lock-held (*l*) (make-thread (lambda () (with-lock-held (*l*) …))))`
//!   stops at the `lambda` before the spawn matters. A dedicated
//!   `make-thread` check was written, found to be unreachable by any test that
//!   the closure check did not already cover, and removed rather than left as
//!   code nothing could exercise.
//!
//! # What it does not attempt
//!
//! - **Reentry through a function call.** `(with-lock-held (*l*) (helper))`
//!   where `helper` takes `*l*` too is the more common real deadlock, and it is
//!   not visible from one form. Same-file, same-form nesting is what can be
//!   established syntactically.
//! - **Deciding whether the path is reachable.** See above; hence "risk".
//! - **Two names for one lock.** `*l*` and a local alias bound to it read as
//!   different locks.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use serde_json::{Value, json};

use crate::support::{
    LOCK_SCOPE_HEADS, MANUAL_ACQUIRE_HEADS, REENTRANT_LOCK_SCOPE_HEADS, for_each_evaluated_subview,
    for_each_evaluated_subview_where, head_is, locked_designator,
};

/// The lock-taking forms this rule anchors on and searches for.
///
/// Clojure's `locking` is deliberately not here even though it is a lock scope:
/// JVM monitors are reentrant, so nesting one is correct.
pub const NON_REENTRANT_LOCK_HEADS: &[&str] = &[
    "with-lock-held",
    "with-mutex",
    "with-locked-hash-table",
    "acquire-lock",
    "grab-mutex",
];

/// Forms whose body is stored rather than run where it is written, so a lock
/// taken inside one is not taken by the thread holding the outer lock — at
/// least, not provably.
const DEFERRED_BODY_HEADS: &[&str] = &["lambda", "fn", "fn*", "defun", "flet", "labels"];

#[derive(Debug, Clone)]
pub struct RecursiveLockReentryRiskItem {
    /// The span of the *inner* acquisition — the one that would block.
    pub span: ByteSpan,
    /// The lock both forms name, normalized.
    pub lock: String,
}

impl Finding for RecursiveLockReentryRiskItem {
    fn kind(&self) -> &'static str {
        "recursive-lock-reentry-risk"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("lock={}", self.lock)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("lock", json!(self.lock))]
    }

    /// Phrased as a risk, not a proof. Whether this path is ever taken on the
    /// same thread is not established here, and the sentence must not claim it
    /// is.
    fn message(&self) -> String {
        format!(
            "{} is taken again inside its own scope; if this runs on the holding thread it \
             deadlocks, since the lock is not recursive — use a recursive lock or restructure",
            self.lock
        )
    }
}

/// Whether a form takes a lock reentrantly by design.
fn is_reentrant(view: &ExpressionView) -> bool {
    head_is(view, REENTRANT_LOCK_SCOPE_HEADS)
}

/// Whether a form takes a lock at all — including the reentrant spellings, so
/// that an outer recursive scope can be recognized and skipped.
fn takes_a_lock(view: &ExpressionView) -> bool {
    head_is(view, LOCK_SCOPE_HEADS) || head_is(view, MANUAL_ACQUIRE_HEADS)
}

pub fn examine_lock_scope(
    view: &ExpressionView,
    lock_form_count: &mut usize,
    violations: &mut Vec<RecursiveLockReentryRiskItem>,
) {
    if !head_is(view, NON_REENTRANT_LOCK_HEADS) {
        return;
    }
    *lock_form_count += 1;

    let Some(outer) = locked_designator(view) else {
        return;
    };

    for_each_evaluated_subview_where(
        view,
        // A lock inside a nested closure is taken whenever that closure runs,
        // which may be on another thread, later, or never. Since a Common Lisp
        // thread body is always a closure, this covers the nested-spawn case
        // too — see the module documentation.
        |node| !head_is(node, DEFERRED_BODY_HEADS),
        |node| {
            if node.span == view.span || !takes_a_lock(node) || is_reentrant(node) {
                return;
            }
            if locked_designator(node).as_deref() != Some(outer.as_str()) {
                return;
            }
            violations.push(RecursiveLockReentryRiskItem {
                span: node.span,
                lock: outer.clone(),
            });
        },
    );
}

/// Collects every same-lock nesting in one file, with the number of
/// non-recursive lock forms scanned as the denominator beside them.
pub fn build_recursive_lock_reentry_risk_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RecursiveLockReentryRiskItem>> {
    let mut lock_form_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_lock_scope(view, &mut lock_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::CommonLisp,
        tree.source(),
        violations,
        vec![("lock_form_count", json!(lock_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<RecursiveLockReentryRiskItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_recursive_lock_reentry_risk_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build report")
    }

    fn violations(input: &str) -> Vec<RecursiveLockReentryRiskItem> {
        report(input).findings
    }

    #[test]
    fn flags_the_same_lock_taken_inside_its_own_scope() {
        let found = violations("(bt:with-lock-held (*l*) (work) (bt:with-lock-held (*l*) (more)))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].lock, "*l*");
    }

    #[test]
    fn flags_a_manual_acquisition_inside_a_scope() {
        assert_eq!(
            violations("(bt:with-lock-held (*l*) (bt:acquire-lock *l*))").len(),
            1
        );
    }

    #[test]
    fn flags_the_sb_thread_spelling() {
        assert_eq!(
            violations("(sb-thread:with-mutex (*m*) (sb-thread:with-mutex (*m*) (work)))").len(),
            1
        );
    }

    /// Deliberately silent, and this is a false *negative*. A lambda written
    /// inside a lock scope is far more often stored than immediately applied —
    /// registering a locking callback under the registry lock is the common
    /// shape — and nothing here distinguishes the two. For a heuristic, missing
    /// the `mapcar` case costs less than reporting the registration case.
    #[test]
    fn does_not_flag_reentry_through_a_nested_closure() {
        assert!(
            violations(
                "(with-lock-held (*l*) (mapcar (lambda (x) (with-lock-held (*l*) (f x))) xs))"
            )
            .is_empty()
        );
        assert!(
            violations(
                "(bt:with-lock-held (*registry-lock*)\n\
                 \x20 (setf (gethash name *handlers*)\n\
                 \x20       (lambda (event) (bt:with-lock-held (*registry-lock*) (dispatch event)))))"
            )
            .is_empty()
        );
    }

    #[test]
    fn the_span_points_at_the_inner_acquisition() {
        let source = "(with-lock-held (*l*) (with-lock-held (*l*) (work)))";
        let span = violations(source)[0].span;
        assert_eq!(
            &source[span.start().get()..span.end().get()],
            "(with-lock-held (*l*) (work))"
        );
    }

    // --- correct code that must stay silent --------------------------------

    #[test]
    fn does_not_flag_a_single_lock_scope() {
        assert!(violations("(bt:with-lock-held (*l*) (work))").is_empty());
    }

    #[test]
    fn does_not_flag_two_different_locks() {
        assert!(
            violations("(bt:with-lock-held (*a*) (bt:with-lock-held (*b*) (work)))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_recursive_lock_on_either_side() {
        assert!(
            violations(
                "(bt:with-recursive-lock-held (*l*) (bt:with-recursive-lock-held (*l*) (work)))"
            )
            .is_empty()
        );
        assert!(
            violations("(bt:with-lock-held (*l*) (bt:with-recursive-lock-held (*l*) (work)))")
                .is_empty()
        );
        assert!(
            violations("(sb-thread:with-recursive-lock (*m*) (sb-thread:with-mutex (*m*) (w)))")
                .is_empty(),
            "an outer recursive scope is not an anchor for this rule"
        );
    }

    #[test]
    fn does_not_flag_a_lock_retaken_on_a_thread_this_form_starts() {
        assert!(
            violations(
                "(bt:with-lock-held (*l*)\n\
                 \x20 (bt:make-thread (lambda () (bt:with-lock-held (*l*) (work)))))"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_two_sibling_scopes_on_the_same_lock() {
        assert!(
            violations("(progn (with-lock-held (*l*) (a)) (with-lock-held (*l*) (b)))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_computed_designator_on_either_side() {
        assert!(
            violations("(with-lock-held ((lock-of x)) (with-lock-held ((lock-of x)) (w)))")
                .is_empty()
        );
        assert!(violations("(with-lock-held (*l*) (with-lock-held ((lock-of x)) (w)))").is_empty());
    }

    // --- reader-syntax negatives -------------------------------------------

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(violations("'(with-lock-held (*l*) (with-lock-held (*l*) (w)))").is_empty());
        assert!(violations("(quote (with-lock-held (*l*) (with-lock-held (*l*) (w))))").is_empty());
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_a_literal_comma_and_stays_data() {
        assert!(violations("'(a ,(with-lock-held (*l*) (with-lock-held (*l*) (w))))").is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(violations("`(with-lock-held (*l*) (with-lock-held (*l*) (w)))").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(
            violations("`(progn ,(with-lock-held (*l*) (with-lock-held (*l*) (w))))").len(),
            1
        );
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(
            violations("(format t \"(with-lock-held (*l*) (with-lock-held (*l*) (w)))\")")
                .is_empty()
        );
    }

    // --- envelope ----------------------------------------------------------

    #[test]
    fn the_summary_counts_every_lock_form_scanned() {
        // The outer and the inner scope are both anchors, so both are counted;
        // only the outer one produces the finding.
        let report = report("(with-lock-held (*l*) (with-lock-held (*l*) (w)))");
        assert_eq!(report.summary, vec![("lock_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_kind_and_lock() {
        let report =
            report("(defun f ()\n  (with-lock-held (*l*)\n    (with-lock-held (*l*) (w))))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "recursive-lock-reentry-risk");
        assert_eq!(finding.json_fields(), vec![("lock", json!("*l*"))]);
        assert_eq!(finding.text_columns(), vec!["lock=*l*".to_owned()]);
    }

    /// The message must read as a risk. A rule that cannot prove the path is
    /// taken must not say the program deadlocks.
    #[test]
    fn the_message_is_phrased_as_a_risk_not_a_proof() {
        let message = violations("(with-lock-held (*l*) (with-lock-held (*l*) (w)))")[0].message();
        assert!(
            message.contains("if this runs on the holding thread"),
            "{message}"
        );
        assert!(!message.contains("will deadlock"), "{message}");
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        // Clojure's `locking` nests safely, so this rule must not run there.
        let tree = SyntaxTree::parse_with_dialect("(locking o (locking o (w)))", Dialect::Clojure)
            .expect("parse");
        let report =
            build_recursive_lock_reentry_risk_report(Path::new("a.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("lock_form_count", json!(0))]);
    }
}

//! A lock taken by hand with nothing arranging to give it back.
//!
//! `(bt:acquire-lock *lock*)` takes a lock and returns. If the work that
//! follows signals — or throws, or returns from an outer block — the release
//! never runs and the lock is held forever, which is a deadlock the next
//! acquirer discovers and the original code never mentions. Common Lisp's
//! answer is `unwind-protect`, and the answer almost every program should
//! actually use is `bt:with-lock-held`, which is `unwind-protect` already
//! written.
//!
//! # What it looks at
//!
//! Only the manual acquisition operators: `acquire-lock` (`bordeaux-threads`)
//! and `grab-mutex` (`sb-thread`). The scoped macros — `with-lock-held`,
//! `with-mutex`, `with-recursive-lock`, `with-recursive-lock-held` — release on
//! every exit by construction and never reach this rule at all, because they
//! are not in its head filter.
//!
//! A match is reported only after four questions come back the wrong way:
//!
//! 1. Is an `unwind-protect` — or any `with-…` macro, which conventionally has
//!    a cleanup — among its ancestors? Then it is protected.
//! 2. Does an `unwind-protect` appear among the forms that *follow* it in the
//!    same body? That is the standard `(progn (acquire-lock l) (unwind-protect
//!    … (release-lock l)))` idiom, and it is correct.
//! 3. Is it the last form of its enclosing body? Then this is a function whose
//!    job is to take the lock, and the release belongs to its caller.
//! 4. Is its *value* being used — as an `if`/`when` test, a `let` init, an
//!    argument? `acquire-lock` takes a `:wait-p` argument and returns whether
//!    it got the lock, so a conditional acquisition is a different shape with a
//!    different discipline.
//!
//! What is left is a lock taken in the middle of a body with no cleanup
//! anywhere. The message distinguishes the two cases it can still be:
//! [`ReleaseDiscipline::OnlyOnSuccess`], where a matching release does appear
//! later on the normal path, and [`ReleaseDiscipline::Never`], where none does.
//!
//! # What it does not attempt
//!
//! - **Checking that the cleanup releases *this* lock.** An `unwind-protect`
//!   above or after the acquisition silences it whatever its cleanup does. That
//!   is the same blanket exemption `lint-safety`'s `unclosed-stream` makes, and
//!   it errs silent.
//! - **Following a release into a helper.** `(release-everything)` is not a
//!   release as far as this rule can tell — but a body that calls one is
//!   usually protected too, and if it is not, the finding says "never
//!   released", which is what the text shows.
//! - **Any interprocedural reasoning.** A lock acquired in one function and
//!   released in another is exactly the shape question 3 exempts.
//! - **`acquire-recursive-lock`.** `bordeaux-threads` has a recursive
//!   acquire/release pair too, and leaking one leaks just as badly. It is left
//!   out because it is rare enough that including it buys little, and because
//!   the recursive spellings are shared with `recursive-lock-reentry-risk`,
//!   where they mean the opposite thing. A missed one is a false negative.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in, unqualified};
use serde_json::{Value, json};

use crate::support::{
    MANUAL_ACQUIRE_HEADS, MANUAL_RELEASE_HEADS, for_each_evaluated_subview, head_is,
    locked_designator, with_ancestor_chain,
};

/// Heads whose arguments are values rather than a body, so an acquisition
/// sitting in one is being used for its return value.
///
/// `acquire-lock`'s `:wait-p nil` makes it a *try*-acquire that answers with a
/// boolean, and `(when (acquire-lock l :wait-p nil) …)` is the correct way to
/// write that. Nothing about it is the shape this rule reports.
const VALUE_CONTEXT_HEADS: &[&str] = &[
    "if",
    "when",
    "unless",
    "and",
    "or",
    "cond",
    "not",
    "let",
    "let*",
    "setf",
    "setq",
    "assert",
    "return",
    "return-from",
    "check-type",
];

/// Whether a release was written at all, and on which paths it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseDiscipline {
    /// A matching release follows on the normal path, but a non-local exit
    /// skips it.
    OnlyOnSuccess,
    /// No release for this lock anywhere in the enclosing body.
    Never,
}

impl ReleaseDiscipline {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::OnlyOnSuccess => "only-on-success",
            Self::Never => "never",
        }
    }

    const fn detail(self) -> &'static str {
        match self {
            Self::OnlyOnSuccess => {
                "is released only on the path that reaches the release form, so a non-local \
                 exit skips it"
            }
            Self::Never => "is never released in this body",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LockAcquiredNotReleasedItem {
    /// The span of the acquisition form.
    pub span: ByteSpan,
    /// The lock's name, normalized, or `computed` when it is not a bare symbol.
    pub lock: String,
    pub discipline: ReleaseDiscipline,
}

impl Finding for LockAcquiredNotReleasedItem {
    fn kind(&self) -> &'static str {
        "lock-acquired-not-released"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("lock={}", self.lock),
            format!("discipline={}", self.discipline.label()),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("lock", json!(self.lock)),
            ("discipline", json!(self.discipline.label())),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} {}; wrap the body in unwind-protect, or use with-lock-held",
            self.lock,
            self.discipline.detail()
        )
    }
}

/// Whether a form is, or contains anywhere, an `unwind-protect`.
fn contains_unwind_protect(view: &ExpressionView) -> bool {
    let mut found = false;
    for_each_evaluated_subview(view, |node| {
        found = found || head_is(node, &["unwind-protect"]);
    });
    found
}

/// Whether a form contains a release of `lock`.
fn releases(view: &ExpressionView, lock: &str) -> bool {
    let mut found = false;
    for_each_evaluated_subview(view, |node| {
        if found || !head_is(node, MANUAL_RELEASE_HEADS) {
            return;
        }
        found = locked_designator(node).is_some_and(|named| named == lock);
    });
    found
}

/// Whether an ancestor already arranges a cleanup.
///
/// `unwind-protect` by name, and any `with-…` macro by convention: the whole
/// point of that naming convention is that the macro owns something for the
/// duration of its body and gives it back afterwards. Treating every `with-…`
/// as protective over-approximates in the silent direction.
fn protected_by_ancestor(chain: &[&ExpressionView]) -> bool {
    chain.iter().any(|ancestor| {
        list_head(ancestor).is_some_and(|head| {
            let name = unqualified(head).to_ascii_lowercase();
            name == "unwind-protect" || name.starts_with("with-")
        })
    })
}

///
/// Needs the tree because the verdict depends on what encloses the acquisition,
/// and [`paredit_core_lint_engine::engine::RuleContext`] carries no parent
/// pointer. The ancestor walk costs the node's depth and happens only once a
/// manual acquisition has already matched.
pub fn examine_acquire(
    tree: &SyntaxTree,
    view: &ExpressionView,
    acquire_count: &mut usize,
    violations: &mut Vec<LockAcquiredNotReleasedItem>,
) {
    if !head_is(view, MANUAL_ACQUIRE_HEADS) {
        return;
    }
    *acquire_count += 1;

    let lock = locked_designator(view).unwrap_or_else(|| "computed".to_owned());

    let Some(verdict) = with_ancestor_chain(tree, view.span, |chain| {
        if protected_by_ancestor(chain) {
            return None;
        }
        let parent = *chain.last()?;
        // A body is a `(...)` list. A binding list or a vector is not one, and
        // an acquisition inside one is not being sequenced.
        if !is_paren_list(parent) {
            return None;
        }
        // Its value is being consumed, not its effect sequenced.
        if list_head(parent).is_some_and(|head| symbol_in(head, VALUE_CONTEXT_HEADS)) {
            return None;
        }
        let position = parent
            .children
            .iter()
            .position(|child| child.span == view.span)?;
        let rest = parent.children.get(position + 1..)?;
        // The last form of a body: a function whose job is to take the lock.
        if rest.is_empty() {
            return None;
        }
        // `(progn (acquire-lock l) (unwind-protect … (release-lock l)))`.
        if rest.iter().any(contains_unwind_protect) {
            return None;
        }
        Some(if rest.iter().any(|form| releases(form, &lock)) {
            ReleaseDiscipline::OnlyOnSuccess
        } else {
            ReleaseDiscipline::Never
        })
    })
    .flatten() else {
        return;
    };

    violations.push(LockAcquiredNotReleasedItem {
        span: view.span,
        lock,
        discipline: verdict,
    });
}

/// Collects every unprotected manual lock acquisition in one file, with the
/// number of manual acquisitions scanned as the denominator beside them.
pub fn build_lock_acquired_not_released_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<LockAcquiredNotReleasedItem>> {
    let mut acquire_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_acquire(tree, view, &mut acquire_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::CommonLisp,
        tree.source(),
        violations,
        vec![("manual_acquire_count", json!(acquire_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<LockAcquiredNotReleasedItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_lock_acquired_not_released_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build report")
    }

    fn violations(input: &str) -> Vec<LockAcquiredNotReleasedItem> {
        report(input).findings
    }

    #[test]
    fn flags_an_acquisition_with_no_release_at_all() {
        let found = violations("(defun f () (bt:acquire-lock *l*) (work))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].discipline, ReleaseDiscipline::Never);
        assert_eq!(found[0].lock, "*l*");
    }

    #[test]
    fn flags_an_acquisition_released_only_on_the_normal_path() {
        let found = violations("(defun f () (bt:acquire-lock *l*) (work) (bt:release-lock *l*))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].discipline, ReleaseDiscipline::OnlyOnSuccess);
    }

    #[test]
    fn flags_the_sb_thread_spelling_too() {
        let found = violations("(defun f () (sb-thread:grab-mutex *m*) (work))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].lock, "*m*");
    }

    #[test]
    fn a_computed_lock_designator_is_still_reported_as_unprotected() {
        let found = violations("(defun f () (acquire-lock (lock-of x)) (work))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].lock, "computed");
    }

    // --- realistic, correct concurrent code that must stay silent -----------

    /// The trap. `with-lock-held` *is* the release, and it must never appear.
    #[test]
    fn does_not_flag_the_scoped_lock_macros() {
        for form in [
            "(bt:with-lock-held (*l*) (work))",
            "(sb-thread:with-mutex (*m*) (work))",
            "(bt:with-recursive-lock-held (*l*) (work))",
            "(sb-thread:with-recursive-lock (*m*) (work))",
        ] {
            let source = format!("(defun f () {form})");
            assert!(violations(&source).is_empty(), "{form} should be silent");
        }
    }

    #[test]
    fn does_not_flag_an_acquisition_inside_an_unwind_protect() {
        assert!(
            violations(
                "(defun f ()\n\
                 \x20 (unwind-protect\n\
                 \x20     (progn (bt:acquire-lock *l*) (work))\n\
                 \x20   (bt:release-lock *l*)))"
            )
            .is_empty()
        );
    }

    /// The idiom where the acquisition is a *sibling* of the `unwind-protect`
    /// rather than inside it — the shape a naive ancestors-only check reports.
    #[test]
    fn does_not_flag_an_acquisition_followed_by_an_unwind_protect() {
        assert!(
            violations(
                "(defun f ()\n\
                 \x20 (bt:acquire-lock *l*)\n\
                 \x20 (unwind-protect (work)\n\
                 \x20   (bt:release-lock *l*)))"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_an_acquisition_inside_any_with_macro() {
        assert!(
            violations("(defun f () (with-my-resource (r) (acquire-lock *l*) (work)))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_function_whose_only_job_is_to_take_the_lock() {
        assert!(violations("(defun lock-it () (bt:acquire-lock *l*))").is_empty());
    }

    #[test]
    fn does_not_flag_a_conditional_try_acquire() {
        assert!(
            violations("(defun f () (when (bt:acquire-lock *l* :wait-p nil) (work)) (rest))")
                .is_empty()
        );
        assert!(
            violations("(defun f () (let ((got (acquire-lock *l* :wait-p nil))) (use got)))")
                .is_empty()
        );
        assert!(
            violations("(defun f () (if (acquire-lock *l* :wait-p nil) (a) (b)) (c))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_release_of_a_different_lock_as_a_match() {
        let found = violations("(defun f () (acquire-lock *a*) (work) (release-lock *b*))");
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].discipline,
            ReleaseDiscipline::Never,
            "releasing another lock is not releasing this one"
        );
    }

    // --- reader-syntax negatives -------------------------------------------

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(violations("'(progn (acquire-lock *l*) (work))").is_empty());
        assert!(violations("(quote (progn (acquire-lock *l*) (work)))").is_empty());
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_a_literal_comma_and_stays_data() {
        assert!(violations("'(a ,(progn (acquire-lock *l*) (work)))").is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(violations("`(progn (acquire-lock *l*) (work))").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(
            violations("`(a ,(progn (acquire-lock *l*) (work)))").len(),
            1
        );
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(violations("(format t \"(acquire-lock *l*) (work)\")").is_empty());
    }

    // --- envelope ----------------------------------------------------------

    #[test]
    fn the_summary_counts_every_manual_acquisition_scanned() {
        let report =
            report("(defun a () (acquire-lock *l*))\n(defun b () (acquire-lock *l*) (work))\n");
        assert_eq!(report.summary, vec![("manual_acquire_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_kind_lock_and_discipline() {
        let report = report("(defun f ()\n  (bt:acquire-lock *l*)\n  (work))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "lock-acquired-not-released");
        assert_eq!(
            finding.json_fields(),
            vec![("lock", json!("*l*")), ("discipline", json!("never"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["lock=*l*".to_owned(), "discipline=never".to_owned()]
        );
    }

    #[test]
    fn the_message_distinguishes_the_two_disciplines() {
        assert_eq!(
            violations("(defun f () (acquire-lock *l*) (work))")[0].message(),
            "*l* is never released in this body; wrap the body in unwind-protect, or use \
             with-lock-held"
        );
        assert_eq!(
            violations("(defun f () (acquire-lock *l*) (work) (release-lock *l*))")[0].message(),
            "*l* is released only on the path that reaches the release form, so a non-local \
             exit skips it; wrap the body in unwind-protect, or use with-lock-held"
        );
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(acquire-lock l)", Dialect::Clojure).expect("parse");
        let report =
            build_lock_acquired_not_released_report(Path::new("a.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("manual_acquire_count", json!(0))]);
    }
}

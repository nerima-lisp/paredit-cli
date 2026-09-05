//! A thread body that inlines several steps of work with no handler anywhere in
//! it.
//!
//! An error on a worker thread does not reach the thread that started it. In
//! Common Lisp it enters that thread's own debugger — which, in a daemon with
//! no terminal, means the thread stops and nothing says so, and the work
//! silently stops happening.
//!
//! # What it looks at
//!
//! One shape: `(make-thread (lambda () form₁ form₂ …))`, with the thunk written
//! inline. `#'(lambda …)` counts; `#'worker` does not.
//!
//! It fires when the body has **two or more forms** and no handler head appears
//! anywhere inside it. A leading `(declare …)` and a leading docstring are not
//! forms of work and do not count towards the two.
//!
//! # Why Clojure's `future` is not here
//!
//! It was, and it was wrong. Dereferencing a future *rethrows* the stored
//! exception in the calling thread — that is `future`'s entire error-propagation
//! contract — so
//!
//! ```clojure
//! (let [users  (future (fetch-users uid) (normalize-users))
//!       orders (future (fetch-orders uid) (normalize-orders))]
//!   {:users @users :orders @orders})
//! ```
//!
//! is correct code with no handler in sight, and it is the shape of essentially
//! every fan-out/join in a Clojure codebase. The genuinely defective case is a
//! future whose value is never read, and that already has a rule:
//! [`crate::future_promise_never_realized`]. Keeping a Clojure half here would
//! have fired on the correct half of that dichotomy.
//!
//! Common Lisp has no such contract. An unhandled error in a `bordeaux-threads`
//! worker enters that thread's own debugger, and in a daemon with no terminal
//! the thread simply stops.
//!
//! # Why two or more forms
//!
//! This is the rule's false-negative bias, and it is deliberate. A thread body
//! that is a single call — `(make-thread (lambda () (run-worker)))` — is very
//! often a call to a function that installs its own handler, and nothing visible here can tell that apart from one that does
//! not. Reporting it would be a guess. A body that inlines several steps is
//! code written *at the spawn site*, where the handler would also have to be
//! written, and where its absence is visible rather than inferred.
//!
//! The cost is real: a single-form body with no handler anywhere is missed.
//! That is the direction this package errs in.
//!
//! # What it does not attempt
//!
//! - **Where the handler is.** A `handler-case` around the whole body, around
//!   one form, or nested five levels down all count. A handler installed
//!   *inside* the thunk is a handler, and this rule stays silent.
//! - **Whether the handler catches anything useful.** `(handler-case … (foo
//!   (c) …))` for an unrelated condition type silences it. Judging that is
//!   `lint-condition-system`'s subject, not this one's.
//! - **Named functions.** `(make-thread #'worker)` is never flagged; whether
//!   `worker` handles its own errors is not knowable from here.
//! - **`unwind-protect`.** It runs cleanup, it does not handle. It is
//!   deliberately absent from the handler list, so it does not silence this.
//!
//! Scope: Common Lisp only — see "Why Clojure's `future` is not here" above.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{HANDLER_HEADS, for_each_evaluated_subview, head_is, normalized_symbol};

/// The Common Lisp spawn, whose function argument is a thunk. Spelled the same
/// by `bordeaux-threads` and `sb-thread`.
pub const SPAWN_HEADS: &[&str] = &["make-thread"];

/// The smallest body this rule will judge. See the module documentation: a
/// one-form body is usually a call to something that handles its own errors,
/// and telling those apart is not possible from here.
const MINIMUM_INLINED_FORMS: usize = 2;

#[derive(Debug, Clone)]
pub struct ThreadSpawnedWithoutErrorHandlerItem {
    /// The span of the whole spawn form.
    pub span: ByteSpan,
    /// The spawn operator, normalized.
    pub spawn: String,
    /// How many forms the body inlines.
    pub body_form_count: usize,
}

impl Finding for ThreadSpawnedWithoutErrorHandlerItem {
    fn kind(&self) -> &'static str {
        "thread-spawned-without-error-handler"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("spawn={}", self.spawn),
            format!("body_form_count={}", self.body_form_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("spawn", json!(self.spawn)),
            ("body_form_count", json!(self.body_form_count)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "this {} body has no error handler, so an error in it stops the thread silently",
            self.spawn
        )
    }
}

/// Whether any form in `body` is, or contains, a handler.
fn has_handler(body: &[ExpressionView]) -> bool {
    let mut found = false;
    for form in body {
        for_each_evaluated_subview(form, |node| {
            found = found || head_is(node, HANDLER_HEADS);
        });
        if found {
            return true;
        }
    }
    false
}

/// The forms a spawn will run, when they are written at the spawn site.
///
/// `None` when the thread's work is named rather than written — which is the
/// case this rule deliberately says nothing about.
///
/// A leading docstring and any leading `(declare …)` are dropped: neither is a
/// step of work, and counting them would defeat the single-call exemption on
/// `(lambda () (declare (optimize (speed 3))) (run-worker))`.
fn inlined_body(view: &ExpressionView) -> Option<&[ExpressionView]> {
    if !head_is(view, SPAWN_HEADS) {
        return None;
    }
    let thunk = view.children.get(1)?;
    if list_head(thunk).is_none_or(|head| !symbol_is(head, "lambda")) {
        return None;
    }
    // Past `lambda` and its parameter list.
    let body = thunk.children.get(2..)?;
    let preamble = body
        .iter()
        .take_while(|form| {
            head_is(form, &["declare"]) || atom_text(form).is_some_and(|text| text.starts_with('"'))
        })
        .count();
    body.get(preamble..)
}

pub fn examine_spawn(
    view: &ExpressionView,
    spawn_count: &mut usize,
    violations: &mut Vec<ThreadSpawnedWithoutErrorHandlerItem>,
) {
    if !head_is(view, SPAWN_HEADS) {
        return;
    }
    *spawn_count += 1;

    let Some(body) = inlined_body(view) else {
        return;
    };
    if body.len() < MINIMUM_INLINED_FORMS || has_handler(body) {
        return;
    }
    let Some(spawn) = list_head(view).map(normalized_symbol) else {
        return;
    };
    violations.push(ThreadSpawnedWithoutErrorHandlerItem {
        span: view.span,
        spawn,
        body_form_count: body.len(),
    });
}

/// Collects every unguarded thread body in one file, with the number of spawn
/// forms scanned as the denominator beside them.
pub fn build_thread_spawned_without_error_handler_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ThreadSpawnedWithoutErrorHandlerItem>> {
    let mut spawn_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_spawn(view, &mut spawn_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::CommonLisp,
        tree.source(),
        violations,
        vec![("thread_spawn_count", json!(spawn_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report_in(
        input: &str,
        dialect: Dialect,
    ) -> FileFindings<ThreadSpawnedWithoutErrorHandlerItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse input");
        build_thread_spawned_without_error_handler_report(Path::new("test.src"), dialect, &tree)
            .expect("build report")
    }

    fn lisp(input: &str) -> Vec<ThreadSpawnedWithoutErrorHandlerItem> {
        report_in(input, Dialect::CommonLisp).findings
    }

    fn clojure(input: &str) -> Vec<ThreadSpawnedWithoutErrorHandlerItem> {
        report_in(input, Dialect::Clojure).findings
    }

    // --- Common Lisp -------------------------------------------------------

    #[test]
    fn flags_a_common_lisp_thunk_that_inlines_work_with_no_handler() {
        let found = lisp("(bt:make-thread (lambda () (connect) (serve)))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].spawn, "make-thread");
        assert_eq!(found[0].body_form_count, 2);
    }

    #[test]
    fn flags_a_sharp_quoted_lambda_thunk_too() {
        assert_eq!(lisp("(make-thread #'(lambda () (a) (b)))").len(), 1);
    }

    /// A leading `(declare …)` is not a step of work, and neither is a
    /// docstring. Counting them would defeat the single-call exemption.
    #[test]
    fn a_declare_or_docstring_does_not_count_towards_the_body_size() {
        assert!(
            lisp("(bt:make-thread (lambda () (declare (optimize (speed 3))) (run-worker)))")
                .is_empty()
        );
        assert!(lisp("(bt:make-thread (lambda () \"pool worker\" (run-worker)))").is_empty());
        // Still flagged once there really are two steps of work.
        assert_eq!(
            lisp("(make-thread (lambda () (declare (optimize speed)) (connect) (serve)))").len(),
            1
        );
    }

    // --- Clojure is deliberately out of scope ------------------------------

    /// Dereferencing a future rethrows its exception in the calling thread, so
    /// a handler-less `future` body is not a defect — this is the shape of
    /// every fan-out/join in Clojure. `future-promise-never-realized` covers
    /// the case that *is* defective.
    #[test]
    fn a_clojure_future_is_not_this_rules_subject() {
        assert!(clojure("(future (connect) (serve))").is_empty());
        assert!(
            clojure("(let [users (future (fetch-users) (normalize))] {:users @users})").is_empty()
        );
    }

    #[test]
    fn a_future_in_a_common_lisp_file_is_somebody_elses_macro() {
        assert!(lisp("(future (connect) (serve))").is_empty());
    }

    // --- the traps: correct code that must stay silent ---------------------

    /// The stated trap. A handler *inside* the thunk is a handler.
    #[test]
    fn does_not_flag_a_handler_installed_inside_the_thunk() {
        assert!(
            lisp(
                "(bt:make-thread\n\
                 \x20 (lambda ()\n\
                 \x20   (handler-case (progn (connect) (serve))\n\
                 \x20     (error (c) (log c)))))"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_handler_wrapping_only_part_of_the_body() {
        assert!(
            lisp("(make-thread (lambda () (setup) (handler-case (serve) (error (c) nil))))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_any_of_the_handler_forms() {
        for handler in [
            "(handler-bind ((error #'log)) (serve))",
            "(ignore-errors (serve))",
            "(restart-case (serve) (skip () nil))",
            "(with-simple-restart (abort \"stop\") (serve))",
        ] {
            let source = format!("(make-thread (lambda () (setup) {handler}))");
            assert!(lisp(&source).is_empty(), "{handler} should be silent");
        }
    }

    /// The stated trap. A one-form body is usually a call to something that
    /// handles its own errors, and this rule will not guess.
    #[test]
    fn does_not_flag_a_body_that_is_a_single_call() {
        assert!(lisp("(bt:make-thread (lambda () (run-worker)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_thread_whose_function_is_named_rather_than_written() {
        assert!(lisp("(make-thread #'worker)").is_empty());
        assert!(lisp("(make-thread 'worker :name \"w\")").is_empty());
    }

    #[test]
    fn does_not_flag_an_empty_thunk() {
        assert!(lisp("(make-thread (lambda ()))").is_empty());
    }

    /// `unwind-protect` runs cleanup; it does not handle. A thread body that
    /// only protects still loses its error.
    #[test]
    fn unwind_protect_does_not_count_as_a_handler() {
        assert_eq!(
            lisp("(make-thread (lambda () (setup) (unwind-protect (serve) (cleanup))))").len(),
            1
        );
    }

    // --- reader-syntax negatives -------------------------------------------

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(lisp("'(make-thread (lambda () (a) (b)))").is_empty());
        assert!(lisp("(quote (make-thread (lambda () (a) (b))))").is_empty());
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_a_literal_comma_and_stays_data() {
        assert!(lisp("'(x ,(make-thread (lambda () (a) (b))))").is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(lisp("`(make-thread (lambda () (a) (b)))").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(lisp("`(progn ,(make-thread (lambda () (a) (b))))").len(), 1);
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(lisp("(format t \"(make-thread (lambda () (a) (b)))\")").is_empty());
    }

    // --- envelope ----------------------------------------------------------

    #[test]
    fn the_summary_counts_every_spawn_scanned() {
        let report = report_in(
            "(make-thread #'worker)\n(make-thread (lambda () (a) (b)))\n",
            Dialect::CommonLisp,
        );
        assert_eq!(report.summary, vec![("thread_spawn_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_kind_spawn_and_body_size() {
        let report = report_in(
            "(defun start ()\n  (make-thread (lambda () (connect) (serve))))\n",
            Dialect::CommonLisp,
        );
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "thread-spawned-without-error-handler");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("spawn", json!("make-thread")),
                ("body_form_count", json!(2))
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "spawn=make-thread".to_owned(),
                "body_form_count=2".to_owned()
            ]
        );
        assert_eq!(
            finding.message(),
            "this make-thread body has no error handler, so an error in it stops the thread \
             silently"
        );
    }

    #[test]
    fn common_lisp_is_reported_as_modelled() {
        assert!(report_in("(make-thread #'w)", Dialect::CommonLisp).dialect_modelled);
    }

    #[test]
    fn an_unmodelled_dialect_is_reported_as_such() {
        let report = report_in("(make-thread (lambda () (a) (b)))", Dialect::Clojure);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("thread_spawn_count", json!(0))]);

        let report = report_in("(make-thread (lambda () (a) (b)))", Dialect::EmacsLisp);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("thread_spawn_count", json!(0))]);
    }
}

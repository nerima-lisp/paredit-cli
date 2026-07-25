//! Common Lisp empty-`handler-case` detection: a `(handler-case expr)` with no
//! handler clauses. `handler-case` establishes handlers around `expr` and returns
//! its value(s); with zero clauses it establishes nothing, so it is exactly
//! `expr` — a leftover wrapper (often after the last handler clause was deleted).
//!
//! Only the exact no-clause shape `(handler-case expr)` is matched; a
//! `handler-case` with any clause is a real construct and left alone, as is a
//! reader-conditional protected form.
//!
//! The fix replaces the whole form with the protected form's source, so the rule
//! is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, list_head};

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct HandlerCaseNoClausesItem {
    pub path: PathBuf,
    /// The span of the whole `(handler-case expr)` form.
    pub span: ByteSpan,
    /// The span of the protected form `expr` (for reconstructing the fix).
    pub form_span: ByteSpan,
}

#[derive(Debug)]
pub struct HandlerCaseNoClausesSummary {
    pub handler_case_form_count: usize,
    pub violations: Vec<HandlerCaseNoClausesItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct HandlerCaseNoClausesPolicyOptions {
    fail_on_violation: bool,
}

impl HandlerCaseNoClausesPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct HandlerCaseNoClausesPolicy {
    pub fail_on_violation: bool,
    pub handler_case_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine(
    view: &ExpressionView,
    path: &Path,
    handler_case_form_count: &mut usize,
    violations: &mut Vec<HandlerCaseNoClausesItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("handler-case") {
        return;
    }
    *handler_case_form_count += 1;

    // children: [handler-case, expr] — no handler clauses.
    if view.children.len() != 2 {
        return;
    }
    let form = &view.children[1];
    if is_reader_conditional(form) {
        return;
    }

    violations.push(HandlerCaseNoClausesItem {
        path: path.to_path_buf(),
        span: view.span,
        form_span: form.span,
    });
}

/// Collects every clauseless `(handler-case expr)` across a whole file, along
/// with the total number of `handler-case` forms scanned.
pub fn collect_handler_case_no_clauses(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<HandlerCaseNoClausesItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut handler_case_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut handler_case_form_count, &mut violations)
        });
    }
    Ok((handler_case_form_count, violations))
}

pub fn summarize_handler_case_no_clauses(
    handler_case_form_count: usize,
    violations: Vec<HandlerCaseNoClausesItem>,
) -> HandlerCaseNoClausesSummary {
    HandlerCaseNoClausesSummary {
        handler_case_form_count,
        violations,
    }
}

pub fn evaluate_handler_case_no_clauses_policy(
    options: HandlerCaseNoClausesPolicyOptions,
    summary: &HandlerCaseNoClausesSummary,
) -> HandlerCaseNoClausesPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    HandlerCaseNoClausesPolicy {
        fail_on_violation: options.fail_on_violation(),
        handler_case_form_count: summary.handler_case_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cases(input: &str) -> (usize, Vec<HandlerCaseNoClausesItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_handler_case_no_clauses(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect handler-case no clauses")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_clauseless_handler_case() {
        let source = "(handler-case (compute))";
        let (count, violations) = cases(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].form_span), "(compute)");
    }

    #[test]
    fn does_not_flag_handler_case_with_clauses() {
        assert!(
            cases("(handler-case (compute) (error (e) nil))")
                .1
                .is_empty()
        );
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = cases("(HANDLER-CASE x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = cases("(defun f (x) (handler-case x))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(handler-case x)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_handler_case_no_clauses(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect handler-case no clauses");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = cases("(handler-case x)");
        let summary = summarize_handler_case_no_clauses(count, items);

        let quiet = evaluate_handler_case_no_clauses_policy(
            HandlerCaseNoClausesPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_handler_case_no_clauses_policy(
            HandlerCaseNoClausesPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}

//! Common Lisp funcall-of-lambda detection: a `funcall` whose first argument is
//! a literal `(lambda …)` form. A lambda form evaluates to a function and is
//! directly applicable in operator position, so `(funcall (lambda (x) …) a b)`
//! is exactly `((lambda (x) …) a b)` — same function, same arguments, same
//! values (both pass every returned value through). The `funcall` adds nothing.
//!
//! Only a bare `(lambda …)` first argument is flagged. A sharp-quoted symbol
//! (`(funcall #'foo …)`) is the province of the `redundant-funcall` rule, and a
//! `#'(lambda …)` first argument carries a reader prefix that cannot sit in
//! operator position after the rewrite, so it is left alone here.
//!
//! The fix drops the `funcall ` head, leaving the lambda form as the operator,
//! so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// Whether `view` is a bare `(lambda …)` form (no reader prefix, so a
/// `#'(lambda …)` is excluded — its prefix would be illegal in operator
/// position).
fn is_lambda_form(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && is_paren_list(view)
        && list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("lambda"))
}

#[derive(Debug, Clone)]
pub struct FuncallLambdaItem {
    /// The span of the whole `(funcall (lambda …) …)` form.
    pub span: ByteSpan,
    /// The span of the `funcall` head symbol.
    ///
    /// The rewrite's input, not the report's: with `lambda_span` it bounds the
    /// text the fix deletes, and neither the old report nor this one printed
    /// it.
    pub head_span: ByteSpan,
    /// The span of the `(lambda …)` form (its start marks where the rewrite
    /// keeps the source; everything from the head to here is dropped).
    ///
    /// Unreported for the same reason as `head_span`.
    pub lambda_span: ByteSpan,
}

impl Finding for FuncallLambdaItem {
    /// The rule's name. Every finding here is the one shape — a `funcall` of a
    /// literal lambda — so there is no closed set to discriminate on.
    fn kind(&self) -> &'static str {
        "funcall-lambda"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// Nothing beyond the path and location: the old text row carried only
    /// those, and the finding has no field the report published. `message` is
    /// what a reader gets here.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    /// The same sentence the `funcall-lambda` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "funcall of a lambda applies it directly; ((lambda …) …) drops the funcall".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_funcall(
    view: &ExpressionView,
    funcall_form_count: &mut usize,
    violations: &mut Vec<FuncallLambdaItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("funcall") {
        return;
    }
    *funcall_form_count += 1;

    // children: [funcall, callee, args…] — need at least the callee.
    if view.children.len() < 2 {
        return;
    }
    let callee = &view.children[1];
    if !is_lambda_form(callee) {
        return;
    }

    violations.push(FuncallLambdaItem {
        span: view.span,
        head_span: view.children[0].span,
        lambda_span: callee.span,
    });
}

/// Collects every `funcall` of a bare lambda form in one file, with the number
/// of `funcall` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no funcall of a lambda here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_funcall_lambda_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<FuncallLambdaItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("funcall_form_count", json!(0))],
        ));
    }

    let mut funcall_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_funcall(subview, &mut funcall_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("funcall_form_count", json!(funcall_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<FuncallLambdaItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_funcall_lambda_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build funcall lambda report")
    }

    /// The `(funcall_form_count, violations)` pair the report is built from.
    fn funcalls(input: &str) -> (u64, Vec<FuncallLambdaItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "funcall_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("funcall_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_funcall_of_lambda() {
        let source = "(funcall (lambda (x) (* x x)) 5)";
        let (count, violations) = funcalls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].head_span), "funcall");
        assert_eq!(
            slice(source, violations[0].lambda_span),
            "(lambda (x) (* x x))"
        );
    }

    #[test]
    fn flags_funcall_of_lambda_no_args() {
        let (_, violations) = funcalls("(funcall (lambda () 42))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_funcall_of_sharp_quoted_symbol() {
        // That is the redundant-funcall rule's job.
        let (count, violations) = funcalls("(funcall #'foo 1 2)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_funcall_of_sharp_quoted_lambda() {
        // #'(lambda …) carries a reader prefix illegal in operator position.
        let (_, violations) = funcalls("(funcall #'(lambda (x) x) 5)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_funcall_of_a_variable() {
        let (_, violations) = funcalls("(funcall fn 1 2)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_bare_funcall() {
        let (count, violations) = funcalls("(funcall)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = funcalls("(FUNCALL (lambda (x) x) 1)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_funcall_lambda() {
        let (_, violations) = funcalls("(mapcar (lambda (y) (funcall (lambda (x) x) y)) xs)");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(funcall (lambda (x) x) 5)", Dialect::Clojure)
            .expect("parse");
        let report = build_funcall_lambda_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build funcall lambda report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("funcall_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(funcall fn 1 2)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_leans_on_its_message() {
        let report = report("(defun f ()\n  (funcall (lambda (x) x) 5))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "funcall-lambda");
        assert!(finding.text_columns().is_empty());
        assert!(finding.json_fields().is_empty());
        assert_eq!(
            finding.message(),
            "funcall of a lambda applies it directly; ((lambda …) …) drops the funcall"
        );
    }

    #[test]
    fn the_summary_counts_every_funcall_scanned_not_only_the_flagged_ones() {
        let report = report("(funcall (lambda (x) x) 5)\n(funcall #'foo 1)\n(funcall fn)\n");
        assert_eq!(report.summary, vec![("funcall_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}

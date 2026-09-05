//! `around-method-missing-call-next-method` detection: an `:around` method
//! whose body never reaches `call-next-method`.
//!
//! In the standard method combination an `:around` method runs *instead of*
//! everything more specific than it — the primary methods, the `:before`s and
//! the `:after`s all run only where the `:around` body calls
//! `call-next-method`. An `:around` that never calls it is therefore not a
//! wrapper at all: it silently replaces the entire rest of the combination,
//! which is rarely what "wrap this generic" was meant to mean.
//!
//! `call-next-method` counts wherever it appears in the body, including as a
//! bare function designator: `(apply #'call-next-method args)` reaches it just
//! as much as a direct call does. That means an occurrence inside quoted data
//! also silences the rule — a false negative rather than a false positive,
//! which is the right direction for a rule about what a body *might* do.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, symbol_is};
use serde_json::{Value, json};

use crate::support;

#[derive(Debug, Clone)]
pub struct AroundMethodMissingCallNextMethodItem {
    /// The span of the whole `defmethod` form.
    pub span: ByteSpan,
    /// The generic this method specializes, or `"(setf …)"` for a setf method.
    pub generic: String,
}

impl Finding for AroundMethodMissingCallNextMethodItem {
    fn kind(&self) -> &'static str {
        "around-method-missing-call-next-method"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("generic={}", self.generic)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("generic", json!(self.generic))]
    }

    fn message(&self) -> String {
        format!(
            "the :around method on {} never calls call-next-method: it replaces every \
             primary, :before and :after method rather than wrapping them",
            self.generic
        )
    }
}

pub fn examine_around_method_missing_call_next_method(
    tree: &SyntaxTree,
    view: &ExpressionView,
    around_method_count: &mut usize,
    violations: &mut Vec<AroundMethodMissingCallNextMethodItem>,
) {
    let Some(method) = support::method_form(view) else {
        return;
    };
    if !method
        .qualifiers
        .iter()
        .filter_map(|qualifier| support::symbol_text(qualifier))
        .any(|qualifier| symbol_is(qualifier, ":around"))
    {
        return;
    }
    let Some(site) = support::locate(tree, view.span) else {
        return;
    };
    if site.quoted || !site.top_level {
        return;
    }
    *around_method_count += 1;

    if method
        .body
        .iter()
        .any(|form| support::mentions(form, &["call-next-method"]))
    {
        return;
    }

    violations.push(AroundMethodMissingCallNextMethodItem {
        span: view.span,
        generic: support::definition_name(view).unwrap_or_else(|| "an unnamed generic".to_owned()),
    });
}

/// Collects every short-circuiting `:around` method in one file, with the
/// number of `:around` methods scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_around_method_missing_call_next_method_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<AroundMethodMissingCallNextMethodItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("around_method_count", json!(0))],
        ));
    }

    let mut around_method_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_around_method_missing_call_next_method(
                tree,
                subview,
                &mut around_method_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("around_method_count", json!(around_method_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<AroundMethodMissingCallNextMethodItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_around_method_missing_call_next_method_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build around method report")
    }

    fn violations(input: &str) -> Vec<AroundMethodMissingCallNextMethodItem> {
        report(input).findings
    }

    #[test]
    fn flags_an_around_method_that_never_calls_the_next_one() {
        let found = violations("(defmethod draw :around ((s circle) stream) (log-it s))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].generic, "draw");
    }

    #[test]
    fn flags_an_around_method_with_an_empty_body() {
        let found = violations("(defmethod draw :around ((s circle) stream))");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn folds_the_case_of_the_qualifier() {
        let found = violations("(defmethod draw :AROUND ((s circle)) (log-it s))");
        assert_eq!(found.len(), 1);
    }

    /// The near miss: the call is there, nested inside the body.
    #[test]
    fn does_not_flag_an_around_method_that_calls_the_next_one() {
        let found = violations(
            "(defmethod draw :around ((s circle) stream)\n  (with-lock () (call-next-method)))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_an_around_method_that_only_passes_the_function_along() {
        let found =
            violations("(defmethod draw :around ((s circle)) (apply #'call-next-method nil))");
        assert!(found.is_empty());
    }

    /// The other near miss: a primary method has no next method to call, and
    /// is not required to call one.
    #[test]
    fn does_not_flag_a_primary_method() {
        let found = violations("(defmethod draw ((s circle) stream) (log-it s))");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_before_or_after_method() {
        let found = violations(
            "(defmethod draw :before ((s circle)) (log-it s))\n\
             (defmethod draw :after ((s circle)) (log-it s))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_around_method() {
        let found = violations("(setf template '(defmethod draw :around ((s circle)) (log-it s)))");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_an_around_method_inside_an_unescaped_quasiquote() {
        let found =
            violations("(defmacro wrap () `(defmethod draw :around ((s circle)) (log-it s)))");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_an_around_method_written_inside_a_string() {
        let found =
            violations("(defparameter *doc* \"(defmethod draw :around ((s circle)) (log-it s))\")");
        assert!(found.is_empty());
    }

    #[test]
    fn the_denominator_counts_every_around_method_scanned() {
        let built = report(
            "(defmethod draw :around ((s circle)) (call-next-method))\n\
             (defmethod draw :around ((s square)) (log-it s))\n\
             (defmethod draw ((s tri)) s)",
        );
        assert_eq!(built.summary, vec![("around_method_count", json!(2))]);
        assert_eq!(built.findings.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(defmethod draw :around ((s c)) s)", Dialect::Clojure)
                .expect("parse");
        let built = build_around_method_missing_call_next_method_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn a_finding_carries_its_line_and_its_message() {
        let built = report("(defun f ())\n(defmethod draw :around ((s circle)) (log-it s))\n");
        let finding = &built.findings[0];
        assert_eq!(built.line_of(finding), 2);
        assert_eq!(finding.kind(), "around-method-missing-call-next-method");
        assert_eq!(finding.text_columns(), vec!["generic=draw".to_owned()]);
        assert!(finding.message().contains("call-next-method"));
    }
}

//! `ignore-errors` around a `signal` it cannot catch.
//!
//! `ignore-errors` is `(handler-case … (error (c) (values nil c)))` — it catches
//! `error` and nothing else. Wrapping it around a `signal` of a condition that
//! does *not* inherit from `error` therefore protects nothing: the condition
//! passes straight through the wrapper to whatever is outside it, while the
//! wrapper's presence tells every later reader that this call site is guarded.
//! A false sense of safety is worse than none, because it is the one nobody
//! re-checks.
//!
//! The claim needs two facts and refuses to fire without both: the `signal` has
//! to name a type literally, and that type has to be defined by a
//! `define-condition` in this same file. An unknown type could inherit from
//! `error`, and this rule would be asserting the opposite.
//!
//! The search stops at any inner `handler-case`, `handler-bind` or nested
//! `ignore-errors`: a `signal` that something closer already handles is not one
//! that escapes.
//!
//! # Relationship to `handler-case-swallows-error`
//!
//! That rule reports *every* `ignore-errors` form, on the separate ground that
//! discarding an error unconditionally is a bad idea. This one reports the
//! `signal` inside, on the ground that this particular condition is not even
//! caught. The spans differ, the diagnoses differ, and a file can legitimately
//! draw both.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{calls, is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{
    LazyHierarchy, for_each_evaluated_subview, for_each_evaluated_subview_where,
    signalled_condition_type,
};

#[derive(Debug, Clone)]
pub struct IgnoreErrorsWrapsNonErrorSignalItem {
    /// The span of the `(signal …)` call, not of the `ignore-errors`: the call
    /// is what escapes, and the wrapper may well be right about everything else
    /// it contains.
    pub span: ByteSpan,
    /// The condition type the escaping call names.
    pub condition_type: String,
}

impl Finding for IgnoreErrorsWrapsNonErrorSignalItem {
    fn kind(&self) -> &'static str {
        "ignore-errors-wraps-non-error-signal"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("condition={}", self.condition_type)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("condition", json!(self.condition_type))]
    }

    fn message(&self) -> String {
        format!(
            "ignore-errors catches only error subtypes, but `{}` is not one: this signal \
             escapes the wrapper that appears to guard it",
            self.condition_type
        )
    }
}

/// The forms that establish a handler of their own, and so end this search.
const HANDLER_FORMS: [&str; 3] = ["handler-case", "handler-bind", "ignore-errors"];

/// Examines one node, consulting the file's condition hierarchy only once an
/// enclosed `signal` has actually named a type.
///
/// Shared with the lint suite's rule, which reaches every node through the
/// single dispatch pass instead of walking the tree again.
pub fn examine_ignore_errors(
    view: &ExpressionView,
    hierarchy: &LazyHierarchy<'_>,
    ignore_errors_form_count: &mut usize,
    violations: &mut Vec<IgnoreErrorsWrapsNonErrorSignalItem>,
) {
    if !is_paren_list(view) || !list_head(view).is_some_and(|head| symbol_is(head, "ignore-errors"))
    {
        return;
    }
    *ignore_errors_form_count += 1;

    for body_form in view.children.iter().skip(1) {
        for_each_evaluated_subview_where(
            body_form,
            |inner| !calls(inner, &HANDLER_FORMS),
            |inner| {
                if !calls(inner, &["signal"]) {
                    return;
                }
                let Some(condition_type) = signalled_condition_type(inner) else {
                    return;
                };
                let hierarchy = hierarchy.get();
                if !hierarchy.declares(&condition_type) || hierarchy.is_error_type(&condition_type)
                {
                    return;
                }
                violations.push(IgnoreErrorsWrapsNonErrorSignalItem {
                    span: inner.span,
                    condition_type,
                });
            },
        );
    }
}

/// Collects every escaping `signal` under an `ignore-errors` in one file, with
/// the number of `ignore-errors` forms scanned as the denominator beside them.
pub fn build_ignore_errors_wraps_non_error_signal_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<IgnoreErrorsWrapsNonErrorSignalItem>> {
    let mut ignore_errors_form_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        let hierarchy = LazyHierarchy::new(tree);
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_ignore_errors(
                view,
                &hierarchy,
                &mut ignore_errors_form_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::CommonLisp,
        tree.source(),
        violations,
        vec![("ignore_errors_form_count", json!(ignore_errors_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<IgnoreErrorsWrapsNonErrorSignalItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_ignore_errors_wraps_non_error_signal_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn violations(input: &str) -> Vec<IgnoreErrorsWrapsNonErrorSignalItem> {
        report(input).findings
    }

    const PROGRESS: &str = "(define-condition progress (warning) ())\n";
    const DISK_FULL: &str = "(define-condition disk-full (error) ())\n";

    #[test]
    fn flags_a_signal_of_a_non_error_condition() {
        let found = violations(&format!("{PROGRESS}(ignore-errors (signal 'progress))"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].condition_type, "progress");
    }

    #[test]
    fn flags_a_signal_nested_deep_in_the_protected_body() {
        let found = violations(&format!(
            "{PROGRESS}(ignore-errors (dolist (x xs) (when (ready-p x) (signal 'progress))))"
        ));
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn flags_a_condition_with_no_supertypes_at_all() {
        let found = violations("(define-condition note () ())\n(ignore-errors (signal 'note))");
        assert_eq!(found.len(), 1);
    }

    /// The near miss: an `ignore-errors` that really does catch what is inside
    /// it.
    #[test]
    fn does_not_flag_a_signal_of_an_error_subtype() {
        assert!(violations(&format!("{DISK_FULL}(ignore-errors (signal 'disk-full))")).is_empty());
    }

    #[test]
    fn does_not_flag_a_type_this_file_does_not_define() {
        assert!(violations("(ignore-errors (signal 'progress))").is_empty());
    }

    #[test]
    fn does_not_flag_a_signal_an_inner_handler_case_already_handles() {
        assert!(
            violations(&format!(
                "{PROGRESS}(ignore-errors (handler-case (signal 'progress) (progress () nil)))"
            ))
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_signal_under_an_inner_handler_bind() {
        assert!(
            violations(&format!(
                "{PROGRESS}(ignore-errors (handler-bind ((progress #'note)) (signal 'progress)))"
            ))
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_signal_outside_the_wrapper() {
        assert!(
            violations(&format!(
                "{PROGRESS}(progn (ignore-errors (f)) (signal 'progress))"
            ))
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_an_error_call_inside_the_wrapper() {
        assert!(violations(&format!("{PROGRESS}(ignore-errors (error 'progress))")).is_empty());
    }

    #[test]
    fn does_not_flag_a_format_control_signal() {
        assert!(violations(&format!("{PROGRESS}(ignore-errors (signal \"raw ~A\" x))")).is_empty());
    }

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(violations(&format!("{PROGRESS}'(ignore-errors (signal 'progress))")).is_empty());
        assert!(
            violations(&format!(
                "{PROGRESS}(quote (ignore-errors (signal 'progress)))"
            ))
            .is_empty()
        );
    }

    #[test]
    fn a_matching_shape_inside_a_backquote_with_no_unquote_is_data() {
        assert!(violations(&format!("{PROGRESS}`(ignore-errors (signal 'progress))")).is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(
            violations(&format!(
                "{PROGRESS}`(progn ,(ignore-errors (signal 'progress)))"
            ))
            .len(),
            1
        );
    }

    /// A quoted `signal` *inside* an evaluated `ignore-errors` is data too.
    #[test]
    fn a_quoted_signal_inside_the_body_is_data() {
        assert!(
            violations(&format!(
                "{PROGRESS}(ignore-errors (emit '(signal 'progress)))"
            ))
            .is_empty()
        );
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(
            violations(&format!(
                "{PROGRESS}(format t \"(ignore-errors (signal 'progress))\")"
            ))
            .is_empty()
        );
    }

    #[test]
    fn the_summary_counts_every_wrapper_scanned_not_only_the_flagged_ones() {
        let report = report(&format!(
            "{PROGRESS}{DISK_FULL}(ignore-errors (signal 'progress))\n\
             (ignore-errors (signal 'disk-full))\n"
        ));
        assert_eq!(report.summary, vec![("ignore_errors_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_and_its_condition_type() {
        let report = report(&format!(
            "{PROGRESS}(defun run ()\n  (ignore-errors\n    (signal 'progress)))\n"
        ));
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 4);
        assert_eq!(finding.kind(), "ignore-errors-wraps-non-error-signal");
        assert_eq!(
            finding.json_fields(),
            vec![("condition", json!("progress"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["condition=progress".to_owned()]
        );
        assert!(finding.message().contains("escapes the wrapper"));
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(ignore-errors (signal 'progress))", Dialect::Clojure)
                .expect("parse");
        let report = build_ignore_errors_wraps_non_error_signal_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("ignore_errors_form_count", json!(0))]);
    }
}

//! `signal` of a condition type that is an error.
//!
//! `signal` and `error` do the same thing until nobody handles the condition.
//! At that point `error` calls `invoke-debugger` and `signal` returns `nil` —
//! so `(signal 'disk-full)`, where `disk-full` inherits from `error`, is a
//! program that reports a serious failure by *continuing normally*. The
//! condition was correctly defined, correctly signalled, and silently dropped.
//!
//! The type has to be named literally (`'my-error`, or `(make-condition
//! 'my-error …)`) and defined by a `define-condition` in the same file. A
//! computed datum cannot be read, and a type from another file could inherit
//! from anything — flagging either would be a guess, and this rule's whole
//! claim is about what the type provably inherits from.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{LazyHierarchy, for_each_evaluated_subview, signalled_condition_type};

#[derive(Debug, Clone)]
pub struct SignalOnErrorConditionReturnsSilentlyItem {
    /// The span of the whole `(signal …)` call.
    pub span: ByteSpan,
    /// The condition type the call names.
    pub condition_type: String,
}

impl Finding for SignalOnErrorConditionReturnsSilentlyItem {
    fn kind(&self) -> &'static str {
        "signal-on-error-condition-returns-silently"
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
            "signal of `{}`, which inherits from error: an unhandled signal returns nil \
             instead of entering the debugger — use error to signal it",
            self.condition_type
        )
    }
}

/// Examines one node, consulting the file's condition hierarchy only once a
/// `signal` call has actually named a type.
///
/// Shared with the lint suite's rule, which reaches every node through the
/// single dispatch pass instead of walking the tree again.
pub fn examine_signal(
    view: &ExpressionView,
    hierarchy: &LazyHierarchy<'_>,
    signal_call_count: &mut usize,
    violations: &mut Vec<SignalOnErrorConditionReturnsSilentlyItem>,
) {
    if !is_paren_list(view) || !list_head(view).is_some_and(|head| symbol_is(head, "signal")) {
        return;
    }
    *signal_call_count += 1;

    let Some(condition_type) = signalled_condition_type(view) else {
        return;
    };
    // Everything above this line is a local read of the matched node. Only here
    // — with a literal type name in hand — is the whole-file hierarchy built.
    let hierarchy = hierarchy.get();
    if !hierarchy.declares(&condition_type) || !hierarchy.is_error_type(&condition_type) {
        return;
    }
    violations.push(SignalOnErrorConditionReturnsSilentlyItem {
        span: view.span,
        condition_type,
    });
}

/// Collects every `signal` of a same-file error type in one file, with the
/// number of `signal` calls scanned as the denominator beside them.
pub fn build_signal_on_error_condition_returns_silently_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SignalOnErrorConditionReturnsSilentlyItem>> {
    let mut signal_call_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        let hierarchy = LazyHierarchy::new(tree);
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_signal(view, &hierarchy, &mut signal_call_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::CommonLisp,
        tree.source(),
        violations,
        vec![("signal_call_count", json!(signal_call_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<SignalOnErrorConditionReturnsSilentlyItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_signal_on_error_condition_returns_silently_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn violations(input: &str) -> Vec<SignalOnErrorConditionReturnsSilentlyItem> {
        report(input).findings
    }

    const DISK_FULL: &str = "(define-condition disk-full (error) ())\n";
    const PROGRESS: &str = "(define-condition progress (warning) ())\n";

    #[test]
    fn flags_a_signal_of_a_same_file_error_type() {
        let found = violations(&format!(
            "{DISK_FULL}(defun write-all () (signal 'disk-full))"
        ));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].condition_type, "disk-full");
    }

    #[test]
    fn flags_a_signal_through_make_condition() {
        let found = violations(&format!(
            "{DISK_FULL}(defun write-all () (signal (make-condition 'disk-full :path p)))"
        ));
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn flags_a_type_that_reaches_error_transitively() {
        let found = violations(
            "(define-condition io-failure (error) ())\n\
             (define-condition disk-full (io-failure) ())\n\
             (signal 'disk-full)",
        );
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn flags_a_type_that_reaches_error_through_the_standard_hierarchy() {
        let found = violations("(define-condition disk-full (file-error) ())\n(signal 'disk-full)");
        assert_eq!(found.len(), 1);
    }

    /// The near miss: the same call on a condition that is deliberately not an
    /// error, which is exactly what `signal` is for.
    #[test]
    fn does_not_flag_a_signal_of_a_non_error_condition() {
        assert!(violations(&format!("{PROGRESS}(signal 'progress)")).is_empty());
    }

    #[test]
    fn does_not_flag_a_type_this_file_does_not_define() {
        assert!(
            violations("(signal 'disk-full)").is_empty(),
            "an undeclared name could inherit from anything"
        );
    }

    #[test]
    fn does_not_flag_error_or_warn() {
        assert!(violations(&format!("{DISK_FULL}(error 'disk-full)")).is_empty());
        assert!(violations(&format!("{DISK_FULL}(warn 'disk-full)")).is_empty());
    }

    #[test]
    fn does_not_flag_a_format_control_signal() {
        assert!(violations(&format!("{DISK_FULL}(signal \"disk full: ~A\" path)")).is_empty());
    }

    #[test]
    fn does_not_flag_a_computed_datum() {
        assert!(violations(&format!("{DISK_FULL}(signal (current-condition))")).is_empty());
    }

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(violations(&format!("{DISK_FULL}'(signal 'disk-full)")).is_empty());
        assert!(violations(&format!("{DISK_FULL}(quote (signal 'disk-full))")).is_empty());
    }

    #[test]
    fn a_matching_shape_inside_a_backquote_with_no_unquote_is_data() {
        assert!(violations(&format!("{DISK_FULL}`(signal 'disk-full)")).is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(
            violations(&format!("{DISK_FULL}`(progn ,(signal 'disk-full))")).len(),
            1
        );
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(violations(&format!("{DISK_FULL}(format t \"(signal 'disk-full)\")")).is_empty());
    }

    /// A `define-condition` that is itself quoted data defines nothing, so the
    /// call that names it has no same-file declaration to appeal to.
    #[test]
    fn a_quoted_definition_does_not_make_a_signal_reportable() {
        assert!(
            violations("'(define-condition disk-full (error) ())\n(signal 'disk-full)").is_empty()
        );
    }

    #[test]
    fn the_summary_counts_every_signal_call_scanned() {
        let report = report(&format!(
            "{DISK_FULL}{PROGRESS}(signal 'disk-full)\n(signal 'progress)\n(signal \"raw\")"
        ));
        assert_eq!(report.summary, vec![("signal_call_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_and_its_condition_type() {
        let report = report(&format!(
            "{DISK_FULL}(defun write-all ()\n  (signal 'disk-full))\n"
        ));
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "signal-on-error-condition-returns-silently");
        assert_eq!(
            finding.json_fields(),
            vec![("condition", json!("disk-full"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["condition=disk-full".to_owned()]
        );
        assert!(finding.message().contains("returns nil"));
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(signal 'disk-full)", Dialect::Clojure).expect("parse");
        let report = build_signal_on_error_condition_returns_silently_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("signal_call_count", json!(0))]);
    }
}

//! Error conditions that never say what went wrong.
//!
//! `:report` is what turns a condition into a diagnostic. Without it, the
//! debugger falls back to the generic printer — `#<DISK-FULL {1004A2B}>`, or
//! `Condition DISK-FULL was signalled` — and every slot the definition
//! carefully declared goes unmentioned. The condition class was designed to
//! carry information and then prints none of it.
//!
//! Only error types are flagged. A `warning` or a bare `condition` subclass is
//! frequently a control-flow signal that nothing ever prints, so demanding a
//! report of it would be noise; an `error` subtype, by contrast, ends up in
//! front of a human by construction.
//!
//! Three things stop this rule firing, and all three are cases of "the report
//! might already exist somewhere this analysis cannot see":
//!
//! - the definition carries `:report` itself;
//! - a supertype defined in the same file carries one, since `:report` is
//!   inherited;
//! - the ancestry mentions a type this file does not define and the standard
//!   hierarchy does not name, which could supply one.
//!
//! `define-condition-empty-superclass-list` owns the `()` case: such a
//! condition is not an error type at all, so this rule stays quiet and the
//! other one explains why.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{LazyHierarchy, for_each_evaluated_subview, read_define_condition};

#[derive(Debug, Clone)]
pub struct DefineConditionMissingReportForErrorTypeItem {
    /// The span of the whole `define-condition` form, where a `:report` option
    /// would be added.
    pub span: ByteSpan,
    /// The condition's name.
    pub condition_name: String,
}

impl Finding for DefineConditionMissingReportForErrorTypeItem {
    fn kind(&self) -> &'static str {
        "define-condition-missing-report-for-error-type"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("condition={}", self.condition_name)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("condition", json!(self.condition_name))]
    }

    fn message(&self) -> String {
        format!(
            "error condition `{}` has no :report and inherits none, so the debugger prints a \
             generic description with no diagnostic detail",
            self.condition_name
        )
    }
}

/// Examines one node, consulting the file's condition hierarchy only once a
/// `define-condition` has actually been read.
///
/// Shared with the lint suite's rule, which reaches every node through the
/// single dispatch pass instead of walking the tree again.
pub fn examine_define_condition(
    view: &ExpressionView,
    hierarchy: &LazyHierarchy<'_>,
    define_condition_form_count: &mut usize,
    violations: &mut Vec<DefineConditionMissingReportForErrorTypeItem>,
) {
    if !is_paren_list(view)
        || !list_head(view).is_some_and(|head| symbol_is(head, "define-condition"))
    {
        return;
    }
    *define_condition_form_count += 1;

    let Some(class) = read_define_condition(view) else {
        return;
    };
    if class.has_report {
        return;
    }
    // Only here — with a well-formed, report-less definition in hand — is the
    // whole-file hierarchy built.
    let hierarchy = hierarchy.get();
    if !hierarchy.is_error_type(&class.name)
        || hierarchy.reports_anywhere(&class.name)
        || hierarchy.ancestry_leaves_the_file(&class.name)
    {
        return;
    }
    violations.push(DefineConditionMissingReportForErrorTypeItem {
        span: view.span,
        condition_name: class.name,
    });
}

/// Collects every report-less error condition in one file, with the number of
/// `define-condition` forms scanned as the denominator beside them.
pub fn build_define_condition_missing_report_for_error_type_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DefineConditionMissingReportForErrorTypeItem>> {
    let mut define_condition_form_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        let hierarchy = LazyHierarchy::new(tree);
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_define_condition(
                view,
                &hierarchy,
                &mut define_condition_form_count,
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
        vec![(
            "define_condition_form_count",
            json!(define_condition_form_count),
        )],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DefineConditionMissingReportForErrorTypeItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_define_condition_missing_report_for_error_type_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn violations(input: &str) -> Vec<DefineConditionMissingReportForErrorTypeItem> {
        report(input).findings
    }

    #[test]
    fn flags_an_error_subtype_with_no_report() {
        let found = violations("(define-condition disk-full (error) ((path :initarg :path)))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].condition_name, "disk-full");
    }

    #[test]
    fn flags_a_type_that_reaches_error_through_the_standard_hierarchy() {
        assert_eq!(
            violations("(define-condition disk-full (file-error) ())").len(),
            1
        );
    }

    #[test]
    fn flags_a_type_that_reaches_error_through_a_same_file_supertype() {
        let found = violations(
            "(define-condition io-failure (error) ())\n\
             (define-condition disk-full (io-failure) ())",
        );
        assert_eq!(found.len(), 2);
    }

    /// The near miss: the same definition with the option it is missing.
    #[test]
    fn does_not_flag_a_definition_that_declares_a_report() {
        assert!(
            violations("(define-condition disk-full (error) ()\n  (:report \"the disk is full\"))")
                .is_empty()
        );
        assert!(
            violations("(define-condition disk-full (error) () (:report report-disk-full))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_subtype_whose_same_file_supertype_reports() {
        let found = violations(
            "(define-condition io-failure (error) () (:report \"io failed\"))\n\
             (define-condition disk-full (io-failure) ())",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_error_condition() {
        assert!(violations("(define-condition progress (warning) ())").is_empty());
        assert!(violations("(define-condition progress (condition) ())").is_empty());
    }

    /// `define-condition-empty-superclass-list` owns this shape, and it is not
    /// an error type anyway.
    #[test]
    fn does_not_flag_an_empty_supertype_list() {
        assert!(violations("(define-condition disk-full () ())").is_empty());
    }

    #[test]
    fn does_not_flag_a_type_whose_ancestry_leaves_the_file() {
        assert!(
            violations("(define-condition disk-full (error app-mixin) ())").is_empty(),
            "app-mixin is undeclared here and could supply the report"
        );
    }

    #[test]
    fn does_not_flag_a_malformed_definition() {
        assert!(violations("(define-condition disk-full)").is_empty());
        assert!(violations("(define-condition disk-full error ())").is_empty());
    }

    #[test]
    fn does_not_flag_defclass() {
        assert!(violations("(defclass disk-full (error) ())").is_empty());
    }

    #[test]
    fn a_slot_named_report_does_not_satisfy_the_option() {
        assert_eq!(
            violations("(define-condition disk-full (error) ((:report :initform nil)))").len(),
            1
        );
    }

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(violations("'(define-condition disk-full (error) ())").is_empty());
        assert!(violations("(quote (define-condition disk-full (error) ()))").is_empty());
    }

    #[test]
    fn a_matching_shape_inside_a_backquote_with_no_unquote_is_data() {
        assert!(violations("`(define-condition disk-full (error) ())").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(
            violations("`(progn ,(define-condition disk-full (error) ()))").len(),
            1
        );
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(violations("(format t \"(define-condition disk-full (error) ())\")").is_empty());
    }

    #[test]
    fn the_summary_counts_every_definition_scanned_not_only_the_flagged_ones() {
        let report = report(
            "(define-condition a (error) ())\n\
             (define-condition b (error) () (:report \"b\"))\n\
             (define-condition c (warning) ())\n",
        );
        assert_eq!(
            report.summary,
            vec![("define_condition_form_count", json!(3))]
        );
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_and_its_condition_name() {
        let report = report("(in-package :app)\n(define-condition disk-full (error) ())\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(
            finding.kind(),
            "define-condition-missing-report-for-error-type"
        );
        assert_eq!(
            finding.json_fields(),
            vec![("condition", json!("disk-full"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["condition=disk-full".to_owned()]
        );
        assert!(finding.message().contains("generic description"));
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(define-condition a (error) ())", Dialect::Clojure)
                .expect("parse");
        let report = build_define_condition_missing_report_for_error_type_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(
            report.summary,
            vec![("define_condition_form_count", json!(0))]
        );
    }
}
